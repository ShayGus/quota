//! The one decision that says whether this build may look for an update.
//!
//! Only the installed release may. A development build, a debug build, the
//! development identity, the sample-data build behind the real-app suite, and a
//! build with agent inspection are all kept out, so no test or developer run
//! can contact the release endpoint or replace itself with a published build.
//! Nothing else decides this: [`super::start`] calls
//! [`may_check_for_updates`] and starts nothing when it says no.

/// The application identifier of the installed release, as `tauri.conf.json`
/// declares it. The development overlay in `tauri.dev.conf.json` replaces it.
pub(crate) const RELEASE_IDENTIFIER: &str = "app.quota.monitor";

/// What the decision reads. Every field is a fact about the build, never a
/// setting, so no environment variable or stored preference reaches it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BuildFacts<'a> {
    /// `cfg!(debug_assertions)`: a build made without `--release`.
    pub debug_assertions: bool,
    /// The `sample-data` feature, which the real-app suite builds.
    pub sample_data: bool,
    /// The `agent-inspection` feature.
    pub agent_inspection: bool,
    /// The identifier the running application resolved.
    pub identifier: &'a str,
}

impl<'a> BuildFacts<'a> {
    /// The facts of the running build.
    pub(crate) const fn current(identifier: &'a str) -> Self {
        Self {
            debug_assertions: cfg!(debug_assertions),
            sample_data: cfg!(feature = "sample-data"),
            agent_inspection: cfg!(feature = "agent-inspection"),
            identifier,
        }
    }
}

/// Why a build does not check for updates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Exclusion {
    /// A debug build, which is every development and test build.
    DebugBuild,
    /// The sample-data build: the real-app suite and the local sample.
    SampleData,
    /// A build that carries development-only agent inspection.
    AgentInspection,
    /// The development identity, or any identity other than the release's.
    NotTheReleaseIdentity,
}

impl Exclusion {
    /// A short reason for the log.
    pub(crate) const fn reason(self) -> &'static str {
        match self {
            Self::DebugBuild => "a debug build",
            Self::SampleData => "the sample-data build",
            Self::AgentInspection => "a build with agent inspection",
            Self::NotTheReleaseIdentity => "not the release identity",
        }
    }
}

/// The answer to "may this build look for an update?".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UpdateDecision {
    /// This is the installed release: check, and offer what is found.
    Check,
    /// This build never checks, for the stated reason.
    Skip(Exclusion),
}

/// Whether this build may look for an update, and if not, why not.
pub(crate) fn may_check_for_updates(facts: &BuildFacts<'_>) -> UpdateDecision {
    if facts.debug_assertions {
        return UpdateDecision::Skip(Exclusion::DebugBuild);
    }
    if facts.sample_data {
        return UpdateDecision::Skip(Exclusion::SampleData);
    }
    if facts.agent_inspection {
        return UpdateDecision::Skip(Exclusion::AgentInspection);
    }
    if facts.identifier != RELEASE_IDENTIFIER {
        return UpdateDecision::Skip(Exclusion::NotTheReleaseIdentity);
    }
    UpdateDecision::Check
}

#[cfg(test)]
mod tests {
    use super::*;

    const RELEASE: BuildFacts<'static> = BuildFacts {
        debug_assertions: false,
        sample_data: false,
        agent_inspection: false,
        identifier: RELEASE_IDENTIFIER,
    };

    #[test]
    fn the_installed_release_checks() {
        assert_eq!(may_check_for_updates(&RELEASE), UpdateDecision::Check);
    }

    #[test]
    fn a_debug_build_never_checks() {
        let facts = BuildFacts {
            debug_assertions: true,
            ..RELEASE
        };
        assert_eq!(
            may_check_for_updates(&facts),
            UpdateDecision::Skip(Exclusion::DebugBuild)
        );
    }

    #[test]
    fn the_sample_data_and_real_app_builds_never_check() {
        let facts = BuildFacts {
            sample_data: true,
            ..RELEASE
        };
        assert_eq!(
            may_check_for_updates(&facts),
            UpdateDecision::Skip(Exclusion::SampleData)
        );
    }

    #[test]
    fn a_build_with_agent_inspection_never_checks() {
        let facts = BuildFacts {
            agent_inspection: true,
            ..RELEASE
        };
        assert_eq!(
            may_check_for_updates(&facts),
            UpdateDecision::Skip(Exclusion::AgentInspection)
        );
    }

    #[test]
    fn the_development_identity_never_checks() {
        let facts = BuildFacts {
            identifier: "app.quota.monitor.dev",
            ..RELEASE
        };
        assert_eq!(
            may_check_for_updates(&facts),
            UpdateDecision::Skip(Exclusion::NotTheReleaseIdentity)
        );
    }

    #[test]
    fn a_release_build_with_the_development_identity_never_checks() {
        // The overlay can be selected on a release-profile build; the identity
        // alone keeps it out.
        let facts = BuildFacts {
            identifier: "app.quota.monitor.dev",
            ..RELEASE
        };
        assert!(!facts.debug_assertions);
        assert_ne!(may_check_for_updates(&facts), UpdateDecision::Check);
    }

    #[test]
    fn every_exclusion_has_its_own_reason() {
        let reasons = [
            Exclusion::DebugBuild,
            Exclusion::SampleData,
            Exclusion::AgentInspection,
            Exclusion::NotTheReleaseIdentity,
        ]
        .map(Exclusion::reason);
        for (index, reason) in reasons.iter().enumerate() {
            assert!(!reason.is_empty());
            assert!(!reasons[..index].contains(reason));
        }
    }

    #[test]
    fn the_release_identifier_is_the_one_in_the_tauri_configuration() {
        let configuration: serde_json::Value =
            serde_json::from_str(include_str!("../../tauri.conf.json")).expect("configuration");
        assert_eq!(configuration["identifier"], RELEASE_IDENTIFIER);
    }

    #[test]
    fn the_running_test_build_is_excluded() {
        // `cargo test` builds with debug assertions, so the facts of this very
        // process must say no, whatever identity it runs under.
        let facts = BuildFacts::current(RELEASE_IDENTIFIER);
        assert_ne!(may_check_for_updates(&facts), UpdateDecision::Check);
    }
}
