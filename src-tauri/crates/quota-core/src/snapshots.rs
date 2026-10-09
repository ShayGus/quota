//! The snapshot builder.
//!
//! It turns the registry into the immutable published snapshot: one revision
//! per accepted change, a canonical account order, and no fabricated values.

use std::collections::HashMap;

use chrono::{DateTime, Utc};

use quota_domain::ids::AppInstanceId;
use quota_domain::ranking::{
    AccountOrder, ORDER_RULE_VERSION, RankingInput, UnrankedOrder, UnrankedReason, compute_order,
};
use quota_domain::snapshot::{
    AccountSnapshot, AppSnapshot, MonitoringState, PersistenceStatus, SNAPSHOT_SCHEMA_VERSION,
};

use crate::accounts::{AccountRegistry, RegisteredAccount};

/// Builds published snapshots and owns their revision counter.
#[derive(Debug)]
pub struct SnapshotBuilder {
    app_instance_id: AppInstanceId,
    revision: u32,
}

impl SnapshotBuilder {
    /// Creates a builder for one running instance.
    #[must_use]
    pub fn new(app_instance_id: AppInstanceId) -> Self {
        Self {
            app_instance_id,
            revision: 0,
        }
    }

    /// The instance this builder publishes for.
    #[must_use]
    pub const fn app_instance_id(&self) -> &AppInstanceId {
        &self.app_instance_id
    }

    /// The revision most recently published.
    #[must_use]
    pub const fn revision(&self) -> u32 {
        self.revision
    }

    /// Builds and advances to the next snapshot.
    ///
    /// The revision is monotonic inside one instance, so a renderer can reject
    /// an older response that arrives after a newer event.
    #[must_use]
    pub fn build(
        &mut self,
        registry: &AccountRegistry,
        monitoring_state: &MonitoringState,
        persistence_status: &PersistenceStatus,
        now: DateTime<Utc>,
    ) -> AppSnapshot {
        self.revision = self.revision.saturating_add(1);
        let paused = monitoring_state == &MonitoringState::Paused;
        let inputs: Vec<RankingInput<'_>> = registry
            .iter()
            .map(|entry| RankingInput {
                account_id: entry.account_id(),
                connection_ordinal: entry.stored.connection_ordinal,
                windows: &entry.stored.windows,
                monitoring_enabled: entry.stored.monitoring_enabled,
                monitoring_paused: paused,
                connection_state: entry.stored.connection_state,
                now,
            })
            .collect();
        let order = compute_order(&inputs);
        let ranked: HashMap<_, _> = order
            .iter()
            .map(|entry| (entry.account_id.clone(), entry.order.clone()))
            .collect();
        let accounts: Vec<AccountSnapshot> = registry
            .iter()
            .map(|entry| {
                let mut account = project(entry, now);
                account.order = ranked
                    .get(&account.account_id)
                    .cloned()
                    .unwrap_or_else(unnamed);
                account
            })
            .collect();
        let in_order: Vec<&AccountSnapshot> = order
            .iter()
            .filter_map(|entry| {
                accounts
                    .iter()
                    .find(|account| account.account_id == entry.account_id)
            })
            .collect();
        let groups = quota_domain::group::group_snapshots(&in_order);

        AppSnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            app_instance_id: self.app_instance_id.clone(),
            revision: self.revision,
            generated_at: now,
            monitoring_state: monitoring_state.clone(),
            persistence_status: persistence_status.clone(),
            connections: registry
                .iter()
                .map(|entry| entry.stored.connection.clone())
                .collect(),
            accounts,
            order,
            groups,
        }
    }
}

fn unnamed() -> AccountOrder {
    AccountOrder::Unranked(UnrankedOrder {
        reason: UnrankedReason::Incomplete,
        rule_version: ORDER_RULE_VERSION,
    })
}

fn project(entry: &RegisteredAccount, now: DateTime<Utc>) -> AccountSnapshot {
    AccountSnapshot {
        account_id: entry.stored.account_id.clone(),
        connection_id: entry.stored.connection.id.clone(),
        connection_generation: entry.stored.connection.generation,
        provider_id: entry.stored.connection.provider_id,
        nickname: entry.stored.nickname.clone(),
        identity: entry.stored.identity.clone(),
        connection_ordinal: entry.stored.connection_ordinal,
        monitoring_enabled: entry.stored.monitoring_enabled,
        connection_state: entry.stored.connection_state,
        fetch_state: entry.stored.fetch_state,
        last_attempt_at: entry.stored.last_attempt_at,
        last_success_at: entry.stored.last_success_at,
        next_attempt_at: entry.stored.next_attempt_at,
        windows: entry.stored.windows.clone(),
        expected_but_missing_window_ids: entry.stored.expected_but_missing_window_ids.clone(),
        order: unnamed(),
        balance: entry
            .stored
            .balance
            .as_ref()
            .map(|ledger| ledger.summary(now)),
        show_key_limit: entry.stored.show_key_limit,
        group: entry.stored.group.clone(),
    }
}
