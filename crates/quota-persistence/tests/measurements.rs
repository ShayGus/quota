//! Reading persistence, coalescing, and the one-transaction guarantee.
//!
//! Fixtures use `unwrap` for readability; this file is not `#[cfg(test)]`, so
//! the workspace's test-mode allowance does not reach it.

#![expect(
    clippy::unwrap_used,
    reason = "test fixtures assert the setup they build, so a broken fixture must fail loudly"
)]

mod support;

use quota_domain::ids::{AccountId, DefinitionVersion, QuotaWindowId};
use quota_domain::quota::measurement::{Measurement, UnavailableReason};
use quota_persistence::SqliteRepositories;
use support::TempDir;

/// An account with a connection, ready to persist readings against.
async fn seeded(directory: &TempDir) -> (sqlx::SqlitePool, SqliteRepositories) {
    let pool = support::migrated(directory).await;
    let repositories = SqliteRepositories::new(pool.clone());
    repositories
        .accounts()
        .upsert_connection(&support::connection("conn-1"))
        .await
        .unwrap();
    repositories
        .accounts()
        .upsert_account(&support::account("acct-1", "conn-1", 0, "First"))
        .await
        .unwrap();
    (pool, repositories)
}

#[tokio::test]
async fn a_reading_and_its_history_are_written_in_one_transaction() {
    let directory = TempDir::new("reading");
    let (pool, repositories) = seeded(&directory).await;
    let repo = repositories.measurements();
    let account_id = AccountId::new("acct-1").unwrap();
    let window_id = QuotaWindowId::new("win-1").unwrap();

    assert!(
        repo.persist_reading(
            &account_id,
            &support::window("win-1", "pool-1", 55.0, support::at(0))
        )
        .await
        .unwrap()
    );

    let stored = repo.latest(&account_id, &window_id).await.unwrap().unwrap();
    let remaining = stored.measurement.remaining_percent().unwrap().value();
    assert!(
        (remaining - 55.0).abs() < f64::EPSILON,
        "the reading must round trip, saw {remaining}"
    );
    assert_eq!(stored.definition_version, DefinitionVersion::INITIAL.0);
    assert_eq!(stored.observed_at, Some(support::at(0)));

    let history = repo.history_for_account(&account_id).await.unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].remaining_percent, Some(55.0));

    // An account that does not exist makes the write fail. The window row is
    // written first in the same transaction, so the failure rolls it back too,
    // and no history row is appended. That is what makes this one transaction.
    let refused = repo
        .persist_reading(
            &AccountId::new("acct-absent").unwrap(),
            &support::window("win-rolled-back", "pool-1", 30.0, support::at(1)),
        )
        .await;
    assert!(refused.is_err(), "the reading must be refused");

    let windows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM quota_windows")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(windows, 1, "the rolled-back window row must not survive");
    assert_eq!(
        repo.history_for_account(&account_id).await.unwrap().len(),
        1,
        "the failed write must not append history"
    );
    assert!(
        repo.latest(&account_id, &window_id)
            .await
            .unwrap()
            .is_some(),
        "the failed write must not remove the existing reading"
    );

    pool.close().await;
}

#[tokio::test]
async fn an_unavailable_measurement_stores_no_invented_percentage() {
    let directory = TempDir::new("unavailable");
    let (pool, repositories) = seeded(&directory).await;
    let repo = repositories.measurements();
    let account_id = AccountId::new("acct-1").unwrap();

    let mut value = support::window("win-1", "pool-1", 50.0, support::at(0));
    value.measurement = Measurement::Unavailable(UnavailableReason::NotReported);

    repo.persist_reading(&account_id, &value).await.unwrap();

    let stored = repo
        .latest(&account_id, &QuotaWindowId::new("win-1").unwrap())
        .await
        .unwrap()
        .unwrap();
    assert!(!stored.measurement.has_number());

    let recorded: Option<f64> =
        sqlx::query_scalar("SELECT remaining_percent FROM measurement_history")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        recorded, None,
        "no percentage may be invented for this reading"
    );

    pool.close().await;
}

