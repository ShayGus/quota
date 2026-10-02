#![doc = "Account dispatch binding behavior."]
#![expect(clippy::unwrap_used, reason = "synthetic account fixtures")]

use chrono::{DateTime, Duration};
use quota_core::{AccountRegistry, CoreError, StoredAccount};
use quota_domain::account::{
    AccountCardinality, ConnectionState, ConnectionSummary, CredentialOwnership, FetchState,
};
use quota_domain::ids::{AccountId, ConnectionId};
use quota_domain::provider::ProviderId;

fn registry() -> (AccountRegistry, AccountId, ConnectionId) {
    let account_id = AccountId::new("synthetic-account").unwrap();
    let connection_id = ConnectionId::new("synthetic-connection").unwrap();
    let stored = StoredAccount {
        account_id: account_id.clone(),
        connection: ConnectionSummary {
            id: connection_id.clone(),
            provider_id: ProviderId::Codex,
            credential_ownership: CredentialOwnership::ExternalClient,
            generation: 0,
            profile_label: Some("codex-home-default".into()),
            cardinality: AccountCardinality::SingleProfile,
            state: ConnectionState::Connected,
            principal_id: None,
            workspace_id: None,
            entitlement_id: None,
        },
        nickname: "Codex".into(),
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
    };
    (
        AccountRegistry::from_stored(vec![stored]),
        account_id,
        connection_id,
    )
}

#[test]
fn reconnect_before_dispatch_rejects_the_old_binding_without_stamping() {
    let (mut registry, account_id, connection_id) = registry();
    let captured = registry.get(&account_id).unwrap().binding.clone();
    registry.set_generation(&connection_id, 1).unwrap();
    let before = registry.get(&account_id).unwrap().clone();
    let now = DateTime::UNIX_EPOCH;
    assert!(matches!(
        registry.record_dispatch(
            &account_id,
            &captured,
            now,
            Some(now + Duration::seconds(300))
        ),
        Err(CoreError::StaleResult)
    ));
    assert_eq!(registry.get(&account_id), Some(&before));

    let dispatched = registry
        .record_dispatch(
            &account_id,
            &before.binding,
            now,
            Some(now + Duration::seconds(300)),
        )
        .unwrap();
    assert_eq!(dispatched.binding, before.binding);
    assert_eq!(dispatched.stored.last_attempt_at, Some(now));
    assert_eq!(dispatched.stored.last_success_at, None);
    assert_eq!(
        dispatched.stored.next_attempt_at,
        Some(now + Duration::seconds(300))
    );
}
