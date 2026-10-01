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

/// How much room a row takes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum Density {
    /// More rows, smaller text blocks.
    #[default]
    Compact,
    /// Larger touch targets and more spacing.
    Comfortable,
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

/// A named, non-transactional presentation preference.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct PresentationPreferences {
    /// The document schema version.
    pub schema_version: u32,
    /// Monotonic within one stored document.
    pub revision: u64,
    /// The colour scheme.
    pub theme: Theme,
    /// Row density.
    pub density: Density,
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
}

impl Default for PresentationPreferences {
    fn default() -> Self {
        Self {
            schema_version: PREFERENCES_SCHEMA_VERSION,
            revision: 0,
            theme: Theme::default(),
            density: Density::default(),
            indicator_style: IndicatorStyle::default(),
            overview_mode: OverviewMode::default(),
            always_on_top: false,
            launch_behavior: LaunchBehavior::default(),
            privacy_alias_mode: PrivacyAliasMode::default(),
            reduce_motion: false,
        }
    }
}

impl PresentationPreferences {
    /// Bumps the revision and stamps the current schema version.
    pub fn advance(&mut self) -> u64 {
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
    fn revision_advances_monotonically() {
        let mut value = PresentationPreferences::default();
        assert_eq!(value.advance(), 1);
        assert_eq!(value.advance(), 2);
    }
}
