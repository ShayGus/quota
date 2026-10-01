//! Typed backend-to-renderer events.
//!
//! The concrete `tauri_specta::Event` derives live here, next to the registry
//! that mounts them, not in the transport contract crate.

use quota_contracts::{
    ConnectionProgressChanged, MonitoringStateChanged, OverviewWindowStateChanged,
    PersistenceStatusChanged, PreferencesChanged, SnapshotUpdated,
};
use tauri_specta::Event;

/// The primary quota and account update channel.
#[derive(Debug, Clone, Event)]
pub struct SnapshotUpdatedEvent(pub SnapshotUpdated);

/// Progress of one connection attempt.
#[derive(Debug, Clone, Event)]
pub struct ConnectionProgressChangedEvent(pub ConnectionProgressChanged);

/// Emitted after a preference save succeeds, never before.
#[derive(Debug, Clone, Event)]
pub struct PreferencesChangedEvent(pub PreferencesChanged);

/// Whether the supervisor is scheduling reads.
#[derive(Debug, Clone, Event)]
pub struct MonitoringStateChangedEvent(pub MonitoringStateChanged);

/// Confirmed native overview window state, or a typed native failure.
#[derive(Debug, Clone, Event)]
pub struct OverviewWindowStateChangedEvent(pub OverviewWindowStateChanged);

/// Durable storage availability.
#[derive(Debug, Clone, Event)]
pub struct PersistenceStatusChangedEvent(pub PersistenceStatusChanged);
