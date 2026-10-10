//! Command argument and result shapes.
//!
//! Every operation takes an explicit, scoped argument type. There is no generic
//! `set_state`, no JSON patch, no raw URL, path, or SQL argument, and no
//! string-keyed action dispatcher.

use serde::{Deserialize, Serialize};
use specta::Type;

use quota_domain::account::VerifiedIdentity;
use quota_domain::ids::{AccountGroupId, ConnectionAttemptId};
use quota_domain::preferences::OverviewMode;
use quota_domain::provider::{ProviderCapabilities, ProviderId};
use quota_domain::quota::window::QuotaWindow;
use quota_domain::snapshot::AppSnapshot;

use crate::refs::{AccountRef, AttemptRef};

/// Which accounts a read or refresh covers.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AccountSelection {
    /// Every monitored account.
    #[default]
    All,
    /// Only the listed accounts.
    Listed {
        /// The explicitly selected accounts.
        account_refs: Vec<AccountRef>,
    },
}

/// Why a refresh was requested.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum RefreshReason {
    /// The user asked for a refresh.
    UserRequested,
    /// The supervisor scheduled an ordinary interval read.
    Scheduled,
    /// A reported boundary passed and needs verification.
    BoundaryVerification,
    /// The overview was opened and the cached snapshot is older than policy.
    OverviewOpened,
    /// The host reported that it resumed from sleep.
    Resumed,
}

/// Arguments for starting an authorized connection attempt.
///
/// The attempt identity is issued by the backend so a timed-out call can be
/// reconciled instead of creating a duplicate connection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct BeginConnectionRequest {
    /// The provider to connect.
    pub provider_id: ProviderId,
    /// Optional adapter profile, for adapters that support isolated profiles.
    pub profile_label: Option<String>,
    /// The user's chosen display name. Presentation only.
    pub nickname: String,
    /// A credential the person pasted, for a provider Quota signs in to
    /// itself. It is held in memory until the account is added, then kept only
    /// in the system credential store.
    #[serde(default)]
    pub credential: Option<PastedCredential>,
    /// Whether to sign in through the provider's page in the browser, for a
    /// provider Quota signs in to that way. The token it grants is then held
    /// and stored like a pasted credential.
    #[serde(default)]
    pub browser_sign_in: bool,
}

/// The group a new key joins as it is added, for a provider whose account
/// holds several keys.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum KeyGroupChoice {
    /// An existing group of the same provider.
    Existing {
        /// The group to join.
        group_id: AccountGroupId,
    },
    /// A new group, named by the person.
    New {
        /// The name of the provider account.
        name: String,
    },
}

/// A credential the person pasted, such as an API key.
///
/// It never prints its value, so a logged request cannot leak it.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(transparent)]
pub struct PastedCredential(String);

impl PastedCredential {
    /// Wraps a pasted value.
    #[must_use]
    pub const fn new(value: String) -> Self {
        Self(value)
    }

    /// The value, trimmed of the spaces a paste brings along, for the one
    /// place that stores or sends it.
    #[must_use]
    pub fn expose(&self) -> &str {
        self.0.trim()
    }
}

impl std::fmt::Debug for PastedCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PastedCredential(<redacted>)")
    }
}

/// Arguments for enabling or disabling one account.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct SetAccountEnabledRequest {
    /// The account to change.
    pub account_ref: AccountRef,
    /// Whether it should be monitored.
    pub enabled: bool,
    /// The preference revision the caller believes is current.
    pub expected_revision: u32,
}

/// The accepted outcome of a connection attempt.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct ConnectionAttemptAccepted {
    /// The backend-issued attempt identity, used to reconcile a timed-out call.
    pub attempt_ref: AttemptRef,
    /// The raw attempt identifier, for reconciliation lookups.
    pub attempt_id: ConnectionAttemptId,
}

/// A verified identity held for the person's decision, and not yet saved.
/// Nothing about this type names a stored account, so a candidate that is never
/// confirmed leaves no account behind. It carries no secret material.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct VerifiedCandidate {
    /// The adapter that reported this identity.
    pub provider_id: ProviderId,
    /// The nickname the person asked for. Presentation only.
    pub nickname: String,
    /// The provider-verified principal, workspace, plan, and source.
    pub identity: VerifiedIdentity,
    /// The quota reading this attempt verified.
    pub windows: Vec<QuotaWindow>,
}

/// One compiled adapter and what it declares it can do.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct RegisteredProvider {
    /// The provider identifier.
    pub provider_id: ProviderId,
    /// What the adapter declares.
    pub capabilities: ProviderCapabilities,
    /// Whether the adapter is compiled into this build at all.
    pub compiled_in_this_build: bool,
}

/// A confirmed overview mode change, or the state that was actually reached.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum WindowModeChange {
    /// The window is now in this mode.
    Applied(OverviewMode),
    /// The platform refused, and the previous mode is still in effect.
    Refused {
        /// The sanitized reason the platform gave.
        reason: String,
        /// The mode still in effect.
        current: OverviewMode,
    },
}

/// The settings surface requested by an overview action.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum SettingsDestination {
    /// General window and monitoring settings.
    General,
    /// Connected account management.
    Accounts,
    /// Provider connection wizard.
    Connect,
    /// The connection wizard, adding a key to one account group.
    AddKey {
        /// The group the new key joins.
        group_id: AccountGroupId,
    },
}

/// The snapshot plus the revision the renderer should reconcile against.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct SnapshotResponse {
    /// The complete, sanitized snapshot.
    pub snapshot: AppSnapshot,
}

/// Which way the mini widget window grows for its details drawer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum WidgetGrowth {
    /// The top edge stays and the bottom moves down.
    Down,
    /// The bottom edge stays and the top moves up.
    Up,
}

/// What fitting the mini widget to its content decided.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct FitWidget {
    /// The fitted content height in logical pixels; below the asked height
    /// only when neither side had room and the drawer is capped.
    pub height: f64,
    /// The side the window grew or shrank on.
    pub direction: WidgetGrowth,
    /// The free room above the window in logical pixels.
    pub room_above: f64,
    /// The free room below the window in logical pixels.
    pub room_below: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_defaults_to_every_account() {
        assert_eq!(AccountSelection::default(), AccountSelection::All);
    }

    #[test]
    fn refresh_reasons_round_trip_as_a_closed_union() {
        for reason in [
            RefreshReason::UserRequested,
            RefreshReason::Scheduled,
            RefreshReason::BoundaryVerification,
            RefreshReason::OverviewOpened,
            RefreshReason::Resumed,
        ] {
            let json = serde_json::to_string(&reason).unwrap();
            assert_eq!(
                serde_json::from_str::<RefreshReason>(&json).unwrap(),
                reason
            );
        }
    }
}
