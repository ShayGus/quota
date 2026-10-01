//! Small helpers the command handlers share.
//!
//! These are the only places where a domain value, a transport value, and a
//! native side effect meet. Each one is small on purpose.

use quota_contracts::CommandError;
use quota_contracts::preferences::Preferences;
use quota_domain::preferences::PresentationPreferences;
use quota_domain::provider::ProviderId;
use tauri::Manager;

/// The preference schema version this build writes.
pub const PREFERENCES_SCHEMA_VERSION: u32 = 1;

/// Maps the renderer aggregate onto the typed store document.
#[must_use]
pub fn to_presentation(preferences: &Preferences) -> PresentationPreferences {
    PresentationPreferences {
        schema_version: PREFERENCES_SCHEMA_VERSION,
        revision: preferences.revision,
        theme: preferences.theme,
        density: preferences.density,
        indicator_style: preferences.indicator_style,
        overview_mode: preferences.overview_mode,
        always_on_top: preferences.always_on_top,
        launch_behavior: preferences.launch_behavior,
        privacy_alias_mode: preferences.privacy.alias_mode,
        reduce_motion: preferences.reduce_motion,
    }
}

/// Maps the typed store document back onto the renderer aggregate.
#[must_use]
pub fn from_presentation(stored: &PresentationPreferences) -> Preferences {
    Preferences {
        schema_version: PREFERENCES_SCHEMA_VERSION,
        revision: stored.revision,
        theme: stored.theme,
        density: stored.density,
        indicator_style: stored.indicator_style,
        overview_mode: stored.overview_mode,
        always_on_top: stored.always_on_top,
        launch_behavior: stored.launch_behavior,
        reduce_motion: stored.reduce_motion,
        notifications: quota_contracts::preferences::NotificationPolicy {
            enabled: true,
            thresholds: Default::default(),
            quiet_hours: quota_contracts::preferences::QuietHours::Never,
            recovery_enabled: true,
        },
        privacy: quota_contracts::preferences::PrivacyPolicy {
            alias_mode: stored.privacy_alias_mode,
            retain_history: true,
            export_identities: false,
        },
        polling: Vec::new(),
    }
}

/// The allowlisted usage page for one provider.
///
/// The renderer never supplies a URL. An unknown provider returns `None` and
/// the command refuses, rather than opening something unvetted.
#[must_use]
pub fn usage_page_of(provider_id: ProviderId) -> Option<&'static str> {
    match provider_id {
        ProviderId::Codex => Some("https://chatgpt.com/codex/settings/usage"),
        ProviderId::Claude => Some("https://claude.ai/settings/usage"),
        ProviderId::OpenCodeGo => Some("https://opencode.ai/zen"),
        ProviderId::ClinePass | ProviderId::Fixture => None,
    }
}

/// Opens an already-vetted address in the external browser.
///
/// # Errors
/// Returns [`CommandError::NativeOperationFailed`] when the platform refuses.
pub fn open_external(app: &tauri::AppHandle, url: &str) -> Result<(), CommandError> {
    use tauri_plugin_opener::OpenerExt;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|_| CommandError::NativeOperationFailed {
            operation: "open_external".into(),
            reason: "the system browser refused the request".into(),
        })
}

/// Writes a redacted diagnostics export.
///
/// The document is rebuilt from typed values here rather than dumped from a log
/// buffer, so a token, cookie, account address, or profile path cannot reach
/// the file by accident.
pub async fn write_diagnostics(
    app: &tauri::AppHandle,
    state: &crate::state::AppState,
    destination: &str,
) -> Result<String, CommandError> {
    let policies = state.policies.read().await;
    let report = serde_json::json!({
        "schema_version": 1,
        "account_count": state.registry.read().await.len(),
        "providers": policies.iter().map(|policy| policy.provider_id).collect::<Vec<_>>(),
        "polling": policies,
    });
    drop(policies);

    let body = serde_json::to_string_pretty(&report).map_err(|_| CommandError::Internal {
        code: "diagnostics_encode".into(),
    })?;
    let path = std::path::Path::new(destination);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| CommandError::NativeOperationFailed {
            operation: "write_diagnostics".into(),
            reason: "the chosen directory could not be created".into(),
        })?;
    }
    std::fs::write(path, body).map_err(|_| CommandError::NativeOperationFailed {
        operation: "write_diagnostics".into(),
        reason: "the chosen path could not be written".into(),
    })?;
    let _ = app;
    Ok(destination.to_owned())
}
