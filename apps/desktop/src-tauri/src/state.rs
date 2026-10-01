//! Managed state the commands read.
//!
//! It holds service handles, not business logic. Commands receive exactly the
//! narrow handle they need through this struct rather than the whole
//! application context.

use std::sync::Arc;

use quota_contracts::CommandError;
use quota_core::accounts::AccountRegistry;
use quota_core::clock::SystemClock;
use quota_core::ports::{
    AccountRepository, BackoffRepository, HistoryRepository, PreferenceRepository,
};
use quota_core::snapshots::SnapshotBuilder;
use quota_domain::polling::ProviderPollingPolicy;

use crate::platform::window::OverviewWindowController;

/// The durable owners and native handles the commands reach.
///
/// Every field is a trait object or a small value. No command receives the
/// whole application context when a narrower handle would do.
#[derive(Clone)]
pub struct AppState {
    /// The account registry, owned by the application core.
    pub registry: Arc<tokio::sync::RwLock<AccountRegistry>>,
    /// The snapshot builder and its revision counter.
    pub snapshots: Arc<tokio::sync::Mutex<SnapshotBuilder>>,
    /// The injected clock.
    pub clock: Arc<SystemClock>,
    /// The mode-aware overview window controller.
    pub window: Arc<tokio::sync::Mutex<OverviewWindowController>>,
    /// Durable account and binding state.
    pub accounts: Arc<dyn AccountRepository>,
    /// Scoped rate-limit and backoff state.
    pub backoff: Arc<dyn BackoffRepository>,
    /// Local reading history.
    pub history: Arc<dyn HistoryRepository>,
    /// Non-transactional presentation preferences.
    pub preferences: Arc<dyn PreferenceRepository>,
    /// The effective per-provider polling policy.
    pub policies: Arc<tokio::sync::RwLock<Vec<ProviderPollingPolicy>>>,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Ports are trait objects; only the shape is logged, never a payload.
        f.debug_struct("AppState").finish_non_exhaustive()
    }
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
        accounts: Arc<dyn AccountRepository>,
        backoff: Arc<dyn BackoffRepository>,
        history: Arc<dyn HistoryRepository>,
        preferences: Arc<dyn PreferenceRepository>,
        policies: Vec<ProviderPollingPolicy>,
    ) -> Result<Self, CommandError> {
        Ok(Self {
            registry: Arc::new(tokio::sync::RwLock::new(registry)),
            snapshots: Arc::new(tokio::sync::Mutex::new(builder)),
            clock: Arc::new(SystemClock),
            window: Arc::new(tokio::sync::Mutex::new(OverviewWindowController::new())),
            accounts,
            backoff,
            history,
            preferences,
            policies: Arc::new(tokio::sync::RwLock::new(policies)),
        })
    }
}
