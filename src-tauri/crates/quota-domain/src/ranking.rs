//! The least-remaining-first account order.
//!
//! The order is relative depletion of the lowest known *included* allowance. It
//! is not a forecast: nothing here predicts when an allowance empties, and the
//! unrounded percentage is what every comparison uses.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::account::ConnectionState;
use crate::ids::{AccountId, QuotaWindowId};
use crate::percent::Percent;
use crate::quota::measurement::Measurement;
use crate::quota::window::{MetricRole, QuotaWindow};

/// The version of the ranking rule, recorded with every result.
pub const ORDER_RULE_VERSION: u16 = 1;

/// Why an account has no comparable numeric rank.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum UnrankedReason {
    /// The reading is past its freshness deadline.
    Stale,
    /// An applicable window was expected and is missing.
    Incomplete,
    /// The account needs a new authorization.
    ReconnectRequired,
    /// A reported boundary has passed and no fresh reading has arrived.
    ResetPending,
    /// The account only reports native units with no denominator.
    NativeUnitsOnly,
    /// Every applicable window is unlimited.
    UnlimitedOnly,
    /// The account is not monitored.
    Disabled,
    /// Monitoring is paused application-wide.
    MonitoringPaused,
    /// No included allowance applies to this account.
    NoIncludedAllowance,
}

impl UnrankedReason {
    /// Whether this account belongs in the "Needs checking" section rather than
    /// in the numeric ranking.
    #[must_use]
    pub const fn needs_checking(self) -> bool {
        !matches!(self, Self::Disabled | Self::MonitoringPaused)
    }
}

/// The outcome of ranking one account.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum AccountOrder {
    /// A comparable remaining percentage exists.
    Ranked(RankedOrder),
    /// No comparable percentage exists, and the reason is explicit.
    Unranked(UnrankedOrder),
}

impl AccountOrder {
    /// The unrounded order value, when this account is ranked.
    #[must_use]
    pub const fn order_value(&self) -> Option<Percent> {
        match self {
            Self::Ranked(value) => Some(value.remaining_percent),
            Self::Unranked(_) => None,
        }
    }
}

/// A comparable rank plus the window that produced it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct RankedOrder {
    /// The lowest known included remaining percentage, unrounded.
    pub remaining_percent: Percent,
    /// The window that controlled the value.
    pub controlling_window_id: QuotaWindowId,
    /// That window's scope, so the UI can explain the position.
    pub scope_label: String,
    /// The version of the rule that produced this result.
    pub rule_version: u16,
}

/// An explicit absence of rank.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct UnrankedOrder {
    /// Why there is no comparable value.
    pub reason: UnrankedReason,
    /// The version of the rule that produced this result.
    pub rule_version: u16,
}

/// The narrow view of an account that ranking needs.
#[derive(Clone, Copy, Debug)]
pub struct RankingInput<'a> {
    /// The account being ranked.
    pub account_id: &'a AccountId,
    /// Stable tie-break order, assigned when the connection was created.
    pub connection_ordinal: u32,
    /// The account's windows.
    pub windows: &'a [QuotaWindow],
    /// Whether the account is monitored at all.
    pub monitoring_enabled: bool,
    /// Whether monitoring is paused application-wide.
    pub monitoring_paused: bool,
    /// Where the connection stands.
    pub connection_state: ConnectionState,
    /// The instant used to evaluate freshness and boundaries.
    pub now: DateTime<Utc>,
}

