//! Typed backend-to-renderer events.
//!
//! The concrete `tauri_specta::Event` derives live here, next to the registry
//! that mounts them, not in the transport contract crate. Each wrapper carries
//! the contract payload so the generated event name and the mirrored listener
//! name are the same word.

use quota_contracts::{
    ConnectionProgressChanged as ConnectionProgressChangedPayload,
    MonitoringStateChanged as MonitoringStateChangedPayload,
    OverviewWindowStateChanged as OverviewWindowStateChangedPayload,
    PersistenceStatusChanged as PersistenceStatusChangedPayload,
    PreferencesChanged as PreferencesChangedPayload, SnapshotUpdated as SnapshotUpdatedPayload,
};
use tauri_specta::Event;

/// The primary quota and account update channel.
#[derive(Debug, Clone, tauri_specta::Event)]
pub struct SnapshotUpdated(pub SnapshotUpdatedPayload);

/// Progress of one connection attempt.
#[derive(Debug, Clone, tauri_specta::Event)]
pub struct ConnectionProgressChanged(pub ConnectionProgressChangedPayload);

/// Emitted after a preference save succeeds, never before.
#[derive(Debug, Clone, tauri_specta::Event)]
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
    if event.emit_to(app, "overview").is_err() {
        tracing::warn!(
            code = "overview_preferences_event_failed",
            "preference event was not delivered"
        );
    }
    if event.emit_to(app, "settings").is_err() {
        tracing::warn!(
            code = "settings_preferences_event_failed",
            "preference event was not delivered"
        );
    }
}

/// Whether the supervisor is scheduling reads.
#[derive(Debug, Clone, tauri_specta::Event)]
pub struct MonitoringStateChanged(pub MonitoringStateChangedPayload);

/// Confirmed native overview window state, or a typed native failure.
#[derive(Debug, Clone, tauri_specta::Event)]
pub struct OverviewWindowStateChanged(pub OverviewWindowStateChangedPayload);

/// Durable storage availability.
#[derive(Debug, Clone, tauri_specta::Event)]
pub struct PersistenceStatusChanged(pub PersistenceStatusChangedPayload);
