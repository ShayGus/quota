//! Turn Ollama Cloud's balance into quota windows.
//!
//! A plan with usage limits reports a five-hour session window and a weekly
//! window, each as the percentage left with its reset time. A plan with a
//! monthly allowance reports the allowance and what is left of it in dollars,
//! which becomes the monthly window's share used, renewing when its period
//! ends. A window the plan does not report is left out, and a reset time
//! Ollama does not give is not guessed.

use chrono::{DateTime, Utc};
use quota_core::ports::ProviderError;
use quota_domain::ids::QuotaPoolId;
use quota_domain::provider::ProviderId;
use quota_domain::quota::issue::QuotaIssue;
use quota_domain::quota::measurement::Measurement;
use quota_domain::quota::scope::ACCOUNT_RESOURCE;
use quota_domain::quota::units::DecimalPrecision;
use quota_domain::quota::window::{MetricRole, QuotaCategory, WindowSemantics};

use crate::decode::{self, DecodedUsage, Numberish, WindowDraft};
use crate::ollama::wire::{BalanceBody, Included, Limit};

/// Decodes one balance answer.
pub(crate) fn decode(
    body: &BalanceBody,
    pool: &QuotaPoolId,
    received_at: DateTime<Utc>,
) -> Result<DecodedUsage, ProviderError> {
    let Some(included) = body.included.as_ref() else {
        return Err(ProviderError::InvalidData {
            detail: "the balance answer carried no included usage".to_owned(),
        });
    };
    let mut decoded = DecodedUsage::new();
    for (category, bucket, seconds) in [
        (QuotaCategory::Session, "session", Some(18_000)),
        (QuotaCategory::Weekly, "weekly", Some(604_800)),
        (QuotaCategory::Monthly, "monthly", None),
    ] {
        let Some(reading) = reading(included, category) else {
            continue;
        };
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
        decoded.push(match reading.measurement {
            Ok(Some(measurement)) => draft.build(measurement, reading.resets_at, Vec::new())?,
            Ok(None) => draft.reported_missing()?,
            Err(issue) => draft.invalid(vec![issue])?,
        });
    }
    if decoded.windows.is_empty() {
        return Err(ProviderError::InvalidData {
            detail: "the balance answer carried no window".to_owned(),
        });
    }
    Ok(decoded)
}

/// One window as the answer reports it: what is used, and when it resets.
struct Reading {
    measurement: Result<Option<Measurement>, QuotaIssue>,
    resets_at: Option<DateTime<Utc>>,
}

/// The window of `category` the answer reports, if it reports one. The
/// monthly window is a limit window when the plan has one, and otherwise the
/// monthly allowance.
fn reading(included: &Included, category: QuotaCategory) -> Option<Reading> {
    let limit = match category {
        QuotaCategory::Session => included.session.as_ref(),
        QuotaCategory::Weekly => included.weekly.as_ref(),
        _ => included.monthly.as_ref(),
    };
    if let Some(limit) = limit {
        return Some(Reading {
            measurement: used_percent(limit),
            resets_at: limit.resets_at.as_ref().and_then(decode::reset_instant),
        });
    }
    if category != QuotaCategory::Monthly {
        return None;
    }
    let allowance = included.allowance_usd.as_ref()?;
    Some(Reading {
        measurement: allowance_used(allowance, included.balance_usd.as_ref()),
        resets_at: included
            .period
            .as_ref()
            .and_then(|period| period.until.as_ref())
            .and_then(decode::reset_instant),
    })
}

/// The share of a limit window used, from the percentage left.
fn used_percent(limit: &Limit) -> Result<Option<Measurement>, QuotaIssue> {
    let Some(reported) = limit.remaining_percent.as_ref() else {
        return Ok(None);
    };
    let Some(field) = reported.field() else {
        return Err(QuotaIssue::NonFiniteValue {
            field: "remaining_percent".to_owned(),
        });
    };
    decode::percentage(
        100.0 - field.value,
        DecimalPrecision::ONE_PLACE,
        "remaining_percent",
    )
    .map(Some)
}

