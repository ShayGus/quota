//! A prepaid balance, measured from its last top-up.
//!
//! A pay-as-you-go provider such as `OpenRouter` reports only what was ever
//! loaded and what was ever spent. A share of everything ever loaded stops
//! meaning anything after a few top-ups, so Quota keeps a small ledger for each
//! account and measures the balance from the last top-up instead.
//!
//! Spending never raises the amount loaded; only a top-up does. A rise between
//! two readings is therefore a top-up of exactly that much, and nothing has to
//! be guessed from the balance. A fall is a refund or a correction, and the
//! balance is measured from that moment. Before Quota has seen a top-up, the
//! balance is measured from the first reading after the account was added.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::quota::measurement::Measurement;
use crate::quota::money::MoneyMeasurement;
use crate::quota::units::CurrencyCode;
use crate::quota::window::{MetricRole, QuotaWindow};

/// How many top-ups the ledger keeps, newest first.
pub const TOP_UP_HISTORY: usize = 20;

/// How far back the spending pace is measured.
pub const RUNWAY_PERIOD_DAYS: i64 = 7;

/// The least history a spending pace is measured over; a shorter one is noise.
pub const RUNWAY_MINIMUM_HOURS: i64 = 24;

/// The least time between two kept spending samples.
const SAMPLE_SPACING_MINUTES: i64 = 60;

/// What a provider reported about a prepaid balance, in minor units.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BalanceReading {
    /// The currency of every amount.
    pub currency: CurrencyCode,
    /// Decimal places in one major unit, for example 2 for cents.
    pub scale: u8,
    /// Everything ever loaded.
    pub loaded_minor: i64,
    /// Everything ever spent.
    pub spent_minor: i64,
    /// What the key spent in the current calendar periods, when reported.
    pub key_spend: Option<PeriodSpend>,
}

/// What a key spent today, this week and this month, in minor units.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct PeriodSpend {
    /// Spent in the current UTC day.
    #[specta(type = Option<f64>)]
    pub today_minor: Option<i64>,
    /// Spent in the current UTC week.
    #[specta(type = Option<f64>)]
    pub week_minor: Option<i64>,
    /// Spent in the current UTC month.
    #[specta(type = Option<f64>)]
    pub month_minor: Option<i64>,
}

/// What the balance is measured from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum BaselineKind {
    /// The first reading after the account was added; no top-up seen yet.
    SinceAdded,
    /// The last top-up.
    TopUp,
    /// A refund or a correction lowered the amount loaded.
    Adjusted,
}

/// One top-up, as Quota saw it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct TopUp {
    /// The reading that first showed it.
    pub detected_at: DateTime<Utc>,
    /// How much was loaded.
    #[specta(type = f64)]
    pub amount_minor: i64,
    /// The balance right after it: the balance before, plus the amount.
    #[specta(type = f64)]
    pub balance_after_minor: i64,
}

/// What had been spent at one moment, for the spending pace.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpendSample {
    /// When it was read.
    pub at: DateTime<Utc>,
    /// Everything ever spent, at that moment.
    pub spent_minor: i64,
}

/// The durable state of one account's prepaid balance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BalanceLedger {
    /// The currency of every amount.
    pub currency: CurrencyCode,
    /// Decimal places in one major unit.
    pub scale: u8,
    /// Everything ever loaded, at the last reading.
    pub loaded_minor: i64,
    /// Everything ever spent, at the last reading.
    pub spent_minor: i64,
    /// The balance the gauge is measured from.
    pub baseline_minor: i64,
    /// When the baseline was set.
    pub baseline_at: DateTime<Utc>,
    /// What the baseline is.
    pub baseline_kind: BaselineKind,
    /// The top-ups seen, newest first.
    pub top_ups: Vec<TopUp>,
    /// Spending samples over the pace period, oldest first.
    pub samples: Vec<SpendSample>,
    /// What the key spent in the current periods, at the last reading.
    pub key_spend: Option<PeriodSpend>,
}

/// The spending pace, and how long the balance lasts at it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct Runway {
    /// Spent per day, on average, over the pace period.
    #[specta(type = f64)]
    pub spend_per_day_minor: i64,
    /// Whole days the balance lasts at that pace.
    pub days_left: u32,
}

