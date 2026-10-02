//! A confirmed account is saved whole or not at all.
//!
//! The account port writes a connection, an account row, its identity, its
//! read times, its pool bindings and its readings. A failure at any of those
//! steps must leave none of them behind, or the next start would load a
//! half-saved account with no windows.

#![expect(
    clippy::tests_outside_test_module,
    reason = "an integration test binary holds nothing but tests and compiles without cfg(test)"
)]
#![expect(
    clippy::unwrap_used,
    reason = "test fixtures assert setup; a broken fixture must fail loudly"
)]

mod support;

use quota_core::ports::{AccountRepository as AccountPort, StoredAccount};
use quota_domain::account::{
    AccountCardinality, ConnectionState, ConnectionSummary, CredentialOwnership, FetchState,
    VerifiedIdentity,
};
use quota_domain::ids::{AccountId, ConnectionId};
use quota_domain::provider::ProviderId;
use quota_persistence::SqliteRepositories;
use quota_persistence::ports::SqliteAccountPortAdapter;
use support::{TempDir, at, migrated, window};

/// A confirmed, never-saved account with one reading.
fn confirmed_account() -> StoredAccount {
    StoredAccount {
        account_id: AccountId::new("acct-new").unwrap(),
        connection: ConnectionSummary {
            id: ConnectionId::new("conn-new").unwrap(),
            provider_id: ProviderId::Codex,
            credential_ownership: CredentialOwnership::AppOwned,
            generation: 0,
            profile_label: None,
            cardinality: AccountCardinality::Independent,
            state: ConnectionState::Connected,
            principal_id: None,
            workspace_id: None,
            entitlement_id: None,
        },
        nickname: "Work".to_owned(),
        connection_ordinal: 1,
        monitoring_enabled: true,
        connection_state: ConnectionState::Connected,
        fetch_state: FetchState::Idle,
        last_attempt_at: Some(at(1)),
        last_success_at: Some(at(2)),
        next_attempt_at: Some(at(3)),
        identity: Some(VerifiedIdentity {
            principal_label: "work@example.test".to_owned(),
            workspace_label: Some("Work".to_owned()),
            plan_label: None,
            source: quota_domain::quota::window::SourceKind::DocumentedApi,
        }),
        windows: vec![window("weekly-new", "pool-new", 41.0, at(2))],
        expected_but_missing_window_ids: Vec::new(),
    }
}

/// Makes one late write of the new account fail, as a full disk or a lost
/// connection would.
const FAILURES: [(&str, &str); 2] = [
    (
        "the reading",
        "CREATE TRIGGER fail_late BEFORE INSERT ON latest_measurements
         WHEN NEW.account_id = 'acct-new'
         BEGIN SELECT RAISE(ABORT, 'injected failure'); END",
    ),
    (
        "the fetch state",
        "CREATE TRIGGER fail_late BEFORE UPDATE OF fetch_state ON accounts
         WHEN NEW.id = 'acct-new'
         BEGIN SELECT RAISE(ABORT, 'injected failure'); END",
    ),
];

#[tokio::test]
async fn a_failed_late_write_leaves_no_part_of_the_account_after_a_restart() {
    for (step, trigger) in FAILURES {
        let directory = TempDir::new("atomic-save");
        let pool = migrated(&directory).await;
        sqlx::query(trigger).execute(&pool).await.unwrap();
        let port = SqliteAccountPortAdapter::new(SqliteRepositories::new(pool.clone()));

        let saved = port.upsert_account(confirmed_account()).await;
        assert!(
            saved.is_err(),
            "a failure writing {step} must refuse the save"
        );

        pool.close().await;
        let reopened = migrated(&directory).await;
        let repositories = SqliteRepositories::new(reopened.clone());
        let port = SqliteAccountPortAdapter::new(repositories.clone());
        assert!(
            port.load_accounts().await.unwrap().is_empty(),
            "after failing on {step}, no account may load after a restart"
        );
        assert!(
            repositories
                .accounts()
                .connection(&ConnectionId::new("conn-new").unwrap())
                .await
                .is_err(),
            "after failing on {step}, no connection may remain"
        );
        reopened.close().await;
    }
}

#[tokio::test]
async fn a_confirmed_account_without_failures_is_saved_whole() {
    let directory = TempDir::new("atomic-save-ok");
    let pool = migrated(&directory).await;
    let port = SqliteAccountPortAdapter::new(SqliteRepositories::new(pool.clone()));
    port.upsert_account(confirmed_account()).await.unwrap();

    pool.close().await;
    let reopened = migrated(&directory).await;
    let port = SqliteAccountPortAdapter::new(SqliteRepositories::new(reopened.clone()));
    let loaded = port.load_accounts().await.unwrap();
    assert_eq!(loaded.len(), 1);
    let account = &loaded[0];
    assert_eq!(account.nickname, "Work");
    assert_eq!(account.windows.len(), 1);
    assert_eq!(account.last_success_at, Some(at(2)));
    assert_eq!(
        account
            .identity
            .as_ref()
            .map(|identity| identity.principal_label.as_str()),
        Some("work@example.test")
    );
    reopened.close().await;
}
