//! Alert evaluation.
//!
//! Pure functions: given the last accepted readings, decide what should notify.
//! Nothing here sends anything; the host does.

use std::collections::HashMap;

use chrono::{DateTime, Utc};

use quota_domain::ids::{AccountId, DefinitionVersion, QuotaPoolId, QuotaWindowId};
use quota_domain::percent::Percent;
use quota_domain::quota::window::QuotaWindow;

use crate::ports::AlertLevel;

/// Builds a percentage from a finite literal, falling back to zero.
fn percent(value: f64) -> Percent {
    match Percent::new(value) {
        Ok(percent) => percent,
        Err(_) => Percent::ZERO,
    }
}

/// When the user wants to be told about remaining allowance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AlertThresholds {
    /// Remaining percentage at which the allowance is called low.
    pub low: Percent,
    /// Remaining percentage at which the allowance is called critical.
    pub critical: Percent,
    /// Remaining percentage at which the allowance is called exhausted.
    pub exhausted: Percent,
    /// How far past a threshold a reading must recover before it re-arms.
    pub hysteresis: Percent,
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self {
            low: percent(20.0),
            critical: percent(10.0),
            exhausted: percent(0.0),
            hysteresis: percent(3.0),
        }
    }
}

/// The rank of a remaining percentage, where higher means more severe.
fn severity_rank(remaining: Percent, thresholds: &AlertThresholds) -> u8 {
    let value = remaining.value();
    match value {
        v if v <= thresholds.exhausted.value() => 3,
        v if v <= thresholds.critical.value() => 2,
        v if v <= thresholds.low.value() => 1,
        _ => 0,
    }
}

fn rank_to_level(rank: u8) -> Option<AlertLevel> {
    match rank {
        1 => Some(AlertLevel::Low),
        2 => Some(AlertLevel::Critical),
        3 => Some(AlertLevel::Exhausted),
        _ => None,
    }
}

/// The highest level a remaining percentage has crossed downward.
#[must_use]
pub fn level_for(remaining: Percent, thresholds: &AlertThresholds) -> Option<AlertLevel> {
    rank_to_level(severity_rank(remaining, thresholds))
}

/// Whether a reading crossed a level the previous reading had not reached.
///
/// The first observation of an account establishes a baseline instead of
/// firing, so connecting an account that already sits at 8% does not produce a
/// burst of notifications.
#[must_use]
pub fn is_new_crossing(
    previous: Option<Percent>,
    current: Percent,
    thresholds: &AlertThresholds,
) -> Option<AlertLevel> {
    let before_rank = severity_rank(previous?, thresholds);
    let now_rank = severity_rank(current, thresholds);
    if now_rank > before_rank {
        rank_to_level(now_rank)
    } else {
        None
    }
}

/// Whether a recovered reading re-arms an episode.
///
/// Recovery needs a fresh comparable observation beyond the threshold plus the
/// hysteresis margin. A clock event alone never recovers anything.
#[must_use]
pub fn has_recovered(current: Percent, level: AlertLevel, thresholds: &AlertThresholds) -> bool {
    let arming_above = match level {
        AlertLevel::Low => thresholds.low.value(),
        AlertLevel::Critical => thresholds.critical.value(),
        AlertLevel::Exhausted => thresholds.exhausted.value(),
    } + thresholds.hysteresis.value();
    current.value() > arming_above
}

/// The identity of one alerting window, independent of which row shows it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AlertKey {
    /// The quota pool, so known aliases of a shared pool share one episode.
    pub pool_id: QuotaPoolId,
    /// The window.
    pub window_id: QuotaWindowId,
    /// The definition version, so a changed definition starts a new episode.
    pub definition_version: DefinitionVersion,
}

/// One newly crossed threshold worth telling the user about.
#[derive(Clone, Debug, PartialEq)]
pub struct Crossing {
    /// The account row to name.
    pub account_id: AccountId,
    /// The window that crossed.
    pub window_id: QuotaWindowId,
    /// The scope label, so a model-specific limit is named as such.
    pub scope_label: String,
    /// The level crossed.
    pub level: AlertLevel,
    /// The unrounded remaining value at the crossing.
    pub remaining_percent: Percent,
    /// When the crossing happened.
    pub at: DateTime<Utc>,
    /// The episode key, so aliases of one shared pool do not double-notify.
    pub key: AlertKey,
}

