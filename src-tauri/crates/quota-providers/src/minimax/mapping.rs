//! Turn `MiniMax`'s token-plan buckets into quota windows.
//!
//! Each bucket is one plan quota with a rolling interval window and a weekly
//! window, each reported as the percent remaining and a status. The
//! `general` bucket is the plan-wide quota every chat model draws from; any
//! other bucket, such as video, meters its own allowance. A model outside the
//! plan is reported as unlimited with zero totals; it is left out rather than
//! shown as untouched quota.

use chrono::{DateTime, Utc};
use quota_core::ports::ProviderError;
use quota_domain::ids::QuotaPoolId;
use quota_domain::provider::ProviderId;
use quota_domain::quota::issue::QuotaIssue;
use quota_domain::quota::measurement::{Measurement, UnavailableReason};
use quota_domain::quota::scope::ACCOUNT_RESOURCE;
use quota_domain::quota::units::DecimalPrecision;
use quota_domain::quota::window::{MetricRole, QuotaCategory, QuotaWindow, WindowSemantics};

use crate::decode::{self, DecodedUsage, Numberish, WindowDraft};
use crate::minimax::wire::Bucket;

/// The plan-wide bucket.
const SHARED_BUCKET: &str = "general";

/// The window statuses `MiniMax` reports.
const EXHAUSTED: i64 = 2;
const UNLIMITED: i64 = 3;

/// The session window's length, in seconds.
const FIVE_HOURS: i64 = 18_000;

/// Decodes every bucket in the plan.
pub(crate) fn decode(
    buckets: &[Bucket],
    pool: &QuotaPoolId,
    received_at: DateTime<Utc>,
) -> Result<DecodedUsage, ProviderError> {
    let mut decoded = DecodedUsage::new();
    for bucket in buckets {
        let Some(name) = bucket
            .model_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
        else {
            continue;
        };
        if outside_plan(bucket) {
            continue;
        }
        let (resource, label) = if name == SHARED_BUCKET {
            (ACCOUNT_RESOURCE.to_owned(), "Token plan".to_owned())
        } else {
            (decode::identifier(name), capitalised(name))
        };
        let interval = Window {
            category: interval_category(bucket),
            bucket: "interval",
            remaining: bucket.current_interval_remaining_percent.as_ref(),
            status: bucket.current_interval_status.as_ref(),
            ends: bucket.end_time.as_ref(),
        };
        let weekly = Window {
            category: QuotaCategory::Weekly,
            bucket: "weekly",
            remaining: bucket.current_weekly_remaining_percent.as_ref(),
            status: bucket.current_weekly_status.as_ref(),
            ends: bucket.weekly_end_time.as_ref(),
        };
        for spec in [interval, weekly] {
            decoded.push(spec.build(&resource, &label, pool, received_at)?);
        }
    }
    if decoded.windows.is_empty() {
        return Err(ProviderError::InvalidData {
            detail: "the payload carried no plan quota".to_owned(),
        });
    }
    Ok(decoded)
}

/// One of a bucket's two windows.
struct Window<'a> {
    category: QuotaCategory,
    bucket: &'static str,
    remaining: Option<&'a Numberish>,
    status: Option<&'a Numberish>,
    ends: Option<&'a Numberish>,
}

impl Window<'_> {
    fn build(
        &self,
        resource: &str,
        label: &str,
        pool: &QuotaPoolId,
        received_at: DateTime<Utc>,
    ) -> Result<QuotaWindow, ProviderError> {
        let bucket = format!("{resource}:{}", self.bucket);
        let draft = WindowDraft {
            provider: ProviderId::Minimax,
            pool_id: pool,
            category: self.category,
            resource,
            resource_label: label,
            bucket_id: Some(&bucket),
            metric_role: MetricRole::IncludedAllowance,
            semantics: WindowSemantics::RollingPeriod,
            duration_seconds: match self.category {
                QuotaCategory::Session => Some(FIVE_HOURS),
                QuotaCategory::Weekly => Some(604_800),
                _ => None,
            },
            received_at,
        };
        let boundary = self.ends.and_then(decode::reset_epoch);
        match self.measurement() {
            Ok(Some(measurement)) => draft.build(measurement, boundary, Vec::new()),
            Ok(None) => draft.build(
                Measurement::Unavailable(UnavailableReason::NotReported),
                boundary,
                Vec::new(),
            ),
            Err(issue) => draft.build(
                Measurement::Unavailable(UnavailableReason::InvalidResponse),
                boundary,
                vec![issue],
            ),
        }
    }

    /// The window's reading. The status outranks the percentage: an exhausted
    /// window may keep a stale one that would read as healthy quota.
    fn measurement(&self) -> Result<Option<Measurement>, QuotaIssue> {
        match self.status.and_then(Numberish::whole) {
            Some(EXHAUSTED) => {
                return decode::percentage(100.0, DecimalPrecision::WHOLE, "status").map(Some);
            }
            Some(UNLIMITED) => return Ok(Some(Measurement::Unlimited)),
            _ => {}
        }
        let Some(remaining) = self.remaining else {
            return Ok(None);
        };
        let Some(field) = remaining.field() else {
            return Err(QuotaIssue::NonFiniteValue {
                field: "remaining_percent".to_owned(),
            });
        };
        decode::percentage(100.0 - field.value, field.decimals, "remaining_percent").map(Some)
    }
}

