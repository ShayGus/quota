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
use quota_contracts::commands::{SettingsDestination, WindowModeChange};
use quota_contracts::events::OverviewWindowState as WindowStateResponse;
use quota_domain::preferences::OverviewMode;
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

/// Sizes the overview to the popover layout and its accounts.
///
/// This is an explicit user action. No reading, label change, or added account
/// may resize or relocate the window on its own.
#[tauri::command]
#[specta::specta]
pub async fn fit_overview_to_accounts(
    state: State<'_, AppState>,
) -> Result<WindowStateResponse, CommandError> {
    let account_count = u32::try_from(state.registry.read().await.len())
        .map_err(|_| window::failed("read_account_count"))?;
    if account_count == 0 {
        return Err(CommandError::ValidationFailed {
            field: "accounts".into(),
            reason: "there are no accounts to fit".into(),
        });
    }
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
    let max_width = (f64::from(area.size.width) / scale).floor();
    let max_height = (f64::from(area.size.height) / scale).floor();
    // The popover is 440 logical pixels wide. Its height holds the header,
    // toolbar, and footer plus one provider card per account, up to the
    // wireframe's 760-pixel ceiling.
    let width = 440.0_f64.min(max_width).max(1.0);
    let height = (170.0 + f64::from(account_count) * 198.0)
        .min(760.0)
        .min(max_height)
        .max(1.0);
    let result = window::transition_mode(&native, &mut controller, OverviewMode::Floating, async {
        native
            .set_size(LogicalSize::new(width, height))
            .map_err(|_| window::failed("fit_window_size"))?;
        native
            .set_position(PhysicalPosition::new(
                f64::from(area.position.x) + (f64::from(area.size.width) - width * scale) / 2.0,
                f64::from(area.position.y) + (f64::from(area.size.height) - height * scale) / 2.0,
            ))
            .map_err(|_| window::failed("center_fitted_window"))?;
        native
            .eval("window.dispatchEvent(new Event('quota-fit-overview'));")
            .map_err(|_| window::failed("fit_overview_content"))?;
        let visible = window::set_visible(&state.app, "overview", true, true)?;
        crate::ipc::commands_prefs::change_preferences(&state, |preferences| {
            preferences.overview_mode = OverviewMode::Floating;
        })
        .await?;
        Ok(visible)
    })
    .await;
    window::publish_state(&state.app, &state.app_instance_id, controller.state());
    result.map(window_state_response)
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

/// Restores a position known to be inside a surviving monitor's work area.
#[tauri::command]
#[specta::specta]
pub async fn reset_overview_position(
    state: State<'_, AppState>,
) -> Result<WindowStateResponse, CommandError> {
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
    let size = native
        .outer_size()
        .map_err(|_| window::failed("read_window_size"))?;
    let result = window::transition_mode(&native, &mut controller, OverviewMode::Floating, async {
        native
            .set_position(PhysicalPosition::new(
                area.position.x + (area.size.width.saturating_sub(size.width) / 2).cast_signed(),
                area.position.y + (area.size.height.saturating_sub(size.height) / 2).cast_signed(),
            ))
            .map_err(|_| window::failed("reset_window_position"))?;
        let visible = window::set_visible(&state.app, "overview", true, true)?;
        crate::ipc::commands_prefs::change_preferences(&state, |preferences| {
            preferences.overview_mode = OverviewMode::Floating;
        })
        .await?;
        Ok(visible)
    })
    .await;
    window::publish_state(&state.app, &state.app_instance_id, controller.state());
    result.map(window_state_response)
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

/// Shows and focuses the settings window.
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
        SettingsDestination::Accounts | SettingsDestination::Connect => {
            let section = if destination == SettingsDestination::Accounts {
                "accounts"
            } else {
                "connect"
            };
            format!(
                "window.location.hash = '#/settings/{section}/{}';",
                uuid::Uuid::new_v4()
            )
        }
    };
    native
        .eval(&script)
        .map_err(|_| window::failed("navigate_settings"))?;
    crate::platform::settings_window::show(&state.app)
}