/// Previous remaining percentages, keyed by window identity.
#[derive(Clone, Debug, Default)]
pub struct PreviousReadings(HashMap<QuotaWindowId, Percent>);

impl PreviousReadings {
    /// Builds a lookup from a previous set of windows.
    #[must_use]
    pub fn from_windows(windows: &[QuotaWindow]) -> Self {
        Self(
            windows
                .iter()
                .filter_map(|window| {
                    window
                        .measurement
                        .remaining_percent()
                        .map(|value| (window.id.clone(), value))
                })
                .collect(),
        )
    }

    /// The previous value for one window.
    #[must_use]
    pub fn get(&self, window_id: &QuotaWindowId) -> Option<Percent> {
        self.0.get(window_id).copied()
    }
}

/// Builds the crossing notifications for one accepted reading.
///
/// Only finite included allowances take part. An extra-spend cap, a credit
/// balance, an unlimited window, and an unknown reading never alert.
#[must_use]
pub fn evaluate(
    account_id: &AccountId,
    windows: &[QuotaWindow],
    previous: &PreviousReadings,
    thresholds: &AlertThresholds,
    now: DateTime<Utc>,
) -> Vec<Crossing> {
    windows
        .iter()
        .filter(|window| is_alertable(window))
        .filter_map(|window| {
            let current = window.measurement.remaining_percent()?;
            let level = is_new_crossing(previous.get(&window.id), current, thresholds)?;
            Some(Crossing {
                account_id: account_id.clone(),
                window_id: window.id.clone(),
                scope_label: window.scope.label().to_owned(),
                level,
                remaining_percent: current,
                at: now,
                key: AlertKey {
                    pool_id: window.pool_id.clone(),
                    window_id: window.id.clone(),
                    definition_version: window.definition_version,
                },
            })
        })
        .collect()
}

/// Whether a window is eligible to alert at all.
#[must_use]
pub fn is_alertable(window: &QuotaWindow) -> bool {
    window.metric_role.is_rankable() && window.measurement.has_number()
}

#[cfg(test)]
mod tests {
    use quota_domain::ids::ResourceId;
    use quota_domain::quota::measurement::{Measurement, PercentageMeasurement, UnavailableReason};
    use quota_domain::quota::scope::QuotaScope;
    use quota_domain::quota::units::DecimalPrecision;
    use quota_domain::quota::window::{
        Completeness, Enforcement, MetricRole, QuotaCategory, SourceKind, WindowSemantics,
    };

    use super::*;

    fn fixture_window(id: &str, remaining: Option<f64>, role: MetricRole) -> QuotaWindow {
        let measurement = match remaining {
            Some(number) => Measurement::Percentage(
                PercentageMeasurement::from_used_percent(
                    100.0 - number,
                    DecimalPrecision::new(0).unwrap(),
                )
                .expect("a finite fixture is always constructible"),
            ),
            None => Measurement::Unavailable(UnavailableReason::NotReported),
        };
        QuotaWindow {
            id: QuotaWindowId::new(id).unwrap(),
            provider_bucket_id: None,
            pool_id: QuotaPoolId::new("pool").unwrap(),
            scope: QuotaScope::new(ResourceId::new("model_x").unwrap(), "Weekly").unwrap(),
            category: QuotaCategory::Weekly,
            semantics: WindowSemantics::Unknown,
            duration: None,
            metric_role: role,
            enforcement: Enforcement::Unknown,
            measurement,
            period_started_at: None,
            boundary: None,
            observed_at: None,
            received_at: DateTime::UNIX_EPOCH,
            valid_until: None,
            source: SourceKind::DocumentedApi,
            completeness: Completeness::Complete,
            definition_version: DefinitionVersion::INITIAL,
            issues: Vec::new(),
        }
    }

