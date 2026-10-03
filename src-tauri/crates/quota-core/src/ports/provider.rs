//! What the core needs from a provider adapter.

use std::pin::Pin;

use chrono::{DateTime, Utc};

use quota_domain::account::{
    AccountCardinality, ConnectionState, CredentialOwnership, VerifiedIdentity,
};
use quota_domain::ids::{
    ConnectionAttemptId, ConnectionId, EntitlementId, ProviderPrincipalId, QuotaPoolId, WorkspaceId,
};
use quota_domain::polling::ProviderPollingPolicy;
use quota_domain::provider::{ProviderCapabilities, ProviderId};
use quota_domain::quota::window::QuotaWindow;

/// A boxed, cancellable provider future.
///
/// The trait stays object-safe so adapters can live in a runtime registry.
pub type ProviderFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Why a read failed, and what the scheduler is allowed to do next.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ProviderError {
    /// A temporary failure. Eligible for jittered backoff.
    #[error("transient failure: {detail}")]
    Transient {
        /// A sanitized, non-identifying description.
        detail: String,
    },
    /// The provider asked for less traffic.
    #[error("rate limited until {retry_after:?}")]
    RateLimited {
        /// The provider's own deadline, when it sent one.
        retry_after: Option<DateTime<Utc>>,
    },
    /// The credential must be renewed by the user.
    #[error("the credential must be reauthorized")]
    Authentication,
    /// The credential is valid but not permitted.
    #[error("the credential is not permitted for this read")]
    Authorization,
    /// The source changed shape.
    #[error("the provider schema is not supported: {detail}")]
    UnsupportedSchema {
        /// A sanitized description of the mismatch.
        detail: String,
    },
    /// The payload could not be interpreted as a quota reading.
    #[error("the provider returned an unusable payload: {detail}")]
    InvalidData {
        /// A sanitized description.
        detail: String,
    },
    /// The work was deliberately cancelled.
    #[error("the read was cancelled")]
    Cancelled,
}

impl ProviderError {
    /// Whether the scheduler may retry this failure on its own.
    #[must_use]
    pub const fn is_retryable(&self) -> bool {
        matches!(self, Self::Transient { .. } | Self::RateLimited { .. })
    }

    /// A stable, non-identifying code for logs.
    #[must_use]
    pub const fn diagnostic_code(&self) -> &'static str {
        match self {
            Self::Transient { .. } => "transient",
            Self::RateLimited { .. } => "rate_limited",
            Self::Authentication => "authentication",
            Self::Authorization => "authorization",
            Self::UnsupportedSchema { .. } => "unsupported_schema",
            Self::InvalidData { .. } => "invalid_data",
            Self::Cancelled => "cancelled",
        }
    }
}

/// The verified binding a read was requested for.
///
/// A result is accepted only when every field still matches the live binding,
/// which is what rejects a late read after a reconnect.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionBinding {
    /// The connection.
    pub connection_id: ConnectionId,
    /// Its generation, incremented on every reconnect.
    pub generation: u32,
    /// The provider.
    pub provider_id: ProviderId,
    /// The verified principal, when known.
    pub principal_id: Option<ProviderPrincipalId>,
    /// The verified workspace, when known.
    pub workspace_id: Option<WorkspaceId>,
    /// The verified entitlement, when known.
    pub entitlement_id: Option<EntitlementId>,
    /// An optional adapter profile, for adapters with isolated profiles.
    pub profile_label: Option<String>,
}

impl ConnectionBinding {
    /// Whether a completed read still belongs to this binding.
    #[must_use]
    pub fn accepts(&self, offered: &Self) -> bool {
        self == offered
    }
}

/// One account an adapter found during discovery.
#[derive(Clone, Debug, PartialEq)]
pub struct DiscoveredAccount {
    /// The provider-verified principal, when the source reports one.
    pub principal_id: Option<ProviderPrincipalId>,
    /// The verified workspace, when the source reports one.
    pub workspace_id: Option<WorkspaceId>,
    /// The verified entitlement, when the source reports one.
    pub entitlement_id: Option<EntitlementId>,
    /// An optional adapter profile label.
    pub profile_label: Option<String>,
    /// A stable identity for the quota pool this account meters.
    pub pool_id: QuotaPoolId,
    /// The verified identity for user confirmation.
    pub identity: VerifiedIdentity,
    /// How many independent accounts this adapter supports.
    pub cardinality: AccountCardinality,
    /// Where the credential came from.
    pub credential_ownership: CredentialOwnership,
    /// Where the connection would stand after discovery.
    pub state: ConnectionState,
    /// A caller-chosen display name. Presentation only.
    pub nickname: String,
}

