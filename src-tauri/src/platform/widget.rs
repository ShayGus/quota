//! The mini widget window.
//!
//! The widget is a small, always-on-top window with every account at a glance.
//! Like settings it is created hidden, and it is shown only when the person
//! turns it on, from the tray or from settings. The choice is saved, so the
//! widget comes back at the next launch, and closing the widget saves it off.
//! The tray's check mark and the settings switch always say the same thing,
//! because both are set from the confirmed native state.

use quota_contracts::CommandError;
use quota_contracts::preferences::Preferences;
use tauri::menu::CheckMenuItem;
use tauri::{AppHandle, Manager, Wry};

use super::window;
use crate::state::AppState;

/// The widget window's label in `tauri.conf.json`.
pub const LABEL: &str = "widget";

/// The tray menu's widget item, kept so a change from settings checks it.
pub struct WidgetMenuItem(pub CheckMenuItem<Wry>);

/// Shows or hides the widget, saves the confirmed state, and checks the tray
/// item to match. The widget never takes focus from what the person is doing.
///
/// # Errors
/// Returns the native failure when the window refuses, before anything is
/// saved, and the persistence failure when the choice could not be saved.
pub async fn apply(state: &AppState, visible: bool) -> Result<Preferences, CommandError> {
    let confirmed = window::set_visible(&state.app, LABEL, visible, false)?;
    reflect(&state.app, confirmed);
    crate::ipc::commands_prefs::change_preferences(state, |preferences| {
        preferences.show_widget = confirmed;
    })
    .await
}

/// Shows the widget at launch when the saved preference has it on.
pub fn restore(app: &AppHandle, show: bool) {
    if !show {
        return;
    }
    match window::set_visible(app, LABEL, true, false) {
        Ok(visible) => reflect(app, visible),
        Err(error) => tracing::warn!(
            code = error.diagnostic_code(),
            "the widget could not be restored"
        ),
    }
}

/// Turns the widget off or on from the tray menu.
pub fn toggle_from_tray(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Some(state) = app.try_state::<AppState>() else {
            // The backend is still starting; the item keeps its old mark.
            reflect(&app, false);
            return;
        };
        let state = state.inner().clone();
        let showing = state.preferences_state.read().await.show_widget;
        if let Err(error) = apply(&state, !showing).await {
            tracing::warn!(
                code = error.diagnostic_code(),
                "the widget could not be changed from the tray"
            );
            reflect(&app, showing);
        }
    });
}

/// Saves the widget off after the person closed it.
pub fn closed(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Some(state) = app.try_state::<AppState>() else {
            return;
        };
        let state = state.inner().clone();
        if let Err(error) = apply(&state, false).await {
            tracing::warn!(
                code = error.diagnostic_code(),
                "the closed widget could not be saved off"
            );
        }
    });
}

/// Checks the tray item when the widget is on screen.
fn reflect(app: &AppHandle, visible: bool) {
    if let Some(item) = app.try_state::<WidgetMenuItem>()
        && let Err(error) = item.0.set_checked(visible)
    {
        tracing::warn!(%error, "the tray widget item could not be updated");
    }
}
