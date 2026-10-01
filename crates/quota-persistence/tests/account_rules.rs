//! The rejections every account and connection write must produce.
//!
//! Fixtures use `unwrap` for readability; this file is not `#[cfg(test)]`, so
//! the workspace's test-mode allowance does not reach it.

#![expect(
    clippy::unwrap_used,
    reason = "test fixtures assert the setup they build, so a broken fixture must fail loudly"
)]

mod support;

use quota_domain::ids::ConnectionId;
use quota_persistence::PersistenceError;
use quota_persistence::SqliteRepositories;
use support::TempDir;

#[tokio::test]
async fn two_accounts_may_not_share_one_connection_ordinal() {
    let directory = TempDir::new("ordinal");
    let pool = support::migrated(&directory).await;
    let accounts = SqliteRepositories::new(pool.clone()).accounts().clone();

    accounts
        .upsert_connection(&support::connection("conn-1"))
        .await
        .unwrap();
    accounts
        .upsert_account(&support::account("acct-1", "conn-1", 0, "First"))
        .await
        .unwrap();

    let refused = accounts
        .upsert_account(&support::account("acct-2", "conn-1", 0, "Second"))
        .await;
    assert_eq!(
        refused.unwrap_err(),
        PersistenceError::IntegrityViolation {
            constraint: "accounts"
        },
        "the unique ordinal is the connection's identity order"
    );

    pool.close().await;
}

#[tokio::test]
async fn an_account_may_not_name_a_connection_that_does_not_exist() {
    let directory = TempDir::new("orphan-account");
    let pool = support::migrated(&directory).await;
    let repositories = SqliteRepositories::new(pool.clone());

    let refused = repositories
        .accounts()
        .upsert_account(&support::account("acct-1", "conn-absent", 0, "First"))
        .await;

    assert_eq!(
        refused.unwrap_err(),
        PersistenceError::RowRejected {
            table: "connections",
            reason: "the account names a connection that does not exist"
        }
    );
    pool.close().await;
}

#[tokio::test]
async fn a_missing_identity_is_reported_instead_of_silently_ignored() {
    let directory = TempDir::new("missing");
    let pool = support::migrated(&directory).await;
    let accounts = SqliteRepositories::new(pool.clone()).accounts().clone();

    assert_eq!(
        accounts
            .bump_generation(&ConnectionId::new("conn-absent").unwrap())
            .await
            .unwrap_err(),
        PersistenceError::RowRejected {
            table: "connections",
            reason: "no row matched the requested identity"
        }
    );
    assert_eq!(
        accounts
            .connection(&ConnectionId::new("conn-absent").unwrap())
            .await
            .unwrap_err(),
        PersistenceError::RowRejected {
            table: "connections",
            reason: "no connection matched the requested identity"
        }
    );

    pool.close().await;
}
