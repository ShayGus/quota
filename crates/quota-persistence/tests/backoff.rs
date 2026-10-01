//! Scoped retry deadlines and rate-limit checks.
//!
//! Fixtures use `unwrap` for readability; this file is not `#[cfg(test)]`, so
//! the workspace's test-mode allowance does not reach it.

#![expect(
    clippy::unwrap_used,
    reason = "test fixtures assert the setup they build, so a broken fixture must fail loudly"
)]

mod support;

use quota_domain::ids::{AccountId, ConnectionId, QuotaPoolId};
use quota_domain::polling::LimitScope;
use quota_domain::provider::ProviderId;
use quota_persistence::SqliteRepositories;
use quota_persistence::sqlite::BackoffRecord;
use support::TempDir;

#[tokio::test]
async fn backoff_persists_and_answers_is_rate_limited_now() {
    let directory = TempDir::new("backoff");
    let pool = support::migrated(&directory).await;
    let repo = SqliteRepositories::new(pool.clone());
    let repo = repo.backoff();

    let scope = support::provider_scope();
    assert!(
        !repo
            .is_rate_limited_now(&scope, support::at(0))
            .await
            .unwrap()
    );

    let record = BackoffRecord {
        scope: scope.clone(),
        attempts: 1,
        next_eligible_at: support::at(2),
        provider_retry_after: Some(support::at(3)),
    };
    repo.persist(&record).await.unwrap();

    assert!(
        repo.is_rate_limited_now(&scope, support::at(1))
            .await
            .unwrap()
    );
    assert!(
        !repo
            .is_rate_limited_now(&scope, support::at(2))
            .await
            .unwrap(),
        "the deadline instant itself is eligible again"
    );
    assert!(
        !repo
            .is_rate_limited_now(&scope, support::at(5))
            .await
            .unwrap()
    );

    let read = repo.read(&scope).await.unwrap().unwrap();
    assert_eq!(read, record);

    // A second failure advances the attempt count and replaces the deadline.
    let second = repo
        .record_failure(&scope, support::at(4), None)
        .await
        .unwrap();
    assert_eq!(second.attempts, 2);
    assert_eq!(second.provider_retry_after, None);
    assert!(
        repo.is_rate_limited_now(&scope, support::at(1))
            .await
            .unwrap()
    );
    assert!(
        !repo
            .is_rate_limited_now(&scope, support::at(4))
            .await
            .unwrap()
    );

    assert!(repo.clear(&scope).await.unwrap());
    assert!(repo.read(&scope).await.unwrap().is_none());
    assert!(
        !repo.clear(&scope).await.unwrap(),
        "clearing twice is a no-op"
    );

    pool.close().await;
}

#[tokio::test]
async fn a_provider_limit_does_not_limit_a_different_scope() {
    let directory = TempDir::new("backoff-scope-isolation");
    let pool = support::migrated(&directory).await;
    let repo = SqliteRepositories::new(pool.clone());
    let repo = repo.backoff();

    let provider = LimitScope::Provider(ProviderId::Claude);
    let account = LimitScope::Account(AccountId::new("acct-1").unwrap());

    repo.record_failure(&provider, support::at(6), None)
        .await
        .unwrap();

    assert!(
        repo.is_rate_limited_now(&provider, support::at(1))
            .await
            .unwrap()
    );
    assert!(
        !repo
            .is_rate_limited_now(&account, support::at(1))
            .await
            .unwrap(),
        "a provider-wide limit must not become an account limit"
    );
    assert!(repo.read(&account).await.unwrap().is_none());

    pool.close().await;
}

#[tokio::test]
async fn backoff_round_trips_every_scope_kind() {
    let directory = TempDir::new("backoff-scopes");
    let pool = support::migrated(&directory).await;
    let repo = SqliteRepositories::new(pool.clone());
    let repo = repo.backoff();

    let scopes = [
        LimitScope::Account(AccountId::new("acct-1").unwrap()),
        LimitScope::Connection(ConnectionId::new("conn-1").unwrap()),
        LimitScope::Provider(ProviderId::OpenCodeGo),
        LimitScope::QuotaPool(QuotaPoolId::new("pool-1").unwrap()),
        LimitScope::SourceAddress,
    ];

    for scope in &scopes {
        repo.persist(&BackoffRecord {
            scope: scope.clone(),
            attempts: 2,
            next_eligible_at: support::at(12),
            provider_retry_after: None,
        })
        .await
        .unwrap();
    }

    for (index, scope) in scopes.iter().enumerate() {
        let read = repo.read(scope).await.unwrap().unwrap();
        assert_eq!(&read.scope, scope, "scope {index} did not round trip");
    }

    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM refresh_backoff")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        usize::try_from(rows).unwrap(),
        scopes.len(),
        "each scope owns exactly one row"
    );

    // The deadline survives a restart, so a provider-wide limit cannot be
    // bypassed by relaunching.
    let reopened = SqliteRepositories::new(pool.clone());
    assert!(
        reopened
            .backoff()
            .is_rate_limited_now(&scopes[2], support::at(1))
            .await
            .unwrap()
    );

    pool.close().await;
}

#[tokio::test]
async fn a_backoff_row_rejects_a_scope_with_a_negative_attempt_count() {
    let directory = TempDir::new("backoff-negative");
    let pool = support::migrated(&directory).await;

    sqlx::query(
        "INSERT INTO refresh_backoff (scope_kind, scope_id, attempts, next_eligible_at)
         VALUES ('provider', 'claude', -1, '2026-03-01T02:00:00.000Z')",
    )
    .execute(&pool)
    .await
    .unwrap();

    let repo = SqliteRepositories::new(pool.clone());
    let read = repo
        .backoff()
        .read(&LimitScope::Provider(ProviderId::Claude))
        .await;
    assert!(
        read.is_err(),
        "a negative attempt count must be rejected, not reported"
    );

    pool.close().await;
}
