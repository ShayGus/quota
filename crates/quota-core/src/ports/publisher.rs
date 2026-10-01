//! What the core needs from the host.
//!
//! The core publishes committed state and asks for window changes; it never
//! touches Tauri itself.

use async_trait::async_trait;

use quota_domain::ids::AccountId;
use quota_domain::preferences::OverviewMode;
use quota_domain::snapshot::{AppSnapshot, MonitoringState, PersistenceStatus};

/// Publishes committed state to the renderer after the durable write succeeds.
#[async_trait]
pub trait SnapshotPublisher: Send + Sync {
    /// Publishes a committed snapshot.
    async fn publish(&self, snapshot: &AppSnapshot) -> Result<(), PublishError>;

    /// Publishes the confirmed monitoring state.
    async fn publish_monitoring(&self, state: &MonitoringState) -> Result<(), PublishError>;

    /// Publishes durable-storage availability, with no file contents.
    async fn publish_persistence(&self, status: &PersistenceStatus) -> Result<(), PublishError>;
}

/// Asks the host to change a native window.
#[async_trait]
pub trait WindowController: Send + Sync {
    /// Shows the overview, restoring the last chosen mode.
    async fn show_overview(&self) -> Result<(), NativeError>;

    /// Moves the overview between floating and tray anchoring.
    async fn set_mode(&self, mode: OverviewMode) -> Result<(), NativeError>;

    /// Changes only the native topmost flag.
    ///
    /// This must not move, resize, detach, hide, or reorder anything.
    async fn set_always_on_top(&self, always_on_top: bool) -> Result<(), NativeError>;

    /// Opens the settings window.
    async fn show_settings(&self) -> Result<(), NativeError>;

    /// Highlights one account row without changing the order.
    async fn focus_account(&self, account_id: &AccountId) -> Result<(), NativeError>;
}

/// A publish attempt failed. Committed state stays available through a snapshot read.
#[derive(Debug, thiserror::Error)]
#[error("the host refused to publish: {reason}")]
pub struct PublishError {
    /// A sanitized, non-identifying reason.
    pub reason: String,
}

impl PublishError {
    /// Builds an error.
    #[must_use]
    pub fn new(reason: impl Into<String>) -> Self {
        Self {
            reason: reason.into(),
        }
    }
}

/// A native window operation was refused or failed.
#[derive(Debug, thiserror::Error)]
#[error("native window operation `{operation}` failed: {reason}")]
pub struct NativeError {
    /// The refused operation.
    pub operation: &'static str,
    /// A sanitized, non-identifying reason.
    pub reason: String,
}

impl NativeError {
    /// Builds an error for one operation.
    #[must_use]
    pub fn new(operation: &'static str, reason: impl Into<String>) -> Self {
        Self {
            operation,
            reason: reason.into(),
        }
    }
}
