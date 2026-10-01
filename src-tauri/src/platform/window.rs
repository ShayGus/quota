//! The mode-aware overview window controller.
//!
//! Floating and tray anchoring are separate from geometry and from topmost.
//! Dragging a tray-anchored view detaches it to floating without enabling
//! topmost, and changing topmost moves nothing.

use quota_contracts::CommandError;
use tauri::{AppHandle, Manager, WebviewWindow};

use quota_domain::preferences::OverviewMode;

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

/// Keeps both configured windows alive when the user closes them.
pub fn install_close_handlers(app: &AppHandle) {
    for label in ["overview", "settings"] {
        let Some(native) = app.get_webview_window(label) else {
            continue;
        };
        let app = app.clone();
        let native_for_event = native.clone();
        native.on_window_event(move |event| match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let _ = native_for_event.hide();
                if label == "overview"
                    && let Some(state) = app.try_state::<crate::state::AppState>()
                {
                    let controller = state.window.clone();
                    let app = app.clone();
                    let app_instance_id = state.app_instance_id.clone();
                    tauri::async_runtime::spawn(async move {
                        let confirmed = controller.lock().await.set_visible(false);
                        publish_state(&app, &app_instance_id, confirmed);
                    });
                }
            }
            tauri::WindowEvent::Focused(false) if label == "overview" => {
                if let Some(state) = app.try_state::<crate::state::AppState>() {
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
            }
            _ => {}
        });
    }
}

/// Publishes confirmed native state to both renderer windows.
pub fn publish_state(
    app: &AppHandle,
    app_instance_id: &quota_domain::ids::AppInstanceId,
    state: OverviewWindowState,
) {
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

    /// Detaches a dragged tray view to floating, leaving topmost untouched.
    pub fn detach_to_floating(&mut self) -> OverviewWindowState {
        self.set_mode(OverviewMode::Floating)
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
    fn dragging_a_tray_view_does_not_enable_topmost() {
        let mut controller = OverviewWindowController::new();
        controller.set_mode(OverviewMode::Tray);
        controller.detach_to_floating();
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
