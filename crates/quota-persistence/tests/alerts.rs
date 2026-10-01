//! Alert episodes and notification deduplication.
//!
//! Fixtures use `unwrap` for readability; this file is not `#[cfg(test)]`, so
//! the workspace's test-mode allowance does not reach it.

#![expect(
    clippy::unwrap_used,
    reason = "test fixtures assert the setup they build, so a broken fixture must fail loudly"
)]

mod support;

use quota_domain::ids::{AccountId, QuotaWindowId};
use quota_persistence::SqliteRepositories;
use quota_persistence::sqlite::{AlertLevel, EpisodeKey};
use support::TempDir;

/// An episode key for one account and window.
fn key(account: &str, window: &str, version: u32, level: AlertLevel) -> EpisodeKey {
    EpisodeKey::new(
        AccountId::new(account).unwrap(),
        QuotaWindowId::new(window).unwrap(),
        version,
        level,
    )
}

#[tokio::test]
async fn an_episode_cannot_arm_twice_while_it_stays_open() {
    let directory = TempDir::new("episodes");
    let pool = support::migrated(&directory).await;
    let repo = SqliteRepositories::new(pool.clone());
    let repo = repo.alerts();

    let low = key("acct-1", "win-1", 1, AlertLevel::Low);

    assert!(repo.open_episode(&low, support::at(0)).await.unwrap());
    assert!(repo.is_open(&low).await.unwrap());
    assert!(!repo.is_armed(&low).await.unwrap());

    // A second open for the same key leaves the live episode alone.
    assert!(!repo.open_episode(&low, support::at(1)).await.unwrap());
    assert_eq!(repo.episode(&low).await.unwrap().opened_at, support::at(0));

    repo.mark_armed(&low, support::at(1)).await.unwrap();
    assert!(repo.is_armed(&low).await.unwrap());

    // The armed flag is what stops a second notification for this episode.
    assert!(repo.close_episode(&low, support::at(2)).await.unwrap());
    assert!(!repo.is_open(&low).await.unwrap());
    assert!(
        !repo.close_episode(&low, support::at(3)).await.unwrap(),
        "closing a closed episode changes nothing"
    );

    // A verified recovery plus a new crossing re-opens the same key and
    // re-arms it, so recovery is announced once per episode.
    assert!(repo.open_episode(&low, support::at(4)).await.unwrap());
    let episode = repo.episode(&low).await.unwrap();
    assert_eq!(episode.opened_at, support::at(4));
    assert_eq!(episode.armed_at, None, "the re-opened episode must re-arm");
    assert_eq!(episode.closed_at, None);

    pool.close().await;
}

#[tokio::test]
async fn two_levels_of_one_window_are_independent_episodes() {
    let directory = TempDir::new("episode-levels");
    let pool = support::migrated(&directory).await;
    let repo = SqliteRepositories::new(pool.clone());
    let repo = repo.alerts();

    let low = key("acct-1", "win-1", 1, AlertLevel::Low);
    let critical = key("acct-1", "win-1", 1, AlertLevel::Critical);

    repo.open_episode(&low, support::at(0)).await.unwrap();
    repo.mark_armed(&low, support::at(0)).await.unwrap();

    assert!(repo.open_episode(&critical, support::at(1)).await.unwrap());
    assert!(repo.is_armed(&low).await.unwrap());
    assert!(
        !repo.is_armed(&critical).await.unwrap(),
        "the critical level starts unarmed"
    );

    pool.close().await;
}

#[tokio::test]
async fn a_new_definition_version_starts_its_own_episode() {
    let directory = TempDir::new("episode-version");
    let pool = support::migrated(&directory).await;
    let repo = SqliteRepositories::new(pool.clone());
    let repo = repo.alerts();

    let first = key("acct-1", "win-1", 1, AlertLevel::Low);
    let second = key("acct-1", "win-1", 2, AlertLevel::Low);

    repo.open_episode(&first, support::at(0)).await.unwrap();
    repo.mark_armed(&first, support::at(0)).await.unwrap();

    assert!(repo.open_episode(&second, support::at(1)).await.unwrap());
    assert!(repo.is_armed(&first).await.unwrap());
    assert!(
        !repo.is_armed(&second).await.unwrap(),
        "a new definition starts unarmed"
    );

    pool.close().await;
}

#[tokio::test]
async fn a_missing_episode_is_reported_instead_of_invented() {
    let directory = TempDir::new("episode-missing");
    let pool = support::migrated(&directory).await;
    let repo = SqliteRepositories::new(pool.clone());
    let repo = repo.alerts();

    let unknown = key("acct-absent", "win-absent", 1, AlertLevel::Low);
    assert!(!repo.is_open(&unknown).await.unwrap());
    assert!(repo.mark_armed(&unknown, support::at(0)).await.is_err());
    assert!(repo.episode(&unknown).await.is_err());
    assert!(repo.close_episode(&unknown, support::at(0)).await.is_err());

    pool.close().await;
}

#[tokio::test]
async fn a_duplicate_notification_cannot_be_enqueued_for_one_episode() {
    let directory = TempDir::new("outbox");
    let pool = support::migrated(&directory).await;
    let repo = SqliteRepositories::new(pool.clone());
    let repo = repo.alerts();

    let one = key("acct-1", "win-1", 1, AlertLevel::Critical);
    let sibling = key("acct-2", "win-1", 1, AlertLevel::Critical);

    assert!(
        repo.enqueue_notification(&one, support::at(0))
            .await
            .unwrap()
    );
    assert!(
        !repo
            .enqueue_notification(&one, support::at(1))
            .await
            .unwrap(),
        "a second enqueue for the same episode must be refused"
    );
    assert!(
        repo.enqueue_notification(&sibling, support::at(0))
            .await
            .unwrap(),
        "a sibling account is a different episode"
    );

    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM notification_outbox")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 2, "one entry per episode, and never two for one");
    assert_eq!(repo.undelivered_keys().await.unwrap().len(), 2);
    assert!(repo.is_queued(&one).await.unwrap());
    assert!(
        !repo
            .is_queued(&key("acct-3", "win-1", 1, AlertLevel::Critical))
            .await
            .unwrap()
    );

    pool.close().await;
}

#[tokio::test]
async fn a_delivered_notification_leaves_the_undelivered_list() {
    let directory = TempDir::new("outbox-delivery");
    let pool = support::migrated(&directory).await;
    let repo = SqliteRepositories::new(pool.clone());
    let repo = repo.alerts();

    let entry = key("acct-1", "win-1", 1, AlertLevel::Low);
    assert!(
        repo.enqueue_notification(&entry, support::at(0))
            .await
            .unwrap()
    );
    assert_eq!(repo.undelivered_keys().await.unwrap().len(), 1);

    sqlx::query("UPDATE notification_outbox SET delivered_at = ? WHERE id = 1")
        .bind(support::at(1).to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .execute(&pool)
        .await
        .unwrap();

    assert!(repo.undelivered_keys().await.unwrap().is_empty());
    assert!(
        repo.is_queued(&entry).await.unwrap(),
        "the deduplication entry stays after delivery"
    );

    pool.close().await;
}
