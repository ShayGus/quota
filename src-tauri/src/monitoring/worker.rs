//! Shared refresh coordinator and supervised read workers.

use quota_contracts::events::SnapshotUpdatedPayload;
use quota_core::clock::Clock;
use quota_domain::snapshot::PersistenceStatus;
use tauri_specta::Event;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinSet;
use tokio::time::{MissedTickBehavior, interval_at};

use crate::ipc::events::SnapshotUpdated as SnapshotUpdatedEvent;

use super::read_path::perform_read;
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
                if shutdown_requested(&shutdown, changed.is_err()) {
                    break;
                }
            }
            _ = timer.tick() => {
                on_scheduled_tick(&runtime).await;
            }
            request = receiver.recv(), if workers.len() < MAX_REMOTE_READS => {
                let Some(request) = request else { break; };
                spawn_supervised_read(&mut workers, runtime.state.clone(), request);
            }
            Some(joined) = workers.join_next(), if !workers.is_empty() => {
                on_worker_joined(joined);
            }
        }
    }

    workers.abort_all();
    while workers.join_next().await.is_some() {}
}

/// Whether a shutdown watch outcome means the coordinator must stop.
fn shutdown_requested(shutdown: &watch::Receiver<bool>, changed_failed: bool) -> bool {
    changed_failed || *shutdown.borrow()
}

/// Queues every enabled account after a periodic tick fires.
async fn on_scheduled_tick(runtime: &MonitoringRuntime) {
    let _ = runtime.request_all(RefreshReason::Scheduled).await;
}

/// Spawns one supervised read under the global two-read budget.
fn spawn_supervised_read(workers: &mut JoinSet<()>, state: RuntimeState, request: RefreshRequest) {
    let permits = state.permits.clone();
    workers.spawn(async move {
        let permit = permits.acquire_owned().await;
        if let Ok(permit) = permit {
            // The floor is recorded before the remote read, so a write that
            // fails afterwards cannot leave the account due again immediately.
            stamp_dispatch_floor(&state, &request).await;
            let result = perform_read(&state, &request).await;
            drop(permit);
            if let Err(error) = result {
                tracing::warn!(target: "quota::scheduler", code = %error, "supervised read did not commit");
            }
        }
        state.pending.lock().await.remove(&request.account_id);
    });
}

/// Records when this account may next be read, before any remote work starts.
///
/// It is a floor, not the final schedule: an accepted commit recomputes it from
/// the policy and the reading it just took.
pub(super) async fn stamp_dispatch_floor(state: &RuntimeState, request: &RefreshRequest) {
    let now = state.clock.now();
    let _commit = state.commit.lock().await;
    let mut registry = state.registry.write().await;
    let Some(entry) = registry.get(&request.account_id).cloned() else {
        return;
    };
    let minimum = {
        let policies = state.policies.read().await;
        policies
            .iter()
            .find(|policy| policy.provider_id == entry.binding.provider_id)
            .map_or_else(
                || chrono::Duration::seconds(300),
                |policy| {
                    chrono::Duration::from_std(policy.strategy.minimum_interval())
                        .unwrap_or_default()
                },
            )
    };
    let floor = now + minimum;
    let _ = registry.record_attempt(
        &request.account_id,
        entry.stored.fetch_state,
        now,
        Some(floor),
    );
}

/// Logs a supervised worker that finished on its own.
fn on_worker_joined(joined: Result<(), tokio::task::JoinError>) {
    if let Err(error) = joined {
        tracing::error!(target: "quota::scheduler", error = %error, "supervised worker task failed");
    }
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
