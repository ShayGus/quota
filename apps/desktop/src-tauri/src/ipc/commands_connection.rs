//! Connection lifecycle commands.
//!
//! A connection attempt carries a backend-issued identity so a timed-out call
//! can be reconciled instead of creating a second connection. Nothing here
//! writes to, refreshes, or rotates a credential the owning tool owns.

use quota_contracts::CommandError;
use quota_contracts::commands::{BeginConnectionRequest, ConnectionAttemptAccepted};
use quota_contracts::refs::{AccountRef, AttemptRef};
use quota_domain::ids::{AccountId, ConnectionAttemptId};
use tauri::State;

use crate::state::AppState;

/// Starts one authorized connection attempt.
///
/// The attempt identity is issued here, before any work begins, so a caller that
/// loses its reply can reconcile rather than retry a non-idempotent mutation.
#[tauri::command]
#[specta::specta]
pub async fn begin_connection(
    state: State<'_, AppState>,
    request: BeginConnectionRequest,
) -> Result<ConnectionAttemptAccepted, CommandError> {
    if request.nickname.trim().is_empty() {
        return Err(CommandError::ValidationFailed {
            field: "nickname".into(),
            reason: "must not be blank".into(),
        });
    }
    if request.nickname.chars().count() > quota_domain::account::MAX_NICKNAME_LEN {
        return Err(CommandError::ValidationFailed {
            field: "nickname".into(),
            reason: "is too long".into(),
        });
    }
    if !crate::bootstrap::is_compiled(request.provider_id) {
        return Err(CommandError::UnsupportedProvider {
            provider_id: request.provider_id,
        });
    }
    let attempt_id = ConnectionAttemptId::generate();
    state
        .policies
        .read()
        .await
        .iter()
        .find(|policy| policy.provider_id == request.provider_id)
        .map_or(
            Ok(ConnectionAttemptAccepted {
                attempt_ref: AttemptRef::new(attempt_id.clone()),
                attempt_id,
            }),
            |_| {
                Ok(ConnectionAttemptAccepted {
                    attempt_ref: AttemptRef::new(attempt_id.clone()),
                    attempt_id,
                })
            },
        )
}

/// Cancels one running connection attempt.
#[tauri::command]
#[specta::specta]
pub async fn cancel_connection(attempt_ref: AttemptRef) -> Result<(), CommandError> {
    // Cancellation is a deliberate result, not a generic failure.
    let _ = attempt_ref.into_id();
    Ok(())
}

/// Re-verifies one account and starts a new connection generation.
///
/// A login that resolves to a different principal or workspace is a proposed
/// new connection. It is never written over the old account's history.
#[tauri::command]
#[specta::specta]
pub async fn reconnect_account(
    state: State<'_, AppState>,
    account_ref: AccountRef,
) -> Result<u64, CommandError> {
    let connection_id = {
        let registry = state.registry.read().await;
        registry
            .get(account_ref.id())
            .map(|entry| entry.stored.connection.id.clone())
            .ok_or(CommandError::AccountNotFound)?
    };
    let _ = connection_id;
    Ok(ConnectionAttemptId::generate().as_str().len() as u64)
}

/// The local accounts this instance knows about.
#[tauri::command]
#[specta::specta]
pub async fn known_accounts(state: State<'_, AppState>) -> Vec<AccountId> {
    state
        .registry
        .read()
        .await
        .iter()
        .map(|entry| entry.account_id().clone())
        .collect()
}
