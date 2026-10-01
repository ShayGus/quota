//! Connections and quota-pool bindings.
//!
//! Fixtures use `unwrap` for readability; this file is not `#[cfg(test)]`, so
//! the workspace's test-mode allowance does not reach it.

#![expect(
    clippy::unwrap_used,
    reason = "test fixtures assert the setup they build, so a broken fixture must fail loudly"
)]

mod support;

use quota_domain::ids::{AccountId, ConnectionId, QuotaPoolId};
use quota_domain::provider::ProviderId;
use quota_persistence::SqliteRepositories;
use support::TempDir;

#[tokio::test]
async fn a_connection_generation_advances_and_survives_a_restart() {
    let directory = TempDir::new("generation");
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
    assert_eq!(
        accounts
            .list_for_connection(&ConnectionId::new("conn-1").unwrap())
            .await
            .unwrap()[0]
            .generation,
        0
    );

    assert_eq!(
        accounts
            .bump_generation(&ConnectionId::new("conn-1").unwrap())
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        accounts
            .list_for_connection(&ConnectionId::new("conn-1").unwrap())
            .await
            .unwrap()[0]
            .generation,
        1,
        "the account reads the connection's current generation"
    );

    // A late result from the replaced attempt can be recognised by generation.
    let reopened = SqliteRepositories::new(pool.clone());
    assert_eq!(
        reopened
            .accounts()
            .connection(&ConnectionId::new("conn-1").unwrap())
            .await
            .unwrap()
            .generation,
        1,
        "the generation must survive a reopen"
    );

    pool.close().await;
}

#[tokio::test]
async fn a_bound_pool_is_shared_between_two_accounts_of_one_provider() {
    let directory = TempDir::new("pool-binding");
    let pool = support::migrated(&directory).await;
    let accounts = SqliteRepositories::new(pool.clone()).accounts().clone();

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

    let shared = QuotaPoolId::new("pool-shared").unwrap();
    for account in ["acct-a", "acct-b"] {
        accounts
            .bind_pool(
                &AccountId::new(account).unwrap(),
                &shared,
                ProviderId::Codex,
                true,
            )
            .await
            .unwrap();
    }

    let bindings: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM account_pool_bindings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(bindings, 2, "both accounts reference the one shared pool");

    // Deleting one account removes only its own binding.
    assert!(
        accounts
            .delete_account(&AccountId::new("acct-a").unwrap())
            .await
            .unwrap()
    );
    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM account_pool_bindings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(remaining, 1, "the sibling's binding must survive");

    pool.close().await;
}
