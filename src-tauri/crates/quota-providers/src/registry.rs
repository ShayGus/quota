//! The compiled-in provider registry.
//!
//! An adapter exists only when this crate was compiled with it. A caller that
//! asks for a provider with no compiled adapter receives `None`, and the
//! application shows an explicit unsupported-provider state. There is no dynamic
//! lookup, no plugin loading, and no remote parser: a provider schema change is
//! answered by a new build, not by downloaded code.

use std::sync::Arc;

use quota_core::ports::{ProviderAdapter, ProviderError, SecretStore};
use quota_domain::provider::ProviderId;

use crate::claude::ClaudeAdapter;
use crate::codex::CodexAdapter;
use crate::grok::GrokAdapter;
use crate::kimi::KimiAdapter;
use crate::minimax::MinimaxAdapter;
use crate::muse::MuseAdapter;
use crate::opencode_go::OpenCodeGoAdapter;
use crate::openrouter::OpenRouterAdapter;
use crate::zai::ZaiAdapter;

/// The adapters this build contains.
#[derive(Debug)]
pub struct ProviderRegistry {
    adapters: Vec<Arc<dyn ProviderAdapter>>,
    secrets: Arc<dyn SecretStore>,
}

impl ProviderRegistry {
    /// Builds the production registry: Codex, Claude, `OpenCode` Go, and the
    /// providers signed in with a pasted key (`OpenRouter`, Z.ai, `MiniMax`,
    /// Kimi) or through the browser (Grok, Muse Code), whose credentials live
    /// in `secrets`.
    ///
    /// # Errors
    /// Returns a transient failure when an adapter's HTTP client cannot be built.
    pub fn production(secrets: Arc<dyn SecretStore>) -> Result<Self, ProviderError> {
        Ok(Self {
            adapters: vec![
                Arc::new(CodexAdapter::new()?),
                Arc::new(ClaudeAdapter::new()?),
                Arc::new(OpenCodeGoAdapter::new()?),
                Arc::new(OpenRouterAdapter::new(Arc::clone(&secrets))?),
                Arc::new(ZaiAdapter::new(Arc::clone(&secrets))?),
                Arc::new(MinimaxAdapter::new(Arc::clone(&secrets))?),
                Arc::new(KimiAdapter::new(Arc::clone(&secrets))?),
                Arc::new(GrokAdapter::new(Arc::clone(&secrets))?),
                Arc::new(MuseAdapter::new(Arc::clone(&secrets))?),
            ],
            secrets,
        })
    }

    /// Where the credentials Quota owns itself are kept.
    #[must_use]
    pub fn secrets(&self) -> &Arc<dyn SecretStore> {
        &self.secrets
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
    pub fn with_fixture(secrets: Arc<dyn SecretStore>) -> Result<Self, ProviderError> {
        let mut registry = Self::production(secrets)?;
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
            ProviderRegistry::with_fixture(crate::secrets::unavailable())
                .expect("the registry builds")
        }
        #[cfg(not(feature = "test-fixtures"))]
        {
            ProviderRegistry::production(crate::secrets::unavailable())
                .expect("the registry builds")
        }
    }

    #[test]
    fn every_real_provider_has_exactly_one_compiled_adapter() {
        let registry = registry();
        for id in [
            ProviderId::Codex,
            ProviderId::Claude,
            ProviderId::OpenCodeGo,
            ProviderId::Openrouter,
            ProviderId::Zai,
            ProviderId::Minimax,
            ProviderId::Kimi,
            ProviderId::Grok,
            ProviderId::MuseCode,
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
        let registry = ProviderRegistry::with_fixture(crate::secrets::unavailable())
            .expect("the registry builds");
        assert!(registry.provider(ProviderId::Fixture).is_some());
    }
}
