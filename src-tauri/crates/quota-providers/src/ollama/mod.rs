//! The Ollama Cloud adapter: the plan's usage from the endpoints behind
//! Ollama's own settings page.
//!
//! Two sign-ins reach them. Ollama's own: the Ed25519 key Ollama keeps, which
//! `ollama signin` links to the account; Quota signs each request with it as
//! the Ollama CLI does, and never changes the key (see [`key`]). Or an API key
//! from ollama.com/settings/keys, which Quota owns and keeps in the system
//! credential store, sent as a bearer token. A connection's profile label says
//! which one it uses.

pub(crate) mod key;
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
use crate::http::Answers;
use crate::keyed::{self, KeyedSource};
use crate::post::{Body, PostRequest};

/// The site the endpoints are on.
const HOST: &str = "https://ollama.com";

/// The plan's usage.
const USAGE_PATH: &str = "/api/usage";

/// The account and its plan.
const ACCOUNT_PATH: &str = "/api/me";

/// How a request is authorized.
enum Signer {
    /// Ollama's own key, signing each request.
    Key(key::SigningKey),
    /// An API key Quota owns.
    Bearer(Secret),
}

impl Signer {
    fn authorization(&self, method: &str, request_uri: &str) -> String {
        match self {
            Self::Key(key) => key.authorization(method, request_uri),
            Self::Bearer(secret) => format!("Bearer {}", secret.expose()),
        }
    }

    fn profile(&self) -> String {
        match self {
            Self::Key(_) => key::PROFILE.to_owned(),
            Self::Bearer(secret) => keyed::profile(secret),
        }
    }
}

/// The adapter for Ollama Cloud.
#[derive(Debug)]
pub(crate) struct OllamaAdapter {
    source: KeyedSource,
}

impl OllamaAdapter {
    /// Builds the adapter over the store pasted keys live in.
    pub(crate) fn new(secrets: Arc<dyn SecretStore>) -> Result<Self, ProviderError> {
        Ok(Self {
            source: KeyedSource::new(secrets)?,
        })
    }

    /// The request line a signature covers: the path and a fresh timestamp.
    fn request_uri(path: &str) -> String {
        format!("{path}?ts={}", Utc::now().timestamp())
    }

    async fn usage(
        &self,
        signer: &Signer,
        context: &ReadContext,
    ) -> Result<wire::UsageBody, ProviderError> {
        let uri = Self::request_uri(USAGE_PATH);
        let authorization = signer.authorization("GET", &uri);
        let headers = [
            ("Authorization", authorization.as_str()),
            ("Accept", "application/json"),
        ];
        let reply = self
            .source
            .get(&format!("{HOST}{uri}"), &headers, context)
            .await?;
        serde_json::from_value(reply.body).map_err(|_| ProviderError::UnsupportedSchema {
            detail: "the payload did not match the supported Ollama usage shape".to_owned(),
        })
    }

    /// The account, best-effort: a failure leaves the identity plain, never
    /// the reading empty.
    async fn account(&self, signer: &Signer, context: &ReadContext) -> wire::Account {
        let uri = Self::request_uri(ACCOUNT_PATH);
        let authorization = signer.authorization("POST", &uri);
        let headers = [
            ("Authorization", authorization.as_str()),
            ("Accept", "application/json"),
        ];
        let reply = self
            .source
            .http()
            .post(PostRequest {
                url: &format!("{HOST}{uri}"),
                headers: &headers,
                body: Body::Json(&serde_json::json!({})),
                deadline: context.deadline,
                answers: Answers::Success,
            })
            .await;
        reply
            .ok()
            .and_then(|reply| serde_json::from_value(reply.body).ok())
            .unwrap_or_default()
    }

    /// The signer a connection reads with.
    async fn signer(&self, binding: &ConnectionBinding) -> Result<Signer, ProviderError> {
        if binding.profile_label.as_deref() == Some(key::PROFILE) {
            return key::load().await.map(Signer::Key);
        }
        self.source.stored_key(binding).await.map(Signer::Bearer)
    }

