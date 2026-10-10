//! Turn the `TypeSafe` console's billing overview into a prepaid balance.
//!
//! `TypeSafe` sells credit: the console reports the credit left, what was spent
//! in the current cycle, and each credit grant with what is left of it and when
//! it expires. The balance is measured from the sum of the active grants, the
//! ones not yet expired with something left, so a new grant reads as a top-up
//! and an expired one leaves the gauge. With no active grant the balance is
//! measured from the first reading after the account was added, as
//! `OpenRouter`'s is (`quota_domain::balance`). It ranks like an allowance.

use chrono::{DateTime, Utc};
use quota_core::ports::ProviderError;
use quota_domain::balance::{BalanceReading, CreditGrant, CreditKind, CycleSpend};
use quota_domain::ids::QuotaPoolId;
use quota_domain::provider::ProviderId;
use quota_domain::quota::issue::QuotaIssue;
use quota_domain::quota::measurement::Measurement;
use quota_domain::quota::money::MoneyMeasurement;
use quota_domain::quota::units::CurrencyCode;
use quota_domain::quota::window::{
    MetricRole, QuotaCategory, QuotaWindow, SourceKind, WindowSemantics,
};

use crate::decode::{DecodedUsage, Numberish, WindowDraft};
use crate::typesafe::wire::{Billing, Credit};

/// Amounts are shown in cents.
const SCALE: u8 = 2;

/// Cents in one dollar.
const CENTS: f64 = 100.0;

/// Decodes one billing summary.
///
/// # Errors
/// Returns [`ProviderError::UnsupportedSchema`] when the balance is missing,
/// which is the one figure the reading cannot do without.
pub(crate) fn decode(
    billing: &Billing,
    pool: &QuotaPoolId,
    received_at: DateTime<Utc>,
) -> Result<DecodedUsage, ProviderError> {
    let draft = WindowDraft {
        provider: ProviderId::Typesafe,
        pool_id: pool,
        category: QuotaCategory::Custom,
        resource: "credits",
        resource_label: "Credit balance",
        bucket_id: Some("credits"),
        metric_role: MetricRole::PrepaidBalance,
        semantics: WindowSemantics::Unknown,
        duration_seconds: None,
        received_at,
    };
    let mut decoded = DecodedUsage::new();
    decoded.plan_label = billing.plan.as_deref().and_then(plan_label);
    let balance = match cents(billing.balance.as_ref()) {
        Ok(Some(balance)) => balance,
        Ok(None) => {
            return Err(ProviderError::UnsupportedSchema {
                detail: "the TypeSafe billing summary carried no balance".to_owned(),
            });
        }
        Err(()) => {
            decoded.push(source(draft.invalid(vec![QuotaIssue::NonFiniteValue {
                field: "billing.balance".to_owned(),
            }])?));
            return Ok(decoded);
        }
    };
    let credits = active_credits(&billing.credits, received_at);
    let granted = credits
        .iter()
        .fold(0_i64, |sum, grant| sum.saturating_add(grant.amount_minor));
    // With no active grant, or grants that do not account for the balance, the
    // balance itself is what was loaded as far as this reading can tell.
    let loaded = granted.max(balance);
    decoded.push(source(draft.build(
        Measurement::Money(MoneyMeasurement {
            currency: usd()?,
            scale: SCALE,
            used_minor_units: Some(loaded - balance),
            remaining_minor_units: Some(balance),
            limit_minor_units: (loaded > 0).then_some(loaded),
        }),
        None,
        Vec::new(),
    )?));
    decoded.balance = Some(BalanceReading {
        currency: usd()?,
        scale: SCALE,
        loaded_minor: loaded,
        spent_minor: loaded - balance,
        credits,
        cycle_spend: cycle_spend(billing),
    });
    Ok(decoded)
}

/// Marks a window as read from the console's own page, not a published API.
fn source(mut window: QuotaWindow) -> QuotaWindow {
    window.source = SourceKind::ObservedWebEndpoint;
    window
}

/// The grants that still count: not expired, with something left, and with
/// readable amounts and an expiry. Others are left out, never guessed at.
fn active_credits(credits: &[Credit], now: DateTime<Utc>) -> Vec<CreditGrant> {
    credits
        .iter()
        .filter_map(|credit| {
            let amount = cents(credit.amount.as_ref()).ok().flatten()?;
            let remaining = cents(credit.remaining.as_ref()).ok().flatten()?;
            let expires_at = DateTime::parse_from_rfc3339(credit.expires_at.as_deref()?)
                .ok()?
                .with_timezone(&Utc);
            (remaining > 0 && amount > 0 && expires_at > now).then(|| CreditGrant {
                kind: credit_kind(credit.reason.as_deref()),
                amount_minor: amount,
                remaining_minor: remaining.min(amount),
                expires_at,
            })
        })
        .collect()
}

/// What a grant was for, from the console's reason.
fn credit_kind(reason: Option<&str>) -> CreditKind {
    match reason {
        Some(reason) if reason.starts_with("free") => CreditKind::Free,
        Some(reason) if reason.starts_with("purchased") => CreditKind::Purchased,
        _ => CreditKind::Other,
    }
}

/// The current cycle's spend, when both its name and its amount are readable.
fn cycle_spend(billing: &Billing) -> Option<CycleSpend> {
    let label = billing
        .cycle_label
        .as_deref()
        .map(str::trim)
        .filter(|label| !label.is_empty())?;
    let spent = cents(billing.spent.as_ref()).ok().flatten()?;
    Some(CycleSpend {
        label: label.to_owned(),
        spent_minor: spent.max(0),
    })
}

