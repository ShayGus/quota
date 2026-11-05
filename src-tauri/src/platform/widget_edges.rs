//! Where the mini widget goes as its width changes, and when it is left near a
//! screen's edge. Its window follows its content, so a widget snapped to the
//! right edge has to stay there as its numbers change.

use std::sync::atomic::{AtomicU8, Ordering};

use tauri::{AppHandle, Manager, PhysicalPosition};

use super::tray_anchor::Rect;
use super::widget::WIDTH;

/// The widest the renderer can ask for, in logical pixels, as `.widget-list` in
/// `widget.css` has it. Limit columns of one width can take the list past the
/// ring strip's width.
pub const MAX_WIDTH: f64 = 400.0;

/// The narrowest the renderer can ask for, in logical pixels.
pub const MIN_WIDTH: f64 = 120.0;

/// How near an edge the widget must rest to snap to it, in logical pixels.
const SNAP_REACH: f64 = 28.0;

/// The gap a snapped widget keeps from the edge, in logical pixels.
const SNAP_MARGIN: f64 = 8.0;

/// Which edge the next width change keeps, as a [`WidthAnchor`].
static NEXT_ANCHOR: AtomicU8 = AtomicU8::new(0);

/// Where a widget left resting at `position` snaps to: `margin` from any edge
/// of `area` nearer than `reach`, and otherwise where it is.
#[must_use]
pub fn snap(
    position: (f64, f64),
    size: (f64, f64),
    area: Rect,
    reach: f64,
    margin: f64,
) -> (f64, f64) {
    let along = |at: f64, length: f64, start: f64, extent: f64| {
        if at - start < reach {
            start + margin
        } else if start + extent - (at + length) < reach {
            start + extent - length - margin
        } else {
            at
        }
    };
    (
        along(position.0, size.0, area.x, area.width),
        along(position.1, size.1, area.y, area.height),
    )
}

/// Which edge stays put when the widget's width changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidthAnchor {
    /// The right edge, when the widget rests against it, else the left.
    Resting,
    /// The left edge: a saved position is where the left edge was left.
    Left,
    /// The right edge: the first-run corner is measured from the right.
    Right,
}

/// The widget's left edge after its width changes from `current` to `width`,
/// all in physical pixels, so a widget snapped to the right edge stays there.
/// A widget that grew past the area's right edge is moved back inside it.
#[must_use]
pub fn fit_x(anchor: WidthAnchor, x: f64, current: f64, width: f64, area: Rect, reach: f64) -> f64 {
    let right = x + current;
    let area_right = area.x + area.width;
    let keeps_right = match anchor {
        WidthAnchor::Resting => (area_right - right).abs() < reach,
        WidthAnchor::Left => false,
        WidthAnchor::Right => true,
    };
    let left = if keeps_right { right - width } else { x };
    if width > current && left + width > area_right && right <= area_right {
        (area_right - width).max(area.x)
    } else {
        left
    }
}

/// The width the renderer asked for, inside what the widget can be.
#[must_use]
pub fn clamp_width(content: f64) -> f64 {
    if content.is_finite() {
        content.clamp(MIN_WIDTH, MAX_WIDTH)
    } else {
        WIDTH
    }
}

/// The edge the next width change keeps, once: after that, the resting rule.
pub fn take_anchor() -> WidthAnchor {
    match NEXT_ANCHOR.swap(0, Ordering::SeqCst) {
        1 => WidthAnchor::Left,
        2 => WidthAnchor::Right,
        _ => WidthAnchor::Resting,
    }
}

/// [`fit_x`] for a window on a real screen: the new left edge, as a window
/// position.
#[must_use]
pub fn left_edge_after(
    anchor: WidthAnchor,
    x: i32,
    current: u32,
    width: f64,
    area: Rect,
    scale: f64,
) -> i32 {
    let left = fit_x(
        anchor,
        f64::from(x),
        f64::from(current),
        width,
        area,
        SNAP_REACH * scale,
    );
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the left edge is rounded from physical pixels already inside an i32 range"
    )]
    let left = left.round() as i32;
    left
}

/// The edge the first fit after showing the widget keeps: the left from a saved
/// spot, the right from the corner.
pub fn anchor_next_fit(at_saved: bool) {
    NEXT_ANCHOR.store(if at_saved { 1 } else { 2 }, Ordering::SeqCst);
}

/// Moves a widget left resting near an edge onto that edge, and answers where
/// it is now. Any failure leaves it where the person left it.
pub fn snapped(app: &AppHandle, position: PhysicalPosition<i32>) -> PhysicalPosition<i32> {
    let Some(native) = app.get_webview_window(super::widget::LABEL) else {
        return position;
    };
    let (Ok(Some(monitor)), Ok(size), Ok(scale)) = (
        native.current_monitor(),
        native.outer_size(),
        native.scale_factor(),
    ) else {
        return position;
    };
    let area = monitor.work_area();
    let (x, y) = snap(
        (f64::from(position.x), f64::from(position.y)),
        (f64::from(size.width), f64::from(size.height)),
        physical_rect(area.position, area.size),
        SNAP_REACH * scale,
        SNAP_MARGIN * scale,
    );
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the position lies on a screen, well inside i32"
    )]
    let moved = PhysicalPosition::new(x.round() as i32, y.round() as i32);
    if moved == position {
        return position;
    }
    super::widget::mark_programmatic(moved);
    if native.set_position(moved).is_err() {
        tracing::warn!("the widget could not snap to the edge");
        return position;
    }
    moved
}

/// A monitor rectangle as the placement takes it.
#[must_use]
pub fn physical_rect(
    position: tauri::PhysicalPosition<i32>,
    size: tauri::PhysicalSize<u32>,
) -> Rect {
    Rect {
        x: f64::from(position.x),
        y: f64::from(position.y),
        width: f64::from(size.width),
        height: f64::from(size.height),
    }
}
