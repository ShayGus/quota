//! Composition and lifecycle registration.
//!
//! The single-instance plugin is registered first. Persistence is restored
//! before the monitor starts, and the monitor starts once on Tauri's shared
//! Tokio runtime. A second launch can only activate the existing overview.

use std::sync::Arc;

use quota_core::ports::{
    AccountRepository, BackoffRepository, HistoryRepository, MonitoringRepository,
    OperationalPreferencesRepository, PreferenceRepository,
};
use quota_domain::ids::AppInstanceId;
use tauri::Manager;
use tauri_plugin_store::StoreExt;

use crate::bootstrap_helpers::PREFERENCES_SCHEMA_VERSION;
use crate::ipc::bindings;
use crate::state::AppState;

pub use crate::provider_catalog::{capabilities_of, is_compiled};

/// Whether this build seeds the ten sample accounts instead of reading real ones.
#[must_use]
pub const fn sample_data_enabled() -> bool {
    cfg!(feature = "sample-data")
}

/// Creates and restores every durable owner before polling starts.
async fn initialize_backend(app: tauri::AppHandle) -> Result<(), String> {
    let sqlite = open_database(&app).await?;
    let repositories = backend_repositories(&app, &sqlite)?;
    let restored = restore_durable_state(&repositories).await?;
    let providers = Arc::new(build_registry(&app)?);
    let (policies, confirmed_operational) =
        resolve_effective_policies(&providers, restored.operational_preferences);
    install_managed_state(
        &app,
        &repositories,
        restored.documents,
        &providers,
        policies,
        &confirmed_operational,
    )
    .await?;
    // Every window loaded while the backend was starting. One snapshot now
    // reaches each of them, so none waits for the next scheduled refresh to
    // show the accounts it could not read during startup.
    if let Err(error) = app.state::<AppState>().monitor.publish().await {
        tracing::warn!(
            code = error.diagnostic_code(),
            "the first snapshot could not be published"
        );
    }
    Ok(())
}

/// Opens the `SQLite` pool, migrates it, and hands it to the SQL plugin.
async fn open_database(
    app: &tauri::AppHandle,
) -> Result<quota_persistence::SqliteRepositories, String> {
    let data_dir = app
        .path()
        .app_config_dir()
        .map_err(|_| "app_config_dir_unavailable")?;
    let data_dir = sample_subdirectory(data_dir);
    std::fs::create_dir_all(&data_dir).map_err(|_| "app_config_dir_create_failed")?;
    let database_path = data_dir.join("quota.sqlite");
    let settings = quota_persistence::SqlitePoolSettings::default();
    let pool = quota_persistence::sqlite::open_pool(&database_path, settings)
        .await
        .map_err(|error| format!("sqlite_open:{error}"))?;
    quota_persistence::sqlite::run_migrations(&pool)
        .await
        .map_err(|_| "sqlite_migration_failed")?;
    quota_persistence::sqlite::verify_pool_settings(&pool, &settings)
        .await
        .map_err(|_| "sqlite_connection_settings_failed")?;

    // The SQL plugin owns the lifecycle registry. Insert the already-configured
    // pool there so plugin shutdown closes this exact pool; do not open a second
    // migration owner through a renderer command.
    app.state::<tauri_plugin_sql::DbInstances>()
        .0
        .write()
        .await
        .insert(
            "sqlite:quota".to_owned(),
            tauri_plugin_sql::DbPool::Sqlite(pool.clone()),
        );
    Ok(quota_persistence::SqliteRepositories::new(pool))
}

/// The durable owners every backend phase shares.
struct BackendRepositories {
    accounts: Arc<dyn AccountRepository>,
    backoff: Arc<dyn BackoffRepository>,
    history: Arc<dyn HistoryRepository>,
    monitoring: Arc<dyn MonitoringRepository>,
    operational_preferences: Arc<dyn OperationalPreferencesRepository>,
    preferences: Arc<dyn PreferenceRepository>,
}

