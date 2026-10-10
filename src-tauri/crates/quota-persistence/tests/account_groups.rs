//! An account's group is saved with the account and lives as long as it has
//! members.
//!
//! A group is saved in the same transaction as each member, loads back the
//! same after a restart, keeps one name for every member, keeps what it
//! shows, and is deleted when its last member leaves it or is removed.

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
use quota_domain::group::AccountGroup;
use quota_domain::ids::{AccountGroupId, AccountId, ConnectionId};
use quota_domain::provider::ProviderId;
use quota_persistence::SqliteRepositories;
use quota_persistence::ports::SqliteAccountPortAdapter;
use support::{TempDir, at, migrated};

fn group(name: &str) -> AccountGroup {
    AccountGroup::new(AccountGroupId::new("group-work").unwrap(), name).unwrap()
}

/// One `OpenRouter` key, in `group` when given.
fn key(id: &str, ordinal: u32, group: Option<AccountGroup>) -> StoredAccount {
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
        last_attempt_at: Some(at(1)),
        last_success_at: Some(at(1)),
        next_attempt_at: None,
        identity: None,
        windows: Vec::new(),
        expected_but_missing_window_ids: Vec::new(),
        balance: None,
        show_key_limit: false,
        group,
    }
}

async fn group_rows(pool: &sqlx::SqlitePool) -> Vec<(String, String, String)> {
    sqlx::query_as("SELECT id, provider_id, name FROM account_groups ORDER BY id")
        .fetch_all(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_group_loads_back_after_a_restart_with_every_member() {
    let directory = TempDir::new("groups-roundtrip");
    let pool = migrated(&directory).await;
    let port = SqliteAccountPortAdapter::new(SqliteRepositories::new(pool.clone()));
    port.upsert_account(key("personal", 1, Some(group("Work"))))
        .await
        .unwrap();
    port.upsert_account(key("ci", 2, Some(group("Work"))))
        .await
        .unwrap();
    port.upsert_account(key("alone", 3, None)).await.unwrap();
    pool.close().await;

    let reopened = migrated(&directory).await;
    let port = SqliteAccountPortAdapter::new(SqliteRepositories::new(reopened.clone()));
    let loaded = port.load_accounts().await.unwrap();
    let group_of = |id: &str| {
        loaded
            .iter()
            .find(|account| account.account_id.as_str() == id)
            .unwrap()
            .group
            .clone()
    };
    assert_eq!(group_of("personal"), Some(group("Work")));
    assert_eq!(group_of("ci"), Some(group("Work")));
    assert_eq!(group_of("alone"), None);
    assert_eq!(
        group_rows(&reopened).await,
        [(
            "group-work".to_owned(),
            "openrouter".to_owned(),
            "Work".to_owned()
        )]
    );
    let snapshot = port
        .snapshot_of(&AccountId::new("ci").unwrap())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.group, Some(group("Work")));
    reopened.close().await;
}

#[tokio::test]
async fn saving_a_member_with_a_new_name_renames_the_group_for_all() {
    let directory = TempDir::new("groups-rename");
    let pool = migrated(&directory).await;
    let port = SqliteAccountPortAdapter::new(SqliteRepositories::new(pool.clone()));
    port.upsert_account(key("personal", 1, Some(group("Work"))))
        .await
        .unwrap();
    port.upsert_account(key("ci", 2, Some(group("Work"))))
        .await
        .unwrap();
    port.upsert_account(key("ci", 2, Some(group("Team"))))
        .await
        .unwrap();
    let loaded = port.load_accounts().await.unwrap();
    assert!(
        loaded
            .iter()
            .all(|account| account.group == Some(group("Team")))
    );
    pool.close().await;
}

#[tokio::test]
async fn a_group_is_deleted_when_its_last_member_leaves_or_is_removed() {
    let directory = TempDir::new("groups-empty");
    let pool = migrated(&directory).await;
    let port = SqliteAccountPortAdapter::new(SqliteRepositories::new(pool.clone()));
    port.upsert_account(key("personal", 1, Some(group("Work"))))
        .await
        .unwrap();
    port.upsert_account(key("ci", 2, Some(group("Work"))))
        .await
        .unwrap();

    port.upsert_account(key("personal", 1, None)).await.unwrap();
    assert_eq!(group_rows(&pool).await.len(), 1, "ci still holds the group");

    port.remove_account(&AccountId::new("ci").unwrap())
        .await
        .unwrap();
    assert!(group_rows(&pool).await.is_empty());
    let loaded = port.load_accounts().await.unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].group, None);
    pool.close().await;
}

#[tokio::test]
async fn what_a_group_shows_loads_back_after_a_restart() {
    let directory = TempDir::new("groups-display");
    let pool = migrated(&directory).await;
    let port = SqliteAccountPortAdapter::new(SqliteRepositories::new(pool.clone()));
    let quiet = AccountGroup {
        spend_shown: false,
        ..group("Work")
    };
    port.upsert_account(key("personal", 1, Some(quiet.clone())))
        .await
        .unwrap();
    let hidden = AccountGroup {
        key_shown: false,
        ..quiet.clone()
    };
    port.upsert_account(key("ci", 2, Some(hidden.clone())))
        .await
        .unwrap();
    pool.close().await;

    let reopened = migrated(&directory).await;
    let port = SqliteAccountPortAdapter::new(SqliteRepositories::new(reopened.clone()));
    let loaded = port.load_accounts().await.unwrap();
    let group_of = |id: &str| {
        loaded
            .iter()
            .find(|account| account.account_id.as_str() == id)
            .unwrap()
            .group
            .clone()
    };
    assert_eq!(group_of("personal"), Some(quiet));
    assert_eq!(group_of("ci"), Some(hidden));

    // Leaving the group forgets that the key was hidden in it.
    port.upsert_account(key("ci", 2, None)).await.unwrap();
    port.upsert_account(key("ci", 2, Some(group("Work"))))
        .await
        .unwrap();
    let reloaded = port.load_accounts().await.unwrap();
    let ci = reloaded
        .iter()
        .find(|account| account.account_id.as_str() == "ci")
        .unwrap();
    assert!(ci.group.as_ref().unwrap().key_shown);
    reopened.close().await;
}