    #[tracing::instrument(
        skip_all,
        fields(provider = ProviderId::OllamaCloud.as_str(), connection = %binding.connection_id)
    )]
    async fn read(
        &self,
        binding: &ConnectionBinding,
        context: ReadContext,
        signer: &Signer,
    ) -> Result<FetchOutcome, ProviderError> {
        let profile = signer.profile();
        decode::ensure_binding(binding, ProviderId::OllamaCloud, None, Some(&profile))?;
        let usage = self.usage(signer, &context).await?;
        let account = self.account(signer, &context).await;
        let pool = decode::pool_id(ProviderId::OllamaCloud, &profile);
        let decoded = mapping::decode(&usage, &pool, Utc::now())?;
        Ok(decoded.into_outcome(identity(signer, &account)))
    }

    /// The account a signer reaches, before anything is stored.
    async fn discover(
        &self,
        signer: &Signer,
        ownership: CredentialOwnership,
    ) -> Result<Vec<DiscoveredAccount>, ProviderError> {
        let context = keyed::verification_context();
        // Reading the usage is what proves the sign-in works.
        self.usage(signer, &context).await?;
        let account = self.account(signer, &context).await;
        let profile = signer.profile();
        Ok(vec![DiscoveredAccount {
            principal_id: None,
            workspace_id: None,
            entitlement_id: None,
            pool_id: decode::pool_id(ProviderId::OllamaCloud, &profile),
            profile_label: Some(profile),
            identity: identity(signer, &account),
            cardinality: AccountCardinality::Independent,
            credential_ownership: ownership,
            state: ConnectionState::Connecting,
            nickname: "Ollama Cloud".to_owned(),
        }])
    }
}

/// What the person confirms: the account, and which sign-in reaches it.
fn identity(signer: &Signer, account: &wire::Account) -> VerifiedIdentity {
    let fallback = match signer {
        Signer::Key(_) => "Ollama sign-in",
        Signer::Bearer(_) => "Ollama API key",
    };
    VerifiedIdentity {
        principal_label: account
            .email
            .as_deref()
            .map_or_else(|| fallback.to_owned(), decode::masked_address),
        workspace_label: None,
        plan_label: account.plan.as_deref().and_then(mapping::plan_label),
        source: SourceKind::ObservedWebEndpoint,
    }
}

impl ProviderAdapter for OllamaAdapter {
    fn provider_id(&self) -> ProviderId {
        ProviderId::OllamaCloud
    }

    fn capabilities(&self) -> ProviderCapabilities {
        capabilities()
    }

    fn policy(&self) -> ProviderPollingPolicy {
        keyed::policy(ProviderId::OllamaCloud)
    }

    fn discover_accounts(
        &self,
    ) -> ProviderFuture<'_, Result<Vec<DiscoveredAccount>, ProviderError>> {
        Box::pin(async move {
            let signer = Signer::Key(key::load().await?);
            self.discover(&signer, CredentialOwnership::ExternalClient)
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
            let signer = self.signer(&binding).await?;
            self.read(&binding, context, &signer).await
        })
    }

    fn discover_with<'a>(
        &'a self,
        credential: &'a Secret,
    ) -> ProviderFuture<'a, Result<Vec<DiscoveredAccount>, ProviderError>> {
        Box::pin(async move {
            let signer = Signer::Bearer(Secret::new(credential.expose().to_owned()));
            self.discover(&signer, CredentialOwnership::AppOwned).await
        })
    }

    fn read_with<'a>(
        &'a self,
        binding: &'a ConnectionBinding,
        context: ReadContext,
        credential: &'a Secret,
    ) -> ProviderFuture<'a, Result<FetchOutcome, ProviderError>> {
        Box::pin(async move {
            let signer = Signer::Bearer(Secret::new(credential.expose().to_owned()));
            self.read(binding, context, &signer).await
        })
    }
}

/// What this adapter declares: a pasted key, or Ollama's own sign-in.
#[must_use]
pub(crate) const fn capabilities() -> ProviderCapabilities {
    keyed::capabilities(ProviderId::OllamaCloud, true, true)
}
