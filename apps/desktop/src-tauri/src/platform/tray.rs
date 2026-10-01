//! The tray controller.
//!
//! The tray exists in Rust so it survives renderer closure. A left click
//! activates the overview; it never hides a window that is merely covered.

use quota_domain::preferences::OverviewMode;

/// What a tray activation should do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayActivation {
    /// Show the overview.
    Show,
    /// Raise and focus the overview because it is already open but covered.
    Raise,
    /// Dismiss the tray-anchored view.
    Dismiss,
    /// Hide the overview to the tray.
    Hide,
    /// Stop scheduling and exit.
    Quit,
}

/// Decides what a tray click means, given the window's confirmed state.
#[must_use]
pub fn activation_for(
    mode: OverviewMode,
    visible: bool,
    click_is_repeated: bool,
) -> TrayActivation {
    if !visible {
        return TrayActivation::Show;
    }
    match mode {
        OverviewMode::Floating => TrayActivation::Raise,
        OverviewMode::Tray if click_is_repeated => TrayActivation::Dismiss,
        OverviewMode::Tray => TrayActivation::Raise,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_click_raises_a_covered_floating_window_instead_of_hiding_it() {
        assert_eq!(activation_for(OverviewMode::Floating, true, false), TrayActivation::Raise);
    }

    #[test]
    fn a_click_shows_a_hidden_window() {
        assert_eq!(activation_for(OverviewMode::Floating, false, false), TrayActivation::Show);
    }

    #[test]
    fn a_repeated_click_toggles_the_tray_view() {
        assert_eq!(activation_for(OverviewMode::Tray, true, true), TrayActivation::Dismiss);
        assert_eq!(activation_for(OverviewMode::Tray, true, false), TrayActivation::Raise);
    }
}