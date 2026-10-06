//! The `TypeSafe` adapter: the credit balance on the `TypeSafe` console.
//!
//! `TypeSafe` sells credit for its models and documents no API for billing; an
//! API key cannot read it. The console's billing page can, so Quota signs in
//! to the console itself, in a window of its own whose browser storage is kept
//! apart from every other browser, and keeps the session that window ends up
//! with as the credential, in the system credential store. Each read asks the
//! billing page's server action for the overview, finding the action's
//! identifier in the page's scripts as `CodexBar` does. The answer also carries
//! the billing address, the invoice address, the payment card and the tax
//! identifier; none of them is read (`wire`).
//!
//! This is an undocumented website, so a redesign can break it; a failure is
//! reported, never a guessed number. A bot check is reported as
//! [`ProviderError::Blocked`] and never passed: Quota does not bypass
//! challenges.

pub(crate) mod mapping;
pub(crate) mod page;
pub(crate) mod session;
#[cfg(test)]
mod tests;
pub(crate) mod transport;
pub(crate) mod wire;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::Utc;
use quota_core::ports::{
    ConnectionBinding, ConsoleSignIn, DiscoveredAccount, FetchOutcome, ProviderAdapter,
    ProviderError, ProviderFuture, ReadContext, Secret, SecretStore,
};
use quota_domain::account::{
    AccountCardinality, ConnectionState, CredentialOwnership, VerifiedIdentity,
};
use quota_domain::polling::{FixedIntervalPolicy, PollingStrategy, ProviderPollingPolicy};
use quota_domain::provider::{ProviderCapabilities, ProviderId};
use quota_domain::quota::window::SourceKind;

use crate::decode;
use crate::typesafe::transport::TextReply;

/// The console's billing page, which also answers the billing action.
const BILLING_URL: &str = "https://console.typesafe.ai/settings/billing";

/// The console's sign-in, returning to the billing page.
const SIGN_IN_URL: &str = "https://console.typesafe.ai/login?returnTo=%2Fsettings%2Fbilling";

/// The address whose cookies are the session.
const COOKIE_URL: &str = "https://console.typesafe.ai/";

/// How long a found action identifier is reused before it is looked up again.
const ACTION_LIFETIME: Duration = Duration::from_secs(12 * 60 * 60);

/// The shortest interval between reads: a website, so read it sparingly.
const MINIMUM_SECONDS: u32 = 600;

/// How many scripts are fetched at once while looking for the action.
const SCRIPT_BATCH: usize = 8;

/// What the person confirms: the console never names the account without
/// personal data, so the account is the console sign-in itself.
const ACCOUNT_LABEL: &str = "TypeSafe console";

/// The pool every `TypeSafe` reading belongs to. The console reports no
/// identifier that is not personal, so one console account is supported.
const POOL_SEED: &str = "console";

/// The adapter for the `TypeSafe` credit balance.
#[derive(Debug)]
pub(crate) struct TypesafeAdapter {
    client: reqwest::Client,
    billing_url: String,
    secrets: Arc<dyn SecretStore>,
    action: Mutex<Option<(String, Instant)>>,
}

impl TypesafeAdapter {
    /// Builds the adapter over the store its sessions live in.
    pub(crate) fn new(secrets: Arc<dyn SecretStore>) -> Result<Self, ProviderError> {
        Self::at(secrets, (*crate::http::endpoint(BILLING_URL)).to_owned())
    }

    /// Builds the adapter for a billing page at another address, for tests.
    fn at(secrets: Arc<dyn SecretStore>, billing_url: String) -> Result<Self, ProviderError> {
        Ok(Self {
            client: transport::client()?,
            billing_url,
            secrets,
            action: Mutex::new(None),
        })
    }

    /// The origin the billing page is served from.
    fn origin(&self) -> String {
        let url = &self.billing_url;
        let after_scheme = url.find("://").map_or(0, |at| at + 3);
        let path = url
            .get(after_scheme..)
            .and_then(|rest| rest.find('/'))
            .map_or(url.len(), |at| after_scheme + at);
        url.get(..path).unwrap_or_default().to_owned()
    }

    /// The cached action identifier, while it is fresh.
    fn cached_action(&self) -> Option<String> {
        let guard = self.action.lock().ok()?;
        guard
            .as_ref()
            .filter(|(_, found)| found.elapsed() < ACTION_LIFETIME)
            .map(|(id, _)| id.clone())
    }

