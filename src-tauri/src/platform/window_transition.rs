use std::future::Future;

use quota_contracts::CommandError;
use quota_domain::preferences::OverviewMode;
use tauri::WebviewWindow;

use super::window::{OverviewWindowController, OverviewWindowState, failed};

pub(crate) trait ModeChrome {
    fn set_decorations(&self, decorated: bool) -> Result<(), CommandError>;
    fn set_skip_taskbar(&self, skip: bool) -> Result<(), CommandError>;
    fn is_visible(&self) -> Result<bool, CommandError>;
}

impl ModeChrome for WebviewWindow {
    fn set_decorations(&self, decorated: bool) -> Result<(), CommandError> {
        WebviewWindow::set_decorations(self, decorated)
            .map_err(|_| failed("set_window_decorations"))
    }

    fn set_skip_taskbar(&self, skip: bool) -> Result<(), CommandError> {
        WebviewWindow::set_skip_taskbar(self, skip).map_err(|_| failed("set_taskbar_visibility"))
    }

    fn is_visible(&self) -> Result<bool, CommandError> {
        WebviewWindow::is_visible(self).map_err(|_| failed("read_window_visibility"))
    }
}

pub(crate) fn apply_mode_chrome(
    native: &impl ModeChrome,
    mode: OverviewMode,
    controller: &mut OverviewWindowController,
) -> Result<(), CommandError> {
    native.set_decorations(mode == OverviewMode::Floating)?;
    controller.set_mode(mode);
    native.set_skip_taskbar(mode == OverviewMode::Tray)
}

pub(crate) async fn transition_mode(
    native: &impl ModeChrome,
    controller: &mut OverviewWindowController,
    mode: OverviewMode,
    operation: impl Future<Output = Result<bool, CommandError>>,
) -> Result<OverviewWindowState, CommandError> {
    let previous = controller.state().mode;
    let result = match apply_mode_chrome(native, mode, controller) {
        Ok(()) => operation.await,
        Err(error) => Err(error),
    };
    match result {
        Ok(visible) => {
            controller.set_visible(visible);
            Ok(controller.record_geometry_change())
        }
        Err(error) => {
            let rollback = apply_mode_chrome(native, previous, controller);
            if let Ok(visible) = native.is_visible() {
                controller.set_visible(visible);
            }
            controller.record_geometry_change();
            rollback.and(Err(error))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use super::*;

    struct NativeState {
        decorations: bool,
        skip_taskbar: bool,
        visible: bool,
        calls: Vec<&'static str>,
        failures: Vec<usize>,
    }

    struct Native(RefCell<NativeState>);

    impl Native {
        fn new(mode: OverviewMode, failures: Vec<usize>) -> Self {
            Self(RefCell::new(NativeState {
                decorations: mode == OverviewMode::Floating,
                skip_taskbar: mode == OverviewMode::Tray,
                visible: false,
                calls: Vec::new(),
                failures,
            }))
        }

        fn step(&self, operation: &'static str) -> Result<(), CommandError> {
            let mut native = self.0.borrow_mut();
            let index = native.calls.len();
            native.calls.push(operation);
            if native.failures.contains(&index) {
                Err(failed(operation))
            } else {
                Ok(())
            }
        }

        fn fit(&self) -> Result<bool, CommandError> {
            self.step("size")?;
            self.step("position")?;
            self.step("content")?;
            self.step("show")?;
            self.0.borrow_mut().visible = true;
            self.step("focus")?;
            let visible = self.is_visible()?;
            self.step("persist")?;
            Ok(visible)
        }
    }

    impl ModeChrome for Native {
        fn set_decorations(&self, decorated: bool) -> Result<(), CommandError> {
            self.step("decorations")?;
            self.0.borrow_mut().decorations = decorated;
            Ok(())
        }

        fn set_skip_taskbar(&self, skip: bool) -> Result<(), CommandError> {
            self.step("taskbar")?;
            self.0.borrow_mut().skip_taskbar = skip;
            Ok(())
        }

        fn is_visible(&self) -> Result<bool, CommandError> {
            self.step("visibility")?;
            Ok(self.0.borrow().visible)
        }
    }

    #[tokio::test]
    async fn every_failed_stage_restores_the_previous_mode() {
        for previous in [OverviewMode::Tray, OverviewMode::Floating] {
            for mode in [OverviewMode::Tray, OverviewMode::Floating] {
                for failure in 0..9 {
                    let native = Native::new(previous, vec![failure]);
                    let mut controller = OverviewWindowController::new();
                    controller.set_mode(previous);
                    let result =
                        transition_mode(&native, &mut controller, mode, async { native.fit() })
                            .await;
                    assert!(result.is_err(), "failure at {failure}");
                    let actual = native.0.borrow();
                    assert_eq!(actual.decorations, previous == OverviewMode::Floating);
                    assert_eq!(actual.skip_taskbar, previous == OverviewMode::Tray);
                    assert_eq!(controller.state().mode, previous);
                    assert_eq!(controller.state().visible, actual.visible);
                }
            }
        }
    }

    #[tokio::test]
    async fn rollback_failures_leave_the_controller_recording_actual_chrome() {
        for mode in [OverviewMode::Floating, OverviewMode::Tray] {
            let previous = if mode == OverviewMode::Tray {
                OverviewMode::Floating
            } else {
                OverviewMode::Tray
            };
            for rollback_failure in [3, 4] {
                let native = Native::new(previous, vec![2, rollback_failure]);
                let mut controller = OverviewWindowController::new();
                controller.set_mode(previous);
                let result =
                    transition_mode(&native, &mut controller, mode, async { native.fit() }).await;
                assert_eq!(
                    result,
                    Err(failed(if rollback_failure == 3 {
                        "decorations"
                    } else {
                        "taskbar"
                    }))
                );
                let actual = native.0.borrow();
                assert_eq!(
                    controller.state().mode == OverviewMode::Floating,
                    actual.decorations
                );
                assert_eq!(controller.state().visible, actual.visible);
            }
        }
    }

    #[tokio::test]
    async fn successful_transitions_confirm_mode_and_visibility_in_both_directions() {
        for mode in [OverviewMode::Floating, OverviewMode::Tray] {
            let previous = if mode == OverviewMode::Tray {
                OverviewMode::Floating
            } else {
                OverviewMode::Tray
            };
            let native = Native::new(previous, Vec::new());
            let mut controller = OverviewWindowController::new();
            controller.set_mode(previous);
            let confirmed = transition_mode(&native, &mut controller, mode, async { native.fit() })
                .await
                .unwrap();
            assert_eq!(confirmed, controller.state());
            assert_eq!(confirmed.mode, mode);
            assert!(confirmed.visible);
            let actual = native.0.borrow();
            assert_eq!(actual.decorations, mode == OverviewMode::Floating);
            assert_eq!(actual.skip_taskbar, mode == OverviewMode::Tray);
            assert_eq!(actual.calls.last(), Some(&"persist"));
        }
    }
}