/// Opens the preference store and wraps every durable owner.
fn backend_repositories(
    app: &tauri::AppHandle,
    sqlite: &quota_persistence::SqliteRepositories,
) -> Result<BackendRepositories, String> {
    let account_repository: Arc<dyn AccountRepository> = Arc::new(
        quota_persistence::ports::SqliteAccountPortAdapter::new(sqlite.clone()),
    );
    let backoff_repository: Arc<dyn BackoffRepository> = Arc::new(
        quota_persistence::ports::SqliteBackoffRepository::new(sqlite.clone()),
    );
    let history_repository: Arc<dyn HistoryRepository> = Arc::new(
        quota_persistence::ports::SqliteHistoryPortAdapter::new(sqlite.clone()),
    );
    let monitoring_repository: Arc<dyn MonitoringRepository> = Arc::new(
        quota_persistence::ports::SqliteMonitoringPortAdapter::new(sqlite.clone()),
    );
    let operational_preferences_repository: Arc<dyn OperationalPreferencesRepository> = Arc::new(
        quota_persistence::ports::SqliteOperationalPreferencesPortAdapter::new(sqlite.clone()),
    );

    let store = app
        .store(store_path(app)?)
        .map_err(|_| "preferences_store_open_failed")?;
    let document_store = quota_persistence::store::plugin::PluginDocumentStore::new(store);
    let codec = quota_persistence::PresentationPreferencesCodec::new(document_store);
    let preference_repository: Arc<dyn PreferenceRepository> = Arc::new(
        quota_persistence::ports::PresentationPreferencesPort::new(codec),
    );
    Ok(BackendRepositories {
        accounts: account_repository,
        backoff: backoff_repository,
        history: history_repository,
        monitoring: monitoring_repository,
        operational_preferences: operational_preferences_repository,
        preferences: preference_repository,
    })
}

/// The documents restored from durable storage before polling starts.
struct RestoredDocuments {
    stored_accounts: Vec<quota_core::ports::StoredAccount>,
    monitoring_state: quota_domain::snapshot::MonitoringState,
    presentation_preferences: quota_domain::preferences::PresentationPreferences,
}

/// The sample database and preference store, isolated from the real ones.
///
/// With `sample-data` off this is the path it was handed, unchanged, so the
/// production layout is byte-identical. With `sample-data` on every durable
/// write lands under a `sample` child of the app config directory.
fn sample_subdirectory(data_dir: std::path::PathBuf) -> std::path::PathBuf {
    if cfg!(feature = "sample-data") {
        data_dir.join("sample")
    } else {
        data_dir
    }
}

/// The preference store path, under the same sample subdirectory.
fn store_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    if !cfg!(feature = "sample-data") {
        return Ok(std::path::PathBuf::from("preferences.json"));
    }
    let data_dir = app
        .path()
        .app_config_dir()
        .map_err(|_| "app_config_dir_unavailable")?;
    Ok(sample_subdirectory(data_dir).join("preferences.json"))
}

/// The registry this build starts with.
///
/// `sample-data` adds the deterministic fixture adapter, so the seeded accounts
/// have an adapter to read through. The production build compiles neither the
/// fixture module nor this branch.
fn build_registry(app: &tauri::AppHandle) -> Result<quota_providers::ProviderRegistry, String> {
    let secrets = quota_providers::secrets::system(&app.config().identifier);
    #[cfg(feature = "sample-data")]
    let registry = quota_providers::ProviderRegistry::with_fixture(secrets);
    #[cfg(not(feature = "sample-data"))]
    let registry = quota_providers::ProviderRegistry::production(secrets);
    registry.map_err(|error| format!("provider_registry:{}", error.diagnostic_code()))
}

/// Everything `initialize_backend` restores before it publishes managed state.
struct RestoredState {
    documents: RestoredDocuments,
    operational_preferences: quota_domain::preferences::OperationalPreferences,
}

/// Loads every durable document and rejects an unsupported preference schema.
async fn restore_durable_state(
    repositories: &BackendRepositories,
) -> Result<RestoredState, String> {
    let stored_accounts = repositories
        .accounts
        .load_accounts()
        .await
        .map_err(|error| format!("accounts_restore:{}", error.owner))?;
    let monitoring_state = repositories
        .monitoring
        .load_monitoring_state()
        .await
        .map_err(|error| format!("monitoring_restore:{}", error.owner))?;
    let operational_preferences = repositories
        .operational_preferences
        .load()
        .await
        .map_err(|error| format!("operational_preferences_restore:{}", error.owner))?;
    let presentation_preferences = repositories
        .preferences
        .load()
        .await
        .map_err(|error| format!("preferences_restore:{}", error.owner))?;
    if presentation_preferences.schema_version != PREFERENCES_SCHEMA_VERSION {
        return Err("preferences_schema_unsupported".to_owned());
    }
    Ok(RestoredState {
        documents: RestoredDocuments {
            stored_accounts,
            monitoring_state,
            presentation_preferences,
        },
        operational_preferences,
    })
}

