//! Where the tray popover goes, on any desktop.
//!
//! The tray sits on whichever screen edge the system reserves for its taskbar
//! or menu bar: the bottom for a default Windows taskbar, the top for the macOS
//! menu bar, any side for a moved taskbar or a Linux panel. Nothing here names
//! an operating system. The edge is read from the tray icon's own rectangle
//! when the system reports one, and otherwise from the side on which the work
//! area is inset from the monitor. The popover is placed against that edge,
//! centred on the icon, inside the work area, and grows away from the edge.

/// A rectangle in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    /// Left edge.
    pub x: f64,
    /// Top edge.
    pub y: f64,
    /// Width.
    pub width: f64,
    /// Height.
    pub height: f64,
}

impl Rect {
    fn right(self) -> f64 {
        self.x + self.width
    }

    fn bottom(self) -> f64 {
        self.y + self.height
    }

    fn centre(self) -> (f64, f64) {
        (self.x + self.width / 2.0, self.y + self.height / 2.0)
    }
}

/// The screen edge the tray is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    /// Below the work area, as a default Windows taskbar.
    Bottom,
    /// Above the work area, as the macOS menu bar.
    Top,
    /// Right of the work area.
    Right,
    /// Left of the work area.
    Left,
}

/// The least inset that counts as a reserved edge, in physical pixels.
const RESERVED: f64 = 0.5;

/// The icon, if it is where a tray is: on this monitor, outside the work area.
///
/// A taskbar or menu-bar icon is in the strip the system reserves, so it says
/// which edge the tray is on. An icon the system has hidden in an overflow
/// flyout reports a rectangle inside the work area, or on no screen at all,
/// which says nothing about the edge, so it is not used.
#[must_use]
pub fn reserved_icon(monitor: Rect, work_area: Rect, icon: Option<Rect>) -> Option<Rect> {
    icon.filter(|icon| {
        let (x, y) = icon.centre();
        let on_monitor =
            x >= monitor.x && x <= monitor.right() && y >= monitor.y && y <= monitor.bottom();
        let in_work_area =
            x > work_area.x && x < work_area.right() && y > work_area.y && y < work_area.bottom();
        on_monitor && !in_work_area
    })
}

/// The edge the tray is on.
///
/// With an icon rectangle from [`reserved_icon`], it is the monitor edge
/// nearest the icon. The work
/// area alone cannot decide this when two edges are reserved, as on macOS,
/// where the menu bar holds the top and the Dock the bottom. Without an icon
/// it is the most inset side, and with no inset at all (a hidden taskbar) the
/// bottom, where the wireframe draws the popover.
#[must_use]
pub fn tray_edge(monitor: Rect, work_area: Rect, icon: Option<Rect>) -> Edge {
    let candidates = if let Some(icon) = icon {
        let (x, y) = icon.centre();
        // Smaller is nearer, so the distances are negated to pick the largest.
        [
            (Edge::Bottom, y - monitor.bottom()),
            (Edge::Top, monitor.y - y),
            (Edge::Right, x - monitor.right()),
            (Edge::Left, monitor.x - x),
        ]
    } else {
        [
            (Edge::Bottom, monitor.bottom() - work_area.bottom()),
            (Edge::Top, work_area.y - monitor.y),
            (Edge::Right, monitor.right() - work_area.right()),
            (Edge::Left, work_area.x - monitor.x),
        ]
    };
    let [first, rest @ ..] = candidates;
    let best = rest.into_iter().fold(first, |best, candidate| {
        if candidate.1 > best.1 {
            candidate
        } else {
            best
        }
    });
    if icon.is_none() && best.1 < RESERVED {
        return Edge::Bottom;
    }
    best.0
}

/// Where the popover's outer top-left corner goes, in physical pixels.
///
/// It sits against the tray edge of the work area, `margin` away from it,
/// centred on the icon along that edge. Without an icon it takes the far end
/// of the edge, the bottom-right corner the wireframe draws. Either way it is
/// kept `margin` inside the work area; a popover larger than the work area
/// keeps its top-left corner inside.
#[must_use]
pub fn place(
    work_area: Rect,
    edge: Edge,
    icon: Option<Rect>,
    size: (f64, f64),
    margin: f64,
) -> (f64, f64) {
    let (width, height) = size;
    let (x, y) = icon.map_or_else(|| (work_area.right(), work_area.bottom()), Rect::centre);
    let (x, y) = match edge {
        Edge::Bottom => (x - width / 2.0, work_area.bottom() - height - margin),
        Edge::Top => (x - width / 2.0, work_area.y + margin),
        Edge::Right => (work_area.right() - width - margin, y - height / 2.0),
        Edge::Left => (work_area.x + margin, y - height / 2.0),
    };
    (
        within(x, work_area.x + margin, work_area.right() - width - margin),
        within(
            y,
            work_area.y + margin,
            work_area.bottom() - height - margin,
        ),
    )
}

