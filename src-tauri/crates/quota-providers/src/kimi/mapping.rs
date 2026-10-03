//! Turn Kimi for Coding's usage into quota windows.
//!
//! The `usage` summary is the weekly allowance. Each `limits[]` entry is a
//! shorter window whose length its `window` states: 300 minutes is the
//! session window. `usages.limit_month_total` is the monthly allowance. Every
//! one is account-wide included quota; Kimi reports counts without a unit, so
//! each becomes a percentage of its allowance.

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
use crate::kimi::wire::{Detail, Span, UsageEnvelope};

/// The account's monthly ratio.
const MONTH_TOTAL: &str = "limit_month_total";

/// Decodes one usage answer.
pub(crate) fn decode(
    envelope: &UsageEnvelope,
    pool: &QuotaPoolId,
    received_at: DateTime<Utc>,
) -> Result<DecodedUsage, ProviderError> {
    let mut decoded = DecodedUsage::new();
    for limit in &envelope.limits {
        let seconds = limit.window.as_ref().and_then(span_seconds);
        let category = match seconds {
            Some(18_000) => QuotaCategory::Session,
            Some(86_400) => QuotaCategory::Daily,
            _ => QuotaCategory::Custom,
        };
        let bucket = format!("limit:{}", seconds.unwrap_or(0));
        let draft = draft(pool, category, &bucket, seconds, received_at);
        decoded.push(counted(limit.detail.as_ref(), &draft)?);
    }
    if let Some(weekly) = envelope.usage.as_ref() {
        let draft = draft(
            pool,
            QuotaCategory::Weekly,
            "weekly",
            Some(604_800),
            received_at,
        );
        decoded.push(counted(Some(weekly), &draft)?);
    }
    if let Some(month) = envelope.usages.get(MONTH_TOTAL) {
        let draft = draft(pool, QuotaCategory::Monthly, "monthly", None, received_at);
        let boundary = month.reset_time.as_ref().and_then(decode::reset_instant);
        let measurement = month.used_ratio.as_ref().map(ratio_percent);
        decoded.push(match measurement {
            Some(Ok(measurement)) => draft.build(measurement, boundary, Vec::new())?,
            Some(Err(issue)) => draft.invalid(vec![issue])?,
            None => draft.reported_missing()?,
        });
    }
    if decoded.windows.is_empty() {
        return Err(ProviderError::InvalidData {
            detail: "the payload carried no usage window".to_owned(),
        });
    }
    decoded.plan_label = plan_label(envelope);
    Ok(decoded)
}

/// The plan's name, from its membership level: `LEVEL_INTERMEDIATE` reads
/// "Intermediate".
pub(crate) fn plan_label(envelope: &UsageEnvelope) -> Option<String> {
    let level = envelope
        .user
        .as_ref()?
        .membership
        .as_ref()?
        .level
        .as_deref()?;
    let name = level
        .trim()
        .trim_start_matches("LEVEL_")
        .to_ascii_lowercase();
    let mut letters = name.chars();
    let first = letters.next()?.to_uppercase().collect::<String>();
    Some(format!("{first}{}", letters.as_str().replace('_', " ")))
}

fn draft<'a>(
    pool: &'a QuotaPoolId,
    category: QuotaCategory,
    bucket: &'a str,
    seconds: Option<i64>,
    received_at: DateTime<Utc>,
) -> WindowDraft<'a> {
    WindowDraft {
        provider: ProviderId::Kimi,
        pool_id: pool,
        category,
        resource: ACCOUNT_RESOURCE,
        resource_label: "Kimi for Coding",
        bucket_id: Some(bucket),
        metric_role: MetricRole::IncludedAllowance,
        semantics: if category == QuotaCategory::Session {
            WindowSemantics::RollingPeriod
        } else {
            WindowSemantics::AnchoredPeriod
        },
        duration_seconds: seconds,
        received_at,
    }
}

