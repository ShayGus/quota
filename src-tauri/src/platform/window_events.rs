//! What closing or leaving a window does.
//!
//! The full window is kept alive while it is the view: closing it hides it,
//! so the tray can bring it back, and a tray popover that loses focus hides, as
//! a popover does. The widget is never hidden by closing: while it is the view
//! it stays on screen, and wherever it is moved is saved. Settings really
//! closes, so its webview is not kept running while it is unused; the
//! connections its wizard left open are cancelled then, and a full window kept
//! hidden only for it goes too (see `lifecycle`).
//! The transient update window has its own close handler in `updates::host`.

use quota_domain::preferences::OverviewMode;
use tauri::{AppHandle, Manager, WebviewWindow};

use super::window::{publish_state, set_visible};

/// Installs what closing, leaving and moving do for a window just created.
pub fn install(app: &AppHandle, native: &WebviewWindow, label: &'static str) {
    let app = app.clone();
    let native_for_event = native.clone();
    let settings = label == super::settings_window::LABEL;
    native.on_window_event(move |event| match event {
        tauri::WindowEvent::CloseRequested { api, .. } if !settings => {
            api.prevent_close();
            close(&app, &native_for_event, label);
        }
        tauri::WindowEvent::Destroyed if settings => {
            cancel_left_connections(&app);
            super::lifecycle::release_hidden_overview(&app);
        }
        tauri::WindowEvent::Focused(false) if label == super::lifecycle::OVERVIEW => {
            overview_lost_focus(&app);
        }
        tauri::WindowEvent::Moved(position) if label == super::widget::LABEL => {
            super::widget::remember(&app, *position);
        }
        _ => {}
    });
}

/// Discards the connection attempts a closed settings window left behind,
/// because the page's own cleanup does not run in a destroyed webview.
fn cancel_left_connections(app: &AppHandle) {
    let Some(state) = app.try_state::<crate::state::AppState>() else {
        return;
    };
    let monitor = state.monitor.clone();
    tauri::async_runtime::spawn(async move {
        monitor.cancel_all_connections().await;
    });
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
    report_overview_hidden(app);
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
