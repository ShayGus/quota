//! Turn Z.ai's coding-plan meters into quota windows.
//!
//! Each `limits[]` entry is one meter over one window: tokens or credits, the
//! plan's own allowance, or web tool requests (search, reader, zread), which
//! are a separate allowance. The window comes from `unit` and `number`: five
//! hours is the session window, one week the weekly, one month the monthly.
//! A count against an allowance is kept in its unit; without one, the
//! server's rounded percentage is used.

use chrono::{DateTime, Utc};
use quota_core::ports::ProviderError;
use quota_domain::ids::QuotaPoolId;
use quota_domain::provider::ProviderId;
use quota_domain::quota::issue::QuotaIssue;
use quota_domain::quota::measurement::{Measurement, UnavailableReason};
use quota_domain::quota::scope::ACCOUNT_RESOURCE;
use quota_domain::quota::units::QuotaUnit;
use quota_domain::quota::window::{MetricRole, QuotaCategory, QuotaWindow, WindowSemantics};

use crate::decode::{self, DecodedUsage, Numberish, WindowDraft};
use crate::zai::wire::{QuotaData, QuotaLimit};

/// Seconds in one hour, one day, and one week.
const HOUR: i64 = 3_600;
const DAY: i64 = 86_400;
const WEEK: i64 = 604_800;

/// Decodes the plan's meters.
pub(crate) fn decode(
    data: &QuotaData,
    pool: &QuotaPoolId,
    received_at: DateTime<Utc>,
) -> Result<DecodedUsage, ProviderError> {
    let mut decoded = DecodedUsage::new();
    for limit in &data.limits {
        if let Some(window) = window(limit, pool, received_at)? {
            decoded.push(window);
        }
    }
    if decoded.windows.is_empty() {
        return Err(ProviderError::InvalidData {
            detail: "the payload carried no quota meter".to_owned(),
        });
    }
    decoded.plan_label = plan_label(data.level.as_deref());
    Ok(decoded)
}

/// The plan's name, from its tier.
pub(crate) fn plan_label(level: Option<&str>) -> Option<String> {
    let level = level.map(str::trim).filter(|level| !level.is_empty())?;
    let mut letters = level.chars();
    let first = letters.next()?.to_uppercase().collect::<String>();
    Some(format!("GLM Coding Plan {first}{}", letters.as_str()))
}

/// One meter's window, or `None` for a meter this build does not know.
fn window(
    limit: &QuotaLimit,
    pool: &QuotaPoolId,
    received_at: DateTime<Utc>,
) -> Result<Option<QuotaWindow>, ProviderError> {
    let (resource, label, unit) = match limit.kind.as_deref() {
        Some("TOKENS_LIMIT") => (ACCOUNT_RESOURCE, "Tokens", QuotaUnit::Tokens),
        Some("CREDIT_LIMIT") => (ACCOUNT_RESOURCE, "Credits", QuotaUnit::Credits),
        Some("TIME_LIMIT") => ("web-tools", "Web tools", QuotaUnit::Requests),
        _ => return Ok(None),
    };
    let span = span(limit);
    let (category, seconds) = match span {
        Some(seconds) if seconds == 5 * HOUR => (QuotaCategory::Session, Some(seconds)),
        Some(DAY) => (QuotaCategory::Daily, Some(DAY)),
        Some(WEEK) => (QuotaCategory::Weekly, Some(WEEK)),
        Some(seconds) if seconds == 30 * DAY => (QuotaCategory::Monthly, None),
        other => (QuotaCategory::Custom, other),
    };
    let bucket = format!(
        "{}:{}",
        limit
            .kind
            .as_deref()
            .unwrap_or("meter")
            .to_ascii_lowercase(),
        span.unwrap_or(0)
    );
    let draft = WindowDraft {
        provider: ProviderId::Zai,
        pool_id: pool,
        category,
        resource,
        resource_label: label,
        bucket_id: Some(&bucket),
        metric_role: MetricRole::IncludedAllowance,
        semantics: if category == QuotaCategory::Session {
            WindowSemantics::RollingPeriod
        } else {
            WindowSemantics::AnchoredPeriod
        },
        duration_seconds: seconds,
        received_at,
    };
    let boundary = limit.next_reset_time.as_ref().and_then(decode::reset_epoch);
    let built = match measurement(limit, unit) {
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
    };
    built.map(Some)
}

