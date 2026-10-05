//! The Muse Code adapter: Meta's Muse Code subscription usage.
//!
//! Quota signs in through the browser with the device sign-in the Muse CLI
//! uses (its public client, as oh-my-pi does), and keeps the account token in
//! the system credential store. Meta's token does not expire and cannot be
//! refreshed. When the Muse CLI is signed in on this computer, its sign-in can
//! be used instead.
//!
//! The usage endpoint is also the endpoint that hands out the subscription's
//! model API key, and Meta rate-limits it heavily. Quota sends it an empty
//! body (never the login-only `onboard`), discards the key it returns without
//! reading it, and reads no more often than every fifteen minutes.

pub(crate) mod cli;
pub(crate) mod mapping;
pub(crate) mod wire;

use std::sync::Arc;

use chrono::Utc;
use quota_core::ports::{
    ConnectionBinding, DeviceAuthorization, DevicePoll, DiscoveredAccount, FetchOutcome,
    ProviderAdapter, ProviderError, ProviderFuture, ReadContext, Secret, SecretStore,
};
use quota_domain::account::{
    AccountCardinality, ConnectionState, CredentialOwnership, VerifiedIdentity,
};
use quota_domain::ids::ProviderPrincipalId;
use quota_domain::polling::{FixedIntervalPolicy, PollingStrategy, ProviderPollingPolicy};
use quota_domain::provider::{ProviderCapabilities, ProviderId};
use quota_domain::quota::window::SourceKind;

use crate::decode;
use crate::device::{DeviceClient, StoredToken};
use crate::http::Answers;
use crate::keyed::{self, KeyedSource};
use crate::post::{Body, PostRequest};

/// The subscription and its usage.
const SUBSCRIPTION_URL: &str = "https://api.meta.ai/muse-code/key";

/// The API version Meta's Muse endpoints require.
const API_VERSION: &str = "1.0.0";

/// The shortest interval between reads: the endpoint is rate-limited.
const MINIMUM_SECONDS: u32 = 900;

/// The Muse CLI's device sign-in.
const CLIENT: DeviceClient<'static> = DeviceClient {
    device_url: "https://auth.meta.com/oidc/device/authorization/",
    token_url: "https://auth.meta.com/oidc/device/token/",
    client_id: "1031625952748946",
    scope: None,
    headers: &[("x-api-version", API_VERSION)],
    page_hosts: &["meta.com", "meta.ai", "facebook.com"],
};

/// The adapter for Muse Code.
#[derive(Debug)]
pub(crate) struct MuseAdapter {
    source: KeyedSource,
}

impl MuseAdapter {
    /// Builds the adapter over the store Quota's own tokens live in.
    pub(crate) fn new(secrets: Arc<dyn SecretStore>) -> Result<Self, ProviderError> {
        Ok(Self {
            source: KeyedSource::new(secrets)?,
        })
    }

    /// The account token a connection reads with.
    async fn account_token(&self, binding: &ConnectionBinding) -> Result<Secret, ProviderError> {
        if binding.profile_label.as_deref() == Some(cli::PROFILE) {
            return cli::token().await;
        }
        let stored = StoredToken::parse(&self.source.stored_key(binding).await?)?;
        Ok(Secret::new(stored.access_token))
    }

    /// The subscription, which also verifies the token.
    async fn subscription(
        &self,
        token: &Secret,
        deadline: Option<chrono::DateTime<Utc>>,
    ) -> Result<wire::SubscriptionAnswer, ProviderError> {
        let authorization = format!("Bearer {}", token.expose());
        let headers = [
            ("Authorization", authorization.as_str()),
            ("Accept", "application/json"),
            ("x-api-version", API_VERSION),
        ];
        let reply = self
            .source
            .http()
            .post(PostRequest {
                url: SUBSCRIPTION_URL,
                headers: &headers,
                body: Body::Json(&serde_json::json!({})),
                deadline,
                answers: Answers::Success,
            })
            .await?;
        let answer = <wire::SubscriptionAnswer as serde::Deserialize>::deserialize(&reply.body)
            .map_err(|_| ProviderError::UnsupportedSchema {
                detail: "the payload did not match the supported Muse Code shape".to_owned(),
            })?;
        if answer.account_id().is_none() {
            // Field names only: the answer also carries a model API key.
            let fields = field_names(&reply.body);
            tracing::warn!(?fields, "the Muse Code answer named no account");
        }
        Ok(answer)
    }

