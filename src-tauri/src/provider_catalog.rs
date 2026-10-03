//! What each provider declares before any account exists, and whether this
//! build contains its adapter.

use quota_domain::account::AccountCardinality;
use quota_domain::provider::{ProviderCapabilities, ProviderId};

/// Declared capability data for one provider identifier.
///
/// A declaration does not claim that a live account was verified.
#[must_use]
pub fn capabilities_of(provider_id: ProviderId) -> ProviderCapabilities {
    match provider_id {
        ProviderId::Codex | ProviderId::Claude | ProviderId::OpenCodeGo => {
            ProviderCapabilities {
                provider_id,
                cardinality: AccountCardinality::SingleProfile,
                supports_app_owned_authorization: false,
                supports_external_profile: true,
                // A plan whose only allowance covers a month reports one
                // window. Claude's account-wide allowances never do.
                reports_monthly_window: !matches!(provider_id, ProviderId::Claude),
                minimum_interval_seconds: 300,
            }
        }
        ProviderId::Openrouter => ProviderCapabilities {
            provider_id,
            // Each pasted key is its own connection.
            cardinality: AccountCardinality::Independent,
            supports_app_owned_authorization: true,
            supports_external_profile: false,
            reports_monthly_window: true,
            minimum_interval_seconds: 300,
        },
        // Signed in with a pasted key; Kimi can also read its CLI's sign-in.
        ProviderId::Zai | ProviderId::Minimax | ProviderId::Kimi => ProviderCapabilities {
            provider_id,
            cardinality: AccountCardinality::Independent,
            supports_app_owned_authorization: true,
            supports_external_profile: matches!(provider_id, ProviderId::Kimi),
            reports_monthly_window: !matches!(provider_id, ProviderId::Minimax),
            minimum_interval_seconds: 300,
        },
        // Signed in through the browser, or with the provider's own CLI.
        ProviderId::Grok | ProviderId::MuseCode => ProviderCapabilities {
            provider_id,
            cardinality: AccountCardinality::Independent,
            supports_app_owned_authorization: true,
            supports_external_profile: true,
            reports_monthly_window: matches!(provider_id, ProviderId::Grok),
            minimum_interval_seconds: if matches!(provider_id, ProviderId::MuseCode) {
                900
            } else {
                300
            },
        },
        ProviderId::Fixture => ProviderCapabilities {
            provider_id,
            cardinality: AccountCardinality::SingleProfile,
            supports_app_owned_authorization: false,
            supports_external_profile: false,
            reports_monthly_window: false,
            minimum_interval_seconds: 300,
        },
    }
}

/// Whether the production build contains an adapter for this provider.
#[must_use]
pub const fn is_compiled(provider_id: ProviderId) -> bool {
    if cfg!(feature = "sample-data") && matches!(provider_id, ProviderId::Fixture) {
        return true;
    }
    matches!(
        provider_id,
        ProviderId::Codex
            | ProviderId::Claude
            | ProviderId::OpenCodeGo
            | ProviderId::Openrouter
            | ProviderId::Zai
            | ProviderId::Minimax
            | ProviderId::Kimi
            | ProviderId::Grok
            | ProviderId::MuseCode
    )
}