/// What the renderer shows about one account's prepaid balance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct BalanceSummary {
    /// The currency of every amount.
    pub currency: CurrencyCode,
    /// Decimal places in one major unit.
    pub scale: u8,
    /// The balance now.
    #[specta(type = f64)]
    pub balance_minor: i64,
    /// The balance the gauge is measured from.
    #[specta(type = f64)]
    pub baseline_minor: i64,
    /// When the baseline was set.
    pub baseline_at: DateTime<Utc>,
    /// What the baseline is.
    pub baseline_kind: BaselineKind,
    /// Everything ever loaded.
    #[specta(type = f64)]
    pub loaded_minor: i64,
    /// Everything ever spent.
    #[specta(type = f64)]
    pub spent_minor: i64,
    /// The top-ups seen, newest first.
    pub top_ups: Vec<TopUp>,
    /// The spending pace, when there is enough history and some spending.
    pub runway: Option<Runway>,
    /// What the key spent in the current periods, when reported.
    pub key_spend: Option<PeriodSpend>,
}

impl BalanceLedger {
    /// The balance now.
    #[must_use]
    pub const fn balance_minor(&self) -> i64 {
        self.loaded_minor.saturating_sub(self.spent_minor)
    }

    /// The ledger after one reading.
    ///
    /// A rise in the amount loaded is a top-up of exactly that much, and the
    /// balance right after it is the balance before plus the amount. A fall
    /// measures the balance from now. A new currency starts a new ledger.
    #[must_use]
    pub fn record(previous: Option<&Self>, reading: &BalanceReading, now: DateTime<Utc>) -> Self {
        let balance = reading.loaded_minor.saturating_sub(reading.spent_minor);
        let Some(previous) = previous.filter(|previous| {
            previous.currency == reading.currency && previous.scale == reading.scale
        }) else {
            return Self {
                currency: reading.currency.clone(),
                scale: reading.scale,
                loaded_minor: reading.loaded_minor,
                spent_minor: reading.spent_minor,
                baseline_minor: balance,
                baseline_at: now,
                baseline_kind: BaselineKind::SinceAdded,
                top_ups: Vec::new(),
                samples: vec![SpendSample {
                    at: now,
                    spent_minor: reading.spent_minor,
                }],
                key_spend: reading.key_spend,
            };
        };
        let mut next = previous.clone();
        next.loaded_minor = reading.loaded_minor;
        next.spent_minor = reading.spent_minor;
        next.key_spend = reading.key_spend;
        if reading.loaded_minor > previous.loaded_minor {
            let amount = reading.loaded_minor - previous.loaded_minor;
            let balance_after = previous.balance_minor().saturating_add(amount);
            next.top_ups.insert(
                0,
                TopUp {
                    detected_at: now,
                    amount_minor: amount,
                    balance_after_minor: balance_after,
                },
            );
            next.top_ups.truncate(TOP_UP_HISTORY);
            next.baseline_minor = balance_after;
            next.baseline_at = now;
            next.baseline_kind = BaselineKind::TopUp;
        } else if reading.loaded_minor < previous.loaded_minor {
            next.baseline_minor = balance;
            next.baseline_at = now;
            next.baseline_kind = BaselineKind::Adjusted;
        }
        // A refunded charge can lift the balance over its baseline; the gauge
        // never shows more than full.
        next.baseline_minor = next.baseline_minor.max(balance);
        next.samples = next_samples(&previous.samples, reading.spent_minor, now);
        next
    }

    /// The gauge's reading: what is left of the balance it is measured from.
    #[must_use]
    pub fn measurement(&self) -> Measurement {
        let balance = self.balance_minor();
        Measurement::Money(MoneyMeasurement {
            currency: self.currency.clone(),
            scale: self.scale,
            used_minor_units: Some(self.baseline_minor.saturating_sub(balance).max(0)),
            remaining_minor_units: Some(balance),
            limit_minor_units: (self.baseline_minor > 0).then_some(self.baseline_minor),
        })
    }

    /// Measures every prepaid-balance window from the baseline.
    pub fn apply_to(&self, windows: &mut [QuotaWindow]) {
        for window in windows
            .iter_mut()
            .filter(|window| window.metric_role == MetricRole::PrepaidBalance)
        {
            window.measurement = self.measurement();
        }
    }

    /// The spending pace over the last week, when there is a day of history and
    /// some spending in it.
    #[must_use]
    pub fn runway(&self, now: DateTime<Utc>) -> Option<Runway> {
        let since = now - Duration::days(RUNWAY_PERIOD_DAYS);
        let first = self.samples.iter().find(|sample| sample.at >= since)?;
        let span = (now - first.at).num_seconds();
        if span < Duration::hours(RUNWAY_MINIMUM_HOURS).num_seconds() {
            return None;
        }
        let spent = self.spent_minor.saturating_sub(first.spent_minor);
        if spent <= 0 {
            return None;
        }
        let per_day = i64::try_from(
            i128::from(spent) * i128::from(Duration::days(1).num_seconds()) / i128::from(span),
        )
        .ok()?
        .max(1);
        let balance = self.balance_minor().max(0);
        let days_left = u32::try_from(balance / per_day).unwrap_or(u32::MAX);
        Some(Runway {
            spend_per_day_minor: per_day,
            days_left,
        })
    }

