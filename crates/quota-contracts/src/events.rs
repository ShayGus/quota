//! Typed backend-to-renderer event payloads.
//!
//! These are transport payloads only. The concrete `tauri_specta::Event` derives
//! live in the desktop host's `ipc::events` module, next to the registry that
//! mounts them.

use serde::{Deserialize, Serialize};
use specta::Type;

use quota_domain::account::ConnectionState;
use quota_domain::ids::{AppInstanceId, ConnectionAttemptId};
use quota_domain::preferences::OverviewMode;
use quota_domain::snapshot::{AppSnapshot, MonitoringState, PersistenceStatus};

use crate::errors::CommandError;
use crate::preferences::Preferences;

/// The primary quota and account update channel.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct SnapshotUpdated {
    /// Which application instance published this.
    pub app_instance_id: AppInstanceId,
    /// The snapshot revision, monotonic within the instance.
    pub revision: u64,
    /// The wire schema version of `snapshot`.
    pub schema_version: u32,
    /// The complete, sanitized snapshot.
    pub snapshot: AppSnapshot,
}

/// Where an authorized connection attempt stands.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case", tag = "kind", content = "context")]
pub enum ConnectionProgress {
    /// The attempt is running.
    Started,
    /// The provider asked the user to do something.
    AwaitingUser,
    /// The attempt produced a verified binding.
    Verified {
        /// The verified connection state.
        state: ConnectionState,
    },
    /// The attempt failed with a recoverable, typed error.
    Failed {
        /// The typed failure.
        error: CommandError,
    },
    /// The attempt was cancelled.
    Cancelled,
}

/// Progress of one connection attempt.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct ConnectionProgressChanged {
    /// Which application instance published this.
    pub app_instance_id: AppInstanceId,
    /// The attempt this progress belongs to.
    pub attempt_id: ConnectionAttemptId,
    /// Monotonic within one attempt.
    pub attempt_revision: u64,
    /// The typed progress state.
    pub progress: ConnectionProgress,
}

/// Emitted after a preference save succeeds, never before.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct PreferencesChanged {
    /// Which application instance published this.
    pub app_instance_id: AppInstanceId,
    /// The confirmed preference revision.
    pub preference_revision: u64,
    /// The confirmed preferences.
    pub preferences: Preferences,
}

/// Whether the supervisor is scheduling reads.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct MonitoringStateChanged {
    /// Which application instance published this.
    pub app_instance_id: AppInstanceId,
    /// Monotonic within one instance.
    pub monitoring_revision: u64,
    /// The confirmed state.
    pub monitoring_state: MonitoringState,
}

/// Confirmed native overview window state, or a typed native failure.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum OverviewWindowState {
    /// The native state was confirmed.
    Confirmed {
        /// The window mode in effect.
        mode: OverviewMode,
        /// Whether the window floats above other applications.
        always_on_top: bool,
        /// Whether the window is currently shown.
        visible: bool,
        /// The geometry revision, incremented on every confirmed change.
        geometry_revision: u64,
    },
    /// The platform refused or failed the operation. The previous state stands.
    Failed {
        /// The typed failure.
        error: CommandError,
    },
}

/// A change to the native overview window.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct OverviewWindowStateChanged {
    /// Which application instance published this.
    pub app_instance_id: AppInstanceId,
    /// The confirmed state or failure.
    pub state: OverviewWindowState,
}

/// Durable storage availability, with no file contents or credential references.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct PersistenceStatusChanged {
    /// Which application instance published this.
    pub app_instance_id: AppInstanceId,
    /// The current status.
    pub status: PersistenceStatus,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn connection_progress_is_a_tagged_union() {
        let json = serde_json::to_string(&ConnectionProgress::Started).unwrap();
        assert_eq!(json, "{\"kind\":\"started\"}");
        let parsed: ConnectionProgress = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, ConnectionProgress::Started);
    }

    #[test]
    fn snapshot_update_carries_the_instance_and_revision() {
        let instance = AppInstanceId::new("instance-1").unwrap();
        let event = SnapshotUpdated {
            app_instance_id: instance.clone(),
            revision: 7,
            schema_version: 1,
            snapshot: AppSnapshot {
                schema_version: 1,
                app_instance_id: instance,
                revision: 7,
                generated_at: chrono::DateTime::UNIX_EPOCH,
                monitoring_state: MonitoringState::Running,
                persistence_status: PersistenceStatus::Available,
                connections: Vec::new(),
                accounts: Vec::new(),
                order: Vec::new(),
            },
        };
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["revision"], 7);
        assert_eq!(json["app_instance_id"], "instance-1");
    }
}
