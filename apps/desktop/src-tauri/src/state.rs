//! Managed state the commands read.
//!
//! It holds service handles, not business logic. Commands receive exactly the
//! narrow handle they need through this struct rather than the whole
//! application context.

use std::sync::Arc;

use quota_core::accounts::AccountRegistry;
use quota_core::clock::SystemClock;
use quota_core::snapshots::SnapshotBuilder;
use quota_contracts::CommandError;

/// The handles a command handler is allowed to reach.
#[derive(Debug)]
pub struct AppState {
    /// The account registry, owned by the application core.
    pub registry: Arc<tokio::sync::RwLock<AccountRegistry>>,
    /// The snapshot builder and its revision counter.
    pub snapshots: Arc<tokio::sync::Mutex<SnapshotBuilder>>,
    /// The injected clock.
    pub clock: Arc<SystemClock>,
}

impl AppState {
    /// Builds the initial state.
    ///
    /// # Errors
    /// Returns [`CommandError::InitializationPending`] when no application
    /// instance identity is available yet.
    pub fn new(
        registry: AccountRegistry,
        builder: SnapshotBuilder,
    ) -> Result<Self, CommandError> {
        Ok(Self {
            registry: Arc::new(tokio::sync::RwLock::new(registry)),
            snapshots: Arc::new(tokio::sync::Mutex::new(builder)),
            clock: Arc::new(SystemClock),
        })
    }
}