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
//!
//! A provider that lists its credit grants, such as `TypeSafe`, reports the
//! amount loaded as the sum of its active grants instead. The balance is then
//! measured from that sum, the grants are kept for the detail view, and a new
//! grant is still recorded as a top-up.

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
    /// The active credit grants, for a provider that lists them. When any are
    /// listed, the balance is measured from their sum.
    pub credits: Vec<CreditGrant>,
    /// What the account spent in its current billing cycle, when reported.
    pub cycle_spend: Option<CycleSpend>,
}

/// One credit grant: how much it was, how much is left, and when it lapses.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct CreditGrant {
    /// What the grant was for.
    pub kind: CreditKind,
    /// How much it granted, in minor units.
    #[specta(type = f64)]
    pub amount_minor: i64,
    /// How much of it is left, in minor units.
    #[specta(type = f64)]
    pub remaining_minor: i64,
    /// When it expires.
    pub expires_at: DateTime<Utc>,
}

/// What a credit grant was for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CreditKind {
    /// Credit the provider gives for free.
    Free,
    /// Credit the person bought.
    Purchased,
    /// Any other grant, such as a promotion.
    Other,
}

/// What the account spent in its current billing cycle.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct CycleSpend {
    /// The cycle, as the provider names it, for example `October 2026`.
    pub label: String,
    /// Spent in it, in minor units.
    #[specta(type = f64)]
    pub spent_minor: i64,
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
    /// The sum of the active credit grants the provider lists.
    Credits,
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
    /// The active credit grants, at the last reading.
    #[serde(default)]
    pub credits: Vec<CreditGrant>,
    /// What the account spent in its current cycle, at the last reading.
    #[serde(default)]
    pub cycle_spend: Option<CycleSpend>,
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
    /// The active credit grants, soonest to expire first.
    pub credits: Vec<CreditGrant>,
    /// What the account spent in its current cycle, when reported.
    pub cycle_spend: Option<CycleSpend>,
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
                credits: Vec::new(),
                cycle_spend: None,
            }
            .with_grants(reading, now);
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
        next.with_grants(reading, now)
    }

    /// Keeps the reading's credit grants and cycle spend and, when it lists any
    /// grants, measures the balance from their sum.
    fn with_grants(mut self, reading: &BalanceReading, now: DateTime<Utc>) -> Self {
        let mut credits = reading.credits.clone();
        credits.sort_by_key(|grant| grant.expires_at);
        self.cycle_spend.clone_from(&reading.cycle_spend);
        if !credits.is_empty() {
            let granted = credits
                .iter()
                .fold(0_i64, |sum, grant| sum.saturating_add(grant.amount_minor));
            if self.baseline_kind != BaselineKind::Credits || self.baseline_minor != granted {
                self.baseline_at = now;
            }
            self.baseline_minor = granted.max(self.balance_minor());
            self.baseline_kind = BaselineKind::Credits;
        }
        self.credits = credits;
        self
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
            credits: self.credits.clone(),
            cycle_spend: self.cycle_spend.clone(),
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
#[path = "balance_tests.rs"]
mod tests;
