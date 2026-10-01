//! Thin command handlers.
//!
//! A handler validates its arguments, calls one narrow service, and maps the
//! result onto the typed contract. It holds no quota rule, starts no worker,
//! and never blocks on a provider.

use quota_contracts::commands::{AccountSelection, RefreshReason, SnapshotResponse};
use quota_contracts::refs::AccountRef;
use quota_contracts::{CommandError, RegisteredProvider};
use quota_domain::account::FetchState;
use quota_domain::ids::{AccountId, AppInstanceId};
use quota_domain::preferences::OverviewMode;
use quota_domain::provider::ProviderId;
use quota_domain::snapshot::{AppSnapshot, MonitoringState, PersistenceStatus};
use tauri::State;

use crate::state::AppState;

/// Reads the complete sanitized application snapshot.
///
/// The renderer registers its listener first and then calls this once, so a
/// missed event is repaired without any frontend polling.
#[tauri::command]
#[specta::specta]
pub async fn get_snapshot(
    state: State<'_, AppState>,
) -> Result<SnapshotResponse, CommandError> {
    let builder = state.snapshots.lock().await;
    let registry = state.registry.read().await;
    let snapshot: AppSnapshot = builder.build(
        &registry,
        &MonitoringState::Running,
        &PersistenceStatus::Available,
        state.clock.now(),
    );
    Ok(SnapshotResponse { snapshot })
}

/// Lists every provider this build knows about, including ones with no compiled adapter.
#[tauri::command]
#[specta::specta]
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

/// Requests a refresh through the shared scheduler.
///
/// The selection is scoped; there is no generic key, path, or action argument.
#[tauri::command]
#[specta::specta]
pub async fn refresh_accounts(
    state: State<'_, AppState>,
    selection: AccountSelection,
    reason: RefreshReason,
) -> Result<Vec<AccountId>, CommandError> {
    let registry = state.registry.read().await;
    let chosen: Vec<AccountId> = match &selection {
        AccountSelection::All => registry.iter().map(|entry| entry.account_id().clone()).collect(),
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
    };
    let _ = reason;
    Ok(chosen)
}

/// Pauses or resumes monitoring.
#[tauri::command]
#[specta::specta]
pub async fn set_monitoring_state(
    state: State<'_, AppState>,
    paused: bool,
) -> Result<MonitoringState, CommandError> {
    let registry = state.registry.read().await;
    if registry.is_empty() && paused {
        return Err(CommandError::Validation {
            field: "monitoring".into(),
            reason: "no accounts are connected".into(),
        });
    }
    Ok(if paused { MonitoringState::Paused } else { MonitoringState::Running })
}

/// Enables or disables monitoring for one account.
#[tauri::command]
#[specta::specta]
pub async fn set_account_enabled(
    state: State<'_, AppState>,
    request: quota_contracts::SetAccountEnabledRequest,
) -> Result<(), CommandError> {
    let mut registry = state.registry.write().await;
    registry
        .set_enabled(request.account_ref.id(), request.enabled)
        .map_err(map_core_error)
}

/// Changes one account's display name without touching its identity or rank.
#[tauri::command]
#[specta::specta]
pub async fn rename_account(
    state: State<'_, AppState>,
    account_ref: AccountRef,
    nickname: String,
) -> Result<(), CommandError> {
    let mut registry = state.registry.write().await;
    registry.rename(account_ref.id(), nickname).map_err(map_core_error)
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
    let mut registry = state.registry.write().await;
    registry.remove(account_ref.id()).map(|_| ()).map_err(map_core_error)
}

/// Reports the fetch state the application last recorded for one account.
#[tauri::command]
#[specta::specta]
pub async fn get_connection_progress(
    state: State<'_, AppState>,
    account_ref: AccountRef,
) -> Result<FetchState, CommandError> {
    let registry = state.registry.read().await;
    registry
        .get(account_ref.id())
        .map(|entry| entry.stored.connection.state.into())
        .ok_or(CommandError::AccountNotFound)
}

/// The application instance identity this build publishes under.
#[tauri::command]
#[specta::specta]
pub fn app_instance_id() -> AppInstanceId {
    AppInstanceId::generate()
}

/// The overview mode this build opens with before any preference is restored.
#[tauri::command]
#[specta::specta]
pub fn default_overview_mode() -> OverviewMode {
    OverviewMode::Floating
}

impl From<quota_domain::account::ConnectionState> for FetchState {
    fn from(state: quota_domain::account::ConnectionState) -> Self {
        match state {
            quota_domain::account::ConnectionState::NeverConnected => Self::Idle,
            quota_domain::account::ConnectionState::Connecting => Self::Fetching,
            quota_domain::account::ConnectionState::Connected => Self::Idle,
            quota_domain::account::ConnectionState::ReauthenticationRequired => Self::Error,
            quota_domain::account::ConnectionState::Unsupported => Self::Error,
            quota_domain::account::ConnectionState::Disconnected => Self::Idle,
        }
    }
}

fn map_core_error(error: quota_core::CoreError) -> CommandError {
    match error {
        quota_core::CoreError::AccountNotFound(_) => CommandError::AccountNotFound,
        quota_core::CoreError::ConnectionNotFound(_) => CommandError::AccountNotFound,
        quota_core::CoreError::ReconnectRequired => CommandError::ReconnectRequired,
        quota_core::CoreError::StaleResult => CommandError::RevisionConflict {
            expected: 0,
            actual: 0,
        },
        quota_core::CoreError::Validation { field, reason } => {
            CommandError::ValidationFailed { field: field.into(), reason: reason.into() }
        }
        quota_core::CoreError::Provider(provider) => match provider {
            quota_core::ProviderError::Authentication => CommandError::ReconnectRequired,
            quota_core::ProviderError::Authorization => CommandError::PermissionDenied {
                window_label: "provider".into(),
            },
            other => CommandError::Internal {
                code: other.diagnostic_code().to_owned(),
            },
        },
        quota_core::CoreError::Persistence { .. } => CommandError::PersistenceUnavailable {
            owner: "sqlite".into(),
        },
    }
}