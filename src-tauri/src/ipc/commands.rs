//! Thin command handlers.
//!
//! Handlers validate scope, call one application service or durable port, and
//! return committed state. Refresh commands enter the one shared supervisor.

#![expect(
    clippy::unreachable,
    clippy::let_underscore_must_use,
    reason = "`#[tauri::command]` expands to `let _check: ReturnType = unreachable!()`, which both lints report against the handler signature"
)]

use quota_contracts::CommandError;
use quota_contracts::RegisteredProvider;
use quota_contracts::commands::{AccountSelection, RefreshReason, SnapshotResponse};
use quota_contracts::refs::AccountRef;
use quota_core::clock::Clock;
use quota_domain::account::{CredentialOwnership, FetchState};
use quota_domain::ids::AccountId;
use quota_domain::provider::ProviderId;
use quota_domain::snapshot::{AppSnapshot, PersistenceStatus};
use tauri::{Manager, State};

use crate::state::AppState;

/// Reads the complete sanitized application snapshot.
///
/// The renderer registers its listener first and then calls this once, so a
/// missed event is repaired without any frontend polling.
///
/// A window loads before the backend has finished starting, so this can be
/// called before the application state exists. It then answers
/// `InitializationPending`, which the renderer retries, rather than failing
/// with a framework error the renderer cannot tell from a real failure: a
/// window that gave up there showed no accounts until the next refresh.
///
/// # Errors
/// `InitializationPending` while the backend is starting.
#[tauri::command]
#[specta::specta]
pub async fn get_snapshot(app: tauri::AppHandle) -> Result<SnapshotResponse, CommandError> {
    let Some(state) = app.try_state::<AppState>() else {
        return Err(CommandError::InitializationPending);
    };
    let monitoring = state.monitoring_state.read().await.clone();
    let registry = state.registry.read().await;
    let snapshot: AppSnapshot = state.snapshots.lock().await.build(
        &registry,
        &monitoring,
        &PersistenceStatus::Available,
        state.clock.now(),
    );
    // The tray states the same attention as the renderer from the first read,
    // rather than waiting for the next scheduled publication.
    crate::platform::tray::reflect_snapshot(&state.app, &snapshot);
    let native_window = state.window.lock().await.state();
    crate::platform::window::publish_state(&state.app, &state.app_instance_id, native_window);
    let preferences = state.preferences_state.read().await.clone();
    crate::ipc::events::publish_preferences(&state.app, &state.app_instance_id, &preferences);
    Ok(SnapshotResponse { snapshot })
}

/// Lists every provider this build knows about, including unavailable adapters.
#[tauri::command]
#[specta::specta]
#[must_use]
pub fn list_provider_capabilities() -> Vec<RegisteredProvider> {
    ProviderId::ALL
        .into_iter()
        .map(|provider_id| RegisteredProvider {
            provider_id,
            capabilities: crate::bootstrap::capabilities_of(provider_id),
            compiled_in_this_build: crate::bootstrap::is_compiled(provider_id),
        })
        .collect()
}

/// Requests a refresh through the shared provider scheduler.
#[tauri::command]
#[specta::specta]
pub async fn refresh_accounts(
    state: State<'_, AppState>,
    selection: AccountSelection,
    reason: RefreshReason,
) -> Result<Vec<AccountId>, CommandError> {
    let chosen = {
        let registry = state.registry.read().await;
        match selection {
            AccountSelection::All => registry
                .iter()
                .filter(|entry| entry.stored.monitoring_enabled)
                .map(|entry| entry.account_id().clone())
                .collect(),
            AccountSelection::Listed { account_refs } => {
                let mut ids = Vec::with_capacity(account_refs.len());
                for reference in account_refs {
                    let id = reference.into_id();
                    if registry.get(&id).is_none() {
                        return Err(CommandError::AccountNotFound);
                    }
                    ids.push(id);
                }
                ids
            }
        }
    };
    let reason = match reason {
        RefreshReason::UserRequested => crate::monitoring::RefreshReason::UserRequested,
        RefreshReason::Scheduled => crate::monitoring::RefreshReason::Scheduled,
        RefreshReason::BoundaryVerification => {
            crate::monitoring::RefreshReason::BoundaryVerification
        }
        RefreshReason::OverviewOpened => crate::monitoring::RefreshReason::OverviewOpened,
        RefreshReason::Resumed => crate::monitoring::RefreshReason::Resumed,
    };
    state.monitor.refresh(chosen, reason).await
}

/// Pauses or resumes monitoring and persists the user's choice first.
#[tauri::command]
#[specta::specta]
pub async fn set_monitoring_state(
    state: State<'_, AppState>,
    paused: bool,
) -> Result<quota_domain::snapshot::MonitoringState, CommandError> {
    let next = if paused {
        quota_domain::snapshot::MonitoringState::Paused
    } else {
        quota_domain::snapshot::MonitoringState::Running
    };
    state
        .monitoring_repository
        .save_monitoring_state(&next)
        .await
        .map_err(|error| CommandError::PersistenceUnavailable {
            owner: error.owner.into(),
        })?;
    *state.monitoring_state.write().await = next.clone();
    if !paused {
        state
            .monitor
            .request_all(crate::monitoring::RefreshReason::Resumed)
            .await?;
    }
    // Paused workers produce no snapshot, so the renderer kept showing
    // "Monitoring active" after the user paused.
    state.monitor.publish().await?;
    Ok(next)
}

