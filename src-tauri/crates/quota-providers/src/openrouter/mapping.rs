//! Turn `OpenRouter`'s key and credit answers into quota windows.
//!
//! `OpenRouter` is pay as you go: nothing is an included allowance. The account
//! has a credit balance, and each API key may carry its own spend limit, which
//! can reset daily, weekly or monthly. The balance is a balance and the key
//! limit is a spend cap, so neither takes part in the least-remaining ranking.
//! A key with no limit is reported as unlimited, never as a zero.

use chrono::{DateTime, Utc};
use quota_core::ports::ProviderError;
use quota_domain::ids::QuotaPoolId;
use quota_domain::provider::ProviderId;
use quota_domain::quota::issue::QuotaIssue;
use quota_domain::quota::measurement::Measurement;
use quota_domain::quota::money::MoneyMeasurement;
use quota_domain::quota::scope::ACCOUNT_RESOURCE;
use quota_domain::quota::units::CurrencyCode;
use quota_domain::quota::window::{
    MetricRole, QuotaCategory, QuotaWindow, SourceKind, WindowSemantics,
};

use crate::decode::{DecodedUsage, Numberish, WindowDraft};
use crate::openrouter::wire::{CreditsData, KeyData};

/// Amounts are shown in cents.
const SCALE: u8 = 2;

/// Cents in one dollar.
const CENTS: f64 = 100.0;

/// Decodes one key answer and, when the key may read it, one credit answer.
pub(crate) fn decode(
    key: &KeyData,
    credits: Option<&CreditsData>,
    pool: &QuotaPoolId,
    received_at: DateTime<Utc>,
) -> Result<DecodedUsage, ProviderError> {
    let mut decoded = DecodedUsage::new();
    if let Some(credits) = credits {
        decoded.push(documented(credit_window(credits, pool, received_at)?));
    }
    decoded.push(documented(key_window(key, pool, received_at)?));
    Ok(decoded)
}

/// Marks a window as read from `OpenRouter`'s published API.
fn documented(mut window: QuotaWindow) -> QuotaWindow {
    window.source = SourceKind::DocumentedApi;
    window
}

/// The account's credit balance: what was bought, spent, and is left.
fn credit_window(
    credits: &CreditsData,
    pool: &QuotaPoolId,
    received_at: DateTime<Utc>,
) -> Result<QuotaWindow, ProviderError> {
    let draft = WindowDraft {
        provider: ProviderId::Openrouter,
        pool_id: pool,
        category: QuotaCategory::Custom,
        resource: "credits",
        resource_label: "Credit balance",
        bucket_id: Some("credits"),
        metric_role: MetricRole::CreditBalance,
        semantics: WindowSemantics::Unknown,
        duration_seconds: None,
        received_at,
    };
    let bought = cents(credits.total_credits.as_ref());
    let spent = cents(credits.total_usage.as_ref());
    match (bought, spent) {
        (Ok(Some(bought)), Ok(Some(spent))) => draft.build(
            money(Some(spent), Some(bought - spent), Some(bought))?,
            None,
            Vec::new(),
        ),
        (Ok(None), _) | (_, Ok(None)) => draft.reported_missing(),
        _ => draft.invalid(vec![QuotaIssue::NonFiniteValue {
            field: "data.total_credits".to_owned(),
        }]),
    }
}

/// The key's own spend limit, in the period it resets on.
fn key_window(
    key: &KeyData,
    pool: &QuotaPoolId,
    received_at: DateTime<Utc>,
) -> Result<QuotaWindow, ProviderError> {
    let (category, semantics) = match key.limit_reset.as_deref() {
        Some("daily") => (QuotaCategory::Daily, WindowSemantics::CalendarCycle),
        Some("weekly") => (QuotaCategory::Weekly, WindowSemantics::CalendarCycle),
        Some("monthly") => (QuotaCategory::Monthly, WindowSemantics::CalendarCycle),
        _ => (QuotaCategory::Custom, WindowSemantics::Unknown),
    };
    let draft = WindowDraft {
        provider: ProviderId::Openrouter,
        pool_id: pool,
        category,
        resource: ACCOUNT_RESOURCE,
        resource_label: "API key limit",
        bucket_id: Some("key_limit"),
        metric_role: MetricRole::ExtraSpendCap,
        semantics,
        duration_seconds: None,
        received_at,
    };
    let Some(limit) = key.limit.as_ref() else {
        // No limit on the key: it can spend the whole balance.
        return draft.build(Measurement::Unlimited, None, Vec::new());
    };
    let limit = cents(Some(limit));
    let remaining = cents(key.limit_remaining.as_ref());
    match (limit, remaining) {
        (Ok(Some(limit)), Ok(remaining)) => draft.build(
            money(
                remaining.map(|remaining| limit - remaining),
                remaining,
                Some(limit),
            )?,
            None,
            Vec::new(),
        ),
        _ => draft.invalid(vec![QuotaIssue::NonFiniteValue {
            field: "data.limit".to_owned(),
        }]),
    }
}

