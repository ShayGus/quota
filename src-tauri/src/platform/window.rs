//! The mode-aware overview window controller.
//!
//! Floating and tray anchoring are separate from geometry and from topmost.
//! Dragging a tray-anchored view detaches it to floating without enabling
//! topmost, and changing topmost moves nothing.
//!
//! Only the overview may open at launch. The settings window is created hidden
//! and stays hidden until a person opens it, so the saved-state plugin must not
//! own visibility: see `restored_state_flags`.

use quota_contracts::CommandError;
use tauri::{AppHandle, Manager, WebviewWindow};

use super::tray_anchor::{self, Edge};
use tauri_plugin_window_state::StateFlags;

pub(crate) use super::window_transition::{apply_mode_chrome, transition_mode};
use quota_domain::preferences::OverviewMode;

/// The window state the saved-state plugin may restore.
///
/// `StateFlags::VISIBLE` is deliberately absent. The plugin shows every window
/// it holds no saved state for, and re-shows any window that was visible when
/// the app last exited, so keeping the flag can open the settings window beside
/// the overview at launch. Visibility belongs to the host: the setup hook
/// shows the overview, and the overview control or the tray menu shows the
/// settings window.
///
/// `StateFlags::DECORATIONS` is absent for the same reason. The saved overview
/// mode owns the window chrome.
#[must_use]
pub fn restored_state_flags() -> StateFlags {
    StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED
}

/// Shows and focuses the overview, the only window a launch opens.
///
/// # Errors
///
/// Returns the native error when the overview window is missing or the
/// operating system refuses the request.
pub fn activate_overview(app: &AppHandle) -> tauri::Result<()> {
    let overview = app
        .get_webview_window("overview")
        .ok_or(tauri::Error::WindowNotFound)?;
    overview.show()?;
    overview.set_focus()?;
    Ok(())
}

/// Returns a host window and maps native details to a safe command error.
pub fn get(app: &AppHandle, label: &'static str) -> Result<WebviewWindow, CommandError> {
    app.get_webview_window(label)
        .ok_or_else(|| CommandError::NativeOperationFailed {
            operation: "get_window".into(),
            reason: "the native window is not available".into(),
        })
}

/// Maps a native error without exposing paths or platform details.
#[must_use]
pub fn failed(operation: &'static str) -> CommandError {
    CommandError::NativeOperationFailed {
        operation: operation.into(),
        reason: "the operating system rejected the window request".into(),
    }
}

/// Applies and confirms visibility through the native window API.
pub fn set_visible(
    app: &AppHandle,
    label: &'static str,
    visible: bool,
    focus: bool,
) -> Result<bool, CommandError> {
    let window = get(app, label)?;
    if visible {
        window.show().map_err(|_| failed("show_window"))?;
        if focus {
            window.set_focus().map_err(|_| failed("focus_window"))?;
        }
    } else {
        window.hide().map_err(|_| failed("hide_window"))?;
    }
    window
        .is_visible()
        .map_err(|_| failed("read_window_visibility"))
}

/// Anchors the window against the tray, on whichever screen edge the tray is.
///
/// See [`tray_anchor`] for how the edge and the
/// position are chosen; nothing here depends on the operating system.
pub fn anchor_to_tray(app: &AppHandle) -> Result<(), CommandError> {
    /// The gap the wireframe leaves between the popover and the screen edge.
    const MARGIN: f64 = 12.0;
    let native = get(app, "overview")?;
    let layout = tray_layout(app, &native)?;
    let size = native
        .outer_size()
        .map_err(|_| failed("read_window_size"))?;
    let (x, y) = tray_anchor::place(
        layout.work_area,
        layout.edge,
        layout.icon,
        (f64::from(size.width), f64::from(size.height)),
        MARGIN * layout.scale,
    );
    native
        .set_position(tauri::PhysicalPosition::new(x, y))
        .map_err(|_| failed("anchor_tray_window"))
}