/// Merges saved polling policies over the compiled defaults.
///
/// A saved entry replaces the compiled default for the same provider. Saved
/// entries for unknown providers are kept for display but never scheduled.
fn resolve_effective_policies(
    providers: &quota_providers::ProviderRegistry,
    operational_preferences: quota_domain::preferences::OperationalPreferences,
) -> (
    Vec<quota_domain::polling::ProviderPollingPolicy>,
    quota_domain::preferences::OperationalPreferences,
) {
    let default_policies = providers
        .registered()
        .into_iter()
        .filter_map(|provider_id| {
            providers
                .provider(provider_id)
                .map(|adapter| adapter.policy())
        })
        .collect::<Vec<_>>();
    let mut displayed_policies = default_policies.clone();
    for saved in &operational_preferences.polling {
        if let Some(current) = displayed_policies
            .iter_mut()
            .find(|current| current.provider_id == saved.provider_id)
        {
            *current = saved.clone();
        } else {
            displayed_policies.push(saved.clone());
        }
    }
    let policies = displayed_policies
        .iter()
        .filter(|policy| providers.provider(policy.provider_id).is_some())
        .cloned()
        .collect();
    let mut confirmed_operational = operational_preferences;
    confirmed_operational.polling = displayed_policies;
    (policies, confirmed_operational)
}

/// Builds managed state, publishes it, and confirms the native window.
async fn install_managed_state(
    app: &tauri::AppHandle,
    repositories: &BackendRepositories,
    documents: RestoredDocuments,
    providers: &Arc<quota_providers::ProviderRegistry>,
    policies: Vec<quota_domain::polling::ProviderPollingPolicy>,
    confirmed_operational: &quota_domain::preferences::OperationalPreferences,
) -> Result<(), String> {
    let initial_preferences = crate::bootstrap_helpers::from_persisted(
        &documents.presentation_preferences,
        confirmed_operational,
    );
    let app_instance_id = AppInstanceId::generate();
    let builder = quota_core::SnapshotBuilder::new(app_instance_id.clone());
    let registry = quota_core::AccountRegistry::from_stored(documents.stored_accounts);
    // The sample build seeds its accounts through the same repository and
    // registry a real connection command uses, then refreshes them through the
    // same supervisor, so the readings arrive by the ordinary path.
    #[cfg(feature = "sample-data")]
    let registry = {
        let mut seeded = registry;
        let adapter = quota_providers::FixtureAdapter::new();
        quota_providers::fixture::seed_sample_accounts(
            &adapter,
            &repositories.accounts,
            &mut seeded,
        )
        .await?;
        seeded
    };
    let state = AppState::new(crate::state::AppStateParts {
        app: app.clone(),
        app_instance_id,
        registry,
        builder,
        accounts: repositories.accounts.clone(),
        backoff: repositories.backoff.clone(),
        history: repositories.history.clone(),
        preference_repository: repositories.preferences.clone(),
        operational_preferences: repositories.operational_preferences.clone(),
        initial_preferences,
        monitoring_repository: repositories.monitoring.clone(),
        monitoring_state: documents.monitoring_state,
        policies,
        providers: providers.clone(),
    });
    app.manage(state);
    let native = app
        .get_webview_window("overview")
        .ok_or_else(|| "overview_window_missing".to_owned())?;
    let state = app.state::<AppState>();
    let always_on_top = state.preferences_state.read().await.always_on_top;
    native
        .set_always_on_top(always_on_top)
        .map_err(|_| "window_topmost_restore_failed")?;
    let confirmed_topmost = native
        .is_always_on_top()
        .map_err(|_| "window_topmost_read_failed")?;
    if confirmed_topmost != always_on_top {
        return Err("window_topmost_confirmation_failed".to_owned());
    }
    // The saved view opens now that the preferences are read.
    crate::platform::app_view::restore_saved(&state)
        .await
        .map_err(|error| format!("view_restore_failed:{}", error.diagnostic_code()))?;
    let visible = native
        .is_visible()
        .map_err(|_| "window_visibility_read_failed")?;
    let mut controller = state.window.lock().await;
    controller.set_always_on_top(confirmed_topmost);
    controller.set_visible(visible);
    let saved_mode = state.preferences_state.read().await.overview_mode;
    // The saved mode's chrome needs no tray geometry, so it applies now. The
    // anchor reads the tray icon's rectangle from the system; if the icon is
    // not placed yet, the next tray event completes it.
    crate::platform::window::apply_mode_chrome(&native, saved_mode, &mut controller)
        .map_err(|_| "window_mode_chrome_restore_failed")?;
    drop(controller);
    if saved_mode == quota_domain::preferences::OverviewMode::Tray
        && let Err(error) = crate::platform::window::anchor_to_tray(app)
    {
        tracing::warn!(
            code = error.diagnostic_code(),
            "overview could not anchor to the tray"
        );
    }
    // One refresh through the shared supervisor fills each seeded account's
    // reading by the ordinary read path.
    #[cfg(feature = "sample-data")]
    state
        .monitor
        .request_all(crate::monitoring::RefreshReason::OverviewOpened)
        .await
        .map_err(|error| format!("sample_refresh:{error:?}"))?;
    Ok(())
}

