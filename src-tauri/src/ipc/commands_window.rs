//! Native overview window commands.
//!
//! Mode, geometry, and topmost are separate controls. No handler here moves,
//! resizes, or hides the window as a side effect of an unrelated change.

#![expect(
    clippy::unreachable,
    clippy::let_underscore_must_use,
    reason = "`#[tauri::command]` expands to `let _check: ReturnType = unreachable!()`, which both lints report against the handler signature"
)]

use quota_contracts::CommandError;
use quota_contracts::commands::{FitWidget, SettingsDestination, WidgetGrowth, WindowModeChange};
use quota_contracts::events::OverviewWindowState as WindowStateResponse;
use quota_domain::preferences::{OverviewMode, WidgetPosition};
use quota_domain::provider::ProviderId;
use tauri::{LogicalSize, PhysicalPosition, State};

use crate::platform::tray_anchor::Edge;
use crate::platform::window::{self, OverviewWindowState as WindowModelState};
use crate::state::AppState;

/// Moves the overview between floating and tray anchoring.
#[tauri::command]
#[specta::specta]
pub async fn set_overview_mode(
    state: State<'_, AppState>,
    mode: OverviewMode,
) -> Result<WindowModeChange, CommandError> {
    let mut controller = state.window.lock().await;
    let native = window::get(&state.app, "overview")?;
    let result = window::transition_mode(&native, &mut controller, mode, async {
        if mode == OverviewMode::Tray {
            window::anchor_to_tray(&state.app)?;
        }
        let visible = native
            .is_visible()
            .map_err(|_| window::failed("read_window_visibility"))?;
        crate::ipc::commands_prefs::change_preferences(&state, |preferences| {
            preferences.overview_mode = mode;
        })
        .await?;
        Ok(visible)
    })
    .await;
    window::publish_state(&state.app, &state.app_instance_id, controller.state());
    Ok(WindowModeChange::Applied(result?.mode))
}

/// Changes the native topmost flag and saves the same confirmed preference.
///
/// It does not change geometry, mode, visibility, monitoring, or account order.
#[tauri::command]
#[specta::specta]
pub async fn set_overview_always_on_top(
    state: State<'_, AppState>,
    always_on_top: bool,
) -> Result<WindowStateResponse, CommandError> {
    let mut controller = state.window.lock().await;
    let native = window::get(&state.app, "overview")?;
    native
        .set_always_on_top(always_on_top)
        .map_err(|_| window::failed("set_always_on_top"))?;
    if native
        .is_always_on_top()
        .map_err(|_| window::failed("read_always_on_top"))?
        != always_on_top
    {
        return Err(window::failed("confirm_always_on_top"));
    }
    let confirmed = controller.set_always_on_top(always_on_top);
    crate::ipc::commands_prefs::change_preferences(&state, |preferences| {
        preferences.always_on_top = always_on_top;
    })
    .await?;
    window::publish_state(&state.app, &state.app_instance_id, confirmed);
    Ok(window_state_response(confirmed))
}

