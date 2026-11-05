//! The global key combination that finds the mini widget.
//!
//! It defaults to Ctrl+Alt+Q, and the person can change it or turn it off,
//! because another app may already hold it. The system says so when it is
//! registered, so Settings can say why it isn't working; the tray keeps
//! working either way. Wayland gives apps no global shortcuts, so the setting
//! isn't offered there.

use std::str::FromStr as _;
use std::sync::atomic::{AtomicBool, Ordering};

use quota_contracts::CommandError;
use quota_contracts::commands::FindShortcutStatus;
use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt as _, Shortcut, ShortcutState};

use super::widget;

/// Whether the saved combination is registered right now.
static ACTIVE: AtomicBool = AtomicBool::new(false);

/// Whether this desktop can have the shortcut, and whether it has it.
#[must_use]
pub fn status() -> FindShortcutStatus {
    FindShortcutStatus {
        supported: !widget::on_wayland(),
        active: ACTIVE.load(Ordering::SeqCst),
    }
}

/// Registers the saved combination at launch. If it is taken, Quota still runs,
/// and Settings says so.
pub fn restore(app: &AppHandle, saved: Option<&str>) {
    if widget::on_wayland() {
        return;
    }
    if let Err(error) = register(app, saved) {
        tracing::warn!(
            code = error.diagnostic_code(),
            "the shortcut that finds the widget is not available"
        );
    }
}

/// Moves from the `previous` combination to the `next` one. If the next one
/// can't be had, the previous one goes on working.
///
/// # Errors
/// Returns [`CommandError::ValidationFailed`] for a combination that can't be
/// read or that another app holds.
pub fn set(
    app: &AppHandle,
    previous: Option<&str>,
    next: Option<&str>,
) -> Result<(), CommandError> {
    if widget::on_wayland() {
        return Err(CommandError::ValidationFailed {
            field: "find_widget_shortcut".into(),
            reason: "This desktop doesn't let apps have a global shortcut.".into(),
        });
    }
    if let Some(next) = next {
        parse(next)?;
    }
    register(app, next).inspect_err(|_| {
        if let Err(error) = register(app, previous) {
            tracing::warn!(
                code = error.diagnostic_code(),
                "the previous shortcut could not be restored"
            );
        }
    })
}

/// Makes `combination` the only registered shortcut, or none.
fn register(app: &AppHandle, combination: Option<&str>) -> Result<(), CommandError> {
    let shortcuts = app.global_shortcut();
    if shortcuts.unregister_all().is_err() {
        tracing::warn!("the old shortcut could not be released");
    }
    ACTIVE.store(false, Ordering::SeqCst);
    let Some(combination) = combination else {
        return Ok(());
    };
    let parsed = parse(combination)?;
    shortcuts
        .on_shortcut(parsed, |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                super::app_view::find_widget(app);
            }
        })
        .map_err(|_| CommandError::ValidationFailed {
            field: "find_widget_shortcut".into(),
            reason: format!("{combination} is taken by another app. Try another."),
        })?;
    ACTIVE.store(true, Ordering::SeqCst);
    Ok(())
}

/// Reads a combination such as `Ctrl+Alt+Q`.
fn parse(combination: &str) -> Result<Shortcut, CommandError> {
    Shortcut::from_str(combination).map_err(|_| CommandError::ValidationFailed {
        field: "find_widget_shortcut".into(),
        reason: format!("{combination} can't be used as a shortcut. Try another."),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_and_what_settings_records_can_be_read() {
        for combination in [
            quota_domain::preferences::DEFAULT_FIND_WIDGET_SHORTCUT,
            "Ctrl+Shift+F9",
            "Alt+Space",
            "Super+Digit1",
        ] {
            assert!(parse(combination).is_ok(), "{combination}");
        }
    }

    #[test]
    fn a_combination_with_no_key_or_a_made_up_key_is_refused() {
        for combination in ["Ctrl+Alt", "Ctrl+Nonsense", ""] {
            assert!(parse(combination).is_err(), "{combination}");
        }
    }
}