    /// What the renderer shows.
    #[must_use]
    pub fn summary(&self, now: DateTime<Utc>) -> BalanceSummary {
        BalanceSummary {
            currency: self.currency.clone(),
            scale: self.scale,
            balance_minor: self.balance_minor(),
            baseline_minor: self.baseline_minor,
            baseline_at: self.baseline_at,
            baseline_kind: self.baseline_kind,
            loaded_minor: self.loaded_minor,
            spent_minor: self.spent_minor,
            top_ups: self.top_ups.clone(),
            runway: self.runway(now),
            key_spend: self.key_spend,
        }
    }
}

/// The samples after one reading: at most one an hour, within the pace period
/// and one sample before it, so a full period always has a starting point. A
/// fall in spending, which only a correction causes, starts them again.
fn next_samples(previous: &[SpendSample], spent: i64, now: DateTime<Utc>) -> Vec<SpendSample> {
    let mut samples = previous.to_vec();
    if samples.last().is_some_and(|last| spent < last.spent_minor) {
        samples.clear();
    }
    let due = samples
        .last()
        .is_none_or(|last| now - last.at >= Duration::minutes(SAMPLE_SPACING_MINUTES));
    if due {
        samples.push(SpendSample {
            at: now,
            spent_minor: spent,
        });
    }
    let since = now - Duration::days(RUNWAY_PERIOD_DAYS);
    let keep_from = samples
        .iter()
        .rposition(|sample| sample.at < since)
        .unwrap_or(0);
    samples.drain(..keep_from);
    samples
}

#[cfg(test)]
mod tests {
    use super::*;

    fn usd() -> CurrencyCode {
        CurrencyCode::new("USD").expect("a currency")
    }

    fn reading(loaded: i64, spent: i64) -> BalanceReading {
        BalanceReading {
            currency: usd(),
            scale: 2,
            loaded_minor: loaded,
            spent_minor: spent,
            key_spend: None,
        }
    }

    fn at(hours: i64) -> DateTime<Utc> {
        DateTime::<Utc>::from_timestamp(1_790_000_000, 0).expect("an instant")
            + Duration::hours(hours)
    }

    fn left(ledger: &BalanceLedger) -> (Option<i64>, Option<i64>) {
        match ledger.measurement() {
            Measurement::Money(money) => (money.remaining_minor_units, money.limit_minor_units),
            other => panic!("not money: {other:?}"),
        }
    }

    #[test]
    fn before_a_top_up_the_balance_is_measured_from_the_first_reading() {
        let first = BalanceLedger::record(None, &reading(10_000, 6_000), at(0));
        assert_eq!(first.baseline_kind, BaselineKind::SinceAdded);
        assert_eq!(left(&first), (Some(4_000), Some(4_000)));
        let later = BalanceLedger::record(Some(&first), &reading(10_000, 7_000), at(1));
        assert_eq!(later.baseline_kind, BaselineKind::SinceAdded);
        assert_eq!(left(&later), (Some(3_000), Some(4_000)));
        assert!(later.top_ups.is_empty());
    }

    #[test]
    fn a_rise_in_the_amount_loaded_is_a_top_up_of_exactly_that_much() {
        let first = BalanceLedger::record(None, &reading(10_000, 9_000), at(0));
        // $50 loaded, and $2 spent since the last reading.
        let topped = BalanceLedger::record(Some(&first), &reading(15_000, 9_200), at(2));
        assert_eq!(topped.baseline_kind, BaselineKind::TopUp);
        assert_eq!(topped.baseline_at, at(2));
        assert_eq!(
            topped.top_ups,
            vec![TopUp {
                detected_at: at(2),
                amount_minor: 5_000,
                balance_after_minor: 6_000,
            }]
        );
        assert_eq!(left(&topped), (Some(5_800), Some(6_000)));
    }

