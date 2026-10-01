//! Shared refresh coordinator and supervised read workers.

use std::time::Duration;

use quota_contracts::events::SnapshotUpdated as SnapshotUpdatedPayload;
use quota_core::clock::Clock;
use quota_core::ports::ProviderError;
use quota_domain::account::{ConnectionState, FetchState};
use quota_domain::ids::ConnectionAttemptId;
use quota_domain::polling::LimitScope;
use quota_domain::snapshot::PersistenceStatus;
use tauri_specta::Event;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinSet;
use tokio::time::{MissedTickBehavior, interval_at};

use crate::ipc::events::SnapshotUpdated as SnapshotUpdatedEvent;

use super::{
    MAX_REMOTE_READS, MonitoringRuntime, PERIODIC_REFRESH, RefreshReason, RefreshRequest,
    RuntimeState,
};
pub(super) async fn run_coordinator(
    runtime: MonitoringRuntime,
    mut receiver: mpsc::Receiver<RefreshRequest>,
    mut shutdown: watch::Receiver<bool>,
) {
    let start = tokio::time::Instant::now() + PERIODIC_REFRESH;
    let mut timer = interval_at(start, PERIODIC_REFRESH);
    timer.set_missed_tick_behavior(MissedTickBehavior::Skip);
    let mut workers = JoinSet::new();
    let _ = runtime.request_all(RefreshReason::Scheduled).await;

    loop {
        tokio::select! {
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    break;
                }
            }
            _ = timer.tick() => {
                let _ = runtime.request_all(RefreshReason::Scheduled).await;
            }
            request = receiver.recv(), if workers.len() < MAX_REMOTE_READS => {
                let Some(request) = request else { break; };
                let state = runtime.state.clone();
                let permits = state.permits.clone();
                workers.spawn(async move {
                    let permit = permits.acquire_owned().await;
                    if let Ok(permit) = permit {
                        let result = perform_read(&state, &request).await;
                        drop(permit);
                        if let Err(error) = result {
                            tracing::warn!(target: "quota::scheduler", code = %error, "supervised read did not commit");
                        }
                    }
                    state.pending.lock().await.remove(&request.account_id);
                });
            }
            Some(joined) = workers.join_next(), if !workers.is_empty() => {
                if let Err(error) = joined {
                    tracing::error!(target: "quota::scheduler", error = %error, "supervised worker task failed");
                }
            }
        }
    }

    workers.abort_all();
    while workers.join_next().await.is_some() {}
}

async fn perform_read(state: &RuntimeState, request: &RefreshRequest) -> Result<(), String> {
    if *state.monitoring.read().await == MonitoringState::Paused {
        return Ok(());
    }
    let entry = state
        .registry
        .read()
        .await
        .get(&request.account_id)
        .cloned()
        .ok_or_else(|| "account_removed".to_owned())?;
    if !entry.stored.monitoring_enabled {
        return Ok(());
    }
    let binding = entry.binding.clone();
    let adapter = state
        .providers
        .provider(binding.provider_id)
        .cloned()
        .ok_or_else(|| "unsupported_provider".to_owned())?;
    let scope = LimitScope::Connection(binding.connection_id.clone());
    let now = state.clock.now();
    if state
        .backoff
        .load_backoff(&scope)
        .await
        .map_err(|error| error.reason)?
        .is_some_and(|backoff| backoff.is_waiting_at(now))
    {
        return Ok(());
    }

    let context = quota_core::ports::ReadContext {
        attempt_id: ConnectionAttemptId::generate(),
        deadline: Some(now + chrono::Duration::seconds(10)),
    };
    let response =
        match tokio::time::timeout(REMOTE_TIMEOUT, adapter.read_quota(&binding, context)).await {
            Err(_) => {
                record_failure(
                    state,
                    &entry,
                    ProviderError::Transient {
                        detail: "request timeout".to_owned(),
                    },
                )
                .await?;
                return Err("request_timeout".to_owned());
            }
            Ok(Err(error)) => {
                let code = error.diagnostic_code().to_owned();
                record_failure(state, &entry, error).await?;
                return Err(code);
            }
            Ok(Ok(response)) => response,
        };
    let Some(read) = response.read() else {
        if let quota_core::ports::FetchOutcome::Failed(error) = response {
            let code = error.diagnostic_code().to_owned();
            record_failure(state, &entry, error).await?;
            return Err(code);
        }
        return Err("provider_returned_no_reading".to_owned());
    };
    let windows = read.windows.clone();
    let expected_missing = read.expected_but_missing.clone();
    let identity = read.identity.clone();
    let now = state.clock.now();
    let stored = {
        let mut registry = state.registry.write().await;
        let live = registry
            .get(&request.account_id)
            .ok_or_else(|| "account_removed_before_commit".to_owned())?;
        if !live.binding.accepts(&binding) {
            return Err("stale_connection_generation".to_owned());
        }
        registry
            .apply_reading(&request.account_id, windows, expected_missing)
            .map_err(|error| error.to_string())?;
        registry
            .set_identity(&request.account_id, identity)
            .map_err(|error| error.to_string())?;
        registry
            .record_attempt(
                &request.account_id,
                FetchState::Idle,
                now,
                Some(now + chrono::Duration::seconds(300)),
            )
            .map_err(|error| error.to_string())?;
        registry
            .get(&request.account_id)
            .map(|account| account.stored.clone())
            .ok_or_else(|| "account_removed_before_persist".to_owned())?
    };

    // The repository commits all current measurement values before the
    // renderer can observe the new revision.
    state
        .accounts
        .upsert_account(stored)
        .await
        .map_err(|error| error.reason)?;
    state
        .backoff
        .clear_backoff(&scope)
        .await
        .map_err(|error| error.reason)?;
    publish_snapshot(state).await
}

