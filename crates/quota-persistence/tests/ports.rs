//! Integration tests for adapters from core ports to typed `SQLite` repositories.

#![expect(
    clippy::unwrap_used,
    reason = "test fixtures assert setup; a broken fixture must fail loudly"
)]

mod support;

use quota_core::ports::{AccountRepository as AccountPort, HistoryRepository as HistoryPort};
use quota_domain::account::{ConnectionState, FetchState};
use quota_domain::ids::{AccountId, ConnectionId};
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
