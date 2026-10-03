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
use quota_domain::polling::{FixedIntervalPolicy, PollingStrategy, ProviderPollingPolicy};
use quota_domain::provider::{ProviderCapabilities, ProviderId};
use quota_domain::quota::window::SourceKind;

use crate::decode;
use crate::http::{GetRequest, HttpReply, ProviderHttp, classify_status};

/// The shortest interval this adapter permits between reads, in seconds.
const MINIMUM_SECONDS: u32 = 300;

/// The interval used during battery saver or prolonged idle, in seconds.
const BATTERY_SAVER_SECONDS: u32 = 900;

/// The remote request deadline, in seconds.
const REQUEST_TIMEOUT_SECONDS: u32 = 10;

/// The key that made the request: its label and its spend limit.
const KEY_URL: &str = "https://openrouter.ai/api/v1/key";

/// The account's purchased credits and total spend.
const CREDITS_URL: &str = "https://openrouter.ai/api/v1/credits";

/// The adapter for `OpenRouter` credits and key limits.
#[derive(Debug)]
pub(crate) struct OpenRouterAdapter {
    http: Arc<ProviderHttp>,
    secrets: Arc<dyn SecretStore>,
}

impl OpenRouterAdapter {
    /// Builds the adapter over the store its keys live in.
    pub(crate) fn new(secrets: Arc<dyn SecretStore>) -> Result<Self, ProviderError> {
        Ok(Self {
            http: Arc::new(ProviderHttp::new()?),
            secrets,
        })
    }

    /// The key stored for one connection.
    async fn stored_key(&self, binding: &ConnectionBinding) -> Result<Secret, ProviderError> {
        let secrets = Arc::clone(&self.secrets);
        let connection = binding.connection_id.clone();
        tokio::task::spawn_blocking(move || secrets.read(&connection))
            .await
            .map_err(|_| ProviderError::Transient {
                detail: "the credential store did not answer".to_owned(),
            })?
            .map_err(|_| ProviderError::Transient {
                detail: "the credential store is unavailable".to_owned(),
            })?
            .ok_or(ProviderError::Authentication)
    }

    /// One authorized GET, classified.
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
        let reply = self
            .http
            .get(GetRequest {
                url,
                headers: &headers,
                deadline: context.deadline,
            })
            .await?;
        match classify_status(reply.status, reply.retry_after) {
            Some(failure) => Err(failure),
            None => Ok(reply),
        }
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
        decode::ensure_binding(binding, ProviderId::Openrouter, None, None)?;
        let data = self.key_data(key, &context).await?;
        let credits = self.credits(key, &context).await?;
        let label = identity_label(&data);
        let pool = decode::pool_id(ProviderId::Openrouter, &label);
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
        ProviderPollingPolicy {
            provider_id: ProviderId::Openrouter,
            strategy: PollingStrategy::FixedInterval(FixedIntervalPolicy {
                visible_seconds: MINIMUM_SECONDS,
                background_seconds: MINIMUM_SECONDS,
                battery_saver_seconds: BATTERY_SAVER_SECONDS,
                minimum_seconds: MINIMUM_SECONDS,
            }),
            request_timeout_seconds: REQUEST_TIMEOUT_SECONDS,
            helper_timeout_seconds: 0,
            backoff_minutes: quota_domain::polling::DEFAULT_BACKOFF_MINUTES.to_vec(),
            max_concurrent_remote_reads: 1,
            version: 1,
        }
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
            let key = self.stored_key(&binding).await?;
            self.read(&binding, context, &key).await
        })
    }

    fn discover_with<'a>(
        &'a self,
        credential: &'a Secret,
    ) -> ProviderFuture<'a, Result<Vec<DiscoveredAccount>, ProviderError>> {
        Box::pin(async move {
            let context = ReadContext {
                attempt_id: quota_domain::ids::ConnectionAttemptId::generate(),
                deadline: None,
            };
            let data = self.key_data(credential, &context).await?;
            let label = identity_label(&data);
            Ok(vec![DiscoveredAccount {
                // The key is the account Quota reads; OpenRouter reports no
                // account identifier for it.
                principal_id: None,
                workspace_id: None,
                entitlement_id: None,
                profile_label: None,
                pool_id: decode::pool_id(ProviderId::Openrouter, &label),
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

/// What this adapter declares, also used before the adapter is built.
#[must_use]
pub(crate) const fn capabilities() -> ProviderCapabilities {
    ProviderCapabilities {
        provider_id: ProviderId::Openrouter,
        cardinality: AccountCardinality::Independent,
        supports_app_owned_authorization: true,
        supports_external_profile: false,
        reports_monthly_window: true,
        minimum_interval_seconds: MINIMUM_SECONDS,
    }
}
