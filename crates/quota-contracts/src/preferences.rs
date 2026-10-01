//! The preference aggregate handed to the renderer.
//!
//! This is assembled from the typed store and `SQLite` owners after both saves
//! succeed. It is not an instruction to write the same object into every store.

use serde::{Deserialize, Serialize};
use specta::Type;

use quota_domain::polling::ProviderPollingPolicy;
use quota_domain::preferences::{
    Density, IndicatorStyle, LaunchBehavior, OverviewMode, PrivacyAliasMode, Theme,
};

/// The wire schema version of the preference aggregate.
pub const PREFERENCES_SCHEMA_VERSION: u32 = 1;

/// Default thresholds, expressed as remaining percentage.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct NotificationThresholds {
    /// Remaining percentage at which the allowance is called low.
    pub low_percent: f64,
    /// Remaining percentage at which the allowance is called critical.
    pub critical_percent: f64,
    /// How far past a threshold a reading must recover before it re-arms.
    pub hysteresis_percent: f64,
}

impl Default for NotificationThresholds {
    fn default() -> Self {
        Self {
            low_percent: 20.0,
            critical_percent: 10.0,
            hysteresis_percent: 3.0,
        }
    }
}

/// When notifications may be shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case", tag = "kind", content = "window")]
pub enum QuietHours {
    /// Notifications are always permitted.
    Never,
    /// Notifications are suppressed inside a daily UTC window.
    DailyUtc {
        /// Minutes from midnight when quiet hours start.
        from_minute: u16,
        /// Minutes from midnight when quiet hours end.
        to_minute: u16,
    },
}

/// What the user wants to be told.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct NotificationPolicy {
    /// Whether notifications are enabled at all.
    pub enabled: bool,
    /// The downward thresholds.
    pub thresholds: NotificationThresholds,
    /// When notifications are suppressed.
    pub quiet_hours: QuietHours,
    /// Whether recovery notifications are sent separately.
    pub recovery_enabled: bool,
}

/// How much local detail is retained and exported.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct PrivacyPolicy {
    /// Whether account identities are replaced by stable aliases.
    pub alias_mode: PrivacyAliasMode,
    /// Whether normalized local history is retained.
    pub retain_history: bool,
    /// Whether diagnostic export includes account labels.
    pub export_identities: bool,
}

/// A window mode change requested by the renderer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct WindowModeChangeRequest {
    /// The mode to move to.
    pub mode: OverviewMode,
    /// The preference revision the caller believes is current.
    pub expected_revision: u64,
}

/// The confirmed preference aggregate.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct Preferences {
    /// The wire schema version.
    pub schema_version: u32,
    /// Monotonic within one installation.
    pub revision: u64,
    /// The colour scheme.
    pub theme: Theme,
    /// Row density.
    pub density: Density,
    /// The allowance indicator.
    pub indicator_style: IndicatorStyle,
    /// Where the overview lives.
    pub overview_mode: OverviewMode,
    /// Whether the overview floats above other applications. Off by default.
    pub always_on_top: bool,
    /// What happens at login.
    pub launch_behavior: LaunchBehavior,
    /// Whether animations are suppressed.
    pub reduce_motion: bool,
    /// Notification behaviour.
    pub notifications: NotificationPolicy,
    /// Retention and export behaviour.
    pub privacy: PrivacyPolicy,
    /// The effective per-provider polling policy.
    pub polling: Vec<ProviderPollingPolicy>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_thresholds_match_the_documented_policy() {
        let value = NotificationThresholds::default();
        assert!((value.low_percent - 20.0).abs() < f64::EPSILON);
        assert!((value.critical_percent - 10.0).abs() < f64::EPSILON);
        assert!((value.hysteresis_percent - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn quiet_hours_is_a_tagged_union() {
        let json = serde_json::to_string(&QuietHours::Never).unwrap();
        assert_eq!(json, "{\"kind\":\"never\"}");
        let parsed: QuietHours = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, QuietHours::Never);
    }
}
