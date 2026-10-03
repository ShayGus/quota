//! Account, connection, and isolation behaviour against a real database.
//!
//! Fixtures use `unwrap` for readability; this file is not `#[cfg(test)]`, so
//! the workspace's test-mode allowance does not reach it.

#![expect(
    clippy::tests_outside_test_module,
    reason = "an integration test binary holds nothing but tests and compiles without cfg(test)"
)]
#![expect(
    clippy::unwrap_used,
    reason = "test fixtures assert the setup they build, so a broken fixture must fail loudly"
)]

mod support;

use quota_domain::account::{ConnectionState, FetchState, VerifiedIdentity};
use quota_domain::ids::{AccountId, ConnectionId, QuotaWindowId};
use quota_domain::quota::window::SourceKind;
use quota_persistence::SqliteRepositories;
use support::TempDir;

#[tokio::test]
async fn deleting_one_account_leaves_its_same_provider_sibling_intact() {
    let directory = TempDir::new("isolation");
    let pool = support::migrated(&directory).await;
    let repositories = SqliteRepositories::new(pool.clone());
    let accounts = repositories.accounts();

    accounts
        .upsert_connection(&support::connection("conn-1"))
        .await
        .unwrap();
    accounts
        .upsert_account(&support::account("acct-a", "conn-1", 0, "Alpha"))
        .await
        .unwrap();
    accounts
        .upsert_account(&support::account("acct-b", "conn-1", 1, "Beta"))
        .await
        .unwrap();

    let repo = repositories.measurements();
    let window_id = QuotaWindowId::new("win-1").unwrap();
    repo.persist_reading(
        &AccountId::new("acct-a").unwrap(),
        &support::window("win-1", "pool-1", 40.0, support::at(0)),
    )
    .await
    .unwrap();
    repo.persist_reading(
        &AccountId::new("acct-b").unwrap(),
        &support::window("win-1", "pool-1", 70.0, support::at(0)),
    )
    .await
    .unwrap();

    assert!(
        accounts
            .delete_account(&AccountId::new("acct-a").unwrap())
            .await
            .unwrap()
    );
    assert!(
        !accounts
            .delete_account(&AccountId::new("acct-a").unwrap())
            .await
            .unwrap()
    );

    let survivors = accounts
        .list_for_connection(&ConnectionId::new("conn-1").unwrap())
        .await
        .unwrap();
    assert_eq!(survivors.len(), 1);
    assert_eq!(
        survivors[0].id.as_str(),
        "acct-b",
        "the sibling must survive"
    );

    let readings: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM latest_measurements")
        .fetch_one(&pool)
        .await
        .unwrap();
    let history: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM measurement_history")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(readings, 1, "only the deleted account's reading may go");
    assert_eq!(history, 1, "only the deleted account's history may go");

    let surviving = repo
        .latest(&AccountId::new("acct-b").unwrap(), &window_id)
        .await
        .unwrap()
        .unwrap();
    let remaining = surviving.measurement.remaining_percent().unwrap().value();
    assert!(
        (remaining - 70.0).abs() < f64::EPSILON,
        "the sibling's reading must be unchanged, saw {remaining}"
    );

    pool.close().await;
}

#[tokio::test]
async fn clearing_one_account_history_leaves_the_sibling_history() {
    let directory = TempDir::new("history-isolation");
    let pool = support::migrated(&directory).await;
    let repositories = SqliteRepositories::new(pool.clone());

    repositories
        .accounts()
        .upsert_connection(&support::connection("conn-1"))
        .await
        .unwrap();
    for (id, ordinal) in [("acct-a", 0_u32), ("acct-b", 1_u32)] {
        repositories
            .accounts()
            .upsert_account(&support::account(id, "conn-1", ordinal, id))
            .await
            .unwrap();
    }

    let repo = repositories.measurements();
    repo.persist_reading(
        &AccountId::new("acct-a").unwrap(),
        &support::window("win-1", "pool-1", 10.0, support::at(0)),
    )
    .await
    .unwrap();
    repo.persist_reading(
        &AccountId::new("acct-b").unwrap(),
        &support::window("win-1", "pool-1", 20.0, support::at(0)),
    )
    .await
    .unwrap();

    assert_eq!(
        repo.clear_history_for_account(&AccountId::new("acct-a").unwrap())
            .await
            .unwrap(),
        1
    );
    assert!(
        repo.history_for_account(&AccountId::new("acct-a").unwrap())
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        repo.history_for_account(&AccountId::new("acct-b").unwrap())
            .await
            .unwrap()
            .len(),
        1,
        "the sibling's history must survive"
    );

    pool.close().await;
}

#[tokio::test]
async fn an_account_records_its_verified_identity_and_fetch_state() {
    let directory = TempDir::new("identity");
    let pool = support::migrated(&directory).await;
    let repositories = SqliteRepositories::new(pool.clone());
    let accounts = repositories.accounts();

    accounts
        .upsert_connection(&support::connection("conn-1"))
        .await
        .unwrap();
    accounts
        .upsert_account(&support::account("acct-1", "conn-1", 0, "First"))
        .await
        .unwrap();

    let identity = VerifiedIdentity {
        principal_label: "user@example.test".to_owned(),
        workspace_label: Some("Work".to_owned()),
        plan_label: Some("Pro".to_owned()),
        source: SourceKind::DocumentedApi,
    };
    accounts
        .record_verified_identity(
            &AccountId::new("acct-1").unwrap(),
            ConnectionState::Connected,
            &identity,
        )
        .await
        .unwrap();
    accounts
        .record_attempt(
            &AccountId::new("acct-1").unwrap(),
            FetchState::Idle,
            Some(support::at(3)),
            Some(support::at(3)),
            Some(support::at(4)),
        )
        .await
        .unwrap();

    let listed = accounts
        .list_for_connection(&ConnectionId::new("conn-1").unwrap())
        .await
        .unwrap();
    let stored = &listed[0];

    assert_eq!(stored.verified_identity, Some(identity));
    assert_eq!(stored.connection_state, ConnectionState::Connected);
    assert_eq!(stored.fetch_state, FetchState::Idle);
    assert_eq!(stored.last_attempt_at, Some(support::at(3)));
    assert_eq!(stored.last_success_at, Some(support::at(3)));
    assert_eq!(stored.next_attempt_at, Some(support::at(4)));
    assert!(stored.monitoring_enabled);

    accounts
        .set_monitoring_enabled(&AccountId::new("acct-1").unwrap(), false)
        .await
        .unwrap();
    assert!(
        !accounts
            .list_for_connection(&ConnectionId::new("conn-1").unwrap())
            .await
            .unwrap()[0]
            .monitoring_enabled
    );

    pool.close().await;
}
