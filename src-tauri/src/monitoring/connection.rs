//! Cancellable connection discovery, verification, and account persistence.

use std::sync::Arc;

use quota_contracts::CommandError;
use quota_contracts::commands::BeginConnectionRequest;
use quota_contracts::events::{ConnectionProgress, ConnectionProgressChangedPayload};
use quota_core::clock::Clock;
use quota_core::ports::{ProviderAdapter, ProviderError};
use quota_domain::account::ConnectionState;
use quota_domain::ids::{AccountId, ConnectionAttemptId, ConnectionId};
use tauri_specta::Event;
use tokio::sync::watch;

use super::worker::publish_snapshot;
use super::{MonitoringRuntime, REMOTE_TIMEOUT, RuntimeState};

pub(super) async fn run_connection_attempt(
    runtime: MonitoringRuntime,
    adapter: Arc<dyn ProviderAdapter>,
    request: BeginConnectionRequest,
    attempt_id: ConnectionAttemptId,
    mut cancelled: watch::Receiver<bool>,
) -> Result<(), CommandError> {
    let Some(candidates) =
        discover_connection_candidates(&runtime, &adapter, &request, &attempt_id, &mut cancelled)
            .await?
    else {
        return Ok(());
    };
    if candidates.is_empty() {
        return Err(CommandError::UnsupportedMethod {
            requested: request
                .profile_label
                .unwrap_or_else(|| "local credential".into()),
        });
    }

    let mut progress_revision = 1_u32;
    for candidate in candidates {
        if *cancelled.borrow() {
            return Ok(());
        }
        let ids = candidate_binding(&adapter, &candidate);
        let Some(read) = read_candidate_quota(
            &runtime,
            &adapter,
            &ids.binding,
            &attempt_id,
            &mut cancelled,
        )
        .await?
        else {
            return Ok(());
        };
        commit_candidate(&runtime, candidate, &request, ids, read).await?;
        progress_revision = progress_revision.saturating_add(1);
        emit_connection_progress(
            &runtime.state,
            &attempt_id,
            progress_revision,
            ConnectionProgress::Verified {
                state: ConnectionState::Connected,
            },
        )
        .await;
    }
    publish_snapshot(&runtime.state)
        .await
        .map_err(|_| CommandError::Internal {
            code: "snapshot_publish_failed".into(),
        })
}

/// Discovers locally visible accounts, filtered to the requested profile.
///
/// Returns `None` when cancellation wins the discovery race.
async fn discover_connection_candidates(
    runtime: &MonitoringRuntime,
    adapter: &Arc<dyn ProviderAdapter>,
    request: &BeginConnectionRequest,
    attempt_id: &ConnectionAttemptId,
    cancelled: &mut watch::Receiver<bool>,
) -> Result<Option<Vec<quota_core::ports::DiscoveredAccount>>, CommandError> {
    emit_connection_progress(&runtime.state, attempt_id, 1, ConnectionProgress::Started).await;
    let discovered = tokio::select! {
        _ = cancelled.changed() => return Ok(None),
        result = tokio::time::timeout(REMOTE_TIMEOUT, adapter.discover_accounts()) => {
            result
                .map_err(|_| CommandError::Internal {
                    code: "connection_discovery_timeout".into(),
                })?
                .map_err(provider_command_error)?
        }
    };
    let candidates: Vec<_> = discovered
        .into_iter()
        .filter(|candidate| {
            request
                .profile_label
                .as_ref()
                .is_none_or(|profile| candidate.profile_label.as_ref() == Some(profile))
        })
        .collect();
    Ok(Some(candidates))
}

/// The fresh identities one verified candidate is committed under.
struct CandidateIds {
    connection_id: ConnectionId,
    account_id: AccountId,
    binding: quota_core::ports::ConnectionBinding,
}

/// Mints the connection, account, and binding for one candidate.
fn candidate_binding(
    adapter: &Arc<dyn ProviderAdapter>,
    candidate: &quota_core::ports::DiscoveredAccount,
) -> CandidateIds {
    let connection_id = ConnectionId::generate();
    let account_id = AccountId::generate();
    let binding = quota_core::ports::ConnectionBinding {
        connection_id: connection_id.clone(),
        generation: 0,
        provider_id: adapter.provider_id(),
        principal_id: candidate.principal_id.clone(),
        workspace_id: candidate.workspace_id.clone(),
        entitlement_id: candidate.entitlement_id.clone(),
        profile_label: candidate.profile_label.clone(),
    };
    CandidateIds {
        connection_id,
        account_id,
        binding,
    }
}

