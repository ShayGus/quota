//! Integration tests for adapters from core ports to typed `SQLite` repositories.

#![expect(
    clippy::tests_outside_test_module,
    reason = "an integration test binary holds nothing but tests and compiles without cfg(test)"
)]
#![expect(
    clippy::unwrap_used,
    reason = "test fixtures assert setup; a broken fixture must fail loudly"
)]

mod support;

use quota_core::AccountRegistry;
use quota_core::ports::{
    AccountRepository as AccountPort, HistoryRepository as HistoryPort, MonitoringRepository,
    StoredAccount,
};
use quota_domain::account::{ConnectionState, FetchState};
use quota_domain::ids::{AccountId, ConnectionId};
use quota_domain::snapshot::MonitoringState;
use quota_persistence::SqliteRepositories;
use quota_persistence::ports::{SqliteAccountPortAdapter, SqliteHistoryPortAdapter};
use support::{TempDir, account, at, connection, migrated, window};

#[tokio::test]
async fn account_port_round_trips_binding_reading_and_mutations() {
    let directory = TempDir::new("core-port");
    let pool = migrated(&directory).await;
    let repositories = SqliteRepositories::new(pool);
    repositories
        .accounts()
        .upsert_connection(&connection("conn-a"))
        .await
        .unwrap();
    repositories
        .accounts()
        .set_connection_state(
            &ConnectionId::new("conn-a").unwrap(),
            ConnectionState::Connected,
        )
        .await
        .unwrap();
    repositories
        .accounts()
        .upsert_account(&account("acct-a", "conn-a", 4, "Personal"))
        .await
        .unwrap();

    let observed = at(3);
    let reading = window("weekly-a", "pool-a", 37.5, observed);
    repositories
        .measurements()
        .persist_reading(&AccountId::new("acct-a").unwrap(), &reading)
        .await
        .unwrap();

    let port = SqliteAccountPortAdapter::new(repositories.clone());
    let loaded = port.load_accounts().await.unwrap();
    assert_eq!(loaded.len(), 1);
    let stored = &loaded[0];
    assert_eq!(stored.account_id.as_str(), "acct-a");
    assert_eq!(stored.connection.id.as_str(), "conn-a");
    assert_eq!(stored.connection.generation, 0);
    assert_eq!(stored.nickname, "Personal");
    assert_eq!(stored.connection_ordinal, 4);
    assert!(stored.monitoring_enabled);
    assert_eq!(stored.connection_state, ConnectionState::Connected);
    assert_eq!(stored.fetch_state, FetchState::Idle);
    assert_eq!(stored.windows.len(), 1);
    assert_eq!(stored.windows[0].id.as_str(), "weekly-a");
    assert!(
        (stored.windows[0]
            .measurement
            .remaining_percent()
            .unwrap()
            .value()
            - 37.5)
            .abs()
            < f64::EPSILON
    );

    port.set_enabled(&AccountId::new("acct-a").unwrap(), false)
        .await
        .unwrap();
    assert!(!port.load_accounts().await.unwrap()[0].monitoring_enabled);
    assert_eq!(
        port.bump_generation(&ConnectionId::new("conn-a").unwrap())
            .await
            .unwrap(),
        1
    );
    let snapshot = port
        .snapshot_of(&AccountId::new("acct-a").unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.account_id.as_str(), "acct-a");
    assert_eq!(snapshot.connection_generation, 1);
    assert!(
        (snapshot.windows[0]
            .measurement
            .remaining_percent()
            .unwrap()
            .value()
            - 37.5)
            .abs()
            < f64::EPSILON
    );
}