/// What the scheduler hands an adapter for one read.
#[derive(Clone, Debug)]
pub struct ReadContext {
    /// The attempt that produced this read, for tracing and reconciliation.
    pub attempt_id: ConnectionAttemptId,
    /// The absolute deadline for the whole read, including helper startup.
    pub deadline: Option<DateTime<Utc>>,
}

/// A validated reading.
#[derive(Clone, Debug, PartialEq)]
pub struct QuotaRead {
    /// The verified identity, for user confirmation.
    pub identity: VerifiedIdentity,
    /// The normalised windows.
    pub windows: Vec<QuotaWindow>,
    /// Windows the provider was expected to report and did not.
    pub expected_but_missing: Vec<quota_domain::ids::QuotaWindowId>,
    /// Provider-specific metadata for local diagnostics only. It never carries
    /// a token, cookie, or unrelated account content.
    pub debug_metadata: Option<serde_json::Value>,
}

/// What an adapter returns for one read.
#[derive(Clone, Debug, PartialEq)]
pub enum FetchOutcome {
    /// Every applicable field arrived.
    Complete(QuotaRead),
    /// Some applicable fields are missing. Known values stay visible.
    Partial {
        /// The reading that did arrive.
        read: QuotaRead,
        /// Why the response was incomplete.
        detail: String,
    },
    /// Nothing usable arrived. Never a fabricated zero.
    Failed(ProviderError),
}

impl FetchOutcome {
    /// The reading, when one arrived.
    #[must_use]
    pub const fn read(&self) -> Option<&QuotaRead> {
        match self {
            Self::Complete(read) | Self::Partial { read, .. } => Some(read),
            Self::Failed(_) => None,
        }
    }

    /// Whether the response was missing applicable fields.
    #[must_use]
    pub const fn is_partial(&self) -> bool {
        matches!(self, Self::Partial { .. })
    }
}

/// A compiled provider adapter.
pub trait ProviderAdapter: Send + Sync + std::fmt::Debug {
    /// Which provider this adapter serves.
    fn provider_id(&self) -> ProviderId;

    /// What the adapter declares it can do.
    fn capabilities(&self) -> ProviderCapabilities;

    /// The adapter's validated polling policy.
    fn policy(&self) -> ProviderPollingPolicy;

    /// Every account the local credential source can see right now.
    fn discover_accounts(
        &self,
    ) -> ProviderFuture<'_, Result<Vec<DiscoveredAccount>, ProviderError>>;

    /// Reads the quota for one verified binding.
    ///
    /// Implementations must not start a conversation, turn, or tool call, and
    /// must not write to or refresh a credential the owning tool owns.
    fn read_quota(
        &self,
        binding: &ConnectionBinding,
        context: ReadContext,
    ) -> ProviderFuture<'_, Result<FetchOutcome, ProviderError>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding(generation: u32) -> ConnectionBinding {
        ConnectionBinding {
            connection_id: ConnectionId::new("c1").unwrap(),
            generation,
            provider_id: ProviderId::Codex,
            principal_id: None,
            workspace_id: None,
            entitlement_id: None,
            profile_label: None,
        }
    }

    #[test]
    fn a_late_read_from_a_superseded_generation_is_rejected() {
        let live = binding(4);
        let late = binding(3);
        assert!(live.accepts(&live));
        assert!(!live.accepts(&late));
    }

    #[test]
    fn a_failed_outcome_carries_no_reading() {
        let outcome = FetchOutcome::Failed(ProviderError::Cancelled);
        assert!(outcome.read().is_none());
        assert!(!outcome.is_partial());
    }

    #[test]
    fn only_transient_and_rate_limited_failures_are_retryable() {
        assert!(ProviderError::Transient { detail: "x".into() }.is_retryable());
        assert!(ProviderError::RateLimited { retry_after: None }.is_retryable());
        assert!(!ProviderError::Authentication.is_retryable());
        assert!(!ProviderError::InvalidData { detail: "x".into() }.is_retryable());
    }
}
