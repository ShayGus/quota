//! Account groups: putting the keys of one provider account together.
//!
//! A group is one provider account, such as an `OpenRouter` account with
//! several API keys. Each key stays its own account, read on its own; a group
//! only says which accounts belong together, so the overview can show the
//! account's total beside each key. The provider names no account, so a group
//! is always the person's choice.

#![expect(
    clippy::unreachable,
    clippy::let_underscore_must_use,
    reason = "`#[tauri::command]` expands to `let _check: ReturnType = unreachable!()`, which both lints report against the handler signature"
)]

use quota_contracts::CommandError;
use quota_contracts::refs::AccountRef;
use quota_domain::ids::{AccountGroupId, AccountId};
use tauri::State;

use super::commands::map_core_error;
use crate::state::AppState;

/// Puts accounts of one provider in a new group, and returns the group.
///
/// An account already in another group moves to the new one.
#[tauri::command]
#[specta::specta]
pub async fn create_account_group(
    state: State<'_, AppState>,
    name: String,
    account_refs: Vec<AccountRef>,
) -> Result<AccountGroupId, CommandError> {
    let members: Vec<AccountId> = account_refs.into_iter().map(AccountRef::into_id).collect();
    let group_id = AccountGroupId::generate();
    {
        let _commit = state.monitor.commit().await;
        let mut registry = state.registry.write().await;
        let changed = registry
            .create_group(group_id.clone(), &name, &members)
            .map_err(map_core_error)?;
        save(&state, &registry, &changed).await?;
    }
    state.monitor.publish().await?;
    Ok(group_id)
}

/// Moves one account into an existing group, or out of its group with `None`.
///
/// A group left with no account is deleted.
#[tauri::command]
#[specta::specta]
pub async fn set_account_group(
    state: State<'_, AppState>,
    account_ref: AccountRef,
    group_id: Option<AccountGroupId>,
) -> Result<(), CommandError> {
    let account_id = account_ref.into_id();
    {
        let _commit = state.monitor.commit().await;
        let mut registry = state.registry.write().await;
        registry
            .set_group(&account_id, group_id.as_ref())
            .map_err(map_core_error)?;
        save(&state, &registry, std::slice::from_ref(&account_id)).await?;
    }
    state.monitor.publish().await
}

/// Renames a group without touching its members.
#[tauri::command]
#[specta::specta]
pub async fn rename_account_group(
    state: State<'_, AppState>,
    group_id: AccountGroupId,
    name: String,
) -> Result<(), CommandError> {
    {
        let _commit = state.monitor.commit().await;
        let mut registry = state.registry.write().await;
        let changed = registry
            .rename_group(&group_id, &name)
            .map_err(map_core_error)?;
        save(&state, &registry, &changed).await?;
    }
    state.monitor.publish().await
}

/// Shows or hides the line of what a group's keys spent together.
#[tauri::command]
#[specta::specta]
pub async fn set_group_spend_shown(
    state: State<'_, AppState>,
    group_id: AccountGroupId,
    shown: bool,
) -> Result<(), CommandError> {
    {
        let _commit = state.monitor.commit().await;
        let mut registry = state.registry.write().await;
        let changed = registry
            .set_group_spend_shown(&group_id, shown)
            .map_err(map_core_error)?;
        save(&state, &registry, &changed).await?;
    }
    state.monitor.publish().await
}

/// Shows or hides one key in its group's card and in the widget. A hidden
/// key still counts in the account's total.
#[tauri::command]
#[specta::specta]
pub async fn set_group_key_shown(
    state: State<'_, AppState>,
    account_ref: AccountRef,
    shown: bool,
) -> Result<(), CommandError> {
    let account_id = account_ref.into_id();
    {
        let _commit = state.monitor.commit().await;
        let mut registry = state.registry.write().await;
        registry
            .set_group_key_shown(&account_id, shown)
            .map_err(map_core_error)?;
        save(&state, &registry, std::slice::from_ref(&account_id)).await?;
    }
    state.monitor.publish().await
}

/// Saves each changed account as the registry now holds it.
async fn save(
    state: &AppState,
    registry: &quota_core::accounts::AccountRegistry,
    changed: &[AccountId],
) -> Result<(), CommandError> {
    for account_id in changed {
        let stored = registry
            .get(account_id)
            .map(|entry| entry.stored.clone())
            .ok_or(CommandError::AccountNotFound)?;
        state
            .accounts
            .upsert_account(stored)
            .await
            .map_err(|error| CommandError::PersistenceUnavailable {
                owner: error.owner.into(),
            })?;
    }
    Ok(())
}
