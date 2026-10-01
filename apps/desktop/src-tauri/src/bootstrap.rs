//! Composition and lifecycle registration.
//!
//! Registration order matters: the single-instance plugin is registered first so
//! a second launch is routed to an allowlisted activation action before any
//! persistence or scheduler side effect can happen twice.

use tauri::Manager;

use quota_domain::ids::AppInstanceId;
use quota_domain::provider::{ProviderCapabilities, ProviderId};

use crate::ipc::bindings;
use crate::state::AppState;

/// The provider capabilities this build declares.
///
/// Every entry is a declaration, not a claim of verified support. A provider
/// with no compiled adapter still appears here so the renderer can show an
/// explicit unsupported state instead of silently omitting it.
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

/// Whether a provider has an adapter compiled into this build.
///
/// The fixture adapter exists only under the non-default `test-fixtures`
/// feature, so a release build reports it as not compiled.
#[must_use]
pub const fn is_compiled(provider_id: ProviderId) -> bool {
    matches!(provider_id, ProviderId::Codex | ProviderId::Claude | ProviderId::OpenCodeGo)
}

/// Builds the managed state and mounts the typed IPC registry.
fn build_state(app: &tauri::AppHandle) -> Result<tauri::State<'static, AppState>, String> {
    let registry = quota_core::AccountRegistry::new();
    let builder = quota_core::SnapshotBuilder::new(AppInstanceId::generate());
    let state = AppState::new(registry, builder).map_err(|error| error.diagnostic_code().to_owned())?;
    app.manage(state);
    Ok(app.state::<AppState>())
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

    let builder = bindings::registry();

    tauri::Builder::default()
        // Registered first: a second launch must not run migrations or start a
        // second scheduler. Its arguments are treated as untrusted data.
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window("overview") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(
            tauri_plugin_sql::Builder::default()
                .add_migrations("sqlite:quota", quota_persistence::migrations())
                .build(),
        )
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_positioner::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(builder.mount_events())
        .setup(|app| {
            build_state(app.handle())?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("the desktop host must start");
}