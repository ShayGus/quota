//! Which view presents Quota: the full window or the mini widget.
//!
//! The two never show together. Switching shows the chosen view before it puts
//! the other away, saves the choice, and checks the tray item to match, so the
//! next launch, after a restart or an update, opens the same view. Opening the
//! app from the tray or by launching it again brings back the chosen view, not
//! the other one. Only the chosen view's window exists (see `lifecycle`).

use quota_contracts::CommandError;
use quota_contracts::commands::OverviewTarget;
use quota_contracts::preferences::Preferences;
use quota_domain::preferences::{AppView, OverviewMode, WidgetPosition};
use tauri::{AppHandle, Manager};

use super::lifecycle;
use super::widget;
use super::window;
use crate::state::AppState;

/// Switches to `view` and saves it.
///
/// # Errors
/// Returns the native failure when a window refuses, before the choice is
/// saved, and the persistence failure when it could not be saved.
pub async fn switch(state: &AppState, view: AppView) -> Result<Preferences, CommandError> {
    switch_to(state, view, None).await
}

/// Switches to `view` and saves it; the full window opens at `target` when
/// one is given.
///
/// # Errors
/// As [`switch`].
pub async fn switch_to(
    state: &AppState,
    view: AppView,
    target: Option<&OverviewTarget>,
) -> Result<Preferences, CommandError> {
    let saved = state.preferences_state.read().await.widget_position;
    let mut controller = state.window.lock().await;
    match view {
        AppView::Widget => {
            widget::show(&state.app, saved)?;
            let visible = lifecycle::put_away_overview(&state.app)?;
            let confirmed = controller.set_visible(visible);
            window::publish_state(&state.app, &state.app_instance_id, confirmed);
        }
        AppView::Overview => {
            let route = target.map(route_of);
            let (native, created) =
                lifecycle::overview(&state.app, route.as_deref(), Some(controller.state()))?;
            if !created {
                if controller.state().mode == OverviewMode::Tray
                    && let Err(error) = window::anchor_to_tray(&state.app)
                {
                    tracing::warn!(
                        code = error.diagnostic_code(),
                        "overview could not anchor to the tray"
                    );
                }
                if let Some(target) = target {
                    navigate(&native, target);
                }
            }
            let visible = window::set_visible(&state.app, lifecycle::OVERVIEW, true, true)?;
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

/// The full window's address for `target`. Ids are percent-encoded, so they
/// stay inside their segments.
fn route_of(target: &OverviewTarget) -> String {
    match target {
        OverviewTarget::Overview => "#/".to_owned(),
        OverviewTarget::Detail {
            account_id,
            window_id,
        } => {
            let mut route = format!("#/detail/{}", encode_segment(account_id.as_str()));
            if let Some(window_id) = window_id {
                route.push('/');
                route.push_str(&encode_segment(window_id.as_str()));
            }
            route
        }
    }
}

/// Percent-encodes everything but unreserved characters, as the page's
/// `decodeURIComponent` reads it back.
pub(crate) fn encode_segment(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            for nibble in [byte >> 4, byte & 0x0F] {
                if let Some(digit) = char::from_digit(u32::from(nibble), 16) {
                    encoded.push(digit.to_ascii_uppercase());
                }
            }
        }
    }
    encoded
}

/// Asks a full window that is already open to show `target`.
fn navigate(native: &tauri::WebviewWindow, target: &OverviewTarget) {
    use tauri::Emitter as _;
    let payload = match target {
        OverviewTarget::Overview => serde_json::json!({ "view": "overview" }),
        OverviewTarget::Detail {
            account_id,
            window_id,
        } => serde_json::json!({
            "view": "detail",
            "accountId": account_id,
            "windowId": window_id,
        }),
    };
    if native
        .emit_to(lifecycle::OVERVIEW, NAVIGATE_EVENT, payload)
        .is_err()
    {
        tracing::warn!("the full window was not asked to show the surface");
    }
}

/// The event the full window listens for, in `shared/ipc/navigation.ts`.
const NAVIGATE_EVENT: &str = "quota-popover-navigate";

/// Opens the saved view at launch, once the preferences are read.
///
/// # Errors
/// Returns the native failure when the window refuses.
pub async fn restore_saved(state: &AppState) -> Result<(), CommandError> {
    let (view, saved) = {
        let preferences = state.preferences_state.read().await;
        (preferences.view, preferences.widget_position)
    };
    let chrome = state.window.lock().await.state();
    restore(
        &state.app,
        view,
        saved,
        chrome,
        super::autostart::launched_at_login(std::env::args()),
    )
}

/// Opens a view at launch. The widget is shown even at login, because it is
/// never hidden; the full window waits in the tray at login, as before.
fn restore(
    app: &AppHandle,
    view: AppView,
    saved: Option<WidgetPosition>,
    chrome: window::OverviewWindowState,
    at_login: bool,
) -> Result<(), CommandError> {
    widget::reflect(app, view == AppView::Widget);
    match view {
        AppView::Widget => widget::show(app, saved),
        AppView::Overview if at_login => Ok(()),
        AppView::Overview => {
            lifecycle::overview(app, None, Some(chrome))?;
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
        if let Err(error) = lifecycle::overview(&app, None, Some(controller.state())) {
            tracing::warn!(
                code = error.diagnostic_code(),
                "the overview could not be created"
            );
            return;
        }
        if let Ok(confirmed) = window::set_visible(&app, lifecycle::OVERVIEW, true, true) {
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

#[cfg(test)]
mod tests {
    use quota_contracts::commands::OverviewTarget;
    use quota_domain::ids::{AccountId, QuotaWindowId};

    use super::route_of;

    #[test]
    fn a_detail_address_keeps_each_id_inside_its_segment() {
        assert_eq!(route_of(&OverviewTarget::Overview), "#/");
        let target = OverviewTarget::Detail {
            account_id: AccountId::new("a/1 \"x\"").expect("an account id"),
            window_id: Some(QuotaWindowId::new("w#2").expect("a window id")),
        };
        assert_eq!(route_of(&target), "#/detail/a%2F1%20%22x%22/w%232");
    }
}