/// A US dollar amount.
fn money(
    used: Option<i64>,
    remaining: Option<i64>,
    limit: Option<i64>,
) -> Result<Measurement, ProviderError> {
    let currency = CurrencyCode::new("USD").map_err(|_| ProviderError::InvalidData {
        detail: "the currency code was rejected".to_owned(),
    })?;
    Ok(Measurement::Money(MoneyMeasurement {
        currency,
        scale: SCALE,
        used_minor_units: used,
        remaining_minor_units: remaining,
        limit_minor_units: limit,
    }))
}

/// A dollar amount in whole cents, or `Ok(None)` when it was not reported.
///
/// `OpenRouter` reports fractions of a cent; they round to the nearest cent,
/// which is the precision the amount is shown at.
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
        reason = "the value is finite and bounded just above, so the rounded cents fit an i64"
    )]
    Ok(Some(value.round() as i64))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn number(value: f64) -> Numberish {
        Numberish::Text(value.to_string())
    }

    fn decoded(key: &KeyData, credits: Option<&CreditsData>) -> DecodedUsage {
        let pool = QuotaPoolId::new("pool-1").expect("a pool id");
        let received_at = DateTime::from_timestamp(1_700_000_000, 0).expect("an instant");
        decode(key, credits, &pool, received_at).expect("the answer decodes")
    }

    fn money_of(window: &QuotaWindow) -> &MoneyMeasurement {
        let Measurement::Money(money) = &window.measurement else {
            panic!("expected a money measurement");
        };
        money
    }

    #[test]
    fn the_credit_balance_is_what_was_bought_less_what_was_spent() {
        let credits = CreditsData {
            total_credits: Some(number(25.0)),
            total_usage: Some(number(7.456)),
        };
        let usage = decoded(&KeyData::default(), Some(&credits));
        let balance = &usage.windows[0];
        assert_eq!(balance.metric_role, MetricRole::CreditBalance);
        assert_eq!(balance.source, SourceKind::DocumentedApi);
        let money = money_of(balance);
        assert_eq!(money.limit_minor_units, Some(2500));
        assert_eq!(money.used_minor_units, Some(746));
        assert_eq!(money.remaining_minor_units, Some(1754));
    }

    #[test]
    fn a_key_without_a_limit_is_unlimited_never_zero() {
        let usage = decoded(&KeyData::default(), None);
        assert_eq!(usage.windows.len(), 1);
        assert_eq!(usage.windows[0].measurement, Measurement::Unlimited);
        assert_eq!(usage.windows[0].metric_role, MetricRole::ExtraSpendCap);
    }

    #[test]
    fn a_key_limit_resets_on_its_own_period() {
        let key = KeyData {
            limit: Some(number(10.0)),
            limit_remaining: Some(number(2.5)),
            limit_reset: Some("weekly".to_owned()),
            ..KeyData::default()
        };
        let usage = decoded(&key, None);
        let window = &usage.windows[0];
        assert_eq!(window.category, QuotaCategory::Weekly);
        let money = money_of(window);
        assert_eq!(money.remaining_minor_units, Some(250));
        assert_eq!(money.used_minor_units, Some(750));
    }

    #[test]
    fn an_unusable_amount_is_reported_as_invalid_not_as_zero() {
        let credits = CreditsData {
            total_credits: Some(Numberish::Text("lots".to_owned())),
            total_usage: Some(number(1.0)),
        };
        let usage = decoded(&KeyData::default(), Some(&credits));
        assert!(matches!(
            usage.windows[0].measurement,
            Measurement::Unavailable(_)
        ));
    }
}
