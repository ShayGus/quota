//! Connections, identities, and per-account state.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::ids::{ConnectionId, EntitlementId, ProviderPrincipalId, WorkspaceId};
use crate::provider::ProviderId;
use crate::quota::window::SourceKind;

/// The maximum accepted length of a user-chosen nickname, in characters.
pub const MAX_NICKNAME_LEN: usize = 64;

/// How many independent accounts an adapter can monitor at once.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AccountCardinality {
    /// Each connection is an independent account with its own allowance.
    Independent,
    /// Several workspaces share one authorization.
    WorkspaceScoped,
    /// The provider exposes a single externally owned profile.
    SingleProfile,
}

/// Who owns the credential behind a connection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum CredentialOwnership {
    /// Quota holds and refreshes the credential.
    AppOwned,
    /// Another client owns the credential; Quota only reads it.
    ExternalClient,
}

/// Where a connection stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    /// The account has never been connected.
    NeverConnected,
    /// An authorized attempt is running.
    Connecting,
    /// A verified reading exists.
    Connected,
    /// The credential must be renewed by the user.
    ReauthenticationRequired,
    /// The source is no longer usable.
    Unsupported,
    /// The account is no longer monitored.
    Disconnected,
}

/// How the last read attempt went. This is not a quota severity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum FetchState {
    /// Nothing is due yet.
    Idle,
    /// A read is in flight.
    Fetching,
    /// Waiting out a provider backoff.
    Backoff,
    /// The host reports no network.
    Offline,
    /// The last attempt failed.
    Error,
}

/// What happens when the included allowance runs out.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum ContinuationMode {
    /// New work is refused at the limit.
    BlockedAtLimit,
    /// Paid overage continues the work.
    PaidOverage,
    /// The source did not say.
    Unknown,
}

/// A provider-verified identity, shown for confirmation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct VerifiedIdentity {
    /// Human-readable principal label, such as a masked address.
    pub principal_label: String,
    /// Workspace or team label.
    pub workspace_label: Option<String>,
    /// Plan label, when the provider reports one.
    pub plan_label: Option<String>,
    /// Where the identity was verified.
    pub source: SourceKind,
}

/// A non-secret summary of one authorization connection.
///
/// It never carries a secret-store reference or a token.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct ConnectionSummary {
    /// Opaque local connection identity.
    pub id: ConnectionId,
    /// The compiled provider adapter.
    pub provider_id: ProviderId,
    /// Who holds the credential.
    pub credential_ownership: CredentialOwnership,
    /// Incremented on every reconnect, so late results can be rejected.
    pub generation: u64,
    /// Optional adapter profile label.
    pub profile_label: Option<String>,
    /// What the adapter supports.
    pub cardinality: AccountCardinality,
    /// Where it stands.
    pub state: ConnectionState,
    /// Verified principal, once known.
    pub principal_id: Option<ProviderPrincipalId>,
    /// Verified workspace, once known.
    pub workspace_id: Option<WorkspaceId>,
    /// Verified entitlement context, once known.
    pub entitlement_id: Option<EntitlementId>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_summary_has_no_credential_reference() {
        let summary = ConnectionSummary {
            id: ConnectionId::new("c1").unwrap(),
            provider_id: ProviderId::Fixture,
            credential_ownership: CredentialOwnership::AppOwned,
            generation: 3,
            profile_label: None,
            cardinality: AccountCardinality::Independent,
            state: ConnectionState::Connected,
            principal_id: None,
            workspace_id: None,
            entitlement_id: None,
        };
        let json = serde_json::to_string(&summary).unwrap();
        for forbidden in ["token", "secret", "credential_ref", "keyring"] {
            assert!(!json.contains(forbidden), "{forbidden} leaked into {json}");
        }
    }
}
