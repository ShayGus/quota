//! The Claude adapter: profile verification, a bounded usage read, mapping.
//!
//! Claude usage is read from undocumented OAuth routes with a token Claude Code
//! owns. This adapter never refreshes that token, never runs Claude Code, and has
//! no inference-based quota path at all. When the profile route reports no
//! account identity the reading is refused, because an unverified identity must
//! not be invented.

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
use quota_domain::ids::{ConnectionAttemptId, ProviderPrincipalId};
use quota_domain::polling::{FixedIntervalPolicy, PollingStrategy, ProviderPollingPolicy};
use quota_domain::provider::{ProviderCapabilities, ProviderId};
use quota_domain::quota::window::SourceKind;

use crate::credentials::{self, ClaudeCredential};
use crate::decode::{self, DecodedUsage, masked_address};
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
const USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";

/// The identity route.
const PROFILE_URL: &str = "https://api.anthropic.com/api/oauth/profile";

/// The beta marker these routes require.
const BETA_HEADER: &str = "oauth-2025-04-20";

/// The account identity the profile route proved, plus its display form.
struct VerifiedProfile {
    /// The account identifier the source reported. This is the verified identity.
    account_uuid: String,
    /// The label shown for confirmation.
    identity: VerifiedIdentity,
}

/// The adapter for Claude subscription usage.
#[derive(Debug)]
pub(crate) struct ClaudeAdapter {
    http: Arc<ProviderHttp>,
}

impl ClaudeAdapter {
    /// Builds the adapter and its one pooled HTTP client.
    pub(crate) fn new() -> Result<Self, ProviderError> {
        Ok(Self {
            http: Arc::new(ProviderHttp::new()?),
        })
    }

    /// Requests one route with the credential and the required beta marker.
    async fn get_route(
        &self,
        url: &str,
        credential: &ClaudeCredential,
        context: ReadContext,
    ) -> Result<serde_json::Value, ProviderError> {
        let authorization = format!("Bearer {}", credential.token.expose());
        let headers = [
            ("Authorization", authorization.as_str()),
            ("anthropic-beta", BETA_HEADER),
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
        Ok(reply.body)
    }

    /// Reads and validates the account identity from the profile route.
    async fn verified_profile(
        &self,
        credential: &ClaudeCredential,
        context: ReadContext,
    ) -> Result<VerifiedProfile, ProviderError> {
        let body = self.get_route(PROFILE_URL, credential, context).await?;
        let profile: wire::ClaudeProfile =
            serde_json::from_value(body).map_err(|_| ProviderError::UnsupportedSchema {
                detail: "the payload did not match the supported Claude profile shape".to_owned(),
            })?;
        let account = profile.account.unwrap_or_default();
        // An absent account identifier is an unverified reading, not a new one.
        let account_uuid = account
            .uuid
            .as_deref()
            .map(str::trim)
            .filter(|uuid| !uuid.is_empty())
            .ok_or_else(decode::missing_identity)?
            .to_owned();
        let principal_label = account
            .email
            .as_deref()
            .map_or_else(|| account_uuid.clone(), masked_address);
        Ok(VerifiedProfile {
            identity: VerifiedIdentity {
                principal_label,
                workspace_label: profile.organization.and_then(|org| org.name),
                plan_label: None,
                source: SourceKind::ObservedWebEndpoint,
            },
            account_uuid,
        })
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
            provider = ProviderId::Claude.as_str(),
            connection = %binding.connection_id,
            profile = binding.profile_label.as_deref().unwrap_or("default"),
        )
    )]
    async fn perform_read(
        &self,
        binding: ConnectionBinding,
        context: ReadContext,
    ) -> Result<FetchOutcome, ProviderError> {
        let credential = credentials::claude_credential().await?;
        let profile_label = credential.profile_label.clone();
        let pool = decode::pool_id(ProviderId::Claude, &profile_label);
        // The profile route proves which account this credential belongs
        // to, so a re-login for another account cannot answer for this
        // binding.
        let profile = self.verified_profile(&credential, context.clone()).await?;
        decode::ensure_binding(
            &binding,
            ProviderId::Claude,
            Some(profile.account_uuid.as_str()),
            Some(profile_label.as_str()),
        )?;
        let body = self.get_route(USAGE_URL, &credential, context).await?;
        let usage: wire::ClaudeUsage =
            serde_json::from_value(body).map_err(|_| ProviderError::UnsupportedSchema {
                detail: "the payload did not match the supported Claude usage shape".to_owned(),
            })?;
        let decoded: DecodedUsage = mapping::decode(&usage, &pool, Utc::now())?;
        Ok(decoded.into_outcome(profile.identity))
    }
}

impl ProviderAdapter for ClaudeAdapter {
    fn provider_id(&self) -> ProviderId {
        ProviderId::Claude
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities {
            provider_id: ProviderId::Claude,
            cardinality: AccountCardinality::SingleProfile,
            supports_app_owned_authorization: false,
            supports_external_profile: true,
            // The extra-usage cap is a monthly window, reported when enabled.
            reports_monthly_window: true,
            minimum_interval_seconds: MINIMUM_SECONDS,
        }
    }

    fn policy(&self) -> ProviderPollingPolicy {
        ProviderPollingPolicy {
            provider_id: ProviderId::Claude,
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
            let credential = credentials::claude_credential().await?;
            let context = ReadContext {
                attempt_id: ConnectionAttemptId::generate(),
                deadline: None,
            };
            let profile = self.verified_profile(&credential, context.clone()).await?;
            let profile_label = credential.profile_label.clone();
            let pool = decode::pool_id(ProviderId::Claude, &profile_label);
            let principal = ProviderPrincipalId::new(profile.account_uuid).ok();
            Ok(vec![DiscoveredAccount {
                principal_id: principal,
                workspace_id: None,
                entitlement_id: None,
                profile_label: Some(profile_label),
                pool_id: pool,
                identity: profile.identity,
                cardinality: AccountCardinality::SingleProfile,
                credential_ownership: CredentialOwnership::ExternalClient,
                state: ConnectionState::Connected,
                nickname: "Claude".to_owned(),
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