    fn previous(pairs: &[(&str, f64)]) -> PreviousReadings {
        let windows: Vec<QuotaWindow> = pairs
            .iter()
            .map(|(id, number)| fixture_window(id, Some(*number), MetricRole::IncludedAllowance))
            .collect();
        PreviousReadings::from_windows(&windows)
    }

    #[test]
    fn a_first_observation_at_eight_percent_establishes_a_baseline() {
        let thresholds = AlertThresholds::default();
        assert_eq!(is_new_crossing(None, percent(8.0), &thresholds), None);
    }

    #[test]
    fn a_downward_crossing_fires_once_and_only_once() {
        let thresholds = AlertThresholds::default();
        assert_eq!(
            is_new_crossing(Some(percent(30.0)), percent(18.0), &thresholds),
            Some(AlertLevel::Low)
        );
        assert_eq!(
            is_new_crossing(Some(percent(18.0)), percent(17.0), &thresholds),
            None
        );
    }

    #[test]
    fn a_drop_straight_to_zero_reports_only_the_severest_level() {
        let thresholds = AlertThresholds::default();
        assert_eq!(
            is_new_crossing(Some(percent(50.0)), percent(0.0), &thresholds),
            Some(AlertLevel::Exhausted)
        );
    }

    #[test]
    fn fluctuation_around_a_threshold_needs_hysteresis_to_recover() {
        let thresholds = AlertThresholds::default();
        assert!(!has_recovered(percent(22.0), AlertLevel::Low, &thresholds));
        assert!(has_recovered(percent(24.0), AlertLevel::Low, &thresholds));
    }

    #[test]
    fn only_finite_included_allowances_alert() {
        assert!(is_alertable(&fixture_window(
            "a",
            Some(40.0),
            MetricRole::IncludedAllowance
        )));
        assert!(!is_alertable(&fixture_window(
            "a",
            Some(1.0),
            MetricRole::ExtraSpendCap
        )));
        assert!(!is_alertable(&fixture_window(
            "a",
            None,
            MetricRole::IncludedAllowance
        )));
        // A prepaid balance measured from its last top-up runs out the same way.
        assert!(is_alertable(&fixture_window(
            "a",
            Some(15.0),
            MetricRole::PrepaidBalance
        )));
        assert!(!is_alertable(&fixture_window(
            "a",
            Some(15.0),
            MetricRole::CreditBalance
        )));
    }

    #[test]
    fn evaluation_names_the_window_and_its_scope() {
        let thresholds = AlertThresholds::default();
        let account = AccountId::new("acct").unwrap();
        let crossings = evaluate(
            &account,
            &[fixture_window(
                "w",
                Some(4.0),
                MetricRole::IncludedAllowance,
            )],
            &previous(&[("w", 30.0)]),
            &thresholds,
            DateTime::UNIX_EPOCH,
        );
        assert_eq!(crossings.len(), 1);
        assert_eq!(crossings[0].level, AlertLevel::Critical);
        assert_eq!(crossings[0].scope_label, "Weekly");
        assert_eq!(crossings[0].account_id, account);
    }

    #[test]
    fn a_first_reading_never_fires_a_notification() {
        let thresholds = AlertThresholds::default();
        let account = AccountId::new("acct").unwrap();
        let crossings = evaluate(
            &account,
            &[fixture_window(
                "w",
                Some(2.0),
                MetricRole::IncludedAllowance,
            )],
            &PreviousReadings::default(),
            &thresholds,
            DateTime::UNIX_EPOCH,
        );
        assert!(crossings.is_empty());
    }

    #[test]
    fn an_extra_spend_cap_crossing_never_alerts() {
        let thresholds = AlertThresholds::default();
        let account = AccountId::new("acct").unwrap();
        let crossings = evaluate(
            &account,
            &[fixture_window("cap", Some(0.0), MetricRole::ExtraSpendCap)],
            &previous(&[("cap", 30.0)]),
            &thresholds,
            DateTime::UNIX_EPOCH,
        );
        assert!(crossings.is_empty());
    }
}
