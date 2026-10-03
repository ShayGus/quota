//! The Codex adapter: credential discovery, a bounded read, and normalisation.
//!
//! Codex usage is read from undocumented HTTPS endpoints with a token the Codex
//! CLI owns. This adapter never writes to, refreshes, or rotates that token, and
//! never runs the Codex CLI: a token the endpoint rejects becomes an
//! authentication state for the user to fix in Codex.

pub(crate) mod mapping;
pub(crate) mod wire;

#[cfg(test)]
mod tests;

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

/// Whether a failure must stop endpoint fallback.
///
/// A rejected credential, a refused request, and a rate limit all describe the
/// account rather than the endpoint, so the second endpoint cannot answer them
/// any better. Retrying would only replace that clear reason with a vaguer one.
fn is_decisive(error: &ProviderError) -> bool {
    matches!(
        error,
        ProviderError::Authentication
            | ProviderError::Authorization
            | ProviderError::RateLimited { .. }
    )
}

/// Why one endpoint gave no usable reading.
#[derive(Debug)]
struct EndpointFailure {
    error: ProviderError,
    /// The endpoint answered with success, so it accepted the credential and
    /// only its body could not be used.
    accepted: bool,
}

impl From<ProviderError> for EndpointFailure {
    fn from(error: ProviderError) -> Self {
        Self {
            error,
            accepted: false,
        }
    }
}

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
    ) -> Result<DecodedUsage, EndpointFailure> {
        let authorization = format!("Bearer {}", credential.token.expose());
        // The account header travels only when the credential named an account:
        // an empty header is not the same as an absent one to the endpoint.
        let mut headers = vec![
            ("Authorization", authorization),
            ("Accept", "application/json".to_owned()),
        ];
        headers.extend(
            credential
                .account_id
                .clone()
                .map(|id| ("ChatGPT-Account-Id", id)),
        );
        let header_refs: Vec<(&str, &str)> = headers
            .iter()
            .map(|(name, value)| (*name, value.as_str()))
            .collect();
        let reply = self
            .http
            .get(GetRequest {
                url,
                headers: &header_refs,
                deadline: context.deadline,
            })
            .await?;
        if let Some(failure) = classify_status(reply.status, reply.retry_after) {
            return Err(failure.into());
        }
        let accepted = |error: ProviderError| EndpointFailure {
            error,
            accepted: true,
        };
        let envelope: wire::CodexEnvelope = serde_json::from_value(reply.body).map_err(|_| {
            accepted(ProviderError::UnsupportedSchema {
                detail: "the payload did not match the supported Codex shape".to_owned(),
            })
        })?;
        // The payload may repeat the account it belongs to. A payload for
        // another account is refused rather than attributed to this one.
        if let Some(reported) = envelope.account_id.as_deref()
            && let Some(expected) = credential.account_id.as_deref()
            && reported != expected
        {
            return Err(accepted(ProviderError::InvalidData {
                detail: "the payload belongs to another account".to_owned(),
            }));
        }
        mapping::decode(&envelope, pool, Utc::now()).map_err(accepted)
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
        let usage = self
            .read_usage(&credential, &pool, context, [PRIMARY_URL, FALLBACK_URL])
            .await?;
        let identity = verified_identity(
            usage.principal_label.clone().unwrap_or(profile_label),
            usage.plan_label.clone(),
        );
        Ok(usage.into_outcome(identity))
    }

    async fn read_usage(
        &self,
        credential: &CodexCredential,
        pool: &QuotaPoolId,
        context: ReadContext,
        endpoints: [&str; 2],
    ) -> Result<DecodedUsage, ProviderError> {
        let first = match self
            .read_endpoint(endpoints[0], credential, pool, context.clone())
            .await
        {
            Ok(usage) => return Ok(usage),
            Err(failure) if is_decisive(&failure.error) => return Err(failure.error),
            Err(failure) => failure,
        };
        match self
            .read_endpoint(endpoints[1], credential, pool, context)
            .await
        {
            Ok(usage) => Ok(usage),
            // The first endpoint already accepted the credential, so a refusal
            // from the second, such as a web firewall's 403 page, says nothing
            // about the account and must not replace the real reason.
            Err(failure) if is_decisive(&failure.error) && !first.accepted => Err(failure.error),
            Err(_) => Err(first.error),
        }
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
            // A plan whose only allowance covers a month reports one window.
            reports_monthly_window: true,
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