/// Reads one candidate binding with a timeout.
///
/// Returns `None` when cancellation wins the permit or read race.
async fn read_candidate_quota(
    runtime: &MonitoringRuntime,
    adapter: &Arc<dyn ProviderAdapter>,
    binding: &quota_core::ports::ConnectionBinding,
    attempt_id: &ConnectionAttemptId,
    cancelled: &mut watch::Receiver<bool>,
) -> Result<Option<quota_core::ports::QuotaRead>, CommandError> {
    let permit = tokio::select! {
        _ = cancelled.changed() => return Ok(None),
        permit = runtime.state.permits.clone().acquire_owned() => {
            permit.map_err(|_| CommandError::Cancelled)?
        }
    };
    let response = tokio::select! {
        _ = cancelled.changed() => {
            drop(permit);
            return Ok(None);
        }
        result = tokio::time::timeout(
            REMOTE_TIMEOUT,
            adapter.read_quota(
                binding,
                quota_core::ports::ReadContext {
                    attempt_id: attempt_id.clone(),
                    deadline: Some(runtime.state.clock.now() + chrono::Duration::seconds(10)),
                },
            ),
        ) => result
            .map_err(|_| CommandError::Internal {
                code: "connection_read_timeout".into(),
            })?
            .map_err(provider_command_error)?,
    };
    drop(permit);
    let read = response
        .read()
        .cloned()
        .ok_or_else(|| CommandError::Internal {
            code: "connection_read_unavailable".into(),
        })?;
    Ok(Some(read))
}

/// Persists one verified candidate and registers it with the supervisor.
async fn commit_candidate(
    runtime: &MonitoringRuntime,
    candidate: quota_core::ports::DiscoveredAccount,
    request: &BeginConnectionRequest,
    ids: CandidateIds,
    read: quota_core::ports::QuotaRead,
) -> Result<(), CommandError> {
    let now = runtime.state.clock.now();
    let stored = quota_core::ports::StoredAccount {
        account_id: ids.account_id.clone(),
        connection: quota_domain::account::ConnectionSummary {
            id: ids.connection_id,
            provider_id: ids.binding.provider_id,
            credential_ownership: candidate.credential_ownership,
            generation: 0,
            profile_label: candidate.profile_label.clone(),
            cardinality: candidate.cardinality,
            state: ConnectionState::Connected,
            principal_id: candidate.principal_id,
            workspace_id: candidate.workspace_id,
            entitlement_id: candidate.entitlement_id,
        },
        nickname: request.nickname.clone(),
        connection_ordinal: 0,
        monitoring_enabled: true,
        connection_state: ConnectionState::Connected,
        fetch_state: quota_domain::account::FetchState::Idle,
        last_attempt_at: Some(now),
        last_success_at: Some(now),
        next_attempt_at: Some(now + chrono::Duration::seconds(300)),
        identity: Some(read.identity),
        windows: read.windows,
        expected_but_missing_window_ids: read.expected_but_missing,
    };
    let new_account = quota_core::accounts::NewAccount {
        account_id: ids.account_id.clone(),
        stored: stored.clone(),
        profile_label: candidate.profile_label,
        monitoring_enabled: true,
    };

    // Serialize duplicate check plus durable insert across connection attempts,
    // but never hold the account-registry lock across a database await.
    let _connection_gate = runtime.connection_gate.lock().await;
    if runtime
        .state
        .registry
        .read()
        .await
        .find_duplicate(&new_account)
        .is_some()
    {
        return Err(CommandError::ValidationFailed {
            field: "binding".into(),
            reason: "this verified account and quota pool are already connected".into(),
        });
    }
    runtime
        .state
        .accounts
        .upsert_account(stored)
        .await
        .map_err(|error| CommandError::PersistenceUnavailable {
            owner: error.owner.to_owned(),
        })?;
    let registration = {
        let mut registry = runtime.state.registry.write().await;
        registry.register(new_account).map(|_| ())
    };
    if let Err(error) = registration {
        let _ = runtime.state.accounts.remove_account(&ids.account_id).await;
        return Err(core_command_error(error));
    }
    Ok(())
}

fn provider_command_error(error: ProviderError) -> CommandError {
    match error {
        ProviderError::Authentication => CommandError::ReconnectRequired,
        ProviderError::Authorization => CommandError::PermissionDenied {
            window_label: "provider".into(),
        },
        other => CommandError::Internal {
            code: other.diagnostic_code().to_owned(),
        },
    }
}

fn core_command_error(error: quota_core::CoreError) -> CommandError {
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
        quota_core::CoreError::Provider(error) => provider_command_error(error),
        quota_core::CoreError::Persistence { owner } => CommandError::PersistenceUnavailable {
            owner: owner.into(),
        },
    }
}

pub(super) async fn emit_connection_progress(
    state: &RuntimeState,
    attempt_id: &ConnectionAttemptId,
    attempt_revision: u32,
    progress: ConnectionProgress,
) {
    let app_instance_id = state.snapshots.lock().await.app_instance_id().clone();
    let event = crate::ipc::events::ConnectionProgressChanged(ConnectionProgressChangedPayload {
        app_instance_id,
        attempt_id: attempt_id.clone(),
        attempt_revision,
        progress,
    });
    let _ = event.emit_to(&state.app, "settings");
}
