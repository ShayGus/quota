//! The mode-aware overview window controller.
//!
//! Floating and tray anchoring are separate from geometry and from topmost.
//! Dragging a tray-anchored view detaches it to floating without enabling
//! topmost, and changing topmost moves nothing.

use quota_domain::preferences::OverviewMode;

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
    pub geometry_revision: u64,
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
}