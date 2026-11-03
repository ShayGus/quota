//! The `TypeSafe` adapter: the credit balance on the `TypeSafe` console.
//!
//! `TypeSafe` sells credit for its models and documents no API for billing; an
//! API key cannot read it. The console's billing page can, so Quota signs in
//! to the console itself, in a browser profile of its own kept apart from
//! every other browser, and keeps the session it ends up with as the
//! credential, in the system credential store. Each read asks the endpoint the
//! billing page itself reads, `GET /api/billing/overview`, for the overview.
//! The answer also carries the billing address, the invoice address, the
//! payment card, the tax identifier and the payment history; none of them is
//! read (`wire`).
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

use std::sync::Arc;

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

/// What the console's billing page reads: the balance, spend and credit.
const OVERVIEW_URL: &str = "https://console.typesafe.ai/api/billing/overview";

/// The console's sign-in, returning to the billing page.
const SIGN_IN_URL: &str = "https://console.typesafe.ai/login?returnTo=%2Fbilling";

/// The address whose cookies are the session.
const COOKIE_URL: &str = "https://console.typesafe.ai/";

/// The shortest interval between reads: a website, so read it sparingly.
const MINIMUM_SECONDS: u32 = 600;

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
    overview_url: String,
    secrets: Arc<dyn SecretStore>,
}

impl TypesafeAdapter {
    /// Builds the adapter over the store its sessions live in.
    pub(crate) fn new(secrets: Arc<dyn SecretStore>) -> Result<Self, ProviderError> {
        Self::at(secrets, (*crate::http::endpoint(OVERVIEW_URL)).to_owned())
    }

    /// Builds the adapter for a billing overview at another address, for tests.
    fn at(secrets: Arc<dyn SecretStore>, overview_url: String) -> Result<Self, ProviderError> {
        Ok(Self {
            client: transport::client()?,
            overview_url,
            secrets,
        })
    }

    /// The billing summary the session reads, and the session header with
    /// any cookie the console renewed, when it renewed one.
    async fn billing(
        &self,
        session: &Secret,
    ) -> Result<(wire::Billing, Option<String>), ProviderError> {
        let cookie = session::checked(session)?;
        let answer = transport::send(
            self.client
                .get(self.overview_url.clone())
                .header("Cookie", cookie)
                .header("Accept", "application/json"),
        )
        .await?;
        transport::check(&answer)?;
        let overview: wire::Overview =
            serde_json::from_str(&answer.text).map_err(|_| ProviderError::UnsupportedSchema {
                detail: "the TypeSafe billing overview was not the JSON Quota knows".to_owned(),
            })?;
        let billing = overview
            .billing
            .ok_or_else(|| ProviderError::UnsupportedSchema {
                detail: "the TypeSafe billing overview carried no billing summary".to_owned(),
            })?;
        Ok((billing, session::renewed(cookie, &answer.set_cookies)))
    }

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