/// The plan's name in words: `free_plan` is `Free`, `pro_plan` is `Pro`, and
/// any other identifier is title-cased with its separators as spaces.
pub(crate) fn plan_label(plan: &str) -> Option<String> {
    let words: Vec<String> = plan
        .split(['_', '-', ' '])
        .filter(|word| !word.is_empty())
        .map(|word| {
            let mut letters = word.chars();
            letters.next().map_or_else(String::new, |first| {
                first
                    .to_uppercase()
                    .chain(letters.flat_map(char::to_lowercase))
                    .collect()
            })
        })
        .collect();
    let trimmed: Vec<&String> = match words.last() {
        Some(last) if last == "Plan" && words.len() > 1 => {
            words.iter().take(words.len() - 1).collect()
        }
        _ => words.iter().collect(),
    };
    let label = trimmed
        .iter()
        .map(|word| word.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    (!label.is_empty()).then_some(label)
}

/// The currency every `TypeSafe` amount is in.
fn usd() -> Result<CurrencyCode, ProviderError> {
    CurrencyCode::new("USD").map_err(|_| ProviderError::InvalidData {
        detail: "the currency code was rejected".to_owned(),
    })
}

/// A dollar amount in whole cents, or `Ok(None)` when it was not reported.
fn cents(reported: Option<&Numberish>) -> Result<Option<i64>, ()> {
    let Some(reported) = reported else {
        return Ok(None);
    };
    let value = reported.field().ok_or(())?.value * CENTS;
    if !value.is_finite() || value.abs() > 1e15 {
        return Err(());
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the value is finite and well inside the i64 range"
    )]
    Ok(Some(value.round() as i64))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn billing(value: serde_json::Value) -> Billing {
        serde_json::from_value(value).expect("a billing summary")
    }

    fn pool() -> QuotaPoolId {
        QuotaPoolId::new("typesafe-pool").expect("a pool")
    }

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-06T12:00:00Z")
            .expect("a time")
            .with_timezone(&Utc)
    }

    #[test]
    fn the_balance_is_measured_from_the_active_grants() {
        let decoded = decode(
            &billing(serde_json::json!({
                "balance": 37.2, "spent": 12.8, "cycleLabel": "October 2026", "plan": "free_plan",
                "credits": [
                    { "amount": 5, "remaining": 0, "expiresAt": "2026-12-01T00:00:00Z", "reason": "free_tier_credit" },
                    { "amount": 40, "remaining": 33, "expiresAt": "2027-10-01T00:00:00Z", "reason": "purchased_credits" },
                    { "amount": 10, "remaining": 4.2, "expiresAt": "2026-10-31T00:00:00Z", "reason": "free_tier_credit" },
                    { "amount": 20, "remaining": 20, "expiresAt": "2026-10-01T00:00:00Z", "reason": "purchased_credits" }
                ]
            })),
            &pool(),
            now(),
        )
        .expect("decodes");
        assert_eq!(decoded.plan_label.as_deref(), Some("Free"));
        let window = &decoded.windows[0];
        assert_eq!(window.metric_role, MetricRole::PrepaidBalance);
        let Measurement::Money(money) = &window.measurement else {
            panic!("a money reading");
        };
        assert_eq!(money.remaining_minor_units, Some(3_720));
        assert_eq!(money.limit_minor_units, Some(5_000));
        let reading = decoded.balance.expect("a balance reading");
        assert_eq!(reading.loaded_minor, 5_000);
        assert_eq!(reading.spent_minor, 1_280);
        assert_eq!(
            reading.credits.len(),
            2,
            "the used-up and expired grants are left out"
        );
        assert_eq!(reading.credits[0].kind, CreditKind::Purchased);
        assert_eq!(reading.credits[1].remaining_minor, 420);
        assert_eq!(
            reading.cycle_spend,
            Some(CycleSpend {
                label: "October 2026".to_owned(),
                spent_minor: 1_280
            })
        );
    }

    #[test]
    fn without_an_active_grant_the_balance_is_all_that_was_loaded() {
        let decoded = decode(
            &billing(serde_json::json!({ "balance": 3.5, "credits": null })),
            &pool(),
            now(),
        )
        .expect("decodes");
        let reading = decoded.balance.expect("a reading");
        assert_eq!((reading.loaded_minor, reading.spent_minor), (350, 0));
        assert!(reading.credits.is_empty());
        assert_eq!(reading.cycle_spend, None);
    }

    #[test]
    fn a_missing_balance_is_an_unsupported_shape_and_a_bad_one_is_invalid() {
        assert!(matches!(
            decode(&billing(serde_json::json!({ "spent": 1 })), &pool(), now()),
            Err(ProviderError::UnsupportedSchema { .. })
        ));
        let decoded = decode(
            &billing(serde_json::json!({ "balance": "lots" })),
            &pool(),
            now(),
        )
        .expect("decodes");
        assert!(decoded.balance.is_none());
        assert!(matches!(
            decoded.windows[0].measurement,
            Measurement::Unavailable(_)
        ));
    }

    #[test]
    fn plan_identifiers_read_as_words() {
        assert_eq!(plan_label("free_plan").as_deref(), Some("Free"));
        assert_eq!(
            plan_label("pay_as_you_go").as_deref(),
            Some("Pay As You Go")
        );
        assert_eq!(plan_label("PRO-plan").as_deref(), Some("Pro"));
        assert_eq!(plan_label("plan").as_deref(), Some("Plan"));
        assert_eq!(plan_label("__").as_deref(), None);
    }
}