/// Fits the popover's height to its content, as the wireframe's popover does.
///
/// The renderer reports how tall its content is; the host decides the height
/// and position inside the monitor's work area and records the geometry change,
/// so the window is never moved outside the controller.
#[tauri::command]
#[specta::specta]
pub async fn fit_overview_height(
    state: State<'_, AppState>,
    content_height: u32,
) -> Result<WindowStateResponse, CommandError> {
    use crate::platform::popover_height::{self, Placement, WorkArea};
    let mut controller = state.window.lock().await;
    let native = window::get(&state.app, "overview")?;
    let monitor = native
        .current_monitor()
        .map_err(|_| window::failed("read_current_monitor"))?
        .or(native
            .primary_monitor()
            .map_err(|_| window::failed("read_primary_monitor"))?)
        .ok_or_else(|| window::failed("find_display"))?;
    let area = monitor.work_area();
    let scale = monitor.scale_factor();
    let outer = native
        .outer_size()
        .map_err(|_| window::failed("read_window_size"))?;
    let inner = native
        .inner_size()
        .map_err(|_| window::failed("read_window_size"))?;
    let position = native
        .outer_position()
        .map_err(|_| window::failed("read_window_position"))?;
    let fitted = popover_height::fit(
        f64::from(content_height),
        scale,
        Placement {
            outer_y: position.y,
            outer_height: outer.height,
            inner_height: inner.height,
        },
        WorkArea {
            top: area.position.y,
            height: area.size.height,
        },
        // A tray popover keeps the edge nearest the tray: its bottom above a
        // bottom taskbar, its top below a menu bar or beside a side taskbar.
        controller.state().mode == OverviewMode::Tray
            && window::tray_layout(&state.app, &native)?.edge == Edge::Bottom,
    );
    let width = f64::from(inner.width) / scale;
    native
        .set_size(LogicalSize::new(width, fitted.inner_height))
        .map_err(|_| window::failed("fit_window_height"))?;
    native
        .set_position(PhysicalPosition::new(position.x, fitted.outer_y))
        .map_err(|_| window::failed("fit_window_position"))?;
    let confirmed = controller.record_geometry_change();
    window::publish_state(&state.app, &state.app_instance_id, confirmed);
    Ok(window_state_response(confirmed))
}

/// Fits the mini widget window to its content, growing the drawer's way.
///
/// The renderer reports how tall its content is and which way the drawer
/// opens; the host applies the size inside the monitor's work area and, for
/// upward growth, moves the top edge so the tiles keep their screen position.
/// When neither side has room the larger side wins and the fitted height caps
/// the drawer, which then scrolls its rows. A position reached only because
/// the drawer opened upward is never saved; an upward close saves the
/// restored resting position instead.
#[tauri::command]
#[specta::specta]
pub async fn fit_widget(
    state: State<'_, AppState>,
    content_height: u32,
    direction: WidgetGrowth,
) -> Result<FitWidget, CommandError> {
    use crate::platform::widget::{self, Placement, WorkArea};
    let native = window::get(&state.app, widget::LABEL)?;
    let monitor = native
        .current_monitor()
        .map_err(|_| window::failed("read_current_monitor"))?
        .or(native
            .primary_monitor()
            .map_err(|_| window::failed("read_primary_monitor"))?)
        .ok_or_else(|| window::failed("find_display"))?;
    let area = monitor.work_area();
    // The window's own scale, not its screen's: the size is set in the
    // window's scale, and while it is dragged across screens of different
    // scales the two differ, which would shrink the widget's width.
    let scale = native
        .scale_factor()
        .unwrap_or_else(|_| monitor.scale_factor());
    let outer = native
        .outer_size()
        .map_err(|_| window::failed("read_window_size"))?;
    let inner = native
        .inner_size()
        .map_err(|_| window::failed("read_window_size"))?;
    let position = native
        .outer_position()
        .map_err(|_| window::failed("read_window_position"))?;
    let current = f64::from(inner.height) / scale;
    let fitted = widget::fit(
        f64::from(content_height),
        direction,
        Placement {
            outer_y: position.y,
            outer_height: outer.height,
            inner_height: inner.height,
        },
        WorkArea {
            top: area.position.y,
            height: area.size.height,
        },
        scale,
    );
    native
        .set_size(LogicalSize::new(widget::WIDTH, fitted.height))
        .map_err(|_| window::failed("fit_window_size"))?;
    if fitted.outer_y != position.y {
        let moved = PhysicalPosition::new(position.x, fitted.outer_y);
        native
            .set_position(moved)
            .map_err(|_| window::failed("fit_window_position"))?;
        widget::mark_programmatic(moved);
        if direction == WidgetGrowth::Up && f64::from(content_height) < current {
            widget::save_resting_position(
                &state,
                WidgetPosition {
                    x: position.x,
                    y: fitted.outer_y,
                },
            )
            .await;
        }
    }
    Ok(FitWidget {
        height: fitted.height,
        direction: fitted.direction,
        room_above: fitted.room_above,
        room_below: fitted.room_below,
    })
}

