//! The Codex adapter: credential discovery, a bounded read, and normalisation.
//!
//! Codex usage is read from undocumented HTTPS endpoints with a token the Codex
//! CLI owns. This adapter never writes to, refreshes, or rotates that token, and
//! never runs the Codex CLI: a token the endpoint rejects becomes an
//! authentication state for the user to fix in Codex.

pub(crate) mod mapping;
pub(crate) mod wire;

use std::sync::Arc;

use chrono::Utc;
use quota_core::ports::{
    ConnectionBinding, DiscoveredAccount, FetchOutcome, ProviderAdapter, ProviderError,
    ProviderFuture, ReadContext,
};
use quota_domain::account::{
    AccountCardinality, ConnectionState, CredentialOwnership, VerifiedIdentity,
};
use quota_domain::ids::{ProviderPrincipalId, QuotaPoolId};
use quota_domain::polling::{EventAssistedPolicy, PollingStrategy, ProviderPollingPolicy};
use quota_domain::provider::{ProviderCapabilities, ProviderId};
use quota_domain::quota::window::SourceKind;

use crate::credentials::{self, CodexCredential};
use crate::decode::{self, DecodedUsage};
use crate::http::{GetRequest, ProviderHttp, classify_status};

/// The verification interval the event-assisted policy uses, in seconds.
const VERIFICATION_SECONDS: u32 = 300;

/// The remote request deadline, in seconds.
const REQUEST_TIMEOUT_SECONDS: u32 = 10;

/// The first usage endpoint, tried before the second.
const PRIMARY_URL: &str = "https://chatgpt.com/backend-api/wham/usage";

/// The second usage endpoint, tried when the first yields nothing usable.
const FALLBACK_URL: &str = "https://chatgpt.com/backend-api/codex/usage";

/// The adapter for the Codex CLI's own usage report.
#[derive(Debug)]
pub(crate) struct CodexAdapter {
    http: Arc<ProviderHttp>,
}

impl CodexAdapter {
    /// Builds the adapter and its one pooled HTTP client.
    pub(crate) fn new() -> Result<Self, ProviderError> {
        Ok(Self {
            http: Arc::new(ProviderHttp::new()?),
        })
    }

    /// Reads one endpoint and decodes it into windows.
    ///
    /// The credential travels in the request only; it is never logged, stored,
    /// or copied into a span field.
    async fn read_endpoint(
        &self,
        url: &str,
        credential: &CodexCredential,
        pool: &QuotaPoolId,
        context: ReadContext,
    ) -> Result<DecodedUsage, ProviderError> {
        let authorization = format!("Bearer {}", credential.token.expose());
        let account_id = credential.account_id.clone().unwrap_or_default();
        let headers = [
            ("Authorization", authorization.as_str()),
            ("ChatGPT-Account-Id", account_id.as_str()),
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
        if let Some(failure) = classify_status(reply.status, reply.retry_after) {
            return Err(failure);
        }
        let envelope: wire::CodexEnvelope =
            serde_json::from_value(reply.body).map_err(|_| ProviderError::UnsupportedSchema {
                detail: "the payload did not match the supported Codex shape".to_owned(),
            })?;
        // The payload may repeat the account it belongs to. A payload for
        // another account is refused rather than attributed to this one.
        if let Some(reported) = envelope.account_id.as_deref()
            && let Some(expected) = credential.account_id.as_deref()
            && reported != expected
        {
            return Err(ProviderError::InvalidData {
                detail: "the payload belongs to another account".to_owned(),
            });
        }
        mapping::decode(&envelope, pool, Utc::now())
    }
    /// Performs one read at the credential, HTTP, and decoding boundary.
    ///
    /// `skip_all` keeps every argument out of the span by default, and the field
    /// list records only what is safe to log: the provider, an opaque connection
    /// identity, and a profile label. No credential, address, body, path, or full
    /// URL is recorded.
    #[tracing::instrument(
        skip_all,
        fields(
            provider = ProviderId::Codex.as_str(),
            connection = %binding.connection_id,
            profile = binding.profile_label.as_deref().unwrap_or("default"),
        )
    )]
    async fn perform_read(
        &self,
        binding: ConnectionBinding,
        context: ReadContext,
    ) -> Result<FetchOutcome, ProviderError> {
        let credential = credentials::codex_credential().await?;
        let profile_label = credential.profile_label.clone();
        let pool = decode::pool_id(ProviderId::Codex, &profile_label);
        decode::ensure_binding(
            &binding,
            ProviderId::Codex,
            credential.account_id.as_deref(),
            Some(profile_label.as_str()),
        )?;
        let mut last_error = None;
        for url in [PRIMARY_URL, FALLBACK_URL] {
            match self
                .read_endpoint(url, &credential, &pool, context.clone())
                .await
            {
                Ok(usage) => {
                    let identity = verified_identity(
                        usage
                            .principal_label
                            .clone()
                            .unwrap_or_else(|| profile_label.clone()),
                        usage.plan_label.clone(),
                    );
                    return Ok(usage.into_outcome(identity));
                }
                Err(error) => last_error = Some(error),
            }
        }
        Err(last_error.unwrap_or(ProviderError::Transient {
            detail: "no Codex usage endpoint answered".to_owned(),
        }))
    }
}

impl ProviderAdapter for CodexAdapter {
    fn provider_id(&self) -> ProviderId {
        ProviderId::Codex
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            provider_id: ProviderId::Codex,
            cardinality: AccountCardinality::SingleProfile,
            supports_app_owned_authorization: false,
            supports_external_profile: true,
            // Codex has no monthly allowance.
            reports_monthly_window: false,
            minimum_interval_seconds: VERIFICATION_SECONDS,
        }
    }

    fn policy(&self) -> ProviderPollingPolicy {
        ProviderPollingPolicy {
            provider_id: ProviderId::Codex,
            strategy: PollingStrategy::EventAssisted(EventAssistedPolicy {
                minimum_seconds: VERIFICATION_SECONDS,
                verification_seconds: VERIFICATION_SECONDS,
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
        Box::pin(async move {
            let credential = credentials::codex_credential().await?;
            let profile_label = credential.profile_label.clone();
            let principal = credential
                .account_id
                .clone()
                .and_then(|id| ProviderPrincipalId::new(id).ok());
            let verified = principal.is_some();
            let pool = decode::pool_id(ProviderId::Codex, &profile_label);
            Ok(vec![DiscoveredAccount {
                principal_id: principal,
                workspace_id: None,
                entitlement_id: None,
                profile_label: Some(profile_label.clone()),
                pool_id: pool,
                identity: verified_identity(profile_label, None),
                cardinality: AccountCardinality::SingleProfile,
                credential_ownership: CredentialOwnership::ExternalClient,
                state: if verified {
                    ConnectionState::Connected
                } else {
                    ConnectionState::Connecting
                },
                nickname: "Codex".to_owned(),
            }])
        })
    }

    fn read_quota(
        &self,
        binding: &ConnectionBinding,
        context: ReadContext,
    ) -> ProviderFuture<'_, Result<FetchOutcome, ProviderError>> {
        let binding = binding.clone();
        Box::pin(self.perform_read(binding, context))
    }
}

/// The identity shown for confirmation, from verified values only.
fn verified_identity(principal_label: String, plan_label: Option<String>) -> VerifiedIdentity {
    VerifiedIdentity {
        principal_label,
        workspace_label: None,
        plan_label,
        source: SourceKind::ObservedWebEndpoint,
    }
}
