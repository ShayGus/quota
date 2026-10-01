//! Turn an `OpenCode` Go usage payload into normalised quota windows.
//!
//! The rules this mapping will not break: the rolling window is five hours and
//! the weekly window is seven days, both stated by the provider; the monthly
//! window is real and is kept, because this is the only connector in scope that
//! reports one. A window the source did not report becomes a not-reported
//! window, never a zero.

use chrono::{DateTime, Utc};
use quota_core::ports::ProviderError;
use quota_domain::ids::QuotaPoolId;
use quota_domain::provider::ProviderId;
use quota_domain::quota::issue::QuotaIssue;
use quota_domain::quota::measurement::{Measurement, UnavailableReason};
use quota_domain::quota::window::{MetricRole, QuotaCategory, QuotaWindow, WindowSemantics};

use crate::decode::{self, DecodedUsage, WindowDraft, percentage};
use crate::opencode_go::wire::{OpenCodeGoBucket, OpenCodeGoEnvelope, OpenCodeGoUsage};

/// The duration of the rolling window, in seconds.
const ROLLING_SECONDS: i64 = 18_000;

/// The duration of the weekly window, in seconds.
const WEEKLY_SECONDS: i64 = 604_800;

/// The field name reported when a percentage is unusable.
const USED_FIELD: &str = "percent";

/// Decodes one `OpenCode` Go payload into windows.
pub(crate) fn decode(
    envelope: &OpenCodeGoEnvelope,
    pool: &QuotaPoolId,
    received_at: DateTime<Utc>,
) -> Result<DecodedUsage, ProviderError> {
    let usage = envelope.usage();
    if usage.is_empty() {
        return Err(ProviderError::InvalidData {
            detail: "the payload carried no usage window".to_owned(),
        });
    }
    let mut decoded = DecodedUsage::new();
    for spec in windows() {
        let bucket = spec.window(usage);
        let draft = draft(pool, spec, received_at);
        match bucket {
            Some(bucket) => decoded.push(bucket_window(bucket, &draft, received_at)?),
            None => decoded.push(draft.reported_missing()?),
        }
    }
    Ok(decoded)
}

/// The three windows this provider reports, and how to find each one.
struct WindowSpec {
    /// The period category.
    category: QuotaCategory,
    /// The raw metered-resource text.
    resource: &'static str,
    /// The label shown beside the resource.
    label: &'static str,
    /// The provider's own bucket identifier.
    bucket: &'static str,
    /// What the period means.
    semantics: WindowSemantics,
    /// The provider-stated duration, in seconds. `None` means rolling, which is
    /// what this provider's own bucket names state, and what keeps the boundary
    /// honest.
    stated_seconds: Option<i64>,
}

impl WindowSpec {
    /// The bucket this window reads from the payload.
    fn window<'a>(&self, usage: &'a OpenCodeGoUsage) -> Option<&'a OpenCodeGoBucket> {
        match self.bucket {
            "rolling" => usage.rolling_usage.as_ref(),
            "weekly" => usage.weekly_usage.as_ref(),
            _ => usage.monthly_usage.as_ref(),
        }
    }
}

/// The provider's three windows, in a stable order.
fn windows() -> [WindowSpec; 3] {
    [
        WindowSpec {
            category: QuotaCategory::Session,
            resource: "account",
            label: "OpenCode Go rolling",
            bucket: "rolling",
            // The provider states a five-hour rolling window.
            semantics: WindowSemantics::RollingPeriod,
            stated_seconds: Some(ROLLING_SECONDS),
        },
        WindowSpec {
            category: QuotaCategory::Weekly,
            resource: "account",
            label: "OpenCode Go weekly",
            bucket: "weekly",
            semantics: WindowSemantics::RollingPeriod,
            stated_seconds: Some(WEEKLY_SECONDS),
        },
        WindowSpec {
            // The only monthly window in scope, so it is mapped, never dropped.
            category: QuotaCategory::Monthly,
            resource: "account",
            label: "OpenCode Go monthly",
            bucket: "monthly",
            // A calendar cycle is not assumed: this source states a duration,
            // not the shape of the period.
            semantics: WindowSemantics::RollingPeriod,
            stated_seconds: None,
        },
    ]
}

/// Builds the draft for one window.
fn draft<'a>(
    pool: &'a QuotaPoolId,
    spec: &'a WindowSpec,
    received_at: DateTime<Utc>,
) -> WindowDraft<'a> {
    WindowDraft {
        provider: ProviderId::OpenCodeGo,
        pool_id: pool,
        category: spec.category,
        resource: spec.resource,
        resource_label: spec.label,
        bucket_id: Some(spec.bucket),
        metric_role: MetricRole::IncludedAllowance,
        semantics: spec.semantics,
        duration_seconds: spec.stated_seconds,
        received_at,
    }
}

/// Builds one window from its reported percentage.
fn bucket_window(
    bucket: &OpenCodeGoBucket,
    draft: &WindowDraft<'_>,
    received_at: DateTime<Utc>,
) -> Result<QuotaWindow, ProviderError> {
    let boundary = bucket
        .resets_at
        .as_ref()
        .and_then(decode::reset_instant)
        .or_else(|| {
            bucket
                .reset_in_sec
                .as_ref()
                .and_then(|value| decode::reset_after(value, received_at))
        });
    match decode_bucket(bucket) {
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

/// Reads the reported percentage, from either polarity.
///
/// `Ok(None)` means the window carried no percentage at all.
fn decode_bucket(bucket: &OpenCodeGoBucket) -> Result<Option<Measurement>, QuotaIssue> {
    if let Some(reported) = bucket.percent.as_ref() {
        let Some(field) = reported.field() else {
            return Err(QuotaIssue::NonFiniteValue {
                field: USED_FIELD.to_owned(),
            });
        };
        return percentage(field.value, field.decimals, USED_FIELD).map(Some);
    }
    let Some(remaining) = bucket.percent_remaining.as_ref() else {
        return Ok(None);
    };
    let Some(field) = remaining.field() else {
        return Err(QuotaIssue::NonFiniteValue {
            field: "percentRemaining".to_owned(),
        });
    };
    // The provider reported what is left, so the used value is its complement.
    percentage(100.0 - field.value, field.decimals, "percentRemaining").map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_monthly_window_is_present_and_named() {
        let specs = windows();
        let monthly = specs
            .iter()
            .find(|spec| spec.category == QuotaCategory::Monthly)
            .expect("the monthly window exists");
        assert_eq!(monthly.bucket, "monthly");
    }

    #[test]
    fn a_remaining_percentage_becomes_a_used_percentage() {
        let bucket = OpenCodeGoBucket {
            percent_remaining: Some(crate::decode::Numberish::Text("37.5".to_owned())),
            ..OpenCodeGoBucket::default()
        };
        let measurement = decode_bucket(&bucket)
            .expect("the value parses")
            .expect("a measurement exists");
        let remaining = measurement.remaining_percent().expect("a percentage");
        assert!((remaining.value() - 37.5).abs() < f64::EPSILON);
    }
}
