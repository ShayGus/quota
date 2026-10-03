//! The Cursor adapter: the plan's usage over the billing cycle, read with the
//! sign-in the Cursor app keeps on this computer.
//!
//! Cursor's own app owns this sign-in. Quota reads it from the app's state
//! store on every read and never refreshes or changes it; see [`app`]. The
//! usage summary is read with the session cookie Cursor's own website uses,
//! built from the account and the app's access token.

pub(crate) mod app;
pub(crate) mod mapping;
pub(crate) mod wire;

use chrono::Utc;
use quota_core::ports::{
    ConnectionBinding, DiscoveredAccount, FetchOutcome, ProviderAdapter, ProviderError,
    ProviderFuture, ReadContext,
};
use quota_domain::account::{
    AccountCardinality, ConnectionState, CredentialOwnership, VerifiedIdentity,
};
use quota_domain::ids::ProviderPrincipalId;
use quota_domain::polling::ProviderPollingPolicy;
use quota_domain::provider::{ProviderCapabilities, ProviderId};
use quota_domain::quota::window::SourceKind;

use crate::decode;
use crate::http::{GetRequest, ProviderHttp};
use crate::keyed;

/// The plan's usage over the billing cycle.
const SUMMARY_URL: &str = "https://cursor.com/api/usage-summary";

/// The signed-in account.
const ME_URL: &str = "https://cursor.com/api/auth/me";

/// The adapter for Cursor.
#[derive(Debug)]
pub(crate) struct CursorAdapter {
    http: ProviderHttp,
}

impl CursorAdapter {
    /// Builds the adapter and its HTTP client.
    pub(crate) fn new() -> Result<Self, ProviderError> {
        Ok(Self {
            http: ProviderHttp::new()?,
        })
    }

    /// One authorized GET with the session cookie.
    async fn get<T: serde::de::DeserializeOwned + Default>(
        &self,
        url: &str,
        sign_in: &app::AppSignIn,
        context: &ReadContext,
    ) -> Result<T, ProviderError> {
        let cookie = format!(
            "WorkosCursorSessionToken={}%3A%3A{}",
            sign_in.user_id,
            sign_in.token.expose()
        );
        let headers = [("Cookie", cookie.as_str()), ("Accept", "application/json")];
        let reply = self
            .http
            .get(GetRequest {
                url,
                headers: &headers,
                deadline: context.deadline,
            })
            .await?;
        serde_json::from_value(reply.body).map_err(|_| ProviderError::UnsupportedSchema {
            detail: "the payload did not match the supported Cursor shape".to_owned(),
        })
    }

    #[tracing::instrument(
        skip_all,
        fields(provider = ProviderId::Cursor.as_str(), connection = %binding.connection_id)
    )]
    async fn read(
        &self,
        binding: &ConnectionBinding,
        context: ReadContext,
    ) -> Result<FetchOutcome, ProviderError> {
        let sign_in = app::sign_in().await?;
        decode::ensure_binding(
            binding,
            ProviderId::Cursor,
            Some(&sign_in.user_id),
            Some(app::PROFILE),
        )?;
        let summary: wire::UsageSummary = self.get(SUMMARY_URL, &sign_in, &context).await?;
        let me: wire::Me = self
            .get(ME_URL, &sign_in, &context)
            .await
            .unwrap_or_default();
        let pool = decode::pool_id(ProviderId::Cursor, &sign_in.user_id);
        let decoded = mapping::decode(&summary, &pool, Utc::now())?;
        let plan = decoded.plan_label.clone();
        Ok(decoded.into_outcome(identity(&me, plan)))
    }
}

/// What the person confirms: the Cursor account and its plan.
fn identity(me: &wire::Me, plan: Option<String>) -> VerifiedIdentity {
    VerifiedIdentity {
        principal_label: me
            .email
            .as_deref()
            .map_or_else(|| "Cursor account".to_owned(), decode::masked_address),
        workspace_label: None,
        plan_label: plan,
        source: SourceKind::ObservedWebEndpoint,
    }
}

impl ProviderAdapter for CursorAdapter {
    fn provider_id(&self) -> ProviderId {
        ProviderId::Cursor
    }

    fn capabilities(&self) -> ProviderCapabilities {
        capabilities()
    }

    fn policy(&self) -> ProviderPollingPolicy {
        keyed::policy(ProviderId::Cursor)
    }

    fn discover_accounts(
        &self,
    ) -> ProviderFuture<'_, Result<Vec<DiscoveredAccount>, ProviderError>> {
        Box::pin(async move {
            let sign_in = app::sign_in().await?;
            let context = keyed::verification_context();
            let summary: wire::UsageSummary = self.get(SUMMARY_URL, &sign_in, &context).await?;
            let me: wire::Me = self
                .get(ME_URL, &sign_in, &context)
                .await
                .unwrap_or_default();
            let principal = ProviderPrincipalId::new(sign_in.user_id.clone())
                .map_err(|_| decode::missing_identity())?;
            let plan = summary
                .membership_type
                .as_deref()
                .map(str::trim)
                .filter(|plan| !plan.is_empty())
                .map(mapping::plan_label);
            Ok(vec![DiscoveredAccount {
                pool_id: decode::pool_id(ProviderId::Cursor, &sign_in.user_id),
                principal_id: Some(principal),
                workspace_id: None,
                entitlement_id: None,
                profile_label: Some(app::PROFILE.to_owned()),
                identity: identity(&me, plan),
                cardinality: AccountCardinality::SingleProfile,
                credential_ownership: CredentialOwnership::ExternalClient,
                state: ConnectionState::Connecting,
                nickname: "Cursor".to_owned(),
            }])
        })
    }

    fn read_quota(
        &self,
        binding: &ConnectionBinding,
        context: ReadContext,
    ) -> ProviderFuture<'_, Result<FetchOutcome, ProviderError>> {
        let binding = binding.clone();
        Box::pin(async move { self.read(&binding, context).await })
    }
}

/// What this adapter declares: the Cursor app's own sign-in, one account.
#[must_use]
pub(crate) const fn capabilities() -> ProviderCapabilities {
    ProviderCapabilities {
        provider_id: ProviderId::Cursor,
        cardinality: AccountCardinality::SingleProfile,
        supports_app_owned_authorization: false,
        supports_external_profile: true,
        reports_monthly_window: true,
        minimum_interval_seconds: 300,
    }
}
