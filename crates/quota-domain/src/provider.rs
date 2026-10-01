//! The closed provider vocabulary and per-adapter declared capabilities.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::account::AccountCardinality;

/// The providers this build knows about.
///
/// The vocabulary is closed on purpose: an unknown provider name in imported or
/// newer persisted data produces an explicit unsupported state, never an
/// arbitrary adapter lookup.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Type,
)]
#[serde(rename_all = "snake_case")]
pub enum ProviderId {
    /// `OpenAI` Codex, through its documented `app-server` rate-limit interface.
    Codex,
    /// Claude subscription usage.
    Claude,
    /// `OpenCode` `Go`, the provider reached through a local `OpenCode` Go agent.
    OpenCodeGo,
    /// A deterministic local provider used only by tests and developer runs.
    ///
    /// The adapter for this identifier is compiled only under the non-default
    /// `test-fixtures` Cargo feature, so a release build contains no code that
    /// can serve it.
    Fixture,
}

impl ProviderId {
    /// Every identifier, in a stable order.
    pub const ALL: [Self; 4] = [Self::Codex, Self::Claude, Self::OpenCodeGo, Self::Fixture];

    /// Parses a persisted provider name.
    ///
    /// # Errors
    /// Returns [`UnsupportedProvider`] for any name outside the closed set.
    pub fn parse(name: &str) -> Result<Self, UnsupportedProvider> {
        Self::ALL
            .into_iter()
            .find(|id| id.as_str() == name)
            .ok_or(UnsupportedProvider(name.into()))
    }

    /// The stable wire name of this provider.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
            Self::OpenCodeGo => "opencode_go",
            Self::Fixture => "fixture",
        }
    }

    /// Whether this identifier names a compiled test adapter rather than a real service.
    #[must_use]
    pub const fn is_test_only(self) -> bool {
        matches!(self, Self::Fixture)
    }
}

/// A provider name outside the closed vocabulary.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("unsupported provider: {0}")]
pub struct UnsupportedProvider(pub String);

/// What an adapter declares it can do, before any account exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ProviderCapabilities {
    /// The provider this capability set belongs to.
    pub provider_id: ProviderId,
    /// How many independent accounts can be monitored at once.
    pub cardinality: AccountCardinality,
    /// Whether Quota can obtain its own authorization.
    pub supports_app_owned_authorization: bool,
    /// Whether Quota can read an externally owned client profile.
    pub supports_external_profile: bool,
    /// Whether the provider reports a monthly allowance.
    pub reports_monthly_window: bool,
    /// The shortest interval this adapter permits between reads.
    pub minimum_interval_seconds: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_identifier_round_trips_through_its_wire_name() {
        for id in ProviderId::ALL {
            assert_eq!(ProviderId::parse(id.as_str()), Ok(id));
        }
    }

    #[test]
    fn rejects_a_name_outside_the_closed_set() {
        assert_eq!(
            ProviderId::parse("gemini"),
            Err(UnsupportedProvider("gemini".into()))
        );
    }

    #[test]
    fn the_test_identifier_is_marked_as_such() {
        assert!(ProviderId::Fixture.is_test_only());
        assert!(!ProviderId::Codex.is_test_only());
    }
}
