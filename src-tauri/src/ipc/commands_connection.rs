//! Connection lifecycle commands.
//!
//! Each attempt receives a backend-issued identity so a timed-out renderer call
//! can be reconciled instead of creating a duplicate connection. Credential
//! refresh remains owned by the provider's local tool.

#![expect(
    clippy::unreachable,
    clippy::let_underscore_must_use,
    reason = "`#[tauri::command]` expands to `let _check: ReturnType = unreachable!()`, which both lints report against the handler signature"
)]

use quota_contracts::CommandError;
use quota_contracts::commands::{BeginConnectionRequest, ConnectionAttemptAccepted};
use quota_contracts::refs::{AccountRef, AttemptRef};
use tauri::State;

use crate::state::AppState;

/// Starts one cancellable local-credential connection attempt.
#[tauri::command]
#[specta::specta]
pub async fn begin_connection(
    state: State<'_, AppState>,
    request: BeginConnectionRequest,
) -> Result<ConnectionAttemptAccepted, CommandError> {
    state.monitor.begin_connection(request).await
}

/// Saves the verified candidate one attempt is holding, under the nickname the
/// person confirmed.
///
/// Nothing is written until this command runs, so declining a verified
/// connection in the wizard leaves storage untouched.
#[tauri::command]
#[specta::specta]
pub async fn confirm_connection(
    state: State<'_, AppState>,
    attempt_ref: AttemptRef,
    nickname: String,
    group: Option<quota_contracts::commands::KeyGroupChoice>,
) -> Result<(), CommandError> {
    state
        .monitor
        .confirm_connection(attempt_ref.id(), nickname, group)
        .await
}

/// Cancels one running attempt, or discards one verified candidate.
#[tauri::command]
#[specta::specta]
pub async fn cancel_connection(
    state: State<'_, AppState>,
    attempt_ref: AttemptRef,
) -> Result<(), CommandError> {
    state.monitor.cancel_connection(attempt_ref.id()).await
}

/// Re-verifies one account under a new generation and queues a fresh read.
#[tauri::command]
#[specta::specta]
pub async fn reconnect_account(
    state: State<'_, AppState>,
    account_ref: AccountRef,
) -> Result<u32, CommandError> {
    state.monitor.reconnect_account(account_ref.id()).await
}
