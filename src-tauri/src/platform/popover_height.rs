//! The popover's content-fitted height.
//!
//! The wireframe's popover has no fixed height: it grows with its content up
//! to 760 logical pixels and scrolls beyond that. This module decides the
//! native window's height and vertical position for a given content height,
//! inside the monitor's work area, so the renderer never moves the window
//! itself.

/// The wireframe popover's tallest height, in logical pixels.
pub const MAX_HEIGHT: f64 = 760.0;
/// The shortest useful popover, in logical pixels, when the screen allows it.
pub const MIN_HEIGHT: f64 = 320.0;

/// The window's current placement, in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    /// Top edge of the outer window.
    pub outer_y: i32,
    /// Outer height, including any frame the system adds.
    pub outer_height: u32,
    /// Inner height, the webview's own height.
    pub inner_height: u32,
}

/// The work area's vertical extent, in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkArea {
    /// Top edge of the work area.
    pub top: i32,
    /// Height of the work area, which excludes the taskbar or menu bar.
    pub height: u32,
}

/// Where the window should go: its inner height in logical pixels and its new
/// top edge in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fitted {
    /// The webview's height, in logical pixels.
    pub inner_height: f64,
    /// The outer window's new top edge, in physical pixels.
    pub outer_y: i32,
}

/// Fits the popover to `content` logical pixels within the work area.
///
/// The height is held between the minimum and the wireframe's ceiling, and
/// never exceeds the work area, even when the work area is shorter than the
/// minimum. With `anchor_bottom` the window keeps its bottom edge, as a tray
/// popover above a bottom taskbar does; otherwise it keeps its top edge, as a
/// pinned window or a popover below a menu bar does. Either way the result is
/// moved back inside the work area.
#[must_use]
pub fn fit(
    content: f64,
    scale: f64,
    placement: Placement,
    area: WorkArea,
    anchor_bottom: bool,
) -> Fitted {
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let frame = f64::from(
        placement
            .outer_height
            .saturating_sub(placement.inner_height),
    );
    let available = ((f64::from(area.height) - frame) / scale).max(1.0);
    let ceiling = MAX_HEIGHT.min(available);
    let floor = MIN_HEIGHT.min(ceiling);
    let content = if content.is_finite() {
        content
    } else {
        ceiling
    };
    let inner_height = content.clamp(floor, ceiling).round();
    let outer_height = inner_height.mul_add(scale, frame);
    let top = f64::from(area.top);
    let bottom = top + f64::from(area.height);
    let current_top = f64::from(placement.outer_y);
    let wanted = if anchor_bottom {
        current_top + f64::from(placement.outer_height) - outer_height
    } else {
        current_top
    };
    let lowest = (bottom - outer_height).max(top);
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the position is rounded and lies inside the work area, which fits an i32"
    )]
    let outer_y = wanted.clamp(top, lowest).round() as i32;
    Fitted {
        inner_height,
        outer_y,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const AREA: WorkArea = WorkArea {
        top: 0,
        height: 1380,
    };

    fn placed(outer_y: i32, inner: u32) -> Placement {
        Placement {
            outer_y,
            outer_height: inner + 10,
            inner_height: inner,
        }
    }

    #[test]
    fn follows_the_content_between_the_minimum_and_the_ceiling() {
        let fitted = fit(379.0, 1.25, placed(100, 800), AREA, false);
        assert!((fitted.inner_height - 379.0).abs() < f64::EPSILON);
        assert!(
            (fit(120.0, 1.25, placed(100, 800), AREA, false).inner_height - MIN_HEIGHT).abs()
                < f64::EPSILON
        );
        assert!(
            (fit(2000.0, 1.25, placed(100, 800), AREA, false).inner_height - MAX_HEIGHT).abs()
                < f64::EPSILON
        );
    }

    #[test]
    fn a_tray_popover_keeps_its_bottom_edge() {
        // 800 physical inner + 10 frame, bottom edge at 1300.
        let fitted = fit(400.0, 1.25, placed(490, 800), AREA, true);
        let outer = 400.0f64.mul_add(1.25, 10.0);
        assert!((f64::from(fitted.outer_y) + outer - 1300.0).abs() < 1.0);
    }

    #[test]
    fn a_pinned_window_keeps_its_top_edge_unless_it_would_leave_the_screen() {
        assert_eq!(fit(400.0, 1.25, placed(200, 500), AREA, false).outer_y, 200);
        let low = fit(700.0, 1.25, placed(1200, 400), AREA, false);
        assert!(f64::from(low.outer_y) + 700.0f64.mul_add(1.25, 10.0) <= 1380.0);
    }

    #[test]
    fn a_short_work_area_wins_over_the_minimum() {
        let small = WorkArea {
            top: 0,
            height: 300,
        };
        let fitted = fit(600.0, 1.0, placed(0, 280), small, true);
        assert!(fitted.inner_height <= 290.0);
        assert!(fitted.outer_y >= 0);
    }
}
