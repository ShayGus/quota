//! The Grok adapter: `SuperGrok` usage, read through the Grok CLI's billing
//! endpoint.
//!
//! Quota signs in through the browser with the device sign-in the Grok CLI
//! uses (its public client, as oh-my-pi does), and owns the resulting tokens:
//! it keeps them in the system credential store and refreshes them itself.
//! When the Grok CLI is signed in on this computer, its sign-in can be used
//! instead; that one stays the CLI's and is never refreshed. xAI's API keys
//! are a different product and are never sent here.

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
use quota_domain::polling::ProviderPollingPolicy;
use quota_domain::provider::{ProviderCapabilities, ProviderId};
use quota_domain::quota::window::SourceKind;
use tokio::sync::OnceCell;

use crate::decode;
use crate::device::{DeviceClient, StoredToken};
use crate::keyed::{self, KeyedSource};

/// The Grok CLI's public client.
const CLIENT_ID: &str = "b1a00492-073a-47ea-816f-4c329264a828";

/// The scopes the Grok CLI signs in with.
const SCOPE: &str = "openid profile email offline_access grok-cli:access api:access";

/// Where a sign-in code is requested.
const DEVICE_URL: &str = "https://auth.x.ai/oauth2/device/code";

/// Where the token endpoint is published.
const DISCOVERY_URL: &str = "https://auth.x.ai/.well-known/openid-configuration";

/// The signed-in account.
const USERINFO_URL: &str = "https://auth.x.ai/oauth2/userinfo";

/// The weekly credits.
const CREDITS_URL: &str = "https://cli-chat-proxy.grok.com/v1/billing?format=credits";

/// The monthly included quota, for an account on unified billing.
const MONTHLY_URL: &str = "https://cli-chat-proxy.grok.com/v1/billing";

/// The hosts a verification page may be on.
const PAGE_HOSTS: &[&str] = &["x.ai"];

/// The adapter for `SuperGrok`.
#[derive(Debug)]
pub(crate) struct GrokAdapter {
    source: KeyedSource,
    token_url: OnceCell<String>,
}

/// The signed-in account's identity.
#[derive(Debug, Default, serde::Deserialize)]
struct UserInfo {
    #[serde(default)]
    sub: Option<String>,
    #[serde(default)]
    email: Option<String>,
}

impl GrokAdapter {
    /// Builds the adapter over the store Quota's own tokens live in.
    pub(crate) fn new(secrets: Arc<dyn SecretStore>) -> Result<Self, ProviderError> {
        Ok(Self {
            source: KeyedSource::new(secrets)?,
            token_url: OnceCell::new(),
        })
    }

    /// The token endpoint, from xAI's discovery document, pinned to `x.ai`.
    async fn token_url(&self) -> Result<&str, ProviderError> {
        self.token_url
            .get_or_try_init(|| async {
                let reply = self
                    .source
                    .get(
                        DISCOVERY_URL,
                        &[("Accept", "application/json")],
                        &no_deadline(),
                    )
                    .await?;
                let url = reply
                    .body
                    .get("token_endpoint")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                let host = url
                    .strip_prefix("https://")
                    .and_then(|rest| rest.split('/').next())
                    .unwrap_or_default();
                if host == "x.ai" || host.ends_with(".x.ai") {
                    Ok(url)
                } else {
                    Err(ProviderError::UnsupportedSchema {
                        detail: "xAI published a token endpoint outside x.ai".to_owned(),
                    })
                }
            })
            .await
            .map(String::as_str)
    }