    fn remember_action(&self, id: Option<String>) {
        if let Ok(mut guard) = self.action.lock() {
            *guard = id.map(|id| (id, Instant::now()));
        }
    }

    /// Finds the billing action's identifier in the billing page's scripts.
    async fn find_action(
        &self,
        cookie: &str,
        set_cookies: &mut Vec<String>,
    ) -> Result<String, ProviderError> {
        let page = transport::send(
            self.client
                .get(self.billing_url.clone())
                .header("Cookie", cookie)
                .header("Accept", "text/html"),
        )
        .await?;
        set_cookies.extend(page.set_cookies.iter().cloned());
        transport::check(&page)?;
        let sources = page::script_sources(&page.text, &self.origin());
        let mut failure: Option<ProviderError> = None;
        for batch in sources.chunks(SCRIPT_BATCH) {
            let mut fetches = tokio::task::JoinSet::new();
            for url in batch {
                let request = self.client.get(url.clone());
                fetches.spawn(async move { transport::send(request).await });
            }
            while let Some(joined) = fetches.join_next().await {
                match joined {
                    Ok(Ok(script)) if (200..300).contains(&script.status) => {
                        if let Some(id) = page::action_id(&script.text) {
                            fetches.abort_all();
                            return Ok(id);
                        }
                    }
                    Ok(Ok(script)) if script.status == 429 || script.status >= 500 => {
                        if failure.is_none() {
                            failure = transport::check(&script).err();
                        }
                    }
                    Ok(Err(error)) => {
                        failure.get_or_insert(error);
                    }
                    // A script that is not there, or a task that stopped, names no action.
                    Ok(Ok(_)) | Err(_) => {}
                }
            }
        }
        Err(failure.unwrap_or_else(|| ProviderError::UnsupportedSchema {
            detail: "the TypeSafe billing page no longer names its billing action".to_owned(),
        }))
    }

    /// Calls the billing action once.
    async fn call_action(&self, cookie: &str, id: &str) -> Result<TextReply, ProviderError> {
        transport::send(
            self.client
                .post(self.billing_url.clone())
                .header("Cookie", cookie)
                .header("Origin", self.origin())
                .header("Next-Action", id)
                .header("Accept", "text/x-component")
                .header("Content-Type", "application/json")
                .body("[]"),
        )
        .await
    }

    /// The billing overview for one session, and the session header after any
    /// cookie the console renewed while answering.
    async fn billing(
        &self,
        session: &Secret,
    ) -> Result<(wire::Billing, Option<String>), ProviderError> {
        let cookie = session::checked(session)?;
        let mut set_cookies = Vec::new();
        let (id, found_now) = match self.cached_action() {
            Some(id) => (id, false),
            None => (self.find_action(cookie, &mut set_cookies).await?, true),
        };
        let mut answer = self.call_action(cookie, &id).await?;
        let id = if answer.status == 404 && answer.action_not_found && !found_now {
            // The console was redeployed: look the action up again, once.
            self.remember_action(None);
            let id = self.find_action(cookie, &mut set_cookies).await?;
            answer = self.call_action(cookie, &id).await?;
            id
        } else {
            id
        };
        set_cookies.extend(answer.set_cookies.iter().cloned());
        if answer.status == 404 && answer.action_not_found {
            self.remember_action(None);
            return Err(ProviderError::UnsupportedSchema {
                detail: "the TypeSafe console does not know its billing action".to_owned(),
            });
        }
        transport::check(&answer)?;
        self.remember_action(Some(id));
        let result: wire::BillingResult = page::action_result(&answer.text)
            .and_then(|value| serde_json::from_value(value).ok())
            .ok_or_else(|| ProviderError::UnsupportedSchema {
                detail: "the TypeSafe billing answer carried no result".to_owned(),
            })?;
        if result.ok != Some(true) {
            return Err(ProviderError::InvalidData {
                detail: "the TypeSafe console did not answer the billing request".to_owned(),
            });
        }
        let billing = result.data.and_then(|data| data.billing).ok_or_else(|| {
            ProviderError::UnsupportedSchema {
                detail: "the TypeSafe billing answer carried no billing summary".to_owned(),
            }
        })?;
        Ok((billing, session::renewed(cookie, &set_cookies)))
    }

