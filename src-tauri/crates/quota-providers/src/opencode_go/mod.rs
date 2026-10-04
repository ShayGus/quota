//! The `OpenCode` Go adapter: a bearer-key read and normalisation.
//!
//! This endpoint reports no account, workspace, or entitlement identity. This
//! adapter therefore does not invent one: it uses a stable local pool derived
//! from the credential profile, and leaves the optional principal, workspace,
//! and entitlement fields empty so the account reads as unverified. That is an
//! accepted, honest limitation of this connector, not a defect to paper over.
//!
//! The sign-in is either a key the person pasted, which Quota owns and keeps in
//! the system credential store: see [`crate::keyed`]. Or it is the `OpenCode`
//! login's own key, which this adapter reads from its file on every read and
//! never refreshes. A connection's profile label says which one it uses.

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

use crate::credentials;
use crate::decode::{self, DecodedUsage};
use crate::keyed::{self, KeyedSource};

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

/// What the account reads as when the sign-in is a pasted key: the usage
/// response carries no identity, so only the kind of sign-in is honest here.
const KEY_PRINCIPAL: &str = "OpenCode Go API key";

/// The adapter for `OpenCode` Go usage.
#[derive(Debug)]
pub(crate) struct OpenCodeGoAdapter {
    source: KeyedSource,
}

impl OpenCodeGoAdapter {
    /// Builds the adapter over the store pasted keys live in.
    pub(crate) fn new(secrets: Arc<dyn SecretStore>) -> Result<Self, ProviderError> {
        Ok(Self {
            source: KeyedSource::new(secrets)?,
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
            provider = ProviderId::OpenCodeGo.as_str(),
            connection = %binding.connection_id,
            profile = binding.profile_label.as_deref().unwrap_or("default"),
        )
    )]
    async fn perform_read(
        &self,
        binding: ConnectionBinding,
        context: ReadContext,
        key: &str,
        profile_label: &str,
        principal_label: &str,
    ) -> Result<FetchOutcome, ProviderError> {
        let pool = decode::pool_id(ProviderId::OpenCodeGo, profile_label);
        // There is no provider-reported identity, so the check covers
        // what exists: the provider and the credential profile.
        decode::ensure_binding(&binding, ProviderId::OpenCodeGo, None, Some(profile_label))?;
        let authorization = format!("Bearer {key}");
        let headers = [
            ("Authorization", authorization.as_str()),
            ("Accept", "application/json"),
        ];
        let reply = self.source.get(USAGE_URL, &headers, &context).await?;
        let envelope: wire::OpenCodeGoEnvelope =
            serde_json::from_value(reply.body).map_err(|_| ProviderError::UnsupportedSchema {
                detail: "the payload did not match the supported OpenCode Go shape".to_owned(),
            })?;
        let decoded: DecodedUsage = mapping::decode(&envelope, &pool, Utc::now())?;
        let identity = VerifiedIdentity {
            principal_label: principal_label.to_owned(),
            workspace_label: None,
            plan_label: None,
            source: SourceKind::ObservedWebEndpoint,
        };
        Ok(decoded.into_outcome(identity))
    }

    /// Reads through the `OpenCode` login's own key, as this adapter always has.
    async fn read_local(
        &self,
        binding: ConnectionBinding,
        context: ReadContext,
    ) -> Result<FetchOutcome, ProviderError> {
        let credential = credentials::opencode_go_credential().await?;
        let profile_label = credential.profile_label.as_str();
        let principal_label = format!("OpenCode Go ({profile_label})");
        self.perform_read(
            binding,
            context,
            credential.key.expose(),
            profile_label,
            &principal_label,
        )
        .await
    }

    /// Reads through the key the person pasted, which the store holds under
    /// this connection.
    async fn read_keyed(
        &self,
        binding: ConnectionBinding,
        context: ReadContext,
    ) -> Result<FetchOutcome, ProviderError> {
        let key = self.source.stored_key(&binding).await?;
        let profile_label = keyed::profile(&key);
        self.perform_read(
            binding,
            context,
            key.expose(),
            &profile_label,
            KEY_PRINCIPAL,
        )
        .await
    }
}

