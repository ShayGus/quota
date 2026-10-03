//! The `OpenRouter` adapter: an API key the person pasted, read through
//! `OpenRouter`'s documented API.
//!
//! `OpenRouter` has no client of its own on the user's machine, so Quota owns
//! this sign-in: the key is verified before anything is saved, then kept in the
//! system credential store under the connection it signs in. Each read takes it
//! from there. The key itself never reaches Quota's database, logs, or
//! snapshots.

pub(crate) mod mapping;
pub(crate) mod wire;

use std::sync::Arc;

use chrono::Utc;
use quota_core::ports::{
    ConnectionBinding, DiscoveredAccount, FetchOutcome, ProviderAdapter, ProviderError,
    ProviderFuture, ReadContext, Secret, SecretStore,
};
use quota_domain::account::{
    AccountCardinality, ConnectionState, CredentialOwnership, VerifiedIdentity,
};
use quota_domain::polling::ProviderPollingPolicy;
use quota_domain::provider::{ProviderCapabilities, ProviderId};
use quota_domain::quota::window::SourceKind;

use crate::decode;
use crate::http::HttpReply;
use crate::keyed::{self, KeyedSource};

/// The key that made the request: its label and its spend limit.
const KEY_URL: &str = "https://openrouter.ai/api/v1/key";

/// The account's purchased credits and total spend.
const CREDITS_URL: &str = "https://openrouter.ai/api/v1/credits";

/// The adapter for `OpenRouter` credits and key limits.
#[derive(Debug)]
pub(crate) struct OpenRouterAdapter {
    source: KeyedSource,
}

impl OpenRouterAdapter {
    /// Builds the adapter over the store its keys live in.
    pub(crate) fn new(secrets: Arc<dyn SecretStore>) -> Result<Self, ProviderError> {
        Ok(Self {
            source: KeyedSource::new(secrets)?,
        })
    }

    /// One GET authorized with the key as a bearer token.
    async fn get(
        &self,
        url: &str,
        key: &Secret,
        context: &ReadContext,
    ) -> Result<HttpReply, ProviderError> {
        let authorization = format!("Bearer {}", key.expose());
        let headers = [
            ("Authorization", authorization.as_str()),
            ("Accept", "application/json"),
        ];
        self.source.get(url, &headers, context).await
    }

    /// The key's details, which also verify that the key works.
    async fn key_data(
        &self,
        key: &Secret,
        context: &ReadContext,
    ) -> Result<wire::KeyData, ProviderError> {
        let reply = self.get(KEY_URL, key, context).await?;
        serde_json::from_value::<wire::KeyEnvelope>(reply.body)
            .ok()
            .and_then(|envelope| envelope.data)
            .ok_or_else(|| ProviderError::UnsupportedSchema {
                detail: "the payload did not match the supported OpenRouter key shape".to_owned(),
            })
    }

    /// The account's credits, or `None` when this key may not read them.
    ///
    /// `OpenRouter` documents the credits endpoint for management keys, so an
    /// ordinary key can be refused there while its own limit stays readable.
    async fn credits(
        &self,
        key: &Secret,
        context: &ReadContext,
    ) -> Result<Option<wire::CreditsData>, ProviderError> {
        match self.get(CREDITS_URL, key, context).await {
            Ok(reply) => Ok(serde_json::from_value::<wire::CreditsEnvelope>(reply.body)
                .ok()
                .and_then(|envelope| envelope.data)),
            Err(ProviderError::Authentication | ProviderError::Authorization) => Ok(None),
            Err(other) => Err(other),
        }
    }

    /// Performs one read with the given key.
    #[tracing::instrument(
        skip_all,
        fields(provider = ProviderId::Openrouter.as_str(), connection = %binding.connection_id)
    )]
    async fn read(
        &self,
        binding: &ConnectionBinding,
        context: ReadContext,
        key: &Secret,
    ) -> Result<FetchOutcome, ProviderError> {
        let profile = keyed::check_binding(binding, ProviderId::Openrouter, key)?;
        let data = self.key_data(key, &context).await?;
        let credits = self.credits(key, &context).await?;
        let label = identity_label(&data);
        let pool = decode::pool_id(ProviderId::Openrouter, &profile);
        let decoded = mapping::decode(&data, credits.as_ref(), &pool, Utc::now())?;
        Ok(decoded.into_outcome(identity(label, &data)))
    }
}

/// The key's own label, which `OpenRouter` writes as a masked form of the key.
fn identity_label(data: &wire::KeyData) -> String {
    data.label
        .as_deref()
        .map(str::trim)
        .filter(|label| !label.is_empty())
        .map_or_else(|| "OpenRouter API key".to_owned(), str::to_owned)
}

/// What the person confirms: the key, and whether the account is on the free
/// tier.
fn identity(label: String, data: &wire::KeyData) -> VerifiedIdentity {
    VerifiedIdentity {
        principal_label: label,
        workspace_label: None,
        plan_label: data
            .is_free_tier
            .map(|free| if free { "Free tier" } else { "Pay as you go" }.to_owned()),
        source: SourceKind::DocumentedApi,
    }
}

impl ProviderAdapter for OpenRouterAdapter {
    fn provider_id(&self) -> ProviderId {
        ProviderId::Openrouter
    }

    fn capabilities(&self) -> ProviderCapabilities {
        capabilities()
    }

    fn policy(&self) -> ProviderPollingPolicy {
        keyed::policy(ProviderId::Openrouter)
    }

    fn discover_accounts(
        &self,
    ) -> ProviderFuture<'_, Result<Vec<DiscoveredAccount>, ProviderError>> {
        // There is no local sign-in to find: the person supplies the key.
        Box::pin(async { Ok(Vec::new()) })
    }

    fn read_quota(
        &self,
        binding: &ConnectionBinding,
        context: ReadContext,
    ) -> ProviderFuture<'_, Result<FetchOutcome, ProviderError>> {
        let binding = binding.clone();
        Box::pin(async move {
            let key = self.source.stored_key(&binding).await?;
            self.read(&binding, context, &key).await
        })
    }

    fn discover_with<'a>(
        &'a self,
        credential: &'a Secret,
    ) -> ProviderFuture<'a, Result<Vec<DiscoveredAccount>, ProviderError>> {
        Box::pin(async move {
            let data = self
                .key_data(credential, &keyed::verification_context())
                .await?;
            let label = identity_label(&data);
            let profile = keyed::profile(credential);
            Ok(vec![DiscoveredAccount {
                // The key is the account Quota reads; OpenRouter reports no
                // account identifier for it, so the key's fingerprint tells
                // one connected key from another.
                principal_id: None,
                workspace_id: None,
                entitlement_id: None,
                pool_id: decode::pool_id(ProviderId::Openrouter, &profile),
                profile_label: Some(profile),
                identity: identity(label, &data),
                cardinality: AccountCardinality::Independent,
                credential_ownership: CredentialOwnership::AppOwned,
                state: ConnectionState::Connecting,
                nickname: "OpenRouter".to_owned(),
            }])
        })
    }

    fn read_with<'a>(
        &'a self,
        binding: &'a ConnectionBinding,
        context: ReadContext,
        credential: &'a Secret,
    ) -> ProviderFuture<'a, Result<FetchOutcome, ProviderError>> {
        Box::pin(self.read(binding, context, credential))
    }
}

/// What this adapter declares.
#[must_use]
pub(crate) const fn capabilities() -> ProviderCapabilities {
    keyed::capabilities(ProviderId::Openrouter, false, true)
}