    /// Performs one read with the given session.
    #[tracing::instrument(
        skip_all,
        fields(provider = ProviderId::Typesafe.as_str(), connection = %binding.connection_id)
    )]
    async fn read(
        &self,
        binding: &ConnectionBinding,
        session: &Secret,
        keep_renewed: bool,
    ) -> Result<FetchOutcome, ProviderError> {
        decode::ensure_binding(binding, ProviderId::Typesafe, None, None)?;
        let (billing, renewed) = self.billing(session).await?;
        if keep_renewed && let Some(renewed) = renewed {
            // A renewed session replaces the stored one; failing to keep it
            // only means the next read uses the older cookie.
            if self
                .secrets
                .write(&binding.connection_id, &Secret::new(renewed))
                .await
                .is_err()
            {
                tracing::warn!("a renewed TypeSafe session could not be kept");
            }
        }
        let pool = decode::pool_id(ProviderId::Typesafe, POOL_SEED);
        let decoded = mapping::decode(&billing, &pool, Utc::now())?;
        let plan = decoded.plan_label.clone();
        Ok(decoded.into_outcome(identity(plan)))
    }
}

/// What the person confirms: the console sign-in and its plan.
fn identity(plan: Option<String>) -> VerifiedIdentity {
    VerifiedIdentity {
        principal_label: ACCOUNT_LABEL.to_owned(),
        workspace_label: None,
        plan_label: plan,
        source: SourceKind::ObservedWebEndpoint,
    }
}

impl ProviderAdapter for TypesafeAdapter {
    fn provider_id(&self) -> ProviderId {
        ProviderId::Typesafe
    }

    fn capabilities(&self) -> ProviderCapabilities {
        capabilities()
    }

    fn policy(&self) -> ProviderPollingPolicy {
        let mut policy = crate::keyed::policy(ProviderId::Typesafe);
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
        // There is no local sign-in to find: Quota signs in to the console.
        Box::pin(async { Ok(Vec::new()) })
    }

    fn read_quota(
        &self,
        binding: &ConnectionBinding,
        context: ReadContext,
    ) -> ProviderFuture<'_, Result<FetchOutcome, ProviderError>> {
        let _ = context;
        let binding = binding.clone();
        Box::pin(async move {
            let session = self
                .secrets
                .read(&binding.connection_id)
                .await
                .map_err(|_| ProviderError::Transient {
                    detail: "the credential store is unavailable".to_owned(),
                })?
                .ok_or(ProviderError::Authentication)?;
            self.read(&binding, &session, true).await
        })
    }

    fn discover_with<'a>(
        &'a self,
        credential: &'a Secret,
    ) -> ProviderFuture<'a, Result<Vec<DiscoveredAccount>, ProviderError>> {
        Box::pin(async move {
            let (billing, _) = self.billing(credential).await?;
            Ok(vec![DiscoveredAccount {
                principal_id: None,
                workspace_id: None,
                entitlement_id: None,
                pool_id: decode::pool_id(ProviderId::Typesafe, POOL_SEED),
                profile_label: None,
                identity: identity(billing.plan.as_deref().and_then(mapping::plan_label)),
                cardinality: AccountCardinality::Independent,
                credential_ownership: CredentialOwnership::AppOwned,
                state: ConnectionState::Connecting,
                nickname: "TypeSafe".to_owned(),
            }])
        })
    }

    fn read_with<'a>(
        &'a self,
        binding: &'a ConnectionBinding,
        context: ReadContext,
        credential: &'a Secret,
    ) -> ProviderFuture<'a, Result<FetchOutcome, ProviderError>> {
        let _ = context;
        Box::pin(self.read(binding, credential, false))
    }

    fn console_sign_in(&self) -> Option<ConsoleSignIn> {
        Some(ConsoleSignIn {
            sign_in_url: SIGN_IN_URL,
            cookie_url: COOKIE_URL,
        })
    }
}

/// What this adapter declares: Quota's own console sign-in, nothing local.
#[must_use]
pub(crate) const fn capabilities() -> ProviderCapabilities {
    let mut declared = crate::keyed::capabilities(ProviderId::Typesafe, false, false);
    declared.minimum_interval_seconds = MINIMUM_SECONDS;
    declared
}