#[tokio::test]
async fn history_port_deletes_only_the_selected_account() {
    let directory = TempDir::new("history-port");
    let pool = migrated(&directory).await;
    let repositories = SqliteRepositories::new(pool);
    for (conn, acct, ordinal) in [("conn-a", "acct-a", 1), ("conn-b", "acct-b", 1)] {
        repositories
            .accounts()
            .upsert_connection(&connection(conn))
            .await
            .unwrap();
        repositories
            .accounts()
            .upsert_account(&account(acct, conn, ordinal, acct))
            .await
            .unwrap();
        let reading = window(
            &format!("weekly-{acct}"),
            &format!("pool-{acct}"),
            25.0,
            at(4),
        );
        repositories
            .measurements()
            .persist_reading(&AccountId::new(acct).unwrap(), &reading)
            .await
            .unwrap();
    }
    let port = SqliteHistoryPortAdapter::new(repositories.clone());
    port.clear_history(&AccountId::new("acct-a").unwrap())
        .await
        .unwrap();
    assert!(
        repositories
            .measurements()
            .history_for_account(&AccountId::new("acct-a").unwrap())
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        repositories
            .measurements()
            .history_for_account(&AccountId::new("acct-b").unwrap())
            .await
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn monitoring_state_defaults_once_and_then_round_trips() {
    let directory = TempDir::new("monitoring-port");
    let pool = migrated(&directory).await;
    let repositories = SqliteRepositories::new(pool);
    let port = quota_persistence::ports::SqliteMonitoringPortAdapter::new(repositories);

    assert_eq!(
        MonitoringState::Running,
        port.load_monitoring_state().await.unwrap()
    );
    port.save_monitoring_state(&MonitoringState::Paused)
        .await
        .unwrap();
    assert_eq!(
        MonitoringState::Paused,
        port.load_monitoring_state().await.unwrap()
    );
}

#[tokio::test]
async fn metadata_during_dispatch_does_not_persist_a_successful_read() {
    let directory = TempDir::new("dispatch-metadata");
    let repositories = SqliteRepositories::new(migrated(&directory).await);
    let account_id = AccountId::new("acct-a").unwrap();
    let connection_id = ConnectionId::new("conn-a").unwrap();
    repositories
        .accounts()
        .upsert_connection(&connection("conn-a"))
        .await
        .unwrap();
    repositories
        .accounts()
        .upsert_account(&account("acct-a", "conn-a", 1, "Personal"))
        .await
        .unwrap();
    repositories
        .accounts()
        .record_attempt(
            &account_id,
            FetchState::Idle,
            Some(at(1)),
            Some(at(1)),
            None,
        )
        .await
        .unwrap();
    let port = SqliteAccountPortAdapter::new(repositories);
    let mut registry = AccountRegistry::from_stored(port.load_accounts().await.unwrap());
    let binding = registry.get(&account_id).unwrap().binding.clone();
    registry
        .record_dispatch(&account_id, &binding, at(2), Some(at(3)))
        .unwrap();
    registry.rename(&account_id, "Renamed").unwrap();
    port.upsert_account(registry.get(&account_id).unwrap().stored.clone())
        .await
        .unwrap();
    registry.set_enabled(&account_id, false).unwrap();
    port.upsert_account(registry.get(&account_id).unwrap().stored.clone())
        .await
        .unwrap();
    let generation = port.bump_generation(&connection_id).await.unwrap();
    registry.set_generation(&connection_id, generation).unwrap();
    registry
        .set_connection_state(&connection_id, ConnectionState::Connecting)
        .unwrap();
    port.upsert_account(registry.get(&account_id).unwrap().stored.clone())
        .await
        .unwrap();
    let pending = port.load_accounts().await.unwrap().remove(0);
    assert_eq!(pending.last_success_at, Some(at(1)));
    assert_eq!(pending.last_attempt_at, Some(at(2)));
    assert_eq!(pending.fetch_state, FetchState::Fetching);
    assert_eq!(pending.nickname, "Renamed");
    assert!(!pending.monitoring_enabled);
    assert_eq!(pending.connection.generation, generation);

    for failure in [FetchState::Error, FetchState::Backoff, FetchState::Offline] {
        registry
            .record_attempt(&account_id, failure, at(3), Some(at(4)))
            .unwrap();
        port.upsert_account(registry.get(&account_id).unwrap().stored.clone())
            .await
            .unwrap();
        let restored = AccountRegistry::from_stored(port.load_accounts().await.unwrap());
        assert_eq!(
            restored.get(&account_id).unwrap().stored.last_success_at,
            Some(at(1))
        );
    }
    registry
        .record_attempt(&account_id, FetchState::Idle, at(4), Some(at(5)))
        .unwrap();
    port.upsert_account(registry.get(&account_id).unwrap().stored.clone())
        .await
        .unwrap();
    let restored = AccountRegistry::from_stored(port.load_accounts().await.unwrap());
    assert_eq!(
        restored.get(&account_id).unwrap().stored.last_success_at,
        Some(at(4))
    );
}

#[tokio::test]
async fn read_timestamps_survive_reopening_the_database() {
    let directory = TempDir::new("read-timestamps");
    let mut pool = migrated(&directory).await;
    let repositories = SqliteRepositories::new(pool.clone());
    repositories
        .accounts()
        .upsert_connection(&connection("conn-a"))
        .await
        .unwrap();
    repositories
        .accounts()
        .upsert_account(&account("acct-a", "conn-a", 1, "Personal"))
        .await
        .unwrap();
    let mut port = SqliteAccountPortAdapter::new(repositories);
    let mut stored = port
        .load_accounts()
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    let dispatched = at(1);
    let completed = dispatched + chrono::Duration::seconds(3);
    let next = completed + chrono::Duration::seconds(300);
    for (state, attempted_at, succeeded_at, next_at) in [
        (
            FetchState::Idle,
            Some(dispatched),
            Some(completed),
            Some(next),
        ),
        (
            FetchState::Fetching,
            Some(at(2)),
            Some(completed),
            Some(at(3)),
        ),
        (FetchState::Error, Some(at(2)), Some(completed), Some(at(3))),
        (
            FetchState::Backoff,
            Some(at(2)),
            Some(completed),
            Some(at(3)),
        ),
        (
            FetchState::Offline,
            Some(at(2)),
            Some(completed),
            Some(at(3)),
        ),
        (FetchState::Idle, None, Some(completed), None),
        (FetchState::Idle, Some(dispatched), None, Some(next)),
        (FetchState::Idle, None, None, None),
    ] {
        stored.fetch_state = state;
        stored.last_attempt_at = attempted_at;
        stored.last_success_at = succeeded_at;
        stored.next_attempt_at = next_at;
        port.upsert_account(stored.clone()).await.unwrap();
        pool.close().await;
        pool = migrated(&directory).await;
        port = SqliteAccountPortAdapter::new(SqliteRepositories::new(pool.clone()));
        assert_read_times(&port, &stored).await;
    }
    pool.close().await;
}

/// Checks that the port reads back `expected`'s fetch state and read times, in
/// both the stored account and its snapshot.
async fn assert_read_times(port: &SqliteAccountPortAdapter, expected: &StoredAccount) {
    let restored = port
        .load_accounts()
        .await
        .unwrap()
        .into_iter()
        .next()
        .unwrap();
    assert_eq!(restored.fetch_state, expected.fetch_state);
    assert_eq!(restored.last_attempt_at, expected.last_attempt_at);
    assert_eq!(restored.last_success_at, expected.last_success_at);
    assert_eq!(restored.next_attempt_at, expected.next_attempt_at);
    let snapshot = port
        .snapshot_of(&expected.account_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.last_attempt_at, expected.last_attempt_at);
    assert_eq!(snapshot.last_success_at, expected.last_success_at);
    assert_eq!(snapshot.next_attempt_at, expected.next_attempt_at);
}