    async fn client(&self) -> Result<DeviceClient<'_>, ProviderError> {
        Ok(DeviceClient {
            device_url: DEVICE_URL,
            token_url: self.token_url().await?,
            client_id: CLIENT_ID,
            scope: Some(SCOPE),
            headers: &[],
            page_hosts: PAGE_HOSTS,
        })
    }

    /// The access token a connection reads with, refreshing Quota's own when
    /// it is about to expire.
    async fn access_token(&self, binding: &ConnectionBinding) -> Result<Secret, ProviderError> {
        if binding.profile_label.as_deref() == Some(cli::PROFILE) {
            return cli::token().await;
        }
        let mut token = StoredToken::parse(&self.source.stored_key(binding).await?)?;
        if token.expiring() {
            token = self
                .client()
                .await?
                .refresh(self.source.http(), &token)
                .await?;
            self.source.replace(binding, token.to_secret()?).await?;
        }
        Ok(Secret::new(token.access_token))
    }

    async fn user(&self, access: &Secret) -> Result<UserInfo, ProviderError> {
        let authorization = format!("Bearer {}", access.expose());
        let headers = [
            ("Authorization", authorization.as_str()),
            ("Accept", "application/json"),
        ];
        let reply = self
            .source
            .get(USERINFO_URL, &headers, &no_deadline())
            .await?;
        serde_json::from_value(reply.body).map_err(|_| ProviderError::UnsupportedSchema {
            detail: "the xAI account answer did not match the supported shape".to_owned(),
        })
    }

    /// One billing answer, or `None` when the endpoint has no such figures.
    async fn billing(
        &self,
        url: &str,
        access: &Secret,
        context: &ReadContext,
    ) -> Result<Option<wire::BillingConfig>, ProviderError> {
        let authorization = format!("Bearer {}", access.expose());
        let headers = [
            ("Authorization", authorization.as_str()),
            ("Accept", "application/json"),
            // The Grok CLI's own product gate on this host.
            ("X-XAI-Token-Auth", "xai-grok-cli"),
        ];
        let reply = self.source.get(url, &headers, context).await?;
        Ok(serde_json::from_value::<wire::BillingEnvelope>(reply.body)
            .ok()
            .and_then(|envelope| envelope.config))
    }

    #[tracing::instrument(
        skip_all,
        fields(provider = ProviderId::Grok.as_str(), connection = %binding.connection_id)
    )]
    async fn read(
        &self,
        binding: &ConnectionBinding,
        context: ReadContext,
        access: &Secret,
    ) -> Result<FetchOutcome, ProviderError> {
        let user = self.user(access).await?;
        decode::ensure_binding(binding, ProviderId::Grok, user.sub.as_deref(), None)?;
        let weekly = self.billing(CREDITS_URL, access, &context).await?;
        let unified = weekly
            .as_ref()
            .is_none_or(|config| config.is_unified_billing_user == Some(true));
        let monthly = if unified {
            self.billing(MONTHLY_URL, access, &context).await?
        } else {
            None
        };
        let seed = user.sub.clone().unwrap_or_else(|| "grok".to_owned());
        let pool = decode::pool_id(ProviderId::Grok, &seed);
        let decoded = mapping::decode(weekly.as_ref(), monthly.as_ref(), &pool, Utc::now())?;
        Ok(decoded.into_outcome(identity(&user)))
    }

    /// The account a token signs in, before anything is stored.
    async fn discover(
        &self,
        access: &Secret,
        profile: Option<String>,
        ownership: CredentialOwnership,
    ) -> Result<Vec<DiscoveredAccount>, ProviderError> {
        let user = self.user(access).await?;
        let principal = user
            .sub
            .clone()
            .and_then(|sub| ProviderPrincipalId::new(sub).ok())
            .ok_or_else(decode::missing_identity)?;
        Ok(vec![DiscoveredAccount {
            pool_id: decode::pool_id(ProviderId::Grok, principal.as_str()),
            principal_id: Some(principal),
            workspace_id: None,
            entitlement_id: None,
            profile_label: profile,
            identity: identity(&user),
            cardinality: AccountCardinality::Independent,
            credential_ownership: ownership,
            state: ConnectionState::Connecting,
            nickname: "Grok".to_owned(),
        }])
    }
}

/// What the person confirms: the xAI account.
fn identity(user: &UserInfo) -> VerifiedIdentity {
    VerifiedIdentity {
        principal_label: user
            .email
            .as_deref()
            .map_or_else(|| "xAI account".to_owned(), decode::masked_address),
        workspace_label: None,
        plan_label: Some("SuperGrok".to_owned()),
        source: SourceKind::ObservedWebEndpoint,
    }
}

fn no_deadline() -> ReadContext {
    keyed::verification_context()
}

impl ProviderAdapter for GrokAdapter {
    fn provider_id(&self) -> ProviderId {
        ProviderId::Grok
    }

    fn capabilities(&self) -> ProviderCapabilities {
        capabilities()
    }

    fn policy(&self) -> ProviderPollingPolicy {
        keyed::policy(ProviderId::Grok)
    }

    fn discover_accounts(
        &self,
    ) -> ProviderFuture<'_, Result<Vec<DiscoveredAccount>, ProviderError>> {
        Box::pin(async move {
            let access = cli::token().await?;
            self.discover(
                &access,
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
            let access = self.access_token(&binding).await?;
            self.read(&binding, context, &access).await
        })
    }

    fn discover_with<'a>(
        &'a self,
        credential: &'a Secret,
    ) -> ProviderFuture<'a, Result<Vec<DiscoveredAccount>, ProviderError>> {
        Box::pin(async move {
            let access = Secret::new(StoredToken::parse(credential)?.access_token);
            self.discover(&access, None, CredentialOwnership::AppOwned)
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
            let access = Secret::new(StoredToken::parse(credential)?.access_token);
            self.read(binding, context, &access).await
        })
    }

    fn begin_device_sign_in(
        &self,
    ) -> ProviderFuture<'_, Result<DeviceAuthorization, ProviderError>> {
        Box::pin(async move { self.client().await?.begin(self.source.http()).await })
    }

    fn poll_device_sign_in<'a>(
        &'a self,
        authorization: &'a DeviceAuthorization,
    ) -> ProviderFuture<'a, Result<DevicePoll, ProviderError>> {
        Box::pin(async move {
            self.client()
                .await?
                .poll(self.source.http(), authorization)
                .await
        })
    }
}

/// What this adapter declares: Quota's own browser sign-in, or the Grok CLI's.
#[must_use]
pub(crate) const fn capabilities() -> ProviderCapabilities {
    keyed::capabilities(ProviderId::Grok, true, true)
}