/// The one account a sign-in offers: the usage response carries no identity,
/// so none is fabricated here.
fn account(
    profile_label: &str,
    principal_label: String,
    credential_ownership: CredentialOwnership,
) -> DiscoveredAccount {
    DiscoveredAccount {
        principal_id: None,
        workspace_id: None,
        entitlement_id: None,
        profile_label: Some(profile_label.to_owned()),
        pool_id: decode::pool_id(ProviderId::OpenCodeGo, profile_label),
        identity: VerifiedIdentity {
            // The honest label is the credential profile or the kind of
            // sign-in, not an account the provider never reported.
            principal_label,
            workspace_label: None,
            plan_label: None,
            source: SourceKind::ObservedWebEndpoint,
        },
        cardinality: AccountCardinality::Independent,
        credential_ownership,
        state: ConnectionState::Connecting,
        nickname: "OpenCode Go".to_owned(),
    }
}

impl ProviderAdapter for OpenCodeGoAdapter {
    fn provider_id(&self) -> ProviderId {
        ProviderId::OpenCodeGo
    }

    fn capabilities(&self) -> ProviderCapabilities {
        capabilities()
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
            Ok(vec![account(
                &profile_label,
                format!("OpenCode Go ({profile_label})"),
                CredentialOwnership::ExternalClient,
            )])
        })
    }

    fn read_quota(
        &self,
        binding: &ConnectionBinding,
        context: ReadContext,
    ) -> ProviderFuture<'_, Result<FetchOutcome, ProviderError>> {
        let binding = binding.clone();
        Box::pin(async move {
            // A local `OpenCode` profile reads the login's own file; any other
            // profile is a pasted key, whose store entry is the credential.
            if binding
                .profile_label
                .as_deref()
                .is_some_and(credentials::opencode_go_local_profile)
            {
                return self.read_local(binding, context).await;
            }
            self.read_keyed(binding, context).await
        })
    }

    fn discover_with<'a>(
        &'a self,
        credential: &'a Secret,
    ) -> ProviderFuture<'a, Result<Vec<DiscoveredAccount>, ProviderError>> {
        let profile_label = keyed::profile(credential);
        Box::pin(async move {
            Ok(vec![account(
                &profile_label,
                KEY_PRINCIPAL.to_owned(),
                CredentialOwnership::AppOwned,
            )])
        })
    }

    fn read_with<'a>(
        &'a self,
        binding: &'a ConnectionBinding,
        context: ReadContext,
        credential: &'a Secret,
    ) -> ProviderFuture<'a, Result<FetchOutcome, ProviderError>> {
        let binding = binding.clone();
        let profile_label = keyed::profile(credential);
        Box::pin(async move {
            self.perform_read(
                binding,
                context,
                credential.expose(),
                &profile_label,
                KEY_PRINCIPAL,
            )
            .await
        })
    }
}

/// What this adapter declares: a pasted key, or the `OpenCode` login's own
/// sign-in when no key is given.
#[must_use]
pub(crate) const fn capabilities() -> ProviderCapabilities {
    keyed::capabilities(ProviderId::OpenCodeGo, true, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pasted key is Quota's, and the `OpenCode` login's sign-in still stands
    /// in for one, so the key is offered and never forced.
    #[test]
    fn it_takes_a_pasted_key_and_the_local_sign_in_alike() {
        let capabilities = capabilities();
        assert!(capabilities.supports_app_owned_authorization);
        assert!(capabilities.supports_external_profile);
        assert!(capabilities.reports_monthly_window);
        assert_eq!(capabilities.cardinality, AccountCardinality::Independent);
        assert_eq!(capabilities.minimum_interval_seconds, MINIMUM_SECONDS);
    }

    /// The account a pasted key offers is Quota's own, keyed by the fingerprint
    /// that tells two keys apart without keeping either.
    #[test]
    fn a_pasted_key_offers_an_app_owned_account() {
        let key = Secret::new("opencode-go-key".to_owned());
        let profile_label = keyed::profile(&key);
        let found = account(
            &profile_label,
            KEY_PRINCIPAL.to_owned(),
            CredentialOwnership::AppOwned,
        );
        assert_eq!(found.credential_ownership, CredentialOwnership::AppOwned);
        assert_eq!(found.profile_label.as_deref(), Some(profile_label.as_str()));
        assert_eq!(found.identity.principal_label, KEY_PRINCIPAL);
        assert!(found.principal_id.is_none());
    }
}