#[tokio::test]
async fn an_unchanged_observation_is_coalesced() {
    let directory = TempDir::new("coalesce");
    let (pool, repositories) = seeded(&directory).await;
    let repo = repositories.measurements();
    let account_id = AccountId::new("acct-1").unwrap();

    let first = support::window("win-1", "pool-1", 42.0, support::at(0));
    assert!(repo.persist_reading(&account_id, &first).await.unwrap());
    assert!(repo.coalesce_unchanged(&account_id, &first).await.unwrap());

    // The same value at the same observation time appends nothing, and the
    // stored reading is still updated in place.
    assert!(!repo.persist_reading(&account_id, &first).await.unwrap());
    assert_eq!(
        repo.history_for_account(&account_id).await.unwrap().len(),
        1
    );

    // A later observation of the same value is a new observation.
    let later = support::window("win-1", "pool-1", 42.0, support::at(1));
    assert!(!repo.coalesce_unchanged(&account_id, &later).await.unwrap());
    assert!(repo.persist_reading(&account_id, &later).await.unwrap());
    assert_eq!(
        repo.history_for_account(&account_id).await.unwrap().len(),
        2
    );

    // A moved value at the same instant is also a new observation.
    let moved = support::window("win-1", "pool-1", 41.0, support::at(1));
    assert!(!repo.coalesce_unchanged(&account_id, &moved).await.unwrap());
    assert!(repo.persist_reading(&account_id, &moved).await.unwrap());
    assert_eq!(
        repo.history_for_account(&account_id).await.unwrap().len(),
        3
    );

    let stored = repo
        .latest(&account_id, &QuotaWindowId::new("win-1").unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.observed_at, Some(support::at(1)));

    pool.close().await;
}

#[tokio::test]
async fn two_windows_of_one_account_keep_separate_history() {
    let directory = TempDir::new("windows");
    let (pool, repositories) = seeded(&directory).await;
    let repo = repositories.measurements();
    let account_id = AccountId::new("acct-1").unwrap();

    repo.persist_reading(
        &account_id,
        &support::window("win-1", "pool-1", 80.0, support::at(0)),
    )
    .await
    .unwrap();
    repo.persist_reading(
        &account_id,
        &support::window("win-2", "pool-1", 20.0, support::at(0)),
    )
    .await
    .unwrap();

    let history = repo.history_for_account(&account_id).await.unwrap();
    assert_eq!(history.len(), 2, "one history row per window reading");
    assert_ne!(history[0].window_id, history[1].window_id);

    // Coalescing is per window: an unchanged window 1 must not suppress a
    // changed window 2.
    let window_two = support::window("win-2", "pool-1", 15.0, support::at(1));
    assert!(
        !repo
            .coalesce_unchanged(&account_id, &window_two)
            .await
            .unwrap()
    );

    pool.close().await;
}

#[tokio::test]
async fn an_unknown_account_creates_no_pool_row() {
    let directory = TempDir::new("no-pool");
    let (pool, repositories) = seeded(&directory).await;

    let refused = repositories
        .measurements()
        .persist_reading(
            &AccountId::new("acct-absent").unwrap(),
            &support::window("win-1", "pool-orphan", 10.0, support::at(0)),
        )
        .await;
    assert!(refused.is_err(), "an unknown account must be refused");

    let pools: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM quota_pools")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(pools, 0, "the pool provider must never be invented");

    pool.close().await;
}

#[tokio::test]
async fn a_stored_reading_reports_a_window_that_has_no_reading() {
    let directory = TempDir::new("absent-reading");
    let (pool, repositories) = seeded(&directory).await;

    let absent = repositories
        .measurements()
        .latest(
            &AccountId::new("acct-1").unwrap(),
            &QuotaWindowId::new("win-never-stored").unwrap(),
        )
        .await
        .unwrap();
    assert!(absent.is_none(), "an unstored window reports no reading");

    let empty_history = repositories
        .measurements()
        .history_for_account(&AccountId::new("acct-1").unwrap())
        .await
        .unwrap();
    assert!(empty_history.is_empty());

    pool.close().await;
}

/// A window the provider stops reporting must not come back after a restart.
#[tokio::test]
async fn a_removed_window_is_swept_when_the_whole_set_is_written() {
    let directory = TempDir::new("sweep");
    let (_pool, repositories) = seeded(&directory).await;
    let repo = repositories.measurements();
    let account_id = AccountId::new("acct-1").unwrap();
    let kept = support::window("win-kept", "pool-1", 55.0, support::at(0));
    let dropped = support::window("win-dropped", "pool-1", 12.0, support::at(0));

    repo.replace_readings(&account_id, &[kept.clone(), dropped.clone()])
        .await
        .unwrap();
    assert_eq!(
        repo.windows_for_account(&account_id).await.unwrap().len(),
        2
    );

    // The provider now reports only one window.
    repo.replace_readings(&account_id, std::slice::from_ref(&kept))
        .await
        .unwrap();

    let restored = repo.windows_for_account(&account_id).await.unwrap();
    assert_eq!(
        restored.len(),
        1,
        "the window that is no longer reported must not be restored"
    );
    assert_eq!(restored[0].id, kept.id);
    assert!(
        repo.latest(&account_id, &dropped.id)
            .await
            .unwrap()
            .is_none(),
        "the removed window keeps no current reading"
    );
}