/// The share of the monthly allowance used, from the allowance and what is
/// left of it. An allowance of nothing has no share, so none is shown.
fn allowance_used(
    allowance: &Numberish,
    left: Option<&Numberish>,
) -> Result<Option<Measurement>, QuotaIssue> {
    let Some(left) = left else {
        return Ok(None);
    };
    let (Some(allowance), Some(left)) = (allowance.field(), left.field()) else {
        return Err(QuotaIssue::NonFiniteValue {
            field: "balance_usd".to_owned(),
        });
    };
    if allowance.value <= 0.0 {
        return Ok(None);
    }
    decode::percentage(
        (allowance.value - left.value) / allowance.value * 100.0,
        DecimalPrecision::ONE_PLACE,
        "balance_usd",
    )
    .map(Some)
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

    fn pool() -> QuotaPoolId {
        QuotaPoolId::new("pool-1").expect("a pool id")
    }

    fn body(value: serde_json::Value) -> BalanceBody {
        serde_json::from_value(value).expect("the body decodes")
    }

    #[test]
    fn limit_windows_become_percentages_with_their_reset_times() {
        let body = body(serde_json::json!({
            "included": {
                "session": { "remaining_percent": 65.1, "resets_at": "2026-11-03T21:00:00Z" },
                "weekly": { "remaining_percent": "77", "resets_at": "2026-11-09T00:00:00Z" }
            },
            "purchased": { "balance_usd": 12.5 }
        }));
        let usage = decode(&body, &pool(), Utc::now()).expect("the balance decodes");
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
        let resets: Vec<String> = usage
            .windows
            .iter()
            .map(|window| {
                window
                    .boundary
                    .as_ref()
                    .expect("a reset time")
                    .at
                    .to_rfc3339()
            })
            .collect();
        assert_eq!(
            resets,
            ["2026-11-03T21:00:00+00:00", "2026-11-09T00:00:00+00:00"]
        );
    }

    #[test]
    fn a_monthly_allowance_becomes_the_share_used_until_its_period_ends() {
        let body = body(serde_json::json!({
            "included": {
                "allowance_usd": 60,
                "balance_usd": "52.50",
                "period": { "from": "2026-11-01T00:00:00Z", "until": "2026-12-01T00:00:00Z" }
            }
        }));
        let usage = decode(&body, &pool(), Utc::now()).expect("the balance decodes");
        assert_eq!(usage.windows.len(), 1);
        let window = &usage.windows[0];
        let left = window
            .measurement
            .remaining_percent()
            .expect("a percentage");
        assert!((left.value() - 87.5).abs() < 1e-9);
        assert_eq!(
            window
                .boundary
                .as_ref()
                .map(|boundary| boundary.at.to_rfc3339()),
            Some("2026-12-01T00:00:00+00:00".to_owned())
        );
    }

    #[test]
    fn an_allowance_of_nothing_has_no_share_and_a_bad_one_is_invalid() {
        let empty = body(serde_json::json!({
            "included": { "allowance_usd": 0, "balance_usd": 0 }
        }));
        let usage = decode(&empty, &pool(), Utc::now()).expect("the balance decodes");
        assert!(usage.windows[0].measurement.remaining_percent().is_none());
        let bad = body(serde_json::json!({
            "included": { "weekly": { "remaining_percent": "lots" } }
        }));
        let usage = decode(&bad, &pool(), Utc::now()).expect("the balance decodes");
        assert!(!usage.windows[0].issues.is_empty());
    }

    #[test]
    fn an_answer_without_included_usage_is_invalid() {
        assert!(matches!(
            decode(&BalanceBody::default(), &pool(), Utc::now()),
            Err(ProviderError::InvalidData { .. })
        ));
        assert!(matches!(
            decode(
                &body(serde_json::json!({ "included": {} })),
                &pool(),
                Utc::now()
            ),
            Err(ProviderError::InvalidData { .. })
        ));
        assert_eq!(plan_label("pro").as_deref(), Some("Pro"));
    }
}
