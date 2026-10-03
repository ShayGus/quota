//! The Kimi for Coding adapter: read with an API key the person pasted, or
//! with the sign-in the Kimi CLI keeps on this computer.
//!
//! A pasted key is Quota's own, kept in the system credential store: see
//! [`crate::keyed`]. The CLI's sign-in stays the CLI's: Quota reads its file
//! on every read and never refreshes it. A connection's profile label says
//! which one it uses.

pub(crate) mod cli;
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

/// The coding plan's usage.
const USAGE_URL: &str = "https://api.kimi.com/coding/v1/usages";

/// The adapter for Kimi for Coding.
#[derive(Debug)]
pub(crate) struct KimiAdapter {
    source: KeyedSource,
}

impl KimiAdapter {
    /// Builds the adapter over the store pasted keys live in.
    pub(crate) fn new(secrets: Arc<dyn SecretStore>) -> Result<Self, ProviderError> {
        Ok(Self {
            source: KeyedSource::new(secrets)?,
        })
    }

    /// The plan's usage, which also verifies that the token works.
    async fn usage(
        &self,
        token: &Secret,
        context: &ReadContext,
    ) -> Result<wire::UsageEnvelope, ProviderError> {
        let authorization = format!("Bearer {}", token.expose());
        let headers = [
            ("Authorization", authorization.as_str()),
            ("Accept", "application/json"),
        ];
        let reply = self.source.get(USAGE_URL, &headers, context).await?;
        serde_json::from_value(reply.body).map_err(|_| ProviderError::UnsupportedSchema {
            detail: "the payload did not match the supported Kimi usage shape".to_owned(),
        })
    }

    #[tracing::instrument(
        skip_all,
        fields(provider = ProviderId::Kimi.as_str(), connection = %binding.connection_id)
    )]
    async fn read(
        &self,
        binding: &ConnectionBinding,
        context: ReadContext,
        token: &Secret,
        profile: &str,
    ) -> Result<FetchOutcome, ProviderError> {
        decode::ensure_binding(binding, ProviderId::Kimi, None, Some(profile))?;
        let envelope = self.usage(token, &context).await?;
        let pool = decode::pool_id(ProviderId::Kimi, profile);
        let decoded = mapping::decode(&envelope, &pool, Utc::now())?;
        let plan = decoded.plan_label.clone();
        Ok(decoded.into_outcome(identity(profile, plan)))
    }

    /// The account a token signs in, before anything is stored.
    async fn discover(
        &self,
        token: &Secret,
        profile: String,
        ownership: CredentialOwnership,
    ) -> Result<Vec<DiscoveredAccount>, ProviderError> {
        let envelope = self.usage(token, &keyed::verification_context()).await?;
        Ok(vec![DiscoveredAccount {
            principal_id: None,
            workspace_id: None,
            entitlement_id: None,
            pool_id: decode::pool_id(ProviderId::Kimi, &profile),
            identity: identity(&profile, mapping::plan_label(&envelope)),
            profile_label: Some(profile),
            cardinality: AccountCardinality::Independent,
            credential_ownership: ownership,
            state: ConnectionState::Connecting,
            nickname: "Kimi".to_owned(),
        }])
    }
}

/// What the person confirms: which sign-in this is, and the plan.
fn identity(profile: &str, plan: Option<String>) -> VerifiedIdentity {
    VerifiedIdentity {
        principal_label: if profile == cli::PROFILE {
            "Kimi CLI sign-in".to_owned()
        } else {
            "Kimi API key".to_owned()
        },
        workspace_label: None,
        plan_label: plan,
        source: SourceKind::ObservedWebEndpoint,
    }
}

impl ProviderAdapter for KimiAdapter {
    fn provider_id(&self) -> ProviderId {
        ProviderId::Kimi
    }

    fn capabilities(&self) -> ProviderCapabilities {
        capabilities()
    }

    fn policy(&self) -> ProviderPollingPolicy {
        keyed::policy(ProviderId::Kimi)
    }

    fn discover_accounts(
        &self,
    ) -> ProviderFuture<'_, Result<Vec<DiscoveredAccount>, ProviderError>> {
        Box::pin(async move {
            let token = cli::token().await?;
            self.discover(
                &token,
                cli::PROFILE.to_owned(),
                CredentialOwnership::ExternalClient,
            )
            .await
        })
    }

    fn read_quota(
        &self,
        binding: &ConnectionBinding,
        context: ReadContext,
    ) -> ProviderFuture<'_, Result<FetchOutcome, ProviderError>> {
        let binding = binding.clone();
        Box::pin(async move {
            if binding.profile_label.as_deref() == Some(cli::PROFILE) {
                let token = cli::token().await?;
                return self.read(&binding, context, &token, cli::PROFILE).await;
            }
            let key = self.source.stored_key(&binding).await?;
            let profile = keyed::profile(&key);
            self.read(&binding, context, &key, &profile).await
        })
    }

    fn discover_with<'a>(
        &'a self,
        credential: &'a Secret,
    ) -> ProviderFuture<'a, Result<Vec<DiscoveredAccount>, ProviderError>> {
        Box::pin(self.discover(
            credential,
            keyed::profile(credential),
            CredentialOwnership::AppOwned,
        ))
    }

    fn read_with<'a>(
        &'a self,
        binding: &'a ConnectionBinding,
        context: ReadContext,
        credential: &'a Secret,
    ) -> ProviderFuture<'a, Result<FetchOutcome, ProviderError>> {
        Box::pin(async move {
            let profile = keyed::profile(credential);
            self.read(binding, context, credential, &profile).await
        })
    }
}

/// What this adapter declares: a pasted key, or the CLI's own sign-in.
#[must_use]
pub(crate) const fn capabilities() -> ProviderCapabilities {
    keyed::capabilities(ProviderId::Kimi, true, true)
}
