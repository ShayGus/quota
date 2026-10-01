//! Managed state the commands read.
//!
//! It holds service handles, not business logic. Commands receive exactly the
//! narrow handle they need through this struct rather than the whole context.

use std::sync::Arc;

use quota_core::accounts::AccountRegistry;
use quota_core::clock::SystemClock;
use quota_core::ports::{
    AccountRepository, BackoffRepository, HistoryRepository, MonitoringRepository,
    PreferenceRepository,
};
use quota_core::snapshots::SnapshotBuilder;
use quota_domain::polling::ProviderPollingPolicy;
use quota_domain::snapshot::MonitoringState;

use crate::monitoring::MonitoringRuntime;
use crate::platform::window::OverviewWindowController;

/// The durable owners and native handles the commands reach.
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
    /// Durable application-wide monitoring state.
    pub monitoring_repository: Arc<dyn MonitoringRepository>,
    /// Confirmed monitoring state shared with the supervisor.
    pub monitoring_state: Arc<tokio::sync::RwLock<MonitoringState>>,
    /// Effective provider polling policies.
    pub policies: Arc<tokio::sync::RwLock<Vec<ProviderPollingPolicy>>>,
    /// The one shared supervisor used by automatic and manual refreshes.
    pub monitor: MonitoringRuntime,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppState").finish_non_exhaustive()
    }
}

impl AppState {
    /// Builds managed state and starts the shared monitor on Tauri's runtime.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        app: tauri::AppHandle,
        registry: AccountRegistry,
        builder: SnapshotBuilder,
        accounts: Arc<dyn AccountRepository>,
        backoff: Arc<dyn BackoffRepository>,
        history: Arc<dyn HistoryRepository>,
        preferences: Arc<dyn PreferenceRepository>,
        monitoring_repository: Arc<dyn MonitoringRepository>,
        monitoring_state: MonitoringState,
        policies: Vec<ProviderPollingPolicy>,
        providers: Arc<quota_providers::ProviderRegistry>,
    ) -> Self {
        let registry = Arc::new(tokio::sync::RwLock::new(registry));
        let snapshots = Arc::new(tokio::sync::Mutex::new(builder));
        let monitoring_state = Arc::new(tokio::sync::RwLock::new(monitoring_state));
        let policies = Arc::new(tokio::sync::RwLock::new(policies));
        let monitor = MonitoringRuntime::start(
            app,
            registry.clone(),
            snapshots.clone(),
            accounts.clone(),
            backoff.clone(),
            monitoring_state.clone(),
            policies.clone(),
            providers,
        );
        Self {
            registry,
            snapshots,
            clock: Arc::new(SystemClock),
            window: Arc::new(tokio::sync::Mutex::new(OverviewWindowController::new())),
            accounts,
            backoff,
            history,
            preferences,
            monitoring_repository,
            monitoring_state,
            policies,
            monitor,
        }
    }
}
