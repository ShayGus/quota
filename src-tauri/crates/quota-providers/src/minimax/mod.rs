//! The `MiniMax` adapter: the Coding (token) Plan's quota, read with an API
//! key the person pasted.
//!
//! Quota owns this sign-in, as it does `OpenRouter`'s: see [`crate::keyed`].
//! Coding-plan keys start `sk-cp-`; a pay-as-you-go key has no plan quota and
//! is refused by the endpoint.

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

use crate::decode::{self, Numberish};
use crate::keyed::{self, KeyedSource};

/// The token plan's remaining quota, on the international host.
const REMAINS_URL: &str = "https://api.minimax.io/v1/token_plan/remains";

/// `base_resp.status_code` for a request over the rate limit.
const RATE_LIMITED: i64 = 1002;

/// The adapter for the `MiniMax` Coding Plan.
#[derive(Debug)]
pub(crate) struct MinimaxAdapter {
    source: KeyedSource,
}

impl MinimaxAdapter {
    /// Builds the adapter over the store its keys live in.
    pub(crate) fn new(secrets: Arc<dyn SecretStore>) -> Result<Self, ProviderError> {
        Ok(Self {
            source: KeyedSource::new(secrets)?,
        })
    }

    /// The plan's buckets, which also verify that the key works.
    async fn remains(
        &self,
        key: &Secret,
        context: &ReadContext,
    ) -> Result<wire::RemainsEnvelope, ProviderError> {
        let authorization = format!("Bearer {}", key.expose());
        let headers = [
            ("Authorization", authorization.as_str()),
            ("Accept", "application/json"),
        ];
        let reply = self.source.get(REMAINS_URL, &headers, context).await?;
        let envelope: wire::RemainsEnvelope =
            serde_json::from_value(reply.body).map_err(|_| ProviderError::UnsupportedSchema {
                detail: "the payload did not match the supported MiniMax plan shape".to_owned(),
            })?;
        // MiniMax answers a refused key with HTTP 200 and says so here.
        match envelope
            .base_resp
            .as_ref()
            .and_then(|base| base.status_code.as_ref())
            .and_then(Numberish::whole)
        {
            Some(0) => Ok(envelope),
            Some(RATE_LIMITED) => Err(ProviderError::RateLimited { retry_after: None }),
            _ => Err(ProviderError::Authentication),
        }
    }

    #[tracing::instrument(
        skip_all,
        fields(provider = ProviderId::Minimax.as_str(), connection = %binding.connection_id)
    )]
    async fn read(
        &self,
        binding: &ConnectionBinding,
        context: ReadContext,
        key: &Secret,
    ) -> Result<FetchOutcome, ProviderError> {
        let profile = keyed::check_binding(binding, ProviderId::Minimax, key)?;
        let envelope = self.remains(key, &context).await?;
        let pool = decode::pool_id(ProviderId::Minimax, &profile);
        let decoded = mapping::decode(envelope.buckets(), &pool, Utc::now())?;
        Ok(decoded.into_outcome(identity(envelope.plan())))
    }
}

/// What the person confirms: the key, and its plan when the answer names it.
fn identity(plan: Option<&str>) -> VerifiedIdentity {
    VerifiedIdentity {
        principal_label: "MiniMax API key".to_owned(),
        workspace_label: None,
        plan_label: plan.map(str::to_owned),
        source: SourceKind::ObservedWebEndpoint,
    }
}

impl ProviderAdapter for MinimaxAdapter {
    fn provider_id(&self) -> ProviderId {
        ProviderId::Minimax
    }

    fn capabilities(&self) -> ProviderCapabilities {
        capabilities()
    }

    fn policy(&self) -> ProviderPollingPolicy {
        keyed::policy(ProviderId::Minimax)
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
            let envelope = self
                .remains(credential, &keyed::verification_context())
                .await?;
            let profile = keyed::profile(credential);
            Ok(vec![DiscoveredAccount {
                principal_id: None,
                workspace_id: None,
                entitlement_id: None,
                pool_id: decode::pool_id(ProviderId::Minimax, &profile),
                profile_label: Some(profile),
                identity: identity(envelope.plan()),
                cardinality: AccountCardinality::Independent,
                credential_ownership: CredentialOwnership::AppOwned,
                state: ConnectionState::Connecting,
                nickname: "MiniMax".to_owned(),
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
    keyed::capabilities(ProviderId::Minimax, false, false)
}