/// The window's length in seconds, from its unit and count.
fn span(limit: &QuotaLimit) -> Option<i64> {
    let count = limit
        .number
        .as_ref()
        .and_then(Numberish::whole)
        .filter(|count| *count > 0)
        .unwrap_or(1);
    let unit = match limit.unit.as_ref().and_then(Numberish::whole)? {
        3 => HOUR,
        4 => DAY,
        5 => 30 * DAY,
        6 => WEEK,
        _ => return None,
    };
    count.checked_mul(unit)
}

/// The count against the allowance, or the server's percentage without one.
fn measurement(limit: &QuotaLimit, unit: QuotaUnit) -> Result<Option<Measurement>, QuotaIssue> {
    let value = |field: Option<&Numberish>| field.and_then(Numberish::field).map(|f| f.value);
    if let Some(counted) = decode::counted(
        unit,
        value(limit.current_value.as_ref()),
        value(limit.remaining.as_ref()),
        value(limit.usage.as_ref()),
    ) {
        return Ok(Some(counted));
    }
    let Some(reported) = limit.percentage.as_ref() else {
        return Ok(None);
    };
    let Some(field) = reported.field() else {
        return Err(QuotaIssue::NonFiniteValue {
            field: "percentage".to_owned(),
        });
    };
    decode::percentage(field.value, field.decimals, "percentage").map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zai::wire::QuotaEnvelope;

    fn decoded(body: serde_json::Value) -> DecodedUsage {
        let envelope: QuotaEnvelope = serde_json::from_value(body).expect("the body decodes");
        let pool = QuotaPoolId::new("pool-1").expect("a pool id");
        let received_at = DateTime::from_timestamp(1_700_000_000, 0).expect("an instant");
        decode(&envelope.data.expect("data"), &pool, received_at).expect("the meters decode")
    }

    #[test]
    fn the_coding_plan_windows_are_session_weekly_and_web_tools() {
        let usage = decoded(serde_json::json!({
            "success": true,
            "data": {
                "level": "pro",
                "limits": [
                    { "type": "TOKENS_LIMIT", "unit": 3, "number": 5, "usage": 12000,
                      "currentValue": 1438, "remaining": 10562, "percentage": 11,
                      "nextResetTime": 1_700_010_000_000_i64 },
                    { "type": "TOKENS_LIMIT", "unit": 6, "number": 1, "percentage": 40 },
                    { "type": "TIME_LIMIT", "unit": 5, "number": 1, "usage": 1000,
                      "currentValue": 10 },
                    { "type": "SOMETHING_NEW", "unit": 3, "number": 5 }
                ]
            }
        }));
        assert_eq!(usage.plan_label.as_deref(), Some("GLM Coding Plan Pro"));
        assert_eq!(usage.windows.len(), 3, "an unknown meter is skipped");
        let session = &usage.windows[0];
        assert_eq!(session.category, QuotaCategory::Session);
        assert_eq!(session.scope.resource().as_str(), ACCOUNT_RESOURCE);
        assert_eq!(
            session.boundary.map(|boundary| boundary.at.timestamp()),
            Some(1_700_010_000),
            "the reset time is read as milliseconds"
        );
        // The exact count wins over the server's rounded percentage.
        let remaining = session
            .measurement
            .remaining_percent()
            .expect("a percentage");
        assert!((remaining.value() - 88.016_666).abs() < 0.001);
        assert_eq!(usage.windows[1].category, QuotaCategory::Weekly);
        let weekly = usage.windows[1]
            .measurement
            .remaining_percent()
            .expect("a percentage");
        assert!((weekly.value() - 60.0).abs() < f64::EPSILON);
        assert_eq!(usage.windows[2].category, QuotaCategory::Monthly);
        assert_eq!(usage.windows[2].scope.resource().as_str(), "web-tools");
    }

    #[test]
    fn a_payload_without_meters_is_invalid_not_empty() {
        let envelope: QuotaEnvelope = serde_json::from_value(
            serde_json::json!({ "success": true, "data": { "limits": null } }),
        )
        .expect("the body decodes");
        let pool = QuotaPoolId::new("pool-1").expect("a pool id");
        let decoded = decode(&envelope.data.expect("data"), &pool, Utc::now());
        assert!(matches!(decoded, Err(ProviderError::InvalidData { .. })));
    }
}
