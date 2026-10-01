//! The `OpenCode` Go adapter: a bearer-key read and normalisation.
//!
//! This endpoint reports no account, workspace, or entitlement identity. This
//! adapter therefore does not invent one: it uses a stable local pool derived
//! from the credential profile, and leaves the optional principal, workspace,
//! and entitlement fields empty so the account reads as unverified. That is an
//! accepted, honest limitation of this connector, not a defect to paper over.

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
use quota_domain::polling::{FixedIntervalPolicy, PollingStrategy, ProviderPollingPolicy};
use quota_domain::provider::{ProviderCapabilities, ProviderId};
use quota_domain::quota::window::SourceKind;
use tracing::Instrument;

use crate::credentials;
use crate::decode::{self, DecodedUsage};
use crate::http::{GetRequest, ProviderHttp, classify_status};

/// The shortest interval this adapter permits between reads, in seconds.
const MINIMUM_SECONDS: u32 = 300;

/// The interval used while an overview is visible, in seconds.
const VISIBLE_SECONDS: u32 = 300;

/// The interval used while only the tray is running, in seconds.
const BACKGROUND_SECONDS: u32 = 300;

/// The interval used during battery saver or prolonged idle, in seconds.
const BATTERY_SAVER_SECONDS: u32 = 900;

/// The remote request deadline, in seconds.
const REQUEST_TIMEOUT_SECONDS: u32 = 10;

/// The usage route.
const USAGE_URL: &str = "https://opencode.ai/zen/go/v1/usage";

/// The adapter for `OpenCode` Go usage.
#[derive(Debug)]
pub(crate) struct OpenCodeGoAdapter {
    http: Arc<ProviderHttp>,
}

impl OpenCodeGoAdapter {
    /// Builds the adapter and its one pooled HTTP client.
    pub(crate) fn new() -> Result<Self, ProviderError> {
        Ok(Self {
            http: Arc::new(ProviderHttp::new()?),
        })
    }
}

impl ProviderAdapter for OpenCodeGoAdapter {
    fn provider_id(&self) -> ProviderId {
        ProviderId::OpenCodeGo
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            provider_id: ProviderId::OpenCodeGo,
            cardinality: AccountCardinality::SingleProfile,
            supports_app_owned_authorization: false,
            supports_external_profile: true,
            // This is the one connector in scope that reports a monthly window.
            reports_monthly_window: true,
            minimum_interval_seconds: MINIMUM_SECONDS,
        }
    }

    fn policy(&self) -> ProviderPollingPolicy {
        ProviderPollingPolicy {
            provider_id: ProviderId::OpenCodeGo,
            strategy: PollingStrategy::FixedInterval(FixedIntervalPolicy {
                visible_seconds: VISIBLE_SECONDS,
                background_seconds: BACKGROUND_SECONDS,
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
        Box::pin(async move {
            let credential = credentials::opencode_go_credential().await?;
            let profile_label = credential.profile_label.clone();
            let pool = decode::pool_id(ProviderId::OpenCodeGo, &profile_label);
            Ok(vec![DiscoveredAccount {
                // The response carries no account, workspace, or entitlement
                // identity, so none is fabricated here.
                principal_id: None,
                workspace_id: None,
                entitlement_id: None,
                profile_label: Some(profile_label.clone()),
                pool_id: pool,
                identity: VerifiedIdentity {
                    // The honest label is the credential profile, not an account.
                    principal_label: format!("OpenCode Go ({profile_label})"),
                    workspace_label: None,
                    plan_label: None,
                    source: SourceKind::ObservedWebEndpoint,
                },
                cardinality: AccountCardinality::SingleProfile,
                credential_ownership: CredentialOwnership::ExternalClient,
                state: ConnectionState::Connecting,
                nickname: "OpenCode Go".to_owned(),
            }])
        })
    }

    fn read_quota(
        &self,
        binding: &ConnectionBinding,
        context: ReadContext,
    ) -> ProviderFuture<'_, Result<FetchOutcome, ProviderError>> {
        let binding = binding.clone();
        Box::pin(
            async move {
                let credential = credentials::opencode_go_credential().await?;
                let profile_label = credential.profile_label.clone();
                let pool = decode::pool_id(ProviderId::OpenCodeGo, &profile_label);
                // There is no provider-reported identity, so the check covers
                // what exists: the provider and the credential profile.
                decode::ensure_binding(
                    binding,
                    ProviderId::OpenCodeGo,
                    None,
                    Some(profile_label.as_str()),
                )?;
                let authorization = format!("Bearer {}", credential.key.expose());
                let headers = [
                    ("Authorization", authorization.as_str()),
                    ("Accept", "application/json"),
                ];
                let reply = self
                    .http
                    .get(GetRequest {
                        url: USAGE_URL,
                        headers: &headers,
                        deadline: context.deadline,
                    })
                    .await?;
                if let Some(failure) = classify_status(reply.status, reply.retry_after) {
                    return Err(failure);
                }
                let envelope: wire::OpenCodeGoEnvelope = serde_json::from_value(reply.body)
                    .map_err(|_| ProviderError::UnsupportedSchema {
                        detail: "the payload did not match the supported OpenCode Go shape"
                            .to_owned(),
                    })?;
                let decoded: DecodedUsage = mapping::decode(&envelope, &pool, Utc::now())?;
                let identity = VerifiedIdentity {
                    principal_label: format!("OpenCode Go ({profile_label})"),
                    workspace_label: None,
                    plan_label: None,
                    source: SourceKind::ObservedWebEndpoint,
                };
                Ok(decoded.into_outcome(identity))
            }
            .instrument(tracing::info_span!(
                "quota_provider_read",
                provider = ProviderId::OpenCodeGo.as_str(),
                connection = %binding.connection_id,
                profile = binding.profile_label.as_deref().unwrap_or("default"),
            )),
        )
    }
}