/// `value` held between `low` and `high`, with `low` winning when they cross.
fn within(value: f64, low: f64, high: f64) -> f64 {
    value.min(high).max(low)
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    /// Asserts a placement to within a thousandth of a pixel.
    #[track_caller]
    fn assert_at(actual: (f64, f64), expected: (f64, f64)) {
        assert!(
            (actual.0 - expected.0).abs() < 1e-3 && (actual.1 - expected.1).abs() < 1e-3,
            "placed at {actual:?}, expected {expected:?}"
        );
    }

    const MONITOR: Rect = rect(0.0, 0.0, 1920.0, 1080.0);
    const POPOVER: (f64, f64) = (440.0, 600.0);

    #[test]
    fn a_default_windows_taskbar_puts_the_popover_above_the_icon() {
        let area = rect(0.0, 0.0, 1920.0, 1032.0);
        let icon = rect(1300.0, 1040.0, 32.0, 40.0);
        let edge = tray_edge(MONITOR, area, Some(icon));
        assert_eq!(edge, Edge::Bottom);
        assert_at(
            place(area, edge, Some(icon), POPOVER, 12.0),
            (1096.0, 420.0),
        );
    }

    #[test]
    fn the_macos_menu_bar_puts_the_popover_below_the_icon_despite_the_dock() {
        // The menu bar reserves the top 25 pixels and the Dock the bottom 70.
        let area = rect(0.0, 25.0, 1920.0, 985.0);
        let icon = rect(1500.0, 0.0, 30.0, 25.0);
        let edge = tray_edge(MONITOR, area, Some(icon));
        assert_eq!(edge, Edge::Top);
        assert_at(place(area, edge, Some(icon), POPOVER, 12.0), (1295.0, 37.0));
    }

    #[test]
    fn a_side_taskbar_puts_the_popover_beside_the_icon() {
        let left = rect(62.0, 0.0, 1858.0, 1080.0);
        let icon = rect(10.0, 900.0, 40.0, 32.0);
        let edge = tray_edge(MONITOR, left, Some(icon));
        assert_eq!(edge, Edge::Left);
        assert_at(place(left, edge, Some(icon), POPOVER, 12.0), (74.0, 468.0));
        let right = rect(0.0, 0.0, 1858.0, 1080.0);
        let icon = rect(1870.0, 100.0, 40.0, 32.0);
        let edge = tray_edge(MONITOR, right, Some(icon));
        assert_eq!(edge, Edge::Right);
        assert_at(
            place(right, edge, Some(icon), POPOVER, 12.0),
            (1406.0, 12.0),
        );
    }

    #[test]
    fn without_an_icon_the_reserved_side_decides() {
        // Linux reports no icon rectangle.
        let top_panel = rect(0.0, 28.0, 1920.0, 1052.0);
        assert_eq!(tray_edge(MONITOR, top_panel, None), Edge::Top);
        assert_at(
            place(top_panel, Edge::Top, None, POPOVER, 12.0),
            (1468.0, 40.0),
        );
        let taskbar = rect(0.0, 0.0, 1920.0, 1032.0);
        assert_eq!(tray_edge(MONITOR, taskbar, None), Edge::Bottom);
        assert_at(
            place(taskbar, Edge::Bottom, None, POPOVER, 12.0),
            (1468.0, 420.0),
        );
    }

    #[test]
    fn only_an_icon_in_the_reserved_strip_places_the_popover() {
        let taskbar = rect(0.0, 0.0, 1920.0, 1032.0);
        let on_taskbar = rect(1300.0, 1040.0, 32.0, 40.0);
        assert_eq!(
            reserved_icon(MONITOR, taskbar, Some(on_taskbar)),
            Some(on_taskbar)
        );
        // An icon in the hidden-icons flyout, above the taskbar.
        let in_flyout = rect(1600.0, 900.0, 32.0, 32.0);
        assert_eq!(reserved_icon(MONITOR, taskbar, Some(in_flyout)), None);
        // An icon on another monitor, or reported nowhere.
        let elsewhere = rect(-32000.0, -32000.0, 32.0, 32.0);
        assert_eq!(reserved_icon(MONITOR, taskbar, Some(elsewhere)), None);
        let menu_bar = rect(0.0, 25.0, 1920.0, 985.0);
        let in_menu_bar = rect(1500.0, 0.0, 30.0, 25.0);
        assert_eq!(
            reserved_icon(MONITOR, menu_bar, Some(in_menu_bar)),
            Some(in_menu_bar)
        );
    }

    #[test]
    fn a_hidden_taskbar_falls_back_to_the_bottom_right_corner() {
        assert_eq!(tray_edge(MONITOR, MONITOR, None), Edge::Bottom);
        assert_at(
            place(MONITOR, Edge::Bottom, None, POPOVER, 12.0),
            (1468.0, 468.0),
        );
    }

    #[test]
    fn the_popover_stays_inside_the_work_area() {
        let area = rect(0.0, 0.0, 1920.0, 1032.0);
        // An icon at the far right would centre the popover off the screen.
        let icon = rect(1900.0, 1040.0, 20.0, 40.0);
        assert_at(
            place(area, Edge::Bottom, Some(icon), POPOVER, 12.0),
            (1468.0, 420.0),
        );
        // A popover taller than a short work area keeps its top inside.
        let short = rect(0.0, 0.0, 1920.0, 400.0);
        let (_, top) = place(short, Edge::Bottom, Some(icon), POPOVER, 12.0);
        assert_at((0.0, top), (0.0, 12.0));
    }
}
