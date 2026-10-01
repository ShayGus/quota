//! The compiled-in provider registry.
//!
//! An adapter exists only when this crate was compiled with it. A caller that
//! asks for a provider with no compiled adapter receives `None`, and the
//! application shows an explicit unsupported-provider state. There is no dynamic
//! lookup, no plugin loading, and no remote parser: a provider schema change is
//! answered by a new build, not by downloaded code.

use std::sync::Arc;

use quota_core::ports::{ProviderAdapter, ProviderError};
use quota_domain::provider::ProviderId;

use crate::claude::ClaudeAdapter;
use crate::codex::CodexAdapter;
use crate::opencode_go::OpenCodeGoAdapter;

/// The adapters this build contains.
#[derive(Debug)]
pub struct ProviderRegistry {
    adapters: Vec<Arc<dyn ProviderAdapter>>,
}

impl ProviderRegistry {
    /// Builds the production registry: Codex, Claude, and `OpenCode` Go.
    ///
    /// # Errors
    /// Returns a transient failure when an adapter's HTTP client cannot be built.
    pub fn production() -> Result<Self, ProviderError> {
        Ok(Self {
            adapters: vec![
                Arc::new(CodexAdapter::new()?),
                Arc::new(ClaudeAdapter::new()?),
                Arc::new(OpenCodeGoAdapter::new()?),
            ],
        })
    }

    /// Builds the production registry plus the deterministic local fixture.
    ///
    /// The fixture is a development and test aid. It exists only when this crate
    /// is compiled with the `test-fixtures` feature, which release builds leave
    /// off.
    ///
    /// # Errors
    /// Returns a transient failure when an adapter's HTTP client cannot be built.
    #[cfg(feature = "test-fixtures")]
    pub fn with_fixture() -> Result<Self, ProviderError> {
        let mut registry = Self::production()?;
        registry
            .adapters
            .push(Arc::new(crate::fixture::FixtureAdapter::new()));
        Ok(registry)
    }

    /// The adapter for one provider, when this build contains it.
    #[must_use]
    pub fn provider(&self, id: ProviderId) -> Option<&Arc<dyn ProviderAdapter>> {
        self.adapters
            .iter()
            .find(|adapter| adapter.provider_id() == id)
    }

    /// Every provider this build can read, in a stable order.
    #[must_use]
    pub fn registered(&self) -> Vec<ProviderId> {
        self.adapters
            .iter()
            .map(|adapter| adapter.provider_id())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> ProviderRegistry {
        #[cfg(feature = "test-fixtures")]
        {
            ProviderRegistry::with_fixture().expect("the registry builds")
        }
        #[cfg(not(feature = "test-fixtures"))]
        {
            ProviderRegistry::production().expect("the registry builds")
        }
    }

    #[test]
    fn every_real_provider_has_exactly_one_compiled_adapter() {
        let registry = registry();
        for id in [
            ProviderId::Codex,
            ProviderId::Claude,
            ProviderId::OpenCodeGo,
        ] {
            let adapter = registry.provider(id).expect("a compiled adapter exists");
            assert_eq!(adapter.provider_id(), id);
        }
        assert_eq!(
            registry
                .registered()
                .iter()
                .filter(|id| **id == ProviderId::Codex)
                .count(),
            1
        );
    }

    #[cfg(feature = "test-fixtures")]
    #[test]
    fn the_fixture_adapter_is_offered_when_the_feature_is_on() {
        let registry = ProviderRegistry::with_fixture().expect("the registry builds");
        assert!(registry.provider(ProviderId::Fixture).is_some());
    }
}
