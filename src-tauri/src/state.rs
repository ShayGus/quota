//! Managed state the commands read.
//!
//! It holds service handles, not business logic. Commands receive exactly the
//! narrow handle they need through this struct rather than the whole context.

use std::sync::Arc;

use quota_core::accounts::AccountRegistry;
use quota_core::clock::SystemClock;
use quota_core::ports::{
    AccountRepository, BackoffRepository, HistoryRepository, MonitoringRepository,
    OperationalPreferencesRepository, PreferenceRepository,
};
use quota_core::snapshots::SnapshotBuilder;
use quota_domain::polling::ProviderPollingPolicy;
use quota_domain::snapshot::MonitoringState;

use crate::monitoring::MonitoringRuntime;
use crate::platform::window::OverviewWindowController;

/// The durable owners and native handles the commands reach.
#[derive(Clone)]
pub struct AppState {
    /// The identity shared by this process's snapshots and events.
    pub app_instance_id: quota_domain::ids::AppInstanceId,
    /// The native app handle for window and tray operations.
    pub app: tauri::AppHandle,
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
    /// SQLite-owned notification, privacy, and polling settings.
    pub operational_preferences: Arc<dyn OperationalPreferencesRepository>,
    /// Serializes writes across the two preference owners.
    pub preferences_write: Arc<tokio::sync::Mutex<()>>,
    /// The last aggregate confirmed by both durable owners.
    pub preferences_state: Arc<tokio::sync::RwLock<quota_contracts::Preferences>>,
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
    #[must_use]
    pub fn new(parts: AppStateParts) -> Self {
        let AppStateParts {
            app,
            app_instance_id,
            registry,
            builder,
            accounts,
            backoff,
            history,
            preference_repository,
            operational_preferences,
            initial_preferences,
            monitoring_repository,
            monitoring_state,
            policies,
            providers,
        } = parts;
        let registry = Arc::new(tokio::sync::RwLock::new(registry));
        let snapshots = Arc::new(tokio::sync::Mutex::new(builder));
        let monitoring_state = Arc::new(tokio::sync::RwLock::new(monitoring_state));
        let preferences_state = Arc::new(tokio::sync::RwLock::new(initial_preferences));
        let preferences_write = Arc::new(tokio::sync::Mutex::new(()));
        let policies = Arc::new(tokio::sync::RwLock::new(policies));
        let monitor = MonitoringRuntime::start(crate::monitoring::MonitoringStartup {
            app: app.clone(),
            registry: registry.clone(),
            snapshots: snapshots.clone(),
            accounts: accounts.clone(),
            backoff: backoff.clone(),
            monitoring: monitoring_state.clone(),
            policies: policies.clone(),
            providers,
        });
        Self {
            app,
            app_instance_id,
            registry,
            snapshots,
            clock: Arc::new(SystemClock),
            window: Arc::new(tokio::sync::Mutex::new(OverviewWindowController::new())),
            accounts,
            backoff,
            history,
            preferences: preference_repository,
            operational_preferences,
            preferences_write,
            preferences_state,
            monitoring_repository,
            monitoring_state,
            policies,
            monitor,
        }
    }
}

/// Everything [`AppState::new`] composes, named once.
///
/// The constructor takes one of these rather than fourteen positional arguments,
/// so no caller can transpose two owners silently and so no lint has to be
/// silenced to build the state.
pub struct AppStateParts {
    /// The native app handle.
    pub app: tauri::AppHandle,
    /// The identity shared by this process's snapshots and events.
    pub app_instance_id: quota_domain::ids::AppInstanceId,
    /// The account registry, owned by the application core.
    pub registry: AccountRegistry,
    /// The snapshot builder and its revision counter.
    pub builder: SnapshotBuilder,
    /// Durable account and binding state.
    pub accounts: Arc<dyn AccountRepository>,
    /// Scoped rate-limit and backoff state.
    pub backoff: Arc<dyn BackoffRepository>,
    /// Local reading history.
    pub history: Arc<dyn HistoryRepository>,
    /// Non-transactional presentation preferences.
    pub preference_repository: Arc<dyn PreferenceRepository>,
    /// SQLite-owned notification, privacy, and polling settings.
    pub operational_preferences: Arc<dyn OperationalPreferencesRepository>,
    /// The preference aggregate confirmed by both durable owners.
    pub initial_preferences: quota_contracts::Preferences,
    /// Durable application-wide monitoring state.
    pub monitoring_repository: Arc<dyn MonitoringRepository>,
    /// The confirmed monitoring state shared with the supervisor.
    pub monitoring_state: MonitoringState,
    /// The effective provider polling policies.
    pub policies: Vec<ProviderPollingPolicy>,
    /// The compiled provider registry.
    pub providers: Arc<quota_providers::ProviderRegistry>,
}
