//! A prepaid balance and the key-limit switch are an account's own state.
//!
//! They are saved with the account in the same transaction, load back the same
//! after a restart, and leave with the account, never with a sibling.

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
};
use quota_domain::balance::{BalanceLedger, BalanceReading, BaselineKind};
use quota_domain::ids::{AccountId, ConnectionId};
use quota_domain::provider::ProviderId;
use quota_domain::quota::units::CurrencyCode;
use quota_persistence::SqliteRepositories;
use quota_persistence::ports::SqliteAccountPortAdapter;
use support::{TempDir, at, migrated, window};

fn reading(loaded: i64, spent: i64) -> BalanceReading {
    BalanceReading {
        currency: CurrencyCode::new("USD").unwrap(),
        scale: 2,
        loaded_minor: loaded,
        spent_minor: spent,
        key_spend: None,
        credits: Vec::new(),
        cycle_spend: None,
    }
}

/// An `OpenRouter` account whose balance has seen one top-up.
fn account(id: &str, ordinal: u32) -> StoredAccount {
    let first = BalanceLedger::record(None, &reading(2_000, 1_500), at(0));
    let topped = BalanceLedger::record(Some(&first), &reading(7_000, 1_700), at(3));
    StoredAccount {
        account_id: AccountId::new(id).unwrap(),
        connection: ConnectionSummary {
            id: ConnectionId::new(format!("conn-{id}")).unwrap(),
            provider_id: ProviderId::Openrouter,
            credential_ownership: CredentialOwnership::AppOwned,
            generation: 0,
            profile_label: None,
            cardinality: AccountCardinality::Independent,
            state: ConnectionState::Connected,
            principal_id: None,
            workspace_id: None,
            entitlement_id: None,
        },
        nickname: id.to_owned(),
        connection_ordinal: ordinal,
        monitoring_enabled: true,
        connection_state: ConnectionState::Connected,
        fetch_state: FetchState::Idle,
        last_attempt_at: Some(at(3)),
        last_success_at: Some(at(3)),
        next_attempt_at: None,
        identity: None,
        windows: vec![window(
            &format!("w-{id}"),
            &format!("pool-{id}"),
            50.0,
            at(3),
        )],
        expected_but_missing_window_ids: Vec::new(),
        balance: Some(topped),
        show_key_limit: true,
        group: None,
    }
}

#[tokio::test]
async fn the_ledger_and_the_switch_load_back_after_a_restart() {
    let directory = TempDir::new("balance-roundtrip");
    let pool = migrated(&directory).await;
    let port = SqliteAccountPortAdapter::new(SqliteRepositories::new(pool.clone()));
    let saved = account("acct-or", 1);
    port.upsert_account(saved.clone()).await.unwrap();
    pool.close().await;

    let reopened = migrated(&directory).await;
    let port = SqliteAccountPortAdapter::new(SqliteRepositories::new(reopened.clone()));
    let loaded = port.load_accounts().await.unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].balance, saved.balance);
    assert!(loaded[0].show_key_limit);
    let ledger = loaded[0].balance.as_ref().unwrap();
    assert_eq!(ledger.baseline_kind, BaselineKind::TopUp);
    assert_eq!(ledger.top_ups.len(), 1);

    let snapshot = port
        .snapshot_of(&AccountId::new("acct-or").unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        snapshot.balance.map(|summary| summary.balance_minor),
        Some(5_300)
    );
    assert!(snapshot.show_key_limit);
    reopened.close().await;
}

#[tokio::test]
async fn a_saved_account_without_a_balance_has_its_ledger_removed() {
    let directory = TempDir::new("balance-cleared");
    let pool = migrated(&directory).await;
    let port = SqliteAccountPortAdapter::new(SqliteRepositories::new(pool.clone()));
    port.upsert_account(account("acct-or", 1)).await.unwrap();
    let mut cleared = account("acct-or", 1);
    cleared.balance = None;
    cleared.show_key_limit = false;
    port.upsert_account(cleared).await.unwrap();
    let loaded = port.load_accounts().await.unwrap();
    assert_eq!(loaded[0].balance, None);
    assert!(!loaded[0].show_key_limit);
    pool.close().await;
}

#[tokio::test]
async fn removing_an_account_removes_only_its_own_ledger() {
    let directory = TempDir::new("balance-removed");
    let pool = migrated(&directory).await;
    let port = SqliteAccountPortAdapter::new(SqliteRepositories::new(pool.clone()));
    port.upsert_account(account("acct-a", 1)).await.unwrap();
    port.upsert_account(account("acct-b", 2)).await.unwrap();

    port.remove_account(&AccountId::new("acct-a").unwrap())
        .await
        .unwrap();

    let rows: Vec<String> = sqlx::query_scalar("SELECT account_id FROM account_balances")
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rows, vec!["acct-b".to_owned()]);
    pool.close().await;
}

#[tokio::test]
async fn a_failed_ledger_write_saves_nothing_of_the_account() {
    let directory = TempDir::new("balance-atomic");
    let pool = migrated(&directory).await;
    sqlx::query(
        "CREATE TRIGGER fail_balance BEFORE INSERT ON account_balances
         BEGIN SELECT RAISE(ABORT, 'injected failure'); END",
    )
    .execute(&pool)
    .await
    .unwrap();
    let port = SqliteAccountPortAdapter::new(SqliteRepositories::new(pool.clone()));
    assert!(port.upsert_account(account("acct-or", 1)).await.is_err());
    assert!(port.load_accounts().await.unwrap().is_empty());
    pool.close().await;
}
