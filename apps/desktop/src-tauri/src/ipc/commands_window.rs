//! Native overview window commands.
//!
//! Mode, geometry, and topmost are separate controls. No handler here moves,
//! resizes, or hides the window as a side effect of an unrelated change.

use quota_contracts::CommandError;
use quota_contracts::commands::WindowModeChange;
use quota_contracts::events::OverviewWindowState as WindowStateResponse;
use quota_domain::preferences::OverviewMode;
use quota_domain::provider::ProviderId;
use tauri::{LogicalSize, PhysicalPosition, State};

use crate::platform::tray::{TrayActivation, activation_for};
use crate::platform::window::{self, OverviewWindowState as WindowModelState};
use crate::state::AppState;

use tauri_plugin_positioner::{Position, WindowExt};

/// Moves the overview between floating and tray anchoring.
#[tauri::command]
#[specta::specta]
pub async fn set_overview_mode(
    state: State<'_, AppState>,
    mode: OverviewMode,
) -> Result<WindowModeChange, CommandError> {
    let native = window::get(&state.app, "overview")?;
    if mode == OverviewMode::Tray {
        native
            .move_window_constrained(Position::TrayBottomCenter)
            .map_err(|_| window::failed("anchor_tray_window"))?;
        native
            .set_decorations(false)
            .map_err(|_| window::failed("set_window_decorations"))?;
        native
            .set_skip_taskbar(true)
            .map_err(|_| window::failed("set_taskbar_visibility"))?;
    } else {
        native
            .set_decorations(true)
            .map_err(|_| window::failed("set_window_decorations"))?;
        native
            .set_skip_taskbar(false)
            .map_err(|_| window::failed("set_taskbar_visibility"))?;
    }
    let mut controller = state.window.lock().await;
    let confirmed = controller.set_mode(mode);
    drop(controller);
    crate::ipc::commands_prefs::change_preferences(&state, |preferences| {
        preferences.overview_mode = mode;
    })
    .await?;
    window::publish_state(&state.app, &state.app_instance_id, confirmed);
    Ok(WindowModeChange::Applied(confirmed.mode))
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
    let mut controller = state.window.lock().await;
    let confirmed = controller.set_always_on_top(always_on_top);
    drop(controller);
    crate::ipc::commands_prefs::change_preferences(&state, |preferences| {
        preferences.always_on_top = always_on_top;
    })
    .await?;
    window::publish_state(&state.app, &state.app_instance_id, confirmed);
    Ok(window_state_response(confirmed))
}

/// Widens the overview to the account comparison layout.
///
/// This is an explicit user action. No reading, label change, or added account
/// may resize or relocate the window on its own.
#[tauri::command]
#[specta::specta]
pub async fn fit_overview_to_accounts(
    state: State<'_, AppState>,
) -> Result<WindowStateResponse, CommandError> {
    let account_count = state.registry.read().await.len();
    if account_count == 0 {
        return Err(CommandError::ValidationFailed {
            field: "accounts".into(),
            reason: "there are no accounts to fit".into(),
        });
    }
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
    let max_width = (area.size.width as f64 / scale).floor();
    let max_height = (area.size.height as f64 / scale).floor();
    let width = 810.0_f64.min(max_width).max(1.0);
    let height = (280.0 + account_count as f64 * 68.0)
        .min(max_height)
        .max(1.0);
    native
        .set_size(LogicalSize::new(width, height))
        .map_err(|_| window::failed("fit_window_size"))?;
    native
        .set_position(PhysicalPosition::new(
            area.position.x + ((area.size.width as f64 - width * scale) / 2.0) as i32,
            area.position.y + ((area.size.height as f64 - height * scale) / 2.0) as i32,
        ))
        .map_err(|_| window::failed("center_fitted_window"))?;
    let visible = window::set_visible(&state.app, "overview", true, true)?;
    let mut controller = state.window.lock().await;
    controller.set_visible(visible);
    let confirmed = controller.record_geometry_change();
    window::publish_state(&state.app, &state.app_instance_id, confirmed);
    Ok(window_state_response(confirmed))
}
/// Restores a position known to be inside a surviving monitor's work area.
#[tauri::command]
#[specta::specta]
pub async fn reset_overview_position(
    state: State<'_, AppState>,
) -> Result<WindowStateResponse, CommandError> {
    let native = window::get(&state.app, "overview")?;
    let monitor = native
        .current_monitor()
        .map_err(|_| window::failed("read_current_monitor"))?
        .or(native
            .primary_monitor()
            .map_err(|_| window::failed("read_primary_monitor"))?)
        .ok_or_else(|| window::failed("find_display"))?;
    let area = monitor.work_area();
    let size = native
        .outer_size()
        .map_err(|_| window::failed("read_window_size"))?;
    native
        .set_position(PhysicalPosition::new(
            area.position.x + (area.size.width.saturating_sub(size.width) / 2) as i32,
            area.position.y + (area.size.height.saturating_sub(size.height) / 2) as i32,
        ))
        .map_err(|_| window::failed("reset_window_position"))?;
    native
        .set_decorations(true)
        .map_err(|_| window::failed("set_window_decorations"))?;
    native
        .set_skip_taskbar(false)
        .map_err(|_| window::failed("set_taskbar_visibility"))?;
    let visible = window::set_visible(&state.app, "overview", true, true)?;
    let mut controller = state.window.lock().await;
    controller.set_visible(visible);
    controller.detach_to_floating();
    let confirmed = controller.record_geometry_change();
    window::publish_state(&state.app, &state.app_instance_id, confirmed);
    Ok(window_state_response(confirmed))
}

/// Raises or shows the overview in response to a tray activation.
#[tauri::command]
#[specta::specta]
pub async fn activate_overview(
    state: State<'_, AppState>,
    repeated_click: bool,
) -> Result<WindowStateResponse, CommandError> {
    let mut controller = state.window.lock().await;
    let native = window::get(&state.app, "overview")?;
    let visible = native
        .is_visible()
        .map_err(|_| window::failed("read_window_visibility"))?;
    let activation = activation_for(controller.state().mode, visible, repeated_click);
    let show = matches!(activation, TrayActivation::Show | TrayActivation::Raise);
    let confirmed = window::set_visible(&state.app, "overview", show, show)?;
    let confirmed = controller.set_visible(confirmed);
    window::publish_state(&state.app, &state.app_instance_id, confirmed);
    Ok(window_state_response(confirmed))
}

/// Opens one allowlisted provider usage page in the external browser.
///
/// The renderer cannot supply a URL. It names a provider, and the host owns the
/// address. An unknown provider is refused rather than guessed.
#[tauri::command]
#[specta::specta]
pub fn open_provider_usage_page(
    app: tauri::AppHandle,
    provider_id: ProviderId,
) -> Result<(), CommandError> {
    let Some(url) = crate::bootstrap_helpers::usage_page_of(provider_id) else {
        return Err(CommandError::UnsupportedProvider { provider_id });
    };
    crate::bootstrap_helpers::open_external(&app, url)
}

fn window_state_response(state: WindowModelState) -> WindowStateResponse {
    WindowStateResponse::Confirmed {
        mode: state.mode,
        always_on_top: state.always_on_top,
        visible: state.visible,
        geometry_revision: state.geometry_revision,
    }
}
