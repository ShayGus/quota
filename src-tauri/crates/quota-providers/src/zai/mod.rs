//! The Z.ai adapter: the GLM Coding Plan's quota, read with an API key the
//! person pasted.
//!
//! Z.ai has no tool of its own on the user's computer, so Quota owns this
//! sign-in, as it does `OpenRouter`'s: see [`crate::keyed`]. The quota endpoint
//! takes the key itself as the `Authorization` header, without a scheme.

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
use crate::keyed::{self, KeyedSource};

/// The coding plan's quota meters.
const QUOTA_URL: &str = "https://api.z.ai/api/monitor/usage/quota/limit";

/// The adapter for the Z.ai GLM Coding Plan.
#[derive(Debug)]
pub(crate) struct ZaiAdapter {
    source: KeyedSource,
}

impl ZaiAdapter {
    /// Builds the adapter over the store its keys live in.
    pub(crate) fn new(secrets: Arc<dyn SecretStore>) -> Result<Self, ProviderError> {
        Ok(Self {
            source: KeyedSource::new(secrets)?,
        })
    }

    /// The plan's meters, which also verify that the key works.
    async fn quota(
        &self,
        key: &Secret,
        context: &ReadContext,
    ) -> Result<wire::QuotaData, ProviderError> {
        let headers = [
            ("Authorization", key.expose()),
            ("Accept", "application/json"),
        ];
        let reply = self.source.get(QUOTA_URL, &headers, context).await?;
        let envelope: wire::QuotaEnvelope =
            serde_json::from_value(reply.body).map_err(|_| ProviderError::UnsupportedSchema {
                detail: "the payload did not match the supported Z.ai quota shape".to_owned(),
            })?;
        // Z.ai answers a refused key with a body that says so, not a status.
        if envelope.success != Some(true) {
            return Err(ProviderError::Authentication);
        }
        envelope
            .data
            .ok_or_else(|| ProviderError::UnsupportedSchema {
                detail: "the Z.ai quota answer carried no data".to_owned(),
            })
    }

    #[tracing::instrument(
        skip_all,
        fields(provider = ProviderId::Zai.as_str(), connection = %binding.connection_id)
    )]
    async fn read(
        &self,
        binding: &ConnectionBinding,
        context: ReadContext,
        key: &Secret,
    ) -> Result<FetchOutcome, ProviderError> {
        let profile = keyed::check_binding(binding, ProviderId::Zai, key)?;
        let data = self.quota(key, &context).await?;
        let pool = decode::pool_id(ProviderId::Zai, &profile);
        let decoded = mapping::decode(&data, &pool, Utc::now())?;
        let plan = decoded.plan_label.clone();
        Ok(decoded.into_outcome(identity(plan)))
    }
}

/// What the person confirms: the key, and the plan it belongs to. The quota
/// answer names no account.
fn identity(plan: Option<String>) -> VerifiedIdentity {
    VerifiedIdentity {
        principal_label: "Z.ai API key".to_owned(),
        workspace_label: None,
        plan_label: plan,
        source: SourceKind::ObservedWebEndpoint,
    }
}

impl ProviderAdapter for ZaiAdapter {
    fn provider_id(&self) -> ProviderId {
        ProviderId::Zai
    }

    fn capabilities(&self) -> ProviderCapabilities {
        capabilities()
    }

    fn policy(&self) -> ProviderPollingPolicy {
        keyed::policy(ProviderId::Zai)
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
                .quota(credential, &keyed::verification_context())
                .await?;
            let profile = keyed::profile(credential);
            Ok(vec![DiscoveredAccount {
                principal_id: None,
                workspace_id: None,
                entitlement_id: None,
                pool_id: decode::pool_id(ProviderId::Zai, &profile),
                profile_label: Some(profile),
                identity: identity(mapping::plan_label(data.level.as_deref())),
                cardinality: AccountCardinality::Independent,
                credential_ownership: CredentialOwnership::AppOwned,
                state: ConnectionState::Connecting,
                nickname: "Z.ai".to_owned(),
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
    keyed::capabilities(ProviderId::Zai, false, true)
}