/// The tray's screen edge and the work area beside it, in physical pixels.
pub struct TrayLayout {
    /// The edge the tray is on.
    pub edge: Edge,
    /// The work area of the monitor the tray is on.
    pub work_area: tray_anchor::Rect,
    /// The tray icon, when the system reports where it is.
    pub icon: Option<tray_anchor::Rect>,
    /// That monitor's scale factor.
    pub scale: f64,
}

/// Reads where the tray is: the icon's rectangle when the system reports one
/// (Windows and macOS do, Linux does not), and the monitor it is on, else the
/// overview's own monitor.
pub fn tray_layout(
    app: &AppHandle,
    native: &tauri::WebviewWindow,
) -> Result<TrayLayout, CommandError> {
    let icon = tray_icon_rect(app);
    let beside_icon = icon.and_then(|icon| {
        let (x, y) = (icon.x + icon.width / 2.0, icon.y + icon.height / 2.0);
        app.monitor_from_point(x, y).ok().flatten()
    });
    let monitor = match beside_icon {
        Some(monitor) => monitor,
        None => native
            .current_monitor()
            .map_err(|_| failed("read_current_monitor"))?
            .or(native
                .primary_monitor()
                .map_err(|_| failed("read_primary_monitor"))?)
            .ok_or_else(|| failed("find_display"))?,
    };
    let bounds = physical_rect(*monitor.position(), *monitor.size());
    let area = monitor.work_area();
    let work_area = physical_rect(area.position, area.size);
    let icon = tray_anchor::reserved_icon(bounds, work_area, icon);
    Ok(TrayLayout {
        edge: tray_anchor::tray_edge(bounds, work_area, icon),
        work_area,
        icon,
        scale: monitor.scale_factor(),
    })
}

/// The tray icon's rectangle in physical pixels, when the system reports it.
fn tray_icon_rect(app: &AppHandle) -> Option<tray_anchor::Rect> {
    let rect = app.tray_by_id("quota")?.rect().ok().flatten()?;
    let scale = app
        .primary_monitor()
        .ok()
        .flatten()
        .map_or(1.0, |monitor| monitor.scale_factor());
    let position = rect.position.to_physical::<f64>(scale);
    let size = rect.size.to_physical::<f64>(scale);
    Some(tray_anchor::Rect {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    })
}

/// A monitor rectangle as the placement module takes it.
fn physical_rect(
    position: tauri::PhysicalPosition<i32>,
    size: tauri::PhysicalSize<u32>,
) -> tray_anchor::Rect {
    tray_anchor::Rect {
        x: f64::from(position.x),
        y: f64::from(position.y),
        width: f64::from(size.width),
        height: f64::from(size.height),
    }
}

/// Publishes confirmed native state to both renderer windows.
///
/// Visibility is read from the window itself rather than the controller: the
/// overview is also shown by paths the controller does not see (a second
/// launch, the renderer's own navigation).
pub fn publish_state(
    app: &AppHandle,
    app_instance_id: &quota_domain::ids::AppInstanceId,
    mut state: OverviewWindowState,
) {
    if let Some(visible) = app
        .get_webview_window("overview")
        .and_then(|overview| overview.is_visible().ok())
    {
        state.visible = visible;
    }
    let event = crate::ipc::events::OverviewWindowStateChanged(
        quota_contracts::OverviewWindowStateChangedPayload {
            app_instance_id: app_instance_id.clone(),
            state: quota_contracts::events::OverviewWindowState::Confirmed {
                mode: state.mode,
                always_on_top: state.always_on_top,
                visible: state.visible,
                geometry_revision: state.geometry_revision,
            },
        },
    );
    emit_window_state(app, &event, "overview", "overview_window_event_failed");
    emit_window_state(app, &event, "settings", "settings_window_event_failed");
}

fn emit_window_state(
    app: &AppHandle,
    event: &crate::ipc::events::OverviewWindowStateChanged,
    label: &str,
    code: &str,
) {
    use tauri_specta::Event;

    if event.emit_to(app, label).is_err() {
        tracing::warn!(code = code, "window state event was not delivered");
    }
}