/// Enables or disables monitoring for one account, then commits that value.
#[tauri::command]
#[specta::specta]
pub async fn set_account_enabled(
    state: State<'_, AppState>,
    request: quota_contracts::SetAccountEnabledRequest,
) -> Result<(), CommandError> {
    let account_id = request.account_ref.id().clone();
    {
        // One boundary from reading the account to writing it back, so a
        // concurrent read cannot commit a copy of the value this is replacing.
        let _commit = state.monitor.commit().await;
        let mut registry = state.registry.write().await;
        registry
            .set_enabled(&account_id, request.enabled)
            .map_err(map_core_error)?;
        let stored = registry
            .get(&account_id)
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
    state.monitor.publish().await?;
    if request.enabled {
        state
            .monitor
            .refresh(
                [account_id],
                crate::monitoring::RefreshReason::UserRequested,
            )
            .await?;
    }
    Ok(())
}

/// Changes one account's display name without touching its identity or rank.
#[tauri::command]
#[specta::specta]
pub async fn rename_account(
    state: State<'_, AppState>,
    account_ref: AccountRef,
    nickname: String,
) -> Result<(), CommandError> {
    let account_id = account_ref.into_id();
    let nickname = nickname.trim().to_owned();
    if nickname.is_empty() || nickname.chars().count() > quota_domain::account::MAX_NICKNAME_LEN {
        return Err(CommandError::ValidationFailed {
            field: "nickname".into(),
            reason: "the nickname is blank or too long".into(),
        });
    }
    {
        let _commit = state.monitor.commit().await;
        let mut registry = state.registry.write().await;
        registry
            .rename(&account_id, nickname)
            .map_err(map_core_error)?;
        let stored = registry
            .get(&account_id)
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
    state.monitor.publish().await
}

/// Shows or hides one account's API key spend limit on its card.
///
/// A key that exists only so Quota can read the account has a limit nobody
/// needs to see, so the limit is hidden until the person turns it on.
#[tauri::command]
#[specta::specta]
pub async fn set_key_limit_shown(
    state: State<'_, AppState>,
    account_ref: AccountRef,
    shown: bool,
) -> Result<(), CommandError> {
    let account_id = account_ref.into_id();
    {
        let _commit = state.monitor.commit().await;
        let mut registry = state.registry.write().await;
        registry
            .set_show_key_limit(&account_id, shown)
            .map_err(map_core_error)?;
        let stored = registry
            .get(&account_id)
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
    state.monitor.publish().await
}

/// Removes the application's local reference to one account.
///
/// It does not log out, unlink, or otherwise disturb the owning tool.
#[tauri::command]
#[specta::specta]
pub async fn disconnect_account(
    state: State<'_, AppState>,
    account_ref: AccountRef,
) -> Result<(), CommandError> {
    let account_id = account_ref.into_id();
    // A credential Quota owns for this account goes with it.
    let owned_credential = state
        .registry
        .read()
        .await
        .get(&account_id)
        .map(|entry| &entry.stored.connection)
        .filter(|connection| connection.credential_ownership == CredentialOwnership::AppOwned)
        .map(|connection| (connection.id.clone(), connection.provider_id));
    {
        let _commit = state.monitor.commit().await;
        state
            .accounts
            .remove_account(&account_id)
            .await
            .map_err(|error| CommandError::PersistenceUnavailable {
                owner: error.owner.into(),
            })?;
        state
            .registry
            .write()
            .await
            .remove(&account_id)
            .map(|_| ())
            .map_err(map_core_error)?;
    }
    if let Some((connection, provider)) = owned_credential {
        crate::monitoring::credentials::forget(state.monitor.secrets(), connection).await;
        // A website sign-in's browser profile goes with its account.
        state.monitor.forget_website_sign_in(provider);
    }
    // Removing the last account leaves no worker to publish, so the change is
    // published here or the row stays on screen until the next refresh.
    state.monitor.publish().await
}

/// Reads the last fetch state for one immutable account identity.
#[tauri::command]
#[specta::specta]
pub async fn get_connection_progress(
    state: State<'_, AppState>,
    account_ref: AccountRef,
) -> Result<FetchState, CommandError> {
    state
        .registry
        .read()
        .await
        .get(account_ref.id())
        .map(|entry| entry.stored.fetch_state)
        .ok_or(CommandError::AccountNotFound)
}

pub(crate) fn map_core_error(error: quota_core::CoreError) -> CommandError {
    match error {
        quota_core::CoreError::AccountNotFound(_)
        | quota_core::CoreError::ConnectionNotFound(_) => CommandError::AccountNotFound,
        quota_core::CoreError::ReconnectRequired => CommandError::ReconnectRequired,
        quota_core::CoreError::StaleResult => CommandError::RevisionConflict {
            expected: 0,
            actual: 0,
        },
        quota_core::CoreError::Validation { field, reason } => CommandError::ValidationFailed {
            field: field.into(),
            reason: reason.into(),
        },
        quota_core::CoreError::Provider(error) => match error {
            quota_core::ProviderError::Authentication => CommandError::ReconnectRequired,
            quota_core::ProviderError::Authorization => CommandError::PermissionDenied {
                window_label: "provider".into(),
            },
            other => CommandError::Internal {
                code: other.diagnostic_code().to_owned(),
            },
        },
        quota_core::CoreError::Persistence { owner } => CommandError::PersistenceUnavailable {
            owner: owner.into(),
        },
    }
}