async fn record_failure(
    state: &RuntimeState,
    entry: &quota_core::accounts::RegisteredAccount,
    error: ProviderError,
) -> Result<(), String> {
    let now = state.clock.now();
    let scope = LimitScope::Connection(entry.binding.connection_id.clone());
    let previous = state
        .backoff
        .load_backoff(&scope)
        .await
        .map_err(|e| e.reason)?;
    let attempts = previous.map_or(1, |record| record.attempts.saturating_add(1));
    let policy = state
        .policies
        .read()
        .await
        .iter()
        .find(|policy| policy.provider_id == entry.binding.provider_id)
        .cloned();
    let delay = policy
        .as_ref()
        .map(|policy| policy.backoff_for_attempt(attempts))
        .unwrap_or_else(|| chrono::Duration::minutes(5));
    let provider_retry_after = match &error {
        ProviderError::RateLimited { retry_after } => *retry_after,
        _ => None,
    };
    let next_eligible_at = provider_retry_after.unwrap_or(now + delay);
    state
        .backoff
        .persist_backoff(
            &scope,
            quota_core::ports::BackoffState {
                attempts,
                next_eligible_at,
                provider_retry_after,
            },
        )
        .await
        .map_err(|e| e.reason)?;

    let updated = {
        let mut registry = state.registry.write().await;
        let fetch_state = if matches!(&error, ProviderError::RateLimited { .. }) {
            FetchState::Backoff
        } else {
            FetchState::Error
        };
        registry
            .record_attempt(entry.account_id(), fetch_state, now, Some(next_eligible_at))
            .map_err(|error| error.to_string())?;
        if matches!(&error, ProviderError::Authentication) {
            registry
                .set_connection_state(
                    &entry.binding.connection_id,
                    ConnectionState::ReauthenticationRequired,
                )
                .map_err(|error| error.to_string())?;
        }
        registry
            .get(entry.account_id())
            .map(|value| value.stored.clone())
            .ok_or_else(|| "account_removed_after_failure".to_owned())?
    };
    state
        .accounts
        .upsert_account(updated)
        .await
        .map_err(|e| e.reason)?;
    publish_snapshot(state).await
}

pub(super) async fn publish_snapshot(state: &RuntimeState) -> Result<(), String> {
    let monitoring = state.monitoring.read().await.clone();
    let registry = state.registry.read().await;
    let snapshot = state.snapshots.lock().await.build(
        &registry,
        &monitoring,
        &PersistenceStatus::Available,
        state.clock.now(),
    );
    let event = SnapshotUpdatedEvent(SnapshotUpdatedPayload {
        app_instance_id: snapshot.app_instance_id.clone(),
        revision: snapshot.revision,
        schema_version: snapshot.schema_version,
        snapshot,
    });
    event
        .emit_to(&state.app, "overview")
        .map_err(|_| "overview_snapshot_event_failed".to_owned())?;
    event
        .emit_to(&state.app, "settings")
        .map_err(|_| "settings_snapshot_event_failed".to_owned())?;
    Ok(())
}