/// A window's reading from its counts, as the percentage used.
fn counted(detail: Option<&Detail>, draft: &WindowDraft<'_>) -> Result<QuotaWindow, ProviderError> {
    let Some(detail) = detail else {
        return draft.reported_missing();
    };
    let boundary = detail.reset_time.as_ref().and_then(decode::reset_instant);
    let value = |field: Option<&Numberish>| field.and_then(Numberish::field).map(|f| f.value);
    let limit = value(detail.limit.as_ref()).filter(|limit| *limit > 0.0);
    let used =
        value(detail.used.as_ref()).or_else(|| Some(limit? - value(detail.remaining.as_ref())?));
    let Some((used, limit)) = used.zip(limit) else {
        return draft.build(
            Measurement::Unavailable(UnavailableReason::NotReported),
            boundary,
            Vec::new(),
        );
    };
    match decode::percentage(used / limit * 100.0, DecimalPrecision::ONE_PLACE, "used") {
        Ok(measurement) => draft.build(measurement, boundary, Vec::new()),
        Err(issue) => draft.invalid(vec![issue]),
    }
}

/// A used fraction from 0 to 1, as the percentage used.
fn ratio_percent(ratio: &Numberish) -> Result<Measurement, QuotaIssue> {
    let field = ratio.field().ok_or_else(|| QuotaIssue::NonFiniteValue {
        field: "used_ratio".to_owned(),
    })?;
    decode::percentage(
        field.value * 100.0,
        DecimalPrecision::ONE_PLACE,
        "used_ratio",
    )
}

/// The length of a window, in seconds.
fn span_seconds(span: &Span) -> Option<i64> {
    let duration = span.duration.as_ref()?.whole()?;
    let unit = span.time_unit.as_deref()?.to_ascii_uppercase();
    let per_unit = if unit.contains("MINUTE") {
        60
    } else if unit.contains("HOUR") {
        3_600
    } else if unit.contains("DAY") {
        86_400
    } else if unit.contains("SECOND") {
        1
    } else {
        return None;
    };
    duration.checked_mul(per_unit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_session_weekly_and_monthly_windows_come_from_their_own_fields() {
        let envelope: UsageEnvelope = serde_json::from_value(serde_json::json!({
            "user": { "membership": { "level": "LEVEL_INTERMEDIATE" } },
            "usage": { "limit": "100", "used": "25", "remaining": "75",
                       "resetTime": "2026-10-08T00:00:00Z" },
            "limits": [
                { "window": { "duration": 300, "timeUnit": "TIME_UNIT_MINUTE" },
                  "detail": { "limit": "40", "remaining": "30" } }
            ],
            "usages": { "limit_month_total": { "used_ratio": 0.5 },
                        "limit_5h": { "used_ratio": 0.9 } }
        }))
        .expect("the body decodes");
        let pool = QuotaPoolId::new("pool-1").expect("a pool id");
        let received_at = DateTime::from_timestamp(1_700_000_000, 0).expect("an instant");
        let usage = decode(&envelope, &pool, received_at).expect("the usage decodes");

        assert_eq!(usage.plan_label.as_deref(), Some("Intermediate"));
        let left = |category: QuotaCategory| {
            usage
                .windows
                .iter()
                .find(|window| window.category == category)
                .and_then(|window| window.measurement.remaining_percent())
                .map(quota_domain::Percent::value)
        };
        // Used is derived from the remaining count when it is not reported.
        assert_eq!(left(QuotaCategory::Session), Some(75.0));
        assert_eq!(left(QuotaCategory::Weekly), Some(75.0));
        assert_eq!(left(QuotaCategory::Monthly), Some(50.0));
        assert_eq!(
            usage.windows.len(),
            3,
            "only the account's monthly ratio is read"
        );
    }

    #[test]
    fn an_answer_without_any_window_is_invalid() {
        let pool = QuotaPoolId::new("pool-1").expect("a pool id");
        let decoded = decode(&UsageEnvelope::default(), &pool, Utc::now());
        assert!(matches!(decoded, Err(ProviderError::InvalidData { .. })));
    }
}