/// The lowest comparable remaining percentage among an account's windows.
fn controlling_window(windows: &[QuotaWindow], now: DateTime<Utc>) -> Option<&QuotaWindow> {
    windows
        .iter()
        .filter(|window| {
            window.metric_role.is_rankable()
                && !window.boundary_has_passed(now)
                && !window.is_stale_at(now)
        })
        .filter(|window| match window.measurement {
            Measurement::Percentage(_) => true,
            // A prepaid balance is money measured from its last top-up.
            Measurement::Money(_) => window.metric_role == MetricRole::PrepaidBalance,
            _ => false,
        })
        .min_by(|a, b| {
            let a_value = a.measurement.remaining_percent();
            let b_value = b.measurement.remaining_percent();
            a_value
                .partial_cmp(&b_value)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

/// Ranks one account, or states why it has no comparable value.
#[must_use]
pub fn rank_account(input: &RankingInput<'_>) -> AccountOrder {
    let unranked = |reason: UnrankedReason| {
        AccountOrder::Unranked(UnrankedOrder {
            reason,
            rule_version: ORDER_RULE_VERSION,
        })
    };

    if !input.monitoring_enabled {
        return unranked(UnrankedReason::Disabled);
    }
    if input.monitoring_paused {
        return unranked(UnrankedReason::MonitoringPaused);
    }
    if matches!(
        input.connection_state,
        ConnectionState::ReauthenticationRequired | ConnectionState::Disconnected
    ) {
        return unranked(UnrankedReason::ReconnectRequired);
    }

    if let Some(window) = controlling_window(input.windows, input.now) {
        let Some(remaining) = window.measurement.remaining_percent() else {
            return unranked(UnrankedReason::Incomplete);
        };
        return AccountOrder::Ranked(RankedOrder {
            remaining_percent: remaining,
            controlling_window_id: window.id.clone(),
            scope_label: window.scope.label().to_owned(),
            rule_version: ORDER_RULE_VERSION,
        });
    }

    unranked(absence_reason(input.windows, input.now))
}

/// The explicit reason a window set yields no comparable percentage.
fn absence_reason(windows: &[QuotaWindow], now: DateTime<Utc>) -> UnrankedReason {
    let applicable: Vec<_> = windows
        .iter()
        .filter(|w| w.metric_role.is_rankable())
        .collect();

    if applicable.is_empty() {
        return UnrankedReason::NoIncludedAllowance;
    }
    if applicable.iter().any(|w| w.boundary_has_passed(now)) {
        return UnrankedReason::ResetPending;
    }
    if applicable.iter().any(|w| w.is_stale_at(now)) {
        return UnrankedReason::Stale;
    }
    if applicable.iter().any(|w| !w.measurement.has_number()) {
        return UnrankedReason::Incomplete;
    }
    if applicable
        .iter()
        .all(|w| matches!(w.measurement, Measurement::Unlimited))
    {
        return UnrankedReason::UnlimitedOnly;
    }
    UnrankedReason::NativeUnitsOnly
}

/// Which presentation section an account belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OrderSection {
    /// Accounts that need a connection, reading, or boundary check.
    NeedsChecking,
    /// Accounts with a comparable remaining percentage.
    Ranked,
    /// Accounts the user switched off.
    MonitoringOff,
}

/// One account's position in the canonical order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct OrderEntry {
    /// The account.
    pub account_id: AccountId,
    /// Its section.
    pub section: OrderSection,
    /// Its rank, when ranked.
    pub order: AccountOrder,
    /// Stable tie-break order.
    pub connection_ordinal: u32,
}

/// Computes the canonical order for a whole set of accounts.
///
/// Ties break by connection ordinal and then by account identity, so a rename
/// never reshuffles a row.
#[must_use]
pub fn compute_order(inputs: &[RankingInput<'_>]) -> Vec<OrderEntry> {
    let mut entries: Vec<OrderEntry> = inputs
        .iter()
        .map(|input| {
            let order = rank_account(input);
            let section = match &order {
                AccountOrder::Ranked(_) => OrderSection::Ranked,
                AccountOrder::Unranked(value) => {
                    if value.reason.needs_checking() {
                        OrderSection::NeedsChecking
                    } else {
                        OrderSection::MonitoringOff
                    }
                }
            };
            OrderEntry {
                account_id: input.account_id.clone(),
                section,
                order,
                connection_ordinal: input.connection_ordinal,
            }
        })
        .collect();

    entries.sort_by(|a, b| {
        a.section
            .cmp(&b.section)
            .then_with(|| match (&a.order, &b.order) {
                (AccountOrder::Ranked(x), AccountOrder::Ranked(y)) => x
                    .remaining_percent
                    .value()
                    .partial_cmp(&y.remaining_percent.value())
                    .unwrap_or(std::cmp::Ordering::Equal),
                _ => std::cmp::Ordering::Equal,
            })
            .then_with(|| a.connection_ordinal.cmp(&b.connection_ordinal))
            .then_with(|| a.account_id.cmp(&b.account_id))
    });
    entries
}