/// Whether the bucket names a model the plan does not include.
fn outside_plan(bucket: &Bucket) -> bool {
    let zero = |count: Option<&Numberish>| count.and_then(Numberish::whole) == Some(0);
    let unlimited =
        |status: Option<&Numberish>| status.and_then(Numberish::whole) == Some(UNLIMITED);
    zero(bucket.current_interval_total_count.as_ref())
        && zero(bucket.current_weekly_total_count.as_ref())
        && unlimited(bucket.current_interval_status.as_ref())
        && unlimited(bucket.current_weekly_status.as_ref())
}

/// The interval window's category: five hours is the session window.
fn interval_category(bucket: &Bucket) -> QuotaCategory {
    let start = bucket.start_time.as_ref().and_then(decode::reset_epoch);
    let end = bucket.end_time.as_ref().and_then(decode::reset_epoch);
    match start
        .zip(end)
        .map(|(start, end)| (end - start).num_seconds())
    {
        Some(FIVE_HOURS) => QuotaCategory::Session,
        Some(86_400) => QuotaCategory::Daily,
        _ => QuotaCategory::Custom,
    }
}

fn capitalised(name: &str) -> String {
    let mut letters = name.chars();
    letters.next().map_or_else(String::new, |first| {
        first.to_uppercase().collect::<String>() + letters.as_str()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::minimax::wire::RemainsEnvelope;

    fn decoded(body: serde_json::Value) -> DecodedUsage {
        let envelope: RemainsEnvelope = serde_json::from_value(body).expect("the body decodes");
        let pool = QuotaPoolId::new("pool-1").expect("a pool id");
        let received_at = DateTime::from_timestamp(1_700_000_000, 0).expect("an instant");
        decode(envelope.buckets(), &pool, received_at).expect("the buckets decode")
    }

    #[test]
    fn the_plan_wide_bucket_gives_the_session_and_weekly_windows() {
        let usage = decoded(serde_json::json!({
            "base_resp": { "status_code": 0 },
            "model_remains": [
                { "model_name": "general", "start_time": 1_700_000_000, "end_time": 1_700_018_000,
                  "current_interval_remaining_percent": 75, "current_interval_status": 1,
                  "current_interval_total_count": 600,
                  "weekly_end_time": 1_700_500_000, "current_weekly_remaining_percent": 90,
                  "current_weekly_status": 1, "current_weekly_total_count": 6000 },
                { "model_name": "video", "current_interval_total_count": 0,
                  "current_weekly_total_count": 0, "current_interval_status": 3,
                  "current_weekly_status": 3 }
            ]
        }));
        assert_eq!(
            usage.windows.len(),
            2,
            "a model outside the plan is left out"
        );
        let session = &usage.windows[0];
        assert_eq!(session.category, QuotaCategory::Session);
        assert_eq!(session.scope.resource().as_str(), ACCOUNT_RESOURCE);
        let left = session
            .measurement
            .remaining_percent()
            .expect("a percentage");
        assert!((left.value() - 75.0).abs() < f64::EPSILON);
        assert_eq!(usage.windows[1].category, QuotaCategory::Weekly);
    }

    #[test]
    fn an_exhausted_window_reads_empty_whatever_its_percentage_says() {
        let usage = decoded(serde_json::json!({
            "data": { "model_remains": [
                { "model_name": "general", "current_interval_remaining_percent": 60,
                  "current_interval_status": 2, "current_weekly_status": 1,
                  "current_weekly_remaining_percent": 10 }
            ] }
        }));
        let left = usage.windows[0]
            .measurement
            .remaining_percent()
            .expect("a percentage");
        assert!(left.value().abs() < f64::EPSILON);
    }
}
