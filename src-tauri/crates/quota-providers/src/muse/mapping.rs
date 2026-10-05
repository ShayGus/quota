//! Turn Muse Code's subscription usage into quota windows.
//!
//! The subscription has a rolling window, five hours on today's plans, and a
//! weekly window, each reported as the percent used. Muse omits the usage
//! while the rolling window is idle, and may omit either window. As oh-my-pi
//! does, a window is shown only when Muse reports its percent; one it leaves
//! out is not shown, rather than as an empty allowance or a partial reading.

use chrono::{DateTime, Utc};
use quota_core::ports::ProviderError;
use quota_domain::ids::QuotaPoolId;
use quota_domain::provider::ProviderId;
use quota_domain::quota::issue::QuotaIssue;
use quota_domain::quota::scope::ACCOUNT_RESOURCE;
use quota_domain::quota::window::{MetricRole, QuotaCategory, QuotaWindow, WindowSemantics};

use crate::decode::{self, DecodedUsage, Numberish, WindowDraft};
use crate::muse::wire::{SubscriptionAnswer, UsageWindow};

/// Decodes one subscription answer.
pub(crate) fn decode(
    answer: &SubscriptionAnswer,
    pool: &QuotaPoolId,
    received_at: DateTime<Utc>,
) -> Result<DecodedUsage, ProviderError> {
    if answer.is_subs_active == Some(false) {
        return Err(ProviderError::Authorization);
    }
    let usage = answer.subs_usage.as_ref();
    let rolling = usage.and_then(|usage| usage.window.as_ref());
    let minutes = rolling
        .and_then(|window| window.window_duration_mins.as_ref())
        .and_then(Numberish::whole);
    let session = match minutes {
        None | Some(300) => QuotaCategory::Session,
        Some(_) => QuotaCategory::Custom,
    };
    let mut decoded = DecodedUsage::new();
    for window in [
        window(
            rolling,
            session,
            "rolling",
            minutes.and_then(|minutes| minutes.checked_mul(60)),
            pool,
            received_at,
        )?,
        window(
            usage.and_then(|usage| usage.weekly.as_ref()),
            QuotaCategory::Weekly,
            "weekly",
            Some(604_800),
            pool,
            received_at,
        )?,
    ]
    .into_iter()
    .flatten()
    {
        decoded.push(window);
    }
    decoded.plan_label = answer
        .subs_tier_name
        .as_deref()
        .map(str::trim)
        .filter(|tier| !tier.is_empty())
        .map(str::to_owned);
    Ok(decoded)
}

fn window(
    reported: Option<&UsageWindow>,
    category: QuotaCategory,
    bucket: &str,
    seconds: Option<i64>,
    pool: &QuotaPoolId,
    received_at: DateTime<Utc>,
) -> Result<Option<QuotaWindow>, ProviderError> {
    let draft = WindowDraft {
        provider: ProviderId::MuseCode,
        pool_id: pool,
        category,
        resource: ACCOUNT_RESOURCE,
        resource_label: "Muse Code",
        bucket_id: Some(bucket),
        metric_role: MetricRole::IncludedAllowance,
        semantics: if category == QuotaCategory::Weekly {
            WindowSemantics::AnchoredPeriod
        } else {
            WindowSemantics::RollingPeriod
        },
        duration_seconds: seconds,
        received_at,
    };
    let Some(used) = reported.and_then(|reported| reported.used_percent.as_ref()) else {
        return Ok(None);
    };
    let boundary = reported
        .and_then(|reported| reported.resets_at.as_ref())
        .and_then(decode::reset_instant);
    let Some(field) = used.field() else {
        return draft
            .invalid(vec![QuotaIssue::NonFiniteValue {
                field: "used_percent".to_owned(),
            }])
            .map(Some);
    };
    match decode::percentage(field.value, field.decimals, "used_percent") {
        Ok(measurement) => draft.build(measurement, boundary, Vec::new()),
        Err(issue) => draft.invalid(vec![issue]),
    }
    .map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decoded(body: serde_json::Value) -> Result<DecodedUsage, ProviderError> {
        let unusable = |_| ProviderError::InvalidData {
            detail: "the test body".to_owned(),
        };
        let answer: SubscriptionAnswer = serde_json::from_value(body).map_err(unusable)?;
        let pool = QuotaPoolId::new("pool-1").map_err(|_| ProviderError::InvalidData {
            detail: "the test pool".to_owned(),
        })?;
        decode(&answer, &pool, Utc::now())
    }

    #[test]
    fn the_rolling_and_weekly_windows_and_the_tier_are_read() {
        let usage = decoded(serde_json::json!({
            "is_subs_active": true, "subs_tier_name": "Muse Pro", "api_key": "LLM|secret",
            "subs_usage": {
                "window": { "used_percent": 12.5, "resets_at": "2026-10-03T18:00:00Z",
                            "window_duration_mins": 300 },
                "weekly": { "used_percent": 40, "resets_at": 1_791_100_000 }
            }
        }))
        .expect("the usage decodes");
        assert_eq!(usage.plan_label.as_deref(), Some("Muse Pro"));
        assert_eq!(usage.windows[0].category, QuotaCategory::Session);
        let left = usage.windows[0]
            .measurement
            .remaining_percent()
            .expect("a percentage");
        assert!((left.value() - 87.5).abs() < f64::EPSILON);
        assert!(usage.windows[1].boundary.is_some());
        assert!(usage.is_complete());
    }

    #[test]
    fn idle_usage_shows_no_window_and_is_still_a_complete_reading() {
        let usage = decoded(serde_json::json!({ "is_subs_active": true })).expect("decodes");
        assert!(usage.is_complete());
        assert_eq!(usage.windows, [] as [QuotaWindow; 0]);
    }

    #[test]
    fn only_the_windows_muse_reports_are_shown() {
        let usage = decoded(serde_json::json!({
            "is_subs_active": true,
            "subs_usage": {
                "window": { "resets_at": "2026-10-03T18:00:00Z", "window_duration_mins": 300 },
                "weekly": { "used_percent": 40, "resets_at": 1_791_100_000 }
            }
        }))
        .expect("decodes");
        assert!(usage.is_complete());
        assert_eq!(usage.windows.len(), 1);
        assert_eq!(usage.windows[0].category, QuotaCategory::Weekly);
    }

    #[test]
    fn an_inactive_subscription_is_refused() {
        let refused = decoded(serde_json::json!({ "is_subs_active": false }));
        assert!(matches!(refused, Err(ProviderError::Authorization)));
    }

    #[test]
    fn the_account_is_the_user_id_otherwise_the_address() {
        let answer = |body| -> SubscriptionAnswer {
            serde_json::from_value(body).expect("the answer parses")
        };
        let both = answer(serde_json::json!({ "user_id": "42", "user_email": "A@x.test" }));
        assert_eq!(both.account_id().as_deref(), Some("42"));
        let address = answer(serde_json::json!({ "user_id": " ", "user_email": " A@x.test " }));
        assert_eq!(address.account_id().as_deref(), Some("a@x.test"));
        assert_eq!(answer(serde_json::json!({})).account_id(), None);
    }
}