    #[test]
    fn a_fall_in_the_amount_loaded_measures_the_balance_from_now() {
        let first = BalanceLedger::record(None, &reading(10_000, 2_000), at(0));
        let refunded = BalanceLedger::record(Some(&first), &reading(9_000, 2_500), at(1));
        assert_eq!(refunded.baseline_kind, BaselineKind::Adjusted);
        assert_eq!(left(&refunded), (Some(6_500), Some(6_500)));
        assert!(refunded.top_ups.is_empty());
    }

    #[test]
    fn a_refunded_charge_never_shows_more_than_full() {
        let first = BalanceLedger::record(None, &reading(10_000, 4_000), at(0));
        let lower = BalanceLedger::record(Some(&first), &reading(10_000, 5_000), at(1));
        let refunded = BalanceLedger::record(Some(&lower), &reading(10_000, 3_000), at(2));
        assert_eq!(left(&refunded), (Some(7_000), Some(7_000)));
    }

    #[test]
    fn an_empty_baseline_has_no_share() {
        let empty = BalanceLedger::record(None, &reading(500, 500), at(0));
        assert_eq!(left(&empty), (Some(0), None));
        assert!(empty.measurement().remaining_percent().is_none());
    }

    #[test]
    fn the_top_up_history_is_bounded() {
        let mut ledger = BalanceLedger::record(None, &reading(0, 0), at(0));
        for step in 1..=30 {
            ledger = BalanceLedger::record(Some(&ledger), &reading(step * 100, 0), at(step));
        }
        assert_eq!(ledger.top_ups.len(), TOP_UP_HISTORY);
        assert_eq!(ledger.top_ups[0].amount_minor, 100);
        assert_eq!(ledger.top_ups[0].detected_at, at(30));
    }

    #[test]
    fn the_runway_needs_a_day_of_history_and_some_spending() {
        let first = BalanceLedger::record(None, &reading(10_000, 0), at(0));
        let hours_later = BalanceLedger::record(Some(&first), &reading(10_000, 300), at(6));
        assert_eq!(hours_later.runway(at(6)), None);
        let idle = BalanceLedger::record(Some(&first), &reading(10_000, 0), at(48));
        assert_eq!(idle.runway(at(48)), None);
        // $6 spent over two days is $3 a day, and $94 lasts 31 more days.
        let spending = BalanceLedger::record(Some(&first), &reading(10_000, 600), at(48));
        assert_eq!(
            spending.runway(at(48)),
            Some(Runway {
                spend_per_day_minor: 300,
                days_left: 31,
            })
        );
    }

    #[test]
    fn the_pace_is_measured_over_the_last_week_only() {
        let mut ledger = BalanceLedger::record(None, &reading(100_000, 0), at(0));
        // A big week, then a quiet one at $1 a day.
        ledger = BalanceLedger::record(Some(&ledger), &reading(100_000, 50_000), at(24 * 7));
        for day in 8..=14 {
            ledger = BalanceLedger::record(
                Some(&ledger),
                &reading(100_000, 50_000 + (day - 7) * 100),
                at(24 * day),
            );
        }
        let runway = ledger.runway(at(24 * 14)).expect("a pace");
        assert_eq!(runway.spend_per_day_minor, 100);
        assert!(ledger.samples.len() <= 9, "older samples are dropped");
    }

    #[test]
    fn samples_are_kept_at_most_hourly() {
        let mut ledger = BalanceLedger::record(None, &reading(10_000, 0), at(0));
        for minutes in [5, 10, 30, 59] {
            ledger = BalanceLedger::record(
                Some(&ledger),
                &reading(10_000, minutes),
                at(0) + Duration::minutes(minutes),
            );
        }
        assert_eq!(ledger.samples.len(), 1);
        ledger = BalanceLedger::record(Some(&ledger), &reading(10_000, 70), at(1));
        assert_eq!(ledger.samples.len(), 2);
    }

    #[test]
    fn a_new_currency_starts_a_new_ledger() {
        let first = BalanceLedger::record(None, &reading(10_000, 0), at(0));
        let mut euros = reading(20_000, 0);
        euros.currency = CurrencyCode::new("EUR").expect("a currency");
        let next = BalanceLedger::record(Some(&first), &euros, at(1));
        assert_eq!(next.baseline_kind, BaselineKind::SinceAdded);
        assert!(next.top_ups.is_empty());
    }

    #[test]
    fn the_summary_carries_the_balance_and_lifetime_amounts() {
        let ledger = BalanceLedger::record(None, &reading(10_000, 2_500), at(0));
        let summary = ledger.summary(at(0));
        assert_eq!(summary.balance_minor, 7_500);
        assert_eq!(summary.loaded_minor, 10_000);
        assert_eq!(summary.runway, None);
    }
}
