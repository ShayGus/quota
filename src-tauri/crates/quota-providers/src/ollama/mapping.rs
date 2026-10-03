//! Turn Ollama Cloud's usage into quota windows.
//!
//! The plan reports a five-hour session window, a weekly window and, on some
//! plans, a monthly one, each as the fraction used. Ollama gives no reset
//! times, so none is shown rather than a guessed one. A window the plan does
//! not report is left out: a free plan can report only the monthly one.

use chrono::{DateTime, Utc};
use quota_core::ports::ProviderError;
use quota_domain::ids::QuotaPoolId;
use quota_domain::provider::ProviderId;
use quota_domain::quota::issue::QuotaIssue;
use quota_domain::quota::scope::ACCOUNT_RESOURCE;
use quota_domain::quota::units::DecimalPrecision;
use quota_domain::quota::window::{MetricRole, QuotaCategory, WindowSemantics};

use crate::decode::{self, DecodedUsage, WindowDraft};
use crate::ollama::wire::{Limit, UsageBody};

/// Decodes one usage answer.
pub(crate) fn decode(
    body: &UsageBody,
    pool: &QuotaPoolId,
    received_at: DateTime<Utc>,
) -> Result<DecodedUsage, ProviderError> {
    let Some(limits) = body.limits.as_ref() else {
        return Err(ProviderError::InvalidData {
            detail: "the usage answer carried no limits".to_owned(),
        });
    };
    let mut decoded = DecodedUsage::new();
    for (limit, category, bucket, seconds) in [
        (
            limits.session.as_ref(),
            QuotaCategory::Session,
            "session",
            Some(18_000),
        ),
        (
            limits.weekly.as_ref(),
            QuotaCategory::Weekly,
            "weekly",
            Some(604_800),
        ),
        (
            limits.monthly.as_ref(),
            QuotaCategory::Monthly,
            "monthly",
            None,
        ),
    ] {
        let Some(limit) = limit else { continue };
        let draft = WindowDraft {
            provider: ProviderId::OllamaCloud,
            pool_id: pool,
            category,
            resource: ACCOUNT_RESOURCE,
            resource_label: "Ollama Cloud",
            bucket_id: Some(bucket),
            metric_role: MetricRole::IncludedAllowance,
            semantics: if category == QuotaCategory::Session {
                WindowSemantics::RollingPeriod
            } else {
                WindowSemantics::Unknown
            },
            duration_seconds: seconds,
            received_at,
        };
        decoded.push(match used_percent(limit) {
            Ok(Some(measurement)) => draft.build(measurement, None, Vec::new())?,
            Ok(None) => draft.reported_missing()?,
            Err(issue) => draft.invalid(vec![issue])?,
        });
    }
    if decoded.windows.is_empty() {
        return Err(ProviderError::InvalidData {
            detail: "the usage answer carried no window".to_owned(),
        });
    }
    Ok(decoded)
}

/// The fraction used, as a percentage.
fn used_percent(
    limit: &Limit,
) -> Result<Option<quota_domain::quota::measurement::Measurement>, QuotaIssue> {
    let Some(reported) = limit.usage.as_ref() else {
        return Ok(None);
    };
    let Some(field) = reported.field() else {
        return Err(QuotaIssue::NonFiniteValue {
            field: "usage".to_owned(),
        });
    };
    decode::percentage(field.value * 100.0, DecimalPrecision::ONE_PLACE, "usage").map(Some)
}

/// The plan's name: `pro` reads "Pro".
pub(crate) fn plan_label(plan: &str) -> Option<String> {
    let plan = plan.trim();
    let mut letters = plan.chars();
    let first = letters.next()?.to_uppercase().collect::<String>();
    Some(format!("{first}{}", letters.as_str()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fractions_used_become_percentages_without_reset_times() {
        let body: UsageBody = serde_json::from_value(serde_json::json!({
            "limits": {
                "session": { "usage": 0.349, "models": [{ "name": "kimi-k3", "request_count": 180 }] },
                "weekly": { "usage": 0.23 }
            },
            "activity": { "cost": "0.00000" }
        }))
        .expect("the body decodes");
        let pool = QuotaPoolId::new("pool-1").expect("a pool id");
        let usage = decode(&body, &pool, Utc::now()).expect("the usage decodes");
        assert_eq!(
            usage.windows.len(),
            2,
            "a window the plan does not report is left out"
        );
        let left = usage.windows[0]
            .measurement
            .remaining_percent()
            .expect("a percentage");
        assert!((left.value() - 65.1).abs() < 1e-9);
        assert!(usage.windows.iter().all(|window| window.boundary.is_none()));
    }

    #[test]
    fn an_answer_without_limits_is_invalid() {
        let pool = QuotaPoolId::new("pool-1").expect("a pool id");
        assert!(matches!(
            decode(&UsageBody::default(), &pool, Utc::now()),
            Err(ProviderError::InvalidData { .. })
        ));
        assert_eq!(plan_label("pro").as_deref(), Some("Pro"));
    }
}
