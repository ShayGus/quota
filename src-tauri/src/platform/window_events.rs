//! What closing or leaving a window does.
//!
//! Persistent windows are kept alive: closing the overview or settings hides
//! it, so the tray can bring it back. Closing settings on its add-account page
//! also leaves that page, and a tray popover that loses focus hides, as a
//! popover does. The widget is never hidden by closing: while it is the view it
//! stays on screen, and wherever it is moved is saved.
//! The transient update window has its own close handler in `updates::host`.

use quota_domain::preferences::OverviewMode;
use tauri::{AppHandle, Manager, WebviewWindow};

use super::window::{publish_state, set_visible};

/// Moves the settings window from its add-account page to Accounts, so the
/// wizard unmounts and discards a verified account that was never confirmed.
const LEAVE_ADD_ACCOUNT: &str = "if (window.location.hash.startsWith('#/settings/connect')) { window.location.hash = '#/settings/accounts/closed'; }";

/// Keeps every configured window alive when the user closes it.
pub fn install_close_handlers(app: &AppHandle) {
    for label in ["overview", "settings", super::widget::LABEL] {
        let Some(native) = app.get_webview_window(label) else {
            continue;
        };
        let app = app.clone();
        let native_for_event = native.clone();
        native.on_window_event(move |event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                close(&app, &native_for_event, label);
            }
            tauri::WindowEvent::Focused(false) if label == "overview" => {
                overview_lost_focus(&app);
            }
            tauri::WindowEvent::Moved(position) if label == super::widget::LABEL => {
                super::widget::remember(&app, *position);
            }
            tauri::WindowEvent::ScaleFactorChanged { .. } if label == super::widget::LABEL => {
                super::widget_screens::check_soon(&app);
            }
            _ => {}
        });
    }
}

/// Hides a window instead of closing it. The widget ignores a close: only
/// choosing the full window puts it away.
fn close(app: &AppHandle, native: &WebviewWindow, label: &'static str) {
    if label == super::widget::LABEL {
        return;
    }
    if let Err(error) = native.hide() {
        tracing::warn!(%error, %label, "the window could not be hidden on close");
    }
    if label == "settings" {
        leave_add_account(native);
    } else {
        report_overview_hidden(app);
    }
}

/// Moves a hidden settings window off its add-account page.
fn leave_add_account(native: &WebviewWindow) {
    if let Err(error) = native.eval(LEAVE_ADD_ACCOUNT) {
        tracing::warn!(%error, "settings could not leave the add-account page");
    }
}

/// Publishes the overview as hidden.
fn report_overview_hidden(app: &AppHandle) {
    let Some(state) = app.try_state::<crate::state::AppState>() else {
        return;
    };
    let controller = state.window.clone();
    let app = app.clone();
    let app_instance_id = state.app_instance_id.clone();
    tauri::async_runtime::spawn(async move {
        let confirmed = controller.lock().await.set_visible(false);
        publish_state(&app, &app_instance_id, confirmed);
    });
}

/// Hides a tray popover that lost focus; a floating window stays.
fn overview_lost_focus(app: &AppHandle) {
    let Some(state) = app.try_state::<crate::state::AppState>() else {
        return;
    };
    let controller = state.window.clone();
    let app = app.clone();
    let app_instance_id = state.app_instance_id.clone();
    tauri::async_runtime::spawn(async move {
        if controller.lock().await.state().mode == OverviewMode::Tray
            && set_visible(&app, "overview", false, false) == Ok(false)
        {
            let confirmed = controller.lock().await.set_visible(false);
            publish_state(&app, &app_instance_id, confirmed);
        }
    });
}
