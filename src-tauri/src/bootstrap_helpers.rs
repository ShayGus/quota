//! Small helpers the command handlers share.
//!
//! These are the only places where a domain value, a transport value, and a
//! native side effect meet. Each one is small on purpose.

use std::path::{Path, PathBuf};

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
        view: preferences.view,
        widget_position: preferences.widget_position,
        account_sort: preferences.account_sort,
        account_order: preferences.account_order.clone(),
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
        view: presentation.view,
        widget_position: presentation.widget_position,
        account_sort: presentation.account_sort,
        account_order: presentation.account_order.clone(),
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
        ProviderId::Typesafe => Some("https://console.typesafe.ai/settings/billing"),
        ProviderId::Fixture => None,
    }
}

/// Opens an already-vetted address in the external browser.
///
/// The address is handed to the platform as it stands, so a query string keeps
/// its separators. A refusal names the address and the log, because "nothing
/// happened" is not something anyone can act on.
///
/// The call is generic over the runtime so the handover itself can be tested
/// against a throwaway launcher without building a real window.
///
/// # Errors
/// Returns [`CommandError::NativeOperationFailed`] when the platform refuses.
pub fn open_external<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    url: &str,
) -> Result<(), CommandError> {
    use tauri_plugin_opener::OpenerExt;
    app.opener().open_url(url, None::<&str>).map_err(|_| {
        crate::file_log::browser_failure(url, "the system browser refused the page");
        CommandError::NativeOperationFailed {
            operation: "open_external".into(),
            reason: format!(
                "the browser did not open {url}. The log is at {}",
                crate::file_log::location(app)
            ),
        }
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
///
/// Filesystem and containment failures name the log or report its unavailability.
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

    let body = serde_json::to_string_pretty(&report).map_err(|_| {
        tracing::warn!(reason = "diagnostics_encode", "diagnostics export failed");
        CommandError::Internal {
            code: "diagnostics_encode".into(),
        }
    })?;

    let log = crate::file_log::location(app);
    let write_error = |detail: &str| {
        tracing::warn!(reason = detail, "diagnostics export failed");
        CommandError::NativeOperationFailed {
            operation: "write_diagnostics".into(),
            reason: format!("{detail}. The log is at {log}"),
        }
    };
    let directory = app
        .path()
        .app_data_dir()
        .map_err(|_| write_error("the application data directory is unavailable"))?
        .join("diagnostics");
    std::fs::create_dir_all(&directory)
        .map_err(|_| write_error("the diagnostics directory could not be created"))?;

    let path = directory.join(format!("quota-diagnostics-{label}.json"));
    if !export_lands_inside(&path, &directory) {
        tracing::warn!(
            reason = "destination outside diagnostics directory",
            "diagnostics export failed"
        );
        return Err(CommandError::ValidationFailed {
            field: "destination".into(),
            reason: format!(
                "the export name resolved outside the diagnostics directory. \
                 The file was not written. The log is at {log}"
            ),
        });
    }

    std::fs::write(&path, body)
        .map_err(|_| write_error("the diagnostics file could not be written"))?;
    Ok(path.to_string_lossy().into_owned())
}

/// Whether a file built beside `directory` really lands inside it.
///
/// Both sides are resolved the same way before they are compared. Resolving
/// only the directory would put a verbatim `\\?\C:\...` prefix, which
/// [`std::fs::canonicalize`] returns on Windows, against the lexical `C:\...`
/// form of the file's parent. Those two never compare equal: their prefixes
/// are different variants of the same component kind, so every export would be
/// refused on Windows while passing everywhere else.
#[must_use]
pub fn export_lands_inside(path: &Path, directory: &Path) -> bool {
    path.parent()
        .is_some_and(|parent| resolved(parent).starts_with(resolved(directory)))
}

/// The canonical form of a path, falling back to the path itself.
///
/// The fallback keeps the check meaningful before the directory exists, and
/// both sides fall back together, so they stay in the same form.
fn resolved(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
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

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    static NEXT: AtomicU32 = AtomicU32::new(0);

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "quota-export-{tag}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).expect("the scratch directory can be created");
        dir
    }

    #[test]
    fn an_export_inside_the_directory_is_allowed() {
        let directory = scratch("inside");
        let path = directory.join("quota-diagnostics-settings.json");
        assert!(export_lands_inside(&path, &directory));
    }

    #[test]
    fn an_export_outside_the_directory_is_refused() {
        let base = scratch("outside");
        let directory = base.join("diagnostics");
        std::fs::create_dir(&directory).expect("the directory can be created");
        let path = base.join("elsewhere.json");
        assert!(!export_lands_inside(&path, &directory));
    }

    #[test]
    fn a_directory_that_does_not_exist_yet_is_compared_lexically_on_both_sides() {
        let base = scratch("pending");
        let directory = base.join("not-created-yet");
        assert!(export_lands_inside(
            &directory.join("export.json"),
            &directory
        ));
        assert!(!export_lands_inside(
            &base.join("other").join("export.json"),
            &directory
        ));
    }

    /// The directory may be spelled differently from the file's parent and
    /// still be the same place. Resolving one side and not the other refused
    /// every export on Windows, where a resolved directory carries a verbatim
    /// prefix the parent never has.
    #[cfg(unix)]
    #[test]
    fn a_parent_reached_through_a_link_is_still_the_directory() {
        let base = scratch("link");
        let directory = base.join("diagnostics");
        std::fs::create_dir(&directory).expect("the directory can be created");
        let link = base.join("link-to-diagnostics");
        std::os::unix::fs::symlink(&directory, &link).expect("the link can be made");
        assert!(export_lands_inside(&link.join("export.json"), &directory));
    }

    #[cfg(unix)]
    #[test]
    fn an_export_reached_through_a_link_out_of_the_directory_is_refused() {
        let base = scratch("escape");
        let directory = base.join("diagnostics");
        std::fs::create_dir(&directory).expect("the directory can be created");
        std::os::unix::fs::symlink(&base, directory.join("up")).expect("the link can be made");
        assert!(!export_lands_inside(
            &directory.join("up").join("export.json"),
            &directory
        ));
    }

    #[test]
    fn a_label_cannot_carry_a_path() {
        assert_eq!(safe_export_label("settings-1_2"), "settings-1_2");
        assert_eq!(safe_export_label("../../etc/passwd"), "etcpasswd");
        assert_eq!(safe_export_label(r"C:\Users\sgusi"), "CUserssgusi");
        assert_eq!(safe_export_label(""), "export");
        assert_eq!(safe_export_label("!!!"), "export");
        assert_eq!(safe_export_label(&"a".repeat(64)), "a".repeat(32));
    }
}
