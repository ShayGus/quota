//! Small helpers the command handlers share.
//!
//! These are the only places where a domain value, a transport value, and a
//! native side effect meet. Each one is small on purpose.

use quota_contracts::CommandError;
use quota_contracts::preferences::Preferences;
use quota_domain::preferences::{
    OperationalPreferences, OperationalPrivacyPreferences, PresentationPreferences,
};
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
        indicator_style: preferences.indicator_style,
        overview_mode: preferences.overview_mode,
        always_on_top: preferences.always_on_top,
        launch_behavior: preferences.launch_behavior,
        privacy_alias_mode: preferences.privacy.alias_mode,
        reduce_motion: preferences.reduce_motion,
    }
}

/// Maps SQLite-owned settings out of the renderer aggregate.
#[must_use]
pub fn to_operational(preferences: &Preferences, revision: u32) -> OperationalPreferences {
    OperationalPreferences {
        revision,
        notifications: preferences.notifications,
        privacy: OperationalPrivacyPreferences {
            retain_history: preferences.privacy.retain_history,
            export_identities: preferences.privacy.export_identities,
        },
        polling: preferences.polling.clone(),
    }
}

/// Assembles the renderer aggregate from its two durable owners.
#[must_use]
pub fn from_persisted(
    presentation: &PresentationPreferences,
    operational: &OperationalPreferences,
) -> Preferences {
    Preferences {
        schema_version: PREFERENCES_SCHEMA_VERSION,
        revision: presentation.revision.max(operational.revision),
        theme: presentation.theme,
        indicator_style: presentation.indicator_style,
        overview_mode: presentation.overview_mode,
        always_on_top: presentation.always_on_top,
        launch_behavior: presentation.launch_behavior,
        reduce_motion: presentation.reduce_motion,
        notifications: operational.notifications,
        privacy: quota_contracts::preferences::PrivacyPolicy {
            alias_mode: presentation.privacy_alias_mode,
            retain_history: operational.privacy.retain_history,
            export_identities: operational.privacy.export_identities,
        },
        polling: operational.polling.clone(),
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
        ProviderId::Openrouter => Some("https://openrouter.ai/settings/credits"),
        ProviderId::Zai => Some("https://z.ai/manage-apikey/subscription"),
        ProviderId::Minimax => Some("https://platform.minimax.io/user-center/payment/token-plan"),
        ProviderId::Kimi => Some("https://www.kimi.com/code/console"),
        ProviderId::Grok => Some("https://grok.com/?_s=usage"),
        ProviderId::MuseCode => Some("https://www.meta.ai/muse-code"),
        ProviderId::Cursor => Some("https://cursor.com/dashboard?tab=usage"),
        ProviderId::OllamaCloud => Some("https://ollama.com/settings"),
        ProviderId::Fixture => None,
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

/// Writes a redacted diagnostics export into a host-owned directory.
///
/// The renderer never supplies a path. It may suggest a short label, which is
/// reduced to `[A-Za-z0-9_-]` and capped, so traversal and absolute paths
/// cannot survive. The final path is built inside the application data
/// directory and then re-checked, so an export can never truncate an unrelated
/// file such as another tool's credential store.
///
/// The document is rebuilt from typed values here rather than dumped from a log
/// buffer, so a token, cookie, account address, or profile path cannot reach
/// the file by accident.
pub async fn write_diagnostics(
    app: &tauri::AppHandle,
    state: &crate::state::AppState,
    label: &str,
) -> Result<String, CommandError> {
    let policies = state.policies.read().await;
    let report = serde_json::json!({
        "schema_version": 1,
        "account_count": state.registry.read().await.len(),
        "providers": policies.iter().map(|policy| policy.provider_id).collect::<Vec<_>>(),
        "polling": &*policies,
    });
    drop(policies);

    let body = serde_json::to_string_pretty(&report).map_err(|_| CommandError::Internal {
        code: "diagnostics_encode".into(),
    })?;

    let directory = app
        .path()
        .app_data_dir()
        .map_err(|_| CommandError::NativeOperationFailed {
            operation: "write_diagnostics".into(),
            reason: "the application data directory is unavailable".into(),
        })?
        .join("diagnostics");
    std::fs::create_dir_all(&directory).map_err(|_| CommandError::NativeOperationFailed {
        operation: "write_diagnostics".into(),
        reason: "the diagnostics directory could not be created".into(),
    })?;

    let path = directory.join(format!("quota-diagnostics-{label}.json"));
    let resolved = std::fs::canonicalize(&directory).unwrap_or_else(|_| directory.clone());
    let inside = path
        .parent()
        .is_some_and(|parent| parent.starts_with(&resolved));
    if !inside {
        return Err(CommandError::ValidationFailed {
            field: "destination".into(),
            reason: "the export name resolved outside the diagnostics directory".into(),
        });
    }

    std::fs::write(&path, body).map_err(|_| CommandError::NativeOperationFailed {
        operation: "write_diagnostics".into(),
        reason: "the diagnostics file could not be written".into(),
    })?;
    Ok(path.to_string_lossy().into_owned())
}

/// Reduces a renderer-supplied label to a safe file-name stem.
///
/// Anything outside `[A-Za-z0-9_-]` is dropped, and the result is capped, so no
/// separator, drive letter, or relative segment survives.
#[must_use]
pub fn safe_export_label(label: &str) -> String {
    let stem: String = label
        .chars()
        .filter(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'))
        .take(32)
        .collect();
    if stem.is_empty() {
        "export".to_owned()
    } else {
        stem
    }
}
