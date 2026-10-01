//! Command argument and result shapes.
//!
//! Every operation takes an explicit, scoped argument type. There is no generic
//! `set_state`, no JSON patch, no raw URL, path, or SQL argument, and no
//! string-keyed action dispatcher.

use serde::{Deserialize, Serialize};
use specta::Type;

use quota_domain::ids::ConnectionAttemptId;
use quota_domain::preferences::OverviewMode;
use quota_domain::provider::{ProviderCapabilities, ProviderId};
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

/// The snapshot plus the revision the renderer should reconcile against.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct SnapshotResponse {
    /// The complete, sanitized snapshot.
    pub snapshot: AppSnapshot,
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
