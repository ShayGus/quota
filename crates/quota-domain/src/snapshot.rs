//! The immutable snapshot the backend publishes after every accepted change.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::account::{ConnectionState, ConnectionSummary, FetchState, VerifiedIdentity};
use crate::ids::{AccountId, AppInstanceId, ConnectionId, QuotaWindowId};
use crate::provider::ProviderId;
use crate::quota::window::QuotaWindow;
use crate::ranking::{AccountOrder, OrderEntry};

/// The wire schema version of the published snapshot.
pub const SNAPSHOT_SCHEMA_VERSION: u32 = 1;

/// Whether the supervisor is scheduling reads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case", tag = "kind", content = "reason")]
pub enum MonitoringState {
    /// Reads are scheduled.
    Running,
    /// The user paused monitoring.
    Paused,
    /// The supervisor stopped after a failure and is not scheduling.
    RecoveryRequired(String),
}

/// Whether durable storage is usable.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case", tag = "kind", content = "detail")]
pub enum PersistenceStatus {
    /// Every durable owner is available.
    Available,
    /// One owner is degraded; readings continue without durable history.
    Degraded(String),
    /// A mandatory owner could not be opened, so no polling starts.
    RecoveryRequired(String),
}

/// One monitored account as the renderer sees it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct AccountSnapshot {
    /// Immutable account identity used by every row action.
    pub account_id: AccountId,
    /// The connection that authorises this account.
    pub connection_id: ConnectionId,
    /// The connection generation this reading belongs to.
    pub connection_generation: u64,
    /// The provider adapter.
    pub provider_id: ProviderId,
    /// The user-chosen display name. Presentation only.
    pub nickname: String,
    /// The verified principal, workspace, plan, and source.
    pub identity: Option<VerifiedIdentity>,
    /// Stable tie-break order, assigned when the connection was created.
    pub connection_ordinal: u32,
    /// Whether the account is monitored.
    pub monitoring_enabled: bool,
    /// Where the connection stands.
    pub connection_state: ConnectionState,
    /// How the last read went.
    pub fetch_state: FetchState,
    /// When a read was last attempted.
    pub last_attempt_at: Option<DateTime<Utc>>,
    /// When a reading was last accepted.
    pub last_success_at: Option<DateTime<Utc>>,
    /// When the next read may start.
    pub next_attempt_at: Option<DateTime<Utc>>,
    /// The account's quota windows.
    pub windows: Vec<QuotaWindow>,
    /// Windows the provider is expected to report but did not.
    pub expected_but_missing_window_ids: Vec<QuotaWindowId>,
    /// The order position, recorded so the reason is inspectable.
    pub order: AccountOrder,
}

/// The complete application state at one revision.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct AppSnapshot {
    /// The wire schema version.
    pub schema_version: u32,
    /// Which application instance produced this snapshot.
    pub app_instance_id: AppInstanceId,
    /// Monotonic within one instance.
    pub revision: u64,
    /// When this snapshot was built.
    pub generated_at: DateTime<Utc>,
    /// Whether reads are being scheduled.
    pub monitoring_state: MonitoringState,
    /// Whether durable storage is usable.
    pub persistence_status: PersistenceStatus,
    /// Every connection, without any secret material.
    pub connections: Vec<ConnectionSummary>,
    /// Every monitored account.
    pub accounts: Vec<AccountSnapshot>,
    /// The canonical account order, in presentation sections.
    pub order: Vec<OrderEntry>,
}

impl AppSnapshot {
    /// Borrows one account by immutable identity.
    #[must_use]
    pub fn account(&self, account_id: &AccountId) -> Option<&AccountSnapshot> {
        self.accounts
            .iter()
            .find(|account| &account.account_id == account_id)
    }

    /// The account identities in canonical order.
    #[must_use]
    pub fn ordered_account_ids(&self) -> Vec<&AccountId> {
        self.order.iter().map(|entry| &entry.account_id).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ranking::{UnrankedOrder, UnrankedReason};

    fn account(account_id: &AccountId) -> AccountSnapshot {
        AccountSnapshot {
            account_id: account_id.clone(),
            connection_id: ConnectionId::new("c").unwrap(),
            connection_generation: 1,
            provider_id: ProviderId::Fixture,
            nickname: "Personal".into(),
            identity: None,
            connection_ordinal: 1,
            monitoring_enabled: true,
            connection_state: ConnectionState::Connected,
            fetch_state: FetchState::Idle,
            last_attempt_at: None,
            last_success_at: None,
            next_attempt_at: None,
            windows: Vec::new(),
            expected_but_missing_window_ids: Vec::new(),
            order: AccountOrder::Unranked(UnrankedOrder {
                reason: UnrankedReason::Incomplete,
                rule_version: 1,
            }),
        }
    }

    #[test]
    fn finds_accounts_by_identity_not_position() {
        let first = AccountId::new("a").unwrap();
        let second = AccountId::new("b").unwrap();
        let missing = AccountId::new("missing").unwrap();
        let snapshot = AppSnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            app_instance_id: AppInstanceId::new("i").unwrap(),
            revision: 1,
            generated_at: DateTime::UNIX_EPOCH,
            monitoring_state: MonitoringState::Running,
            persistence_status: PersistenceStatus::Available,
            connections: Vec::new(),
            accounts: vec![account(&first), account(&second)],
            order: Vec::new(),
        };
        assert_eq!(
            snapshot.account(&second).map(|a| a.nickname.as_str()),
            Some("Personal")
        );
        assert!(snapshot.account(&missing).is_none());
    }
}
