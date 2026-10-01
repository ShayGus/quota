//! Composition and lifecycle registration.
//!
//! The single-instance plugin is registered first. Persistence is restored
//! before the monitor starts, and the monitor starts once on Tauri's shared
//! Tokio runtime. A second launch can only activate the existing overview.

use std::sync::Arc;

use quota_core::ports::{
    AccountRepository, BackoffRepository, HistoryRepository, MonitoringRepository,
    PreferenceRepository,
};
use quota_domain::ids::AppInstanceId;
use quota_domain::provider::{ProviderCapabilities, ProviderId};
use tauri::Manager;
use tauri_plugin_store::StoreExt;

use crate::bootstrap_helpers::PREFERENCES_SCHEMA_VERSION;
use crate::ipc::bindings;
use crate::state::AppState;

/// Declared capability data for one provider identifier.
///
/// A declaration does not claim that a live account was verified.
#[must_use]
pub fn capabilities_of(provider_id: ProviderId) -> ProviderCapabilities {
    match provider_id {
        ProviderId::Codex => ProviderCapabilities {
            provider_id,
            cardinality: quota_domain::account::AccountCardinality::SingleProfile,
            supports_app_owned_authorization: false,
            supports_external_profile: true,
            reports_monthly_window: false,
            minimum_interval_seconds: 300,
        },
        ProviderId::Claude => ProviderCapabilities {
            provider_id,
            cardinality: quota_domain::account::AccountCardinality::SingleProfile,
            supports_app_owned_authorization: false,
            supports_external_profile: true,
            reports_monthly_window: false,
            minimum_interval_seconds: 300,
        },
        ProviderId::OpenCodeGo => ProviderCapabilities {
            provider_id,
            cardinality: quota_domain::account::AccountCardinality::SingleProfile,
            supports_app_owned_authorization: false,
            supports_external_profile: true,
            reports_monthly_window: true,
            minimum_interval_seconds: 300,
        },
        ProviderId::ClinePass | ProviderId::Fixture => ProviderCapabilities {
            provider_id,
            cardinality: quota_domain::account::AccountCardinality::SingleProfile,
            supports_app_owned_authorization: false,
            supports_external_profile: false,
            reports_monthly_window: false,
            minimum_interval_seconds: 300,
        },
    }
}

/// Whether the production build contains an adapter for this provider.
#[must_use]
pub const fn is_compiled(provider_id: ProviderId) -> bool {
    matches!(
        provider_id,
        ProviderId::Codex | ProviderId::Claude | ProviderId::OpenCodeGo
    )
}

/// Creates and restores every durable owner before polling starts.
async fn initialize_backend(app: tauri::AppHandle) -> Result<(), String> {
    let data_dir = app
        .path()
        .app_config_dir()
        .map_err(|_| "app_config_dir_unavailable")?;
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
    let repositories = quota_persistence::SqliteRepositories::new(pool);

    let account_repository: Arc<dyn AccountRepository> = Arc::new(
        quota_persistence::ports::SqliteAccountPortAdapter::new(repositories.clone()),
    );
    let backoff_repository: Arc<dyn BackoffRepository> = Arc::new(
        quota_persistence::ports::SqliteBackoffRepository::new(repositories.clone()),
    );
    let history_repository: Arc<dyn HistoryRepository> = Arc::new(
        quota_persistence::ports::SqliteHistoryPortAdapter::new(repositories.clone()),
    );
    let monitoring_repository: Arc<dyn MonitoringRepository> = Arc::new(
        quota_persistence::ports::SqliteMonitoringPortAdapter::new(repositories.clone()),
    );

    let store = app
        .store("preferences.json")
        .map_err(|_| "preferences_store_open_failed")?;
    let document_store = quota_persistence::store::plugin::PluginDocumentStore::new(store);
    let codec = quota_persistence::PresentationPreferencesCodec::new(document_store);
    let preference_repository: Arc<dyn PreferenceRepository> = Arc::new(
        quota_persistence::ports::PresentationPreferencesPort::new(codec),
    );

    let stored_accounts = account_repository
        .load_accounts()
        .await
        .map_err(|error| format!("accounts_restore:{}", error.owner))?;
    let registry = quota_core::AccountRegistry::from_stored(stored_accounts);
    let monitoring_state = monitoring_repository
        .load_monitoring_state()
        .await
        .map_err(|error| format!("monitoring_restore:{}", error.owner))?;
    let presentation_preferences = preference_repository
        .load()
        .await
        .map_err(|error| format!("preferences_restore:{}", error.owner))?;
    if presentation_preferences.schema_version != PREFERENCES_SCHEMA_VERSION {
        return Err("preferences_schema_unsupported".to_owned());
    }

    let providers = Arc::new(
        quota_providers::ProviderRegistry::production()
            .map_err(|error| format!("provider_registry:{}", error.diagnostic_code()))?,
    );
    let policies = providers
        .registered()
        .into_iter()
        .filter_map(|provider_id| {
            providers
                .provider(provider_id)
                .map(|adapter| adapter.policy())
        })
        .collect();
    let app_instance_id = AppInstanceId::generate();
    let builder = quota_core::SnapshotBuilder::new(app_instance_id.clone());
    let state = AppState::new(
        app.clone(),
        app_instance_id,
        registry,
        builder,
        account_repository,
        backoff_repository,
        history_repository,
        preference_repository,
        monitoring_repository,
        monitoring_state,
        policies,
        providers,
    );
    if let Some(native) = app.get_webview_window("overview") {
        let visible = native.is_visible().unwrap_or(false);
        app.state::<AppState>()
            .window
            .lock()
            .await
            .set_visible(visible);
    }
    Ok(())
}

/// Starts the desktop host.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("quota=info,warn")),
        )
        .with_target(false)
        .init();

    let registry = bindings::registry();
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("overview") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_sql::Builder::default().build())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_positioner::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(registry.invoke_handler())
        .setup(move |app| {
            registry.mount_events(app);
            let handle = app.handle().clone();
            crate::platform::tray::install(&handle)?;
            crate::platform::window::install_close_handlers(&handle);
            let overview = app.get_webview_window("overview").ok_or_else(|| {
                std::io::Error::other("the overview window is missing")
            })?;
            overview.show()?;
            overview.set_focus()?;
            tauri::async_runtime::spawn(async move {
                if let Err(error) = initialize_backend(handle).await {
                    tracing::error!(target: "quota::bootstrap", code = %error, "backend initialization failed");
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("the desktop host must start");
}