/// Switches between the full window and the mini widget, and saves the view.
///
/// The chosen view is shown before the other is put away, so the app never
/// disappears, and a view the system refused to show is never saved.
#[tauri::command]
#[specta::specta]
pub async fn set_app_view(
    state: State<'_, AppState>,
    view: quota_domain::preferences::AppView,
) -> Result<quota_contracts::preferences::Preferences, CommandError> {
    crate::platform::app_view::switch(&state, view).await
}

/// Opens one allowlisted provider usage page in the external browser.
///
/// The renderer cannot supply a URL. It names a provider, and the host owns the
/// address. An unknown provider is refused rather than guessed.
#[tauri::command]
#[specta::specta]
pub async fn open_provider_usage_page(
    state: State<'_, AppState>,
    provider_id: ProviderId,
) -> Result<(), CommandError> {
    let Some(url) = crate::bootstrap_helpers::usage_page_of(provider_id) else {
        return Err(CommandError::UnsupportedProvider { provider_id });
    };
    crate::bootstrap_helpers::open_external(&state.app, url)
}

fn window_state_response(state: WindowModelState) -> WindowStateResponse {
    WindowStateResponse::Confirmed {
        mode: state.mode,
        always_on_top: state.always_on_top,
        visible: state.visible,
        geometry_revision: state.geometry_revision,
    }
}

/// The script that moves the settings window to `section`, under a fresh
/// request so the same section opens anew, with the group a new key joins.
///
/// The group id is the renderer's, so it enters the script only as a JSON
/// string literal, URI-encoded by the page: it can never end the string.
fn route_script(section: &str, group: Option<&str>) -> Result<String, CommandError> {
    let request = uuid::Uuid::new_v4();
    let Some(group) = group else {
        return Ok(format!(
            "window.location.hash = '#/settings/{section}/{request}';"
        ));
    };
    let literal = serde_json::to_string(group).map_err(|_| window::failed("navigate_settings"))?;
    Ok(format!(
        "window.location.hash = '#/settings/{section}/{request}/' + encodeURIComponent({literal});"
    ))
}

/// Shows and focuses the settings window, placed over the overview or, when
/// the overview is hidden, beside the tray.
///
/// The window is created hidden at launch and stays hidden until this command
/// or the tray menu runs, so the settings surface never opens beside the
/// overview on its own. The renderer asks the host rather than creating a
/// webview itself, so window labels, permissions and geometry stay owned on
/// one side.
#[tauri::command]
#[specta::specta]
pub async fn open_settings_window(
    state: State<'_, AppState>,
    destination: SettingsDestination,
) -> Result<(), CommandError> {
    let native = window::get(&state.app, "settings")?;
    let script = match destination {
        SettingsDestination::General => "window.location.hash = '#/settings';".to_owned(),
        SettingsDestination::Accounts => route_script("accounts", None)?,
        SettingsDestination::Connect => route_script("connect", None)?,
        SettingsDestination::AddKey { group_id } => {
            route_script("connect", Some(group_id.as_str()))?
        }
    };
    native
        .eval(&script)
        .map_err(|_| window::failed("navigate_settings"))?;
    crate::platform::settings_window::show(&state.app)
}

#[cfg(test)]
mod tests {
    use super::route_script;

    #[test]
    fn a_group_id_enters_the_settings_route_only_as_a_string() {
        let plain = route_script("connect", None).unwrap();
        assert!(plain.starts_with("window.location.hash = '#/settings/connect/"));
        assert!(plain.ends_with("';"));
        let hostile = route_script("connect", Some("x\"); alert(1); //'")).unwrap();
        assert!(hostile.ends_with(r#"encodeURIComponent("x\"); alert(1); //'");"#));
    }
}