/// Starts the desktop host.
///
/// # Errors
///
/// Returns the backend message when the Tauri host cannot start.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn start() -> Result<(), String> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("quota=info,warn")),
        )
        .with_target(false)
        .init();

    let registry = bindings::registry();
    // The window-state plugin writes into the app config directory. The sample
    // build keeps its copy under the same `sample` child as its database.
    let state_flags = crate::platform::window::restored_state_flags();
    // The widget's position is saved in the preferences whenever it moves, so
    // the plugin, which saves only at a clean exit, leaves that window alone.
    let unmanaged = [crate::platform::widget::LABEL];
    #[cfg(feature = "sample-data")]
    let window_state = tauri_plugin_window_state::Builder::default()
        .with_filename("sample/.window-state.json")
        .with_state_flags(state_flags)
        .with_denylist(&unmanaged)
        .build();
    #[cfg(not(feature = "sample-data"))]
    let window_state = tauri_plugin_window_state::Builder::default()
        .with_state_flags(state_flags)
        .with_denylist(&unmanaged)
        .build();
    with_agent_inspection(
        tauri::Builder::default()
            .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
                // Quota runs once. A second launch by a person brings the running
                // overview forward; it never opens the settings window, which
                // stays as the person left it. A login-item launch that finds Quota
                // already running changes nothing.
                if !crate::platform::autostart::launched_at_login(argv.into_iter()) {
                    crate::platform::app_view::activate(app);
                }
            }))
            .plugin(crate::platform::autostart::plugin())
            .plugin(tauri_plugin_store::Builder::new().build())
            .plugin(tauri_plugin_sql::Builder::default().build())
            .plugin(window_state)
            .plugin(tauri_plugin_opener::init())
            .plugin(tauri_plugin_notification::init()),
    )
    .invoke_handler(registry.invoke_handler())
    .setup(move |app| {
        registry.mount_events(app);
        let handle = app.handle().clone();
        crate::platform::tray::install(&handle)?;
        crate::platform::window_events::install_close_handlers(&handle);
        // The saved view, the overview or the widget, is the only window a
        // launch opens, once the backend has read which one it is. The
        // settings window was created hidden and waits for a person to ask for
        // it. A backend that cannot start still opens the overview, which
        // explains the failure, unless this is a quiet launch at login.
        tauri::async_runtime::spawn(async move {
            if let Err(error) = initialize_backend(handle.clone()).await {
                tracing::error!(target: "quota::bootstrap", code = %error, "backend initialization failed");
                if !crate::platform::autostart::launched_at_login(std::env::args())
                    && let Err(error) = crate::platform::window::activate_overview(&handle)
                {
                    tracing::warn!(%error, "the overview could not be shown");
                }
            }
        });
        Ok(())
    })
    .run(tauri::generate_context!())
    .map_err(|error| format!("desktop_host_start_failed:{error}"))
}

/// Registers the agent inspection plugin, which no release build can reach.
///
/// The plugin opens a Unix socket an AI agent connects to for screenshots, the
/// DOM, the console log, and IPC calls. Both conditions must hold: the
/// `agent-inspection` feature, which no release feature set selects, and
/// `debug_assertions`, so a release build excludes this registration call even
/// if the feature is explicitly selected and the dependency is compiled. The
/// plugin's own release refusal stays armed as a third line. See
/// `docs/inspecting-the-app.md` for the client side.
#[cfg(all(debug_assertions, feature = "agent-inspection"))]
fn with_agent_inspection(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    // The defaults start the socket server on a random authentication token,
    // which is written beside the socket as `/tmp/tauri-mcp.sock.token`.
    builder.plugin(tauri_plugin_mcp::init_with_config(
        tauri_plugin_mcp::PluginConfig::new(String::new()),
    ))
}

/// Every other build registers no inspection plugin; see the enabled twin.
#[cfg(not(all(debug_assertions, feature = "agent-inspection")))]
fn with_agent_inspection(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    builder
}
