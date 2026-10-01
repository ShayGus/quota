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
            // Eligibility is decided inside the read, and only a read that is
            // really going out stamps the account's schedule.
            let result = perform_read(&state, &request).await;
            drop(permit);
            if let Err(error) = result {
                tracing::warn!(target: "quota::scheduler", code = %error, "supervised read did not commit");
            }
        }
        state.pending.lock().await.remove(&request.account_id);
    });
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
