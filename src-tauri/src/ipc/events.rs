//! Typed backend-to-renderer events.
//!
//! The concrete `tauri_specta::Event` derives live here, next to the registry
//! that mounts them, not in the transport contract crate. Each wrapper carries
//! the contract payload so the generated event name and the mirrored listener
//! name are the same word.

use quota_contracts::{
    ConnectionProgressChangedPayload, MonitoringStateChangedPayload,
    OverviewWindowStateChangedPayload, PersistenceStatusChangedPayload, PreferencesChangedPayload,
    SnapshotUpdatedPayload,
};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri_specta::Event;

/// The primary quota and account update channel.
#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct SnapshotUpdated(pub SnapshotUpdatedPayload);

/// Progress of one connection attempt.
#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct ConnectionProgressChanged(pub ConnectionProgressChangedPayload);

/// Emitted after a preference save succeeds, never before.
#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct PreferencesChanged(pub PreferencesChangedPayload);

/// Publishes a preference aggregate after every durable owner confirms its save.
pub fn publish_preferences(
    app: &tauri::AppHandle,
    app_instance_id: &quota_domain::ids::AppInstanceId,
    preferences: &quota_contracts::preferences::Preferences,
) {
    let event = PreferencesChanged(PreferencesChangedPayload {
        app_instance_id: app_instance_id.clone(),
        preference_revision: preferences.revision,
        preferences: preferences.clone(),
    });
    emit_preferences(app, &event, "overview", "overview_preferences_event_failed");
    emit_preferences(app, &event, "settings", "settings_preferences_event_failed");
}

fn emit_preferences(app: &tauri::AppHandle, event: &PreferencesChanged, label: &str, code: &str) {
    if event.emit_to(app, label).is_err() {
        tracing::warn!(code = code, "preference event was not delivered");
    }
}

/// Whether the supervisor is scheduling reads.
#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct MonitoringStateChanged(pub MonitoringStateChangedPayload);

/// Confirmed native overview window state, or a typed native failure.
#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct OverviewWindowStateChanged(pub OverviewWindowStateChangedPayload);

/// Durable storage availability.
#[derive(Debug, Clone, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct PersistenceStatusChanged(pub PersistenceStatusChangedPayload);
