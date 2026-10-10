#![doc = "Account groups: which accounts are one provider account."]
#![expect(
    clippy::tests_outside_test_module,
    reason = "an integration test binary holds nothing but tests and compiles without cfg(test)"
)]
#![expect(clippy::unwrap_used, reason = "synthetic account fixtures")]

use quota_core::{AccountRegistry, CoreError, StoredAccount};
use quota_domain::account::{
    AccountCardinality, ConnectionState, ConnectionSummary, CredentialOwnership, FetchState,
};
use quota_domain::ids::{AccountGroupId, AccountId, ConnectionId};
use quota_domain::provider::ProviderId;

fn key(id: &str, provider_id: ProviderId) -> StoredAccount {
    StoredAccount {
        account_id: AccountId::new(id).unwrap(),
        connection: ConnectionSummary {
            id: ConnectionId::new(id).unwrap(),
            provider_id,
            credential_ownership: CredentialOwnership::AppOwned,
            generation: 0,
            profile_label: Some(format!("key-{id}")),
            cardinality: AccountCardinality::SingleProfile,
            state: ConnectionState::Connected,
            principal_id: None,
            workspace_id: None,
            entitlement_id: None,
        },
        nickname: id.into(),
        connection_ordinal: 0,
        monitoring_enabled: true,
        connection_state: ConnectionState::Connected,
        fetch_state: FetchState::Idle,
        last_attempt_at: None,
        last_success_at: None,
        next_attempt_at: None,
        identity: None,
        windows: Vec::new(),
        expected_but_missing_window_ids: Vec::new(),
        balance: None,
        show_key_limit: false,
        group: None,
    }
}

fn id(raw: &str) -> AccountId {
    AccountId::new(raw).unwrap()
}

fn group_id(raw: &str) -> AccountGroupId {
    AccountGroupId::new(raw).unwrap()
}

fn registry() -> AccountRegistry {
    AccountRegistry::from_stored(vec![
        key("personal", ProviderId::Openrouter),
        key("ci", ProviderId::Openrouter),
        key("agent", ProviderId::Openrouter),
        key("claude", ProviderId::Claude),
    ])
}

/// The account's group as (id, name). Every name the tests pass is registered.
fn group_of(registry: &AccountRegistry, account: &str) -> Option<(String, String)> {
    registry
        .get(&id(account))?
        .stored
        .group
        .as_ref()
        .map(|group| (group.id.to_string(), group.name.clone()))
}

#[test]
fn a_new_group_holds_the_named_keys_and_returns_them_to_save() {
    let mut registry = registry();
    let changed = registry
        .create_group(group_id("g"), "  Work  ", &[id("personal"), id("ci")])
        .unwrap();
    assert_eq!(changed, [id("personal"), id("ci")]);
    let expected = Some(("g".to_owned(), "Work".to_owned()));
    assert_eq!(group_of(&registry, "personal"), expected);
    assert_eq!(group_of(&registry, "ci"), expected);
    assert_eq!(group_of(&registry, "agent"), None);
}

#[test]
fn a_group_never_mixes_providers_and_a_refusal_changes_nothing() {
    let mut registry = registry();
    let error = registry
        .create_group(group_id("g"), "Work", &[id("personal"), id("claude")])
        .unwrap_err();
    assert!(matches!(
        error,
        CoreError::Validation { field: "group", .. }
    ));
    assert_eq!(group_of(&registry, "personal"), None);
}

#[test]
fn a_group_needs_a_name_and_at_least_one_known_account() {
    let mut registry = registry();
    assert!(matches!(
        registry.create_group(group_id("g"), "Work", &[]),
        Err(CoreError::Validation {
            field: "members",
            ..
        })
    ));
    assert!(matches!(
        registry.create_group(group_id("g"), "   ", &[id("ci")]),
        Err(CoreError::Validation {
            field: "group name",
            ..
        })
    ));
    assert!(matches!(
        registry.create_group(group_id("g"), "Work", &[id("missing")]),
        Err(CoreError::AccountNotFound(_))
    ));
}

#[test]
fn a_key_joins_and_leaves_an_existing_group() {
    let mut registry = registry();
    registry
        .create_group(group_id("g"), "Work", &[id("personal")])
        .unwrap();
    registry
        .set_group(&id("agent"), Some(&group_id("g")))
        .unwrap();
    assert_eq!(
        group_of(&registry, "agent"),
        Some(("g".to_owned(), "Work".to_owned()))
    );
    registry.set_group(&id("agent"), None).unwrap();
    assert_eq!(group_of(&registry, "agent"), None);
}

#[test]
fn a_key_cannot_join_an_unknown_group_or_one_of_another_provider() {
    let mut registry = registry();
    registry
        .create_group(group_id("g"), "Work", &[id("personal")])
        .unwrap();
    assert!(matches!(
        registry.set_group(&id("ci"), Some(&group_id("missing"))),
        Err(CoreError::Validation { field: "group", .. })
    ));
    assert!(matches!(
        registry.set_group(&id("claude"), Some(&group_id("g"))),
        Err(CoreError::Validation { field: "group", .. })
    ));
    assert_eq!(group_of(&registry, "claude"), None);
}

#[test]
fn renaming_a_group_renames_it_for_every_member() {
    let mut registry = registry();
    registry
        .create_group(group_id("g"), "Work", &[id("personal"), id("ci")])
        .unwrap();
    let changed = registry.rename_group(&group_id("g"), "Team").unwrap();
    assert_eq!(changed.len(), 2);
    assert_eq!(group_of(&registry, "ci").unwrap().1, "Team");
    assert!(matches!(
        registry.rename_group(&group_id("missing"), "Team"),
        Err(CoreError::Validation { field: "group", .. })
    ));
}

#[test]
fn a_group_hides_its_spend_line_for_every_member() {
    let mut registry = registry();
    registry
        .create_group(group_id("g"), "Work", &[id("personal"), id("ci")])
        .unwrap();
    let changed = registry
        .set_group_spend_shown(&group_id("g"), false)
        .unwrap();
    assert_eq!(changed.len(), 2);
    for account in ["personal", "ci"] {
        let group = registry.get(&id(account)).unwrap().stored.group.clone();
        assert!(!group.unwrap().spend_shown);
    }
    let error = registry
        .set_group_spend_shown(&group_id("missing"), false)
        .unwrap_err();
    assert!(matches!(error, CoreError::Validation { .. }));
}

#[test]
fn a_key_is_hidden_in_its_group_and_shown_again_when_it_rejoins() {
    let mut registry = registry();
    registry
        .create_group(group_id("g"), "Work", &[id("personal"), id("ci")])
        .unwrap();
    registry.set_group_key_shown(&id("ci"), false).unwrap();
    let shown = |registry: &AccountRegistry, account: &str| {
        registry
            .get(&id(account))
            .unwrap()
            .stored
            .group
            .as_ref()
            .map(|group| group.key_shown)
    };
    assert_eq!(shown(&registry, "ci"), Some(false));
    assert_eq!(shown(&registry, "personal"), Some(true));
    // A key that joins copies the group from a hidden member, yet joins shown.
    registry
        .set_group(&id("agent"), Some(&group_id("g")))
        .unwrap();
    assert_eq!(shown(&registry, "agent"), Some(true));
    // An ungrouped account has no group to be shown in.
    let error = registry
        .set_group_key_shown(&id("claude"), false)
        .unwrap_err();
    assert!(matches!(error, CoreError::Validation { .. }));
}