    #[tracing::instrument(
        skip_all,
        fields(provider = ProviderId::MuseCode.as_str(), connection = %binding.connection_id)
    )]
    async fn read(
        &self,
        binding: &ConnectionBinding,
        context: ReadContext,
        token: &Secret,
    ) -> Result<FetchOutcome, ProviderError> {
        let answer = self.subscription(token, context.deadline).await?;
        let account = answer.account_id();
        decode::ensure_binding(binding, ProviderId::MuseCode, account.as_deref(), None)?;
        let seed = account.unwrap_or_else(|| "muse".to_owned());
        let pool = decode::pool_id(ProviderId::MuseCode, &seed);
        let decoded = mapping::decode(&answer, &pool, Utc::now())?;
        Ok(decoded.into_outcome(identity(&answer)))
    }

    /// The account a token signs in, before anything is stored.
    async fn discover(
        &self,
        token: &Secret,
        profile: Option<String>,
        ownership: CredentialOwnership,
    ) -> Result<Vec<DiscoveredAccount>, ProviderError> {
        let answer = self.subscription(token, None).await?;
        if answer.is_subs_active == Some(false) {
            return Err(ProviderError::Authorization);
        }
        let principal = answer
            .account_id()
            .and_then(|id| ProviderPrincipalId::new(id).ok())
            .ok_or_else(decode::missing_identity)?;
        Ok(vec![DiscoveredAccount {
            pool_id: decode::pool_id(ProviderId::MuseCode, principal.as_str()),
            principal_id: Some(principal),
            workspace_id: None,
            entitlement_id: None,
            profile_label: profile,
            identity: identity(&answer),
            cardinality: AccountCardinality::Independent,
            credential_ownership: ownership,
            state: ConnectionState::Connecting,
            nickname: "Muse Code".to_owned(),
        }])
    }
}

/// The field names of a JSON object, never its values.
fn field_names(value: &serde_json::Value) -> Vec<String> {
    value
        .as_object()
        .map(|object| object.keys().cloned().collect())
        .unwrap_or_default()
}

/// What the person confirms: the Meta account and its plan.
fn identity(answer: &wire::SubscriptionAnswer) -> VerifiedIdentity {
    VerifiedIdentity {
        principal_label: answer
            .user_email
            .as_deref()
            .map_or_else(|| "Meta account".to_owned(), decode::masked_address),
        workspace_label: None,
        plan_label: answer.subs_tier_name.clone(),
        source: SourceKind::ObservedWebEndpoint,
    }
}

impl ProviderAdapter for MuseAdapter {
    fn provider_id(&self) -> ProviderId {
        ProviderId::MuseCode
    }

    fn capabilities(&self) -> ProviderCapabilities {
        capabilities()
    }

    fn policy(&self) -> ProviderPollingPolicy {
        let mut policy = keyed::policy(ProviderId::MuseCode);
        policy.strategy = PollingStrategy::FixedInterval(FixedIntervalPolicy {
            visible_seconds: MINIMUM_SECONDS,
            background_seconds: MINIMUM_SECONDS,
            battery_saver_seconds: 2 * MINIMUM_SECONDS,
            minimum_seconds: MINIMUM_SECONDS,
        });
        policy
    }

    fn discover_accounts(
        &self,
    ) -> ProviderFuture<'_, Result<Vec<DiscoveredAccount>, ProviderError>> {
        Box::pin(async move {
            let token = cli::token().await?;
            self.discover(
                &token,
                Some(cli::PROFILE.to_owned()),
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
            let token = self.account_token(&binding).await?;
            self.read(&binding, context, &token).await
        })
    }

    fn discover_with<'a>(
        &'a self,
        credential: &'a Secret,
    ) -> ProviderFuture<'a, Result<Vec<DiscoveredAccount>, ProviderError>> {
        Box::pin(async move {
            let token = Secret::new(StoredToken::parse(credential)?.access_token);
            self.discover(&token, None, CredentialOwnership::AppOwned)
                .await
        })
    }

    fn read_with<'a>(
        &'a self,
        binding: &'a ConnectionBinding,
        context: ReadContext,
        credential: &'a Secret,
    ) -> ProviderFuture<'a, Result<FetchOutcome, ProviderError>> {
        Box::pin(async move {
            let token = Secret::new(StoredToken::parse(credential)?.access_token);
            self.read(binding, context, &token).await
        })
    }

    fn begin_device_sign_in(
        &self,
    ) -> ProviderFuture<'_, Result<DeviceAuthorization, ProviderError>> {
        Box::pin(CLIENT.begin(self.source.http()))
    }

    fn poll_device_sign_in<'a>(
        &'a self,
        authorization: &'a DeviceAuthorization,
    ) -> ProviderFuture<'a, Result<DevicePoll, ProviderError>> {
        Box::pin(CLIENT.poll(self.source.http(), authorization))
    }
}

/// What this adapter declares: Quota's own browser sign-in, or the Muse CLI's.
#[must_use]
pub(crate) const fn capabilities() -> ProviderCapabilities {
    let mut declared = keyed::capabilities(ProviderId::MuseCode, true, false);
    declared.minimum_interval_seconds = MINIMUM_SECONDS;
    declared
}
