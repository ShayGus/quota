//! Which windows exist.
//!
//! Every window but the transient update pop-up is configured in
//! `tauri.conf.json` with `create: false` and created when it is first
//! needed, so a launch builds only the window of the saved view, and a view
//! that is put away gives its webview back. The full window, the widget and
//! settings each take a webview renderer of their own, so this keeps one
//! renderer running where Quota once kept three.
//!
//! The full window stays alive while it is the view, hidden between uses, so
//! the tray popover opens at once. Settings is owned by it when both are open,
//! so while settings is open the full window is hidden rather than destroyed;
//! it goes once settings closes (see `settings_window`).

use quota_contracts::CommandError;
use quota_domain::preferences::OverviewMode;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow};
use tauri_plugin_window_state::AppHandleExt;

use super::window::{self, OverviewWindowState, failed};

/// The full window's label in `tauri.conf.json`.
pub const OVERVIEW: &str = "overview";

/// Creates the window `label` from its configuration, at `route` when given,
/// owned by `parent` when given, with its window events installed.
///
/// # Errors
/// Returns a native failure when the configuration is missing or the system
/// refuses the window.
pub fn create(
    app: &AppHandle,
    label: &'static str,
    route: Option<&str>,
    parent: Option<&WebviewWindow>,
) -> Result<WebviewWindow, CommandError> {
    let mut config = app
        .config()
        .app
        .windows
        .iter()
        .find(|config| config.label == label)
        .cloned()
        .ok_or_else(|| failed("find_window_configuration"))?;
    if let Some(route) = route {
        config.url = WebviewUrl::App(format!("index.html{route}").into());
    }
    let mut builder = tauri::WebviewWindowBuilder::from_config(app, &config)
        .map_err(|_| failed("create_window"))?;
    if let Some(parent) = parent {
        builder = builder
            .parent(parent)
            .map_err(|_| failed("create_window"))?;
    }
    let native = builder.build().map_err(|_| failed("create_window"))?;
    super::window_events::install(app, &native, label);
    Ok(native)
}

/// The full window, created at `route` when it does not exist yet; `true`
/// when it was just created.
///
/// A new full window takes the confirmed chrome in `chrome`: its topmost flag,
/// its taskbar entry for the mode, and, in the tray mode, its place by the
/// tray. Without a confirmed state, as when the backend failed to start, it
/// keeps its configured chrome.
///
/// # Errors
/// Returns a native failure when the window cannot be created.
pub fn overview(
    app: &AppHandle,
    route: Option<&str>,
    chrome: Option<OverviewWindowState>,
) -> Result<(WebviewWindow, bool), CommandError> {
    if let Some(native) = app.get_webview_window(OVERVIEW) {
        return Ok((native, false));
    }
    let native = create(app, OVERVIEW, route, None)?;
    if let Some(chrome) = chrome {
        native
            .set_always_on_top(chrome.always_on_top)
            .and_then(|()| native.set_decorations(false))
            .and_then(|()| native.set_skip_taskbar(chrome.mode == OverviewMode::Tray))
            .map_err(|_| failed("restore_window_chrome"))?;
        if chrome.mode == OverviewMode::Tray
            && let Err(error) = window::anchor_to_tray(app)
        {
            tracing::warn!(
                code = error.diagnostic_code(),
                "overview could not anchor to the tray"
            );
        }
    }
    Ok((native, true))
}

/// Puts the full window away when the widget becomes the view, and answers
/// whether it is still visible.
///
/// Its place is saved first, because a destroyed window never reaches the
/// save the window-state plugin makes at exit. While settings is open the
/// window owns it, so it is only hidden until settings closes.
///
/// # Errors
/// Returns a native failure when the window refuses.
pub fn put_away_overview(app: &AppHandle) -> Result<bool, CommandError> {
    let Some(native) = app.get_webview_window(OVERVIEW) else {
        return Ok(false);
    };
    if app
        .get_webview_window(super::settings_window::LABEL)
        .is_some()
    {
        return window::set_visible(app, OVERVIEW, false, false);
    }
    if let Err(error) = app.save_window_state(window::restored_state_flags()) {
        tracing::warn!(%error, "the full window's place could not be saved");
    }
    native.destroy().map_err(|_| failed("close_window"))?;
    Ok(false)
}

/// Destroys the full window once settings has closed, when the widget is the
/// view and the full window was only kept hidden for settings.
pub fn release_hidden_overview(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Some(state) = app.try_state::<crate::state::AppState>() else {
            return;
        };
        let view = state.preferences_state.read().await.view;
        if view == quota_domain::preferences::AppView::Widget
            && let Some(native) = app.get_webview_window(OVERVIEW)
            && !native.is_visible().unwrap_or(true)
            && let Err(error) = put_away_overview(&app)
        {
            tracing::warn!(
                code = error.diagnostic_code(),
                "the hidden full window could not be closed"
            );
        }
    });
}