/// The confirmed native state of the overview window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OverviewWindowState {
    /// Where the window lives.
    pub mode: OverviewMode,
    /// Whether it floats above other applications.
    pub always_on_top: bool,
    /// Whether it is currently shown.
    pub visible: bool,
    /// Incremented on every confirmed change, so a renderer can order updates.
    pub geometry_revision: u32,
}

impl Default for OverviewWindowState {
    fn default() -> Self {
        Self {
            mode: OverviewMode::Floating,
            always_on_top: false,
            visible: false,
            geometry_revision: 0,
        }
    }
}

/// Owns mode, visibility, topmost, and floating geometry as separate concerns.
#[derive(Debug, Default)]
pub struct OverviewWindowController {
    state: OverviewWindowState,
}

impl OverviewWindowController {
    /// Creates a controller in the floating, not-topmost, hidden state.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The last confirmed native state.
    #[must_use]
    pub const fn state(&self) -> OverviewWindowState {
        self.state
    }

    /// Records a confirmed mode change.
    pub fn set_mode(&mut self, mode: OverviewMode) -> OverviewWindowState {
        if self.state.mode != mode {
            self.state.mode = mode;
            self.bump();
        }
        self.state
    }

    /// Records a confirmed topmost change.
    ///
    /// This deliberately touches nothing else: no move, no resize, no detach,
    /// no hide, no reorder.
    pub fn set_always_on_top(&mut self, always_on_top: bool) -> OverviewWindowState {
        if self.state.always_on_top != always_on_top {
            self.state.always_on_top = always_on_top;
            self.bump();
        }
        self.state
    }

    /// Records that the window was shown.
    pub fn set_visible(&mut self, visible: bool) -> OverviewWindowState {
        if self.state.visible != visible {
            self.state.visible = visible;
            self.bump();
        }
        self.state
    }

    /// Records a confirmed size or position change.
    pub fn record_geometry_change(&mut self) -> OverviewWindowState {
        self.bump();
        self.state
    }

    fn bump(&mut self) {
        self.state.geometry_revision = self.state.geometry_revision.saturating_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_saved_state_plugin_never_owns_window_visibility() {
        // The plugin defaults to every flag, visibility included, and it shows
        // any window it holds no saved state for. That default is what opened
        // the settings window beside the overview on every launch.
        assert!(StateFlags::default().contains(StateFlags::VISIBLE));
        let flags = restored_state_flags();
        assert!(!flags.contains(StateFlags::VISIBLE));
        assert!(!flags.contains(StateFlags::DECORATIONS));
        assert!(flags.contains(StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED));
    }

    #[test]
    fn topmost_starts_off_and_is_independent_of_everything_else() {
        let mut controller = OverviewWindowController::new();
        assert!(!controller.state().always_on_top);
        let before = controller.state();
        controller.set_always_on_top(true);
        let after = controller.state();
        assert!(after.always_on_top);
        assert_eq!(after.mode, before.mode);
        assert_eq!(after.visible, before.visible);
        assert!(after.geometry_revision > before.geometry_revision);
    }

    #[test]
    fn changing_a_tray_view_to_floating_does_not_enable_topmost() {
        let mut controller = OverviewWindowController::new();
        controller.set_mode(OverviewMode::Tray);
        controller.set_mode(OverviewMode::Floating);
        assert_eq!(controller.state().mode, OverviewMode::Floating);
        assert!(!controller.state().always_on_top);
    }

    #[test]
    fn setting_the_same_value_does_not_bump_the_revision() {
        let mut controller = OverviewWindowController::new();
        let first = controller.set_always_on_top(false).geometry_revision;
        assert_eq!(controller.set_always_on_top(false).geometry_revision, first);
    }

    #[test]
    fn confirmed_geometry_change_increments_the_revision() {
        let mut controller = OverviewWindowController::new();
        let before = controller.state();
        let after = controller.record_geometry_change();
        assert_eq!(after.geometry_revision, before.geometry_revision + 1);
        assert_eq!(after.mode, before.mode);
        assert_eq!(after.visible, before.visible);
        assert_eq!(after.always_on_top, before.always_on_top);
    }
}
