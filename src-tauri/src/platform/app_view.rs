//! Which view presents Quota: the full window or the mini widget.
//!
//! The two never show together. Switching shows the chosen view before it puts
//! the other away, saves the choice, and checks the tray item to match, so the
//! next launch, after a restart or an update, opens the same view. Opening the
//! app from the tray or by launching it again brings back the chosen view, not
//! the other one.

use quota_contracts::CommandError;
use quota_contracts::preferences::Preferences;
use quota_domain::preferences::{AppView, OverviewMode, WidgetPosition};
use tauri::{AppHandle, Manager};

use super::widget;
use super::window;
use crate::state::AppState;

/// Switches to `view` and saves it.
///
/// # Errors
/// Returns the native failure when a window refuses, before the choice is
/// saved, and the persistence failure when it could not be saved.
pub async fn switch(state: &AppState, view: AppView) -> Result<Preferences, CommandError> {
    let saved = state.preferences_state.read().await.widget_position;
    let mut controller = state.window.lock().await;
    match view {
        AppView::Widget => {
            widget::show(&state.app, saved)?;
            let visible = window::set_visible(&state.app, "overview", false, false)?;
            let confirmed = controller.set_visible(visible);
            window::publish_state(&state.app, &state.app_instance_id, confirmed);
        }
        AppView::Overview => {
            if controller.state().mode == OverviewMode::Tray
                && let Err(error) = window::anchor_to_tray(&state.app)
            {
                tracing::warn!(
                    code = error.diagnostic_code(),
                    "overview could not anchor to the tray"
                );
            }
            let visible = window::set_visible(&state.app, "overview", true, true)?;
            widget::hide(&state.app)?;
            let confirmed = controller.set_visible(visible);
            window::publish_state(&state.app, &state.app_instance_id, confirmed);
        }
    }
    drop(controller);
    widget::reflect(&state.app, view == AppView::Widget);
    crate::ipc::commands_prefs::change_preferences(state, |preferences| {
        preferences.view = view;
    })
    .await
}

/// Opens the saved view at launch, once the preferences are read.
///
/// # Errors
/// Returns the native failure when the window refuses.
pub async fn restore_saved(state: &AppState) -> Result<(), CommandError> {
    let (view, saved) = {
        let preferences = state.preferences_state.read().await;
        (preferences.view, preferences.widget_position)
    };
    restore(
        &state.app,
        view,
        saved,
        super::autostart::launched_at_login(std::env::args()),
    )
}

/// Opens a view at launch. The widget is shown even at login, because it is
/// never hidden; the full window waits in the tray at login, as before.
fn restore(
    app: &AppHandle,
    view: AppView,
    saved: Option<WidgetPosition>,
    at_login: bool,
) -> Result<(), CommandError> {
    widget::reflect(app, view == AppView::Widget);
    match view {
        AppView::Widget => widget::show(app, saved),
        AppView::Overview if at_login => Ok(()),
        AppView::Overview => {
            window::activate_overview(app).map_err(|_| window::failed("show_overview"))
        }
    }
}

/// Brings the chosen view forward: from the tray, or from a second launch.
pub fn activate(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Some(state) = app.try_state::<AppState>() else {
            // The backend is still starting, so no view is saved yet to honour.
            if let Err(error) = window::activate_overview(&app) {
                tracing::warn!(%error, "the overview could not be shown");
            }
            return;
        };
        let state = state.inner().clone();
        let (view, saved) = {
            let preferences = state.preferences_state.read().await;
            (preferences.view, preferences.widget_position)
        };
        if view == AppView::Widget {
            if let Err(error) = widget::show(&app, saved) {
                tracing::warn!(
                    code = error.diagnostic_code(),
                    "the widget could not be shown"
                );
            }
            return;
        }
        let mut controller = state.window.lock().await;
        if let Ok(confirmed) = window::set_visible(&app, "overview", true, true) {
            let confirmed = controller.set_visible(confirmed);
            window::publish_state(&app, &state.app_instance_id, confirmed);
        }
    });
}

/// Switches to the other view from the tray menu.
pub fn toggle_from_tray(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Some(state) = app.try_state::<AppState>() else {
            widget::reflect(&app, false);
            return;
        };
        let state = state.inner().clone();
        let current = state.preferences_state.read().await.view;
        let next = match current {
            AppView::Overview => AppView::Widget,
            AppView::Widget => AppView::Overview,
        };
        if let Err(error) = switch(&state, next).await {
            tracing::warn!(
                code = error.diagnostic_code(),
                "the view could not be switched from the tray"
            );
            widget::reflect(&app, current == AppView::Widget);
        }
    });
}
