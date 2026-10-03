//! Non-transactional presentation preferences.
//!
//! These live in the typed store repository. They are deliberately separate
//! from monitoring and alert state, which belongs to `SQLite`.

use serde::{Deserialize, Serialize};
use specta::Type;

/// The store document schema version this build writes.
pub const PREFERENCES_SCHEMA_VERSION: u32 = 1;

/// The colour scheme.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    /// Follow the operating system.
    #[default]
    System,
    /// Always light.
    Light,
    /// Always dark.
    Dark,
}

/// How a remaining allowance is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum IndicatorStyle {
    /// A small draining ring.
    #[default]
    Ring,
    /// A horizontal bar. The same values, a different indicator.
    Bar,
}

/// Where the overview lives.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum OverviewMode {
    /// A freely movable window that stays open.
    #[default]
    Floating,
    /// A window anchored beside the tray.
    Tray,
}

/// What happens at login.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum LaunchBehavior {
    /// Start in the tray without opening a window.
    #[default]
    QuietInTray,
    /// Restore the last chosen mode.
    RestoreLastMode,
}

/// How account identities appear in the overview.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyAliasMode {
    /// Show the verified nickname and workspace.
    #[default]
    Off,
    /// Replace each account with a stable, distinguishable alias.
    StableAliases,
}

/// Default thresholds, expressed as remaining percentage.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct NotificationThresholds {
    /// Remaining percentage at which the allowance is called low.
    pub low_percent: f64,
    /// Remaining percentage at which the allowance is called critical.
    pub critical_percent: f64,
    /// How far past a threshold a reading must recover before it re-arms.
    pub hysteresis_percent: f64,
    /// Which of the three alerts are switched on.
    #[serde(default)]
    pub alerts: NotificationAlerts,
}

/// Which of the three allowance alerts are switched on.
///
/// Each alert is its own choice: the approved design offers 20%, 10%, and 0% as
/// three separate selections, so one shared percentage cannot stand in for
/// them, and a deselected alert is never written as a null percentage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct NotificationAlerts {
    /// Notify when an allowance reaches the low threshold.
    pub low: bool,
    /// Notify when an allowance reaches the critical threshold.
    pub critical: bool,
    /// Notify when an allowance is exhausted.
    pub exhausted: bool,
}

impl Default for NotificationAlerts {
    fn default() -> Self {
        Self {
            low: true,
            critical: true,
            exhausted: true,
        }
    }
}

impl Default for NotificationThresholds {
    fn default() -> Self {
        Self {
            low_percent: 20.0,
            critical_percent: 10.0,
            hysteresis_percent: 3.0,
            alerts: NotificationAlerts::default(),
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

impl Default for NotificationPolicy {
    fn default() -> Self {
        Self {
            enabled: true,
            thresholds: NotificationThresholds::default(),
            quiet_hours: QuietHours::Never,
            recovery_enabled: true,
        }
    }
}

/// Operational privacy choices stored with monitoring settings.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct OperationalPrivacyPreferences {
    /// Whether normalized local history is retained.
    pub retain_history: bool,
    /// Whether diagnostic export includes account labels.
    pub export_identities: bool,
}

impl Default for OperationalPrivacyPreferences {
    fn default() -> Self {
        Self {
            retain_history: true,
            export_identities: false,
        }
    }
}

/// Settings owned by `SQLite` because they affect monitoring and retained data.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, Type)]
pub struct OperationalPreferences {
    /// Revision shared with the presentation document.
    pub revision: u32,
    /// Notification policy.
    pub notifications: NotificationPolicy,
    /// History retention and diagnostic export choices.
    pub privacy: OperationalPrivacyPreferences,
    /// Effective per-provider polling policies.
    pub polling: Vec<crate::polling::ProviderPollingPolicy>,
}

/// A named, non-transactional presentation preference.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct PresentationPreferences {
    /// The document schema version.
    pub schema_version: u32,
    /// Monotonic within one stored document.
    pub revision: u32,
    /// The colour scheme.
    pub theme: Theme,
    /// The allowance indicator.
    pub indicator_style: IndicatorStyle,
    /// Where the overview lives.
    pub overview_mode: OverviewMode,
    /// Whether the overview floats above other applications.
    ///
    /// Defaults to off. An upgraded preference file must not carry an implicit
    /// opt-in, so this field is only ever set by an explicit user action.
    pub always_on_top: bool,
    /// What happens at login.
    pub launch_behavior: LaunchBehavior,
    /// How identities appear.
    pub privacy_alias_mode: PrivacyAliasMode,
    /// Whether animations are suppressed.
    pub reduce_motion: bool,
    /// Whether the mini widget is on screen.
    ///
    /// Off by default, and a preference file written before the widget existed
    /// reads as off, so only an explicit choice ever shows it.
    #[serde(default)]
    pub show_widget: bool,
}

impl Default for PresentationPreferences {
    fn default() -> Self {
        Self {
            schema_version: PREFERENCES_SCHEMA_VERSION,
            revision: 0,
            theme: Theme::default(),
            indicator_style: IndicatorStyle::default(),
            overview_mode: OverviewMode::default(),
            always_on_top: false,
            launch_behavior: LaunchBehavior::default(),
            privacy_alias_mode: PrivacyAliasMode::default(),
            reduce_motion: false,
            show_widget: false,
        }
    }
}

impl PresentationPreferences {
    /// Bumps the revision and stamps the current schema version.
    pub fn advance(&mut self) -> u32 {
        self.schema_version = PREFERENCES_SCHEMA_VERSION;
        self.revision = self.revision.saturating_add(1);
        self.revision
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn always_on_top_defaults_off_and_survives_a_round_trip() {
        let defaults = PresentationPreferences::default();
        assert!(!defaults.always_on_top);
        let json = serde_json::to_string(&defaults).unwrap();
        let restored: PresentationPreferences = serde_json::from_str(&json).unwrap();
        assert!(!restored.always_on_top);
        assert_eq!(restored, defaults);
    }

    #[test]
    fn a_file_from_before_the_widget_reads_with_the_widget_off() {
        let mut json = serde_json::to_value(PresentationPreferences::default()).unwrap();
        json.as_object_mut().unwrap().remove("show_widget");
        let restored: PresentationPreferences = serde_json::from_value(json).unwrap();
        assert!(!restored.show_widget);
    }

    #[test]
    fn revision_advances_monotonically() {
        let mut value = PresentationPreferences::default();
        assert_eq!(value.advance(), 1);
        assert_eq!(value.advance(), 2);
    }
}
