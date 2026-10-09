//! Keeps the mini widget on a screen when the screens change.
//!
//! The widget is placed when it is shown. Unplugging a monitor, or changing a
//! screen's resolution or scale while Quota runs, can leave it partly off
//! every screen, cut off at an edge, until Quota restarts. So while the widget
//! is shown, the screens are checked every few seconds and whenever the
//! widget's own scale changes; when the widget is no longer wholly inside a
//! screen's work area, it moves the least distance that puts it back inside
//! the work area it overlaps most. The saved position is left as it was, so the
//! widget goes back there as soon as its screen is there again.
//!
//! The screens change at sign-in too: Quota can start before Windows has set
//! up every screen, so the widget is first placed on the screens there are,
//! and returns to its saved spot when that screen arrives. A widget that is
//! the view but not showing after a change is shown again.
//!
//! Every few seconds a shown widget is also put back above the other windows,
//! because the system can drop it below them and never says so.

use std::time::Duration;

use quota_domain::preferences::{AppView, WidgetPosition};
use tauri::{AppHandle, Manager, PhysicalPosition};

use super::tray_anchor::Rect;
use super::widget;
use crate::state::AppState;

/// How often the screens are checked while the widget is shown.
const CHECK_EVERY: Duration = Duration::from_secs(2);

/// How often a shown widget is put back above the other windows, in checks.
const RAISE_EVERY_CHECKS: u32 = 3;

/// How long after a scale change the widget is checked, once the window has
/// taken its new size.
const AFTER_SCALE_CHANGE: Duration = Duration::from_millis(300);

/// Where a window at `window` must move to be wholly inside one of `areas`,
/// or `None` when it already is, or when there are no areas to move into.
///
/// The area it overlaps most is chosen, or the nearest when it overlaps none,
/// and the window moves the least distance that puts it inside. A window
/// larger than the area is aligned with the area's top-left corner.
#[must_use]
pub fn keep_inside(window: Rect, areas: &[Rect]) -> Option<(f64, f64)> {
    if areas.iter().any(|area| inside(window, *area)) {
        return None;
    }
    let area = areas.iter().copied().max_by(|a, b| {
        overlap(window, *a)
            .total_cmp(&overlap(window, *b))
            .then_with(|| distance(window, *b).total_cmp(&distance(window, *a)))
    })?;
    let x = window.x.min(area.x + area.width - window.width).max(area.x);
    let y = window
        .y
        .min(area.y + area.height - window.height)
        .max(area.y);
    Some((x, y))
}

/// Where a shown widget at `window` goes after the screens change: back to
/// its `saved` spot while that spot is on one of `screens`, else wholly inside
/// one of `areas`, or `None` when it stays where it is.
#[must_use]
pub fn resettle(
    window: Rect,
    saved: Option<(f64, f64)>,
    screens: &[Rect],
    areas: &[Rect],
) -> Option<(f64, f64)> {
    if let Some(saved) = saved
        && saved != (window.x, window.y)
        && widget::on_screen(saved, (window.width, window.height), screens)
    {
        return Some(saved);
    }
    keep_inside(window, areas)
}

fn inside(window: Rect, area: Rect) -> bool {
    window.x >= area.x
        && window.y >= area.y
        && window.x + window.width <= area.x + area.width
        && window.y + window.height <= area.y + area.height
}

fn overlap(window: Rect, area: Rect) -> f64 {
    let width = (window.x + window.width).min(area.x + area.width) - window.x.max(area.x);
    let height = (window.y + window.height).min(area.y + area.height) - window.y.max(area.y);
    width.max(0.0) * height.max(0.0)
}

fn distance(window: Rect, area: Rect) -> f64 {
    let dx = (window.x + window.width / 2.0) - (area.x + area.width / 2.0);
    let dy = (window.y + window.height / 2.0) - (area.y + area.height / 2.0);
    dx.hypot(dy)
}

/// Checks the screens while the widget is shown, for as long as Quota runs.
pub fn watch(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        // Only a change of screens moves the widget: one the person parked
        // part off an edge on purpose stays there until the screens change.
        let mut last = screens_layout(&app);
        let mut checks: u32 = 0;
        loop {
            std::thread::sleep(CHECK_EVERY);
            let layout = screens_layout(&app);
            if layout != last {
                last = layout;
                check(&app);
            }
            checks = checks.wrapping_add(1);
            if checks.is_multiple_of(RAISE_EVERY_CHECKS) {
                keep_on_top(&app);
            }
        }
    });
}

/// Checks the widget shortly after its scale changed.
pub fn check_soon(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(AFTER_SCALE_CHANGE);
        check(&app);
    });
}

/// The screens' positions, sizes and scales, so a change of any is noticed.
fn screens_layout(app: &AppHandle) -> String {
    let Some(native) = app.get_webview_window(widget::LABEL) else {
        return String::new();
    };
    native
        .available_monitors()
        .map(|monitors| {
            monitors
                .iter()
                .map(|monitor| {
                    format!(
                        "{:?}{:?}{}",
                        monitor.position(),
                        monitor.size(),
                        monitor.scale_factor()
                    )
                })
                .collect::<Vec<_>>()
                .join(";")
        })
        .unwrap_or_default()
}

/// Moves a shown widget back to its saved spot, or inside a screen's work
/// area when it has left it, and shows a widget that is the view but hidden.
fn check(app: &AppHandle) {
    let Some(native) = app.get_webview_window(widget::LABEL) else {
        return;
    };
    let Some((view, saved)) = saved_view(app) else {
        return;
    };
    if view != AppView::Widget {
        return;
    }
    if !native.is_visible().unwrap_or(false) {
        show_again(app, saved);
        return;
    }
    let (Ok(position), Ok(size), Ok(monitors)) = (
        native.outer_position(),
        native.outer_size(),
        native.available_monitors(),
    ) else {
        return;
    };
    let window = Rect {
        x: f64::from(position.x),
        y: f64::from(position.y),
        width: f64::from(size.width),
        height: f64::from(size.height),
    };
    let screens: Vec<Rect> = monitors
        .iter()
        .map(|monitor| rect_of(*monitor.position(), *monitor.size()))
        .collect();
    let areas: Vec<Rect> = monitors
        .iter()
        .map(|monitor| {
            let area = monitor.work_area();
            rect_of(area.position, area.size)
        })
        .collect();
    let saved = saved.map(|WidgetPosition { x, y }| (f64::from(x), f64::from(y)));
    let Some((x, y)) = resettle(window, saved, &screens, &areas) else {
        return;
    };
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the position lies on a screen, well inside i32"
    )]
    let moved = PhysicalPosition::new(x.round() as i32, y.round() as i32);
    // A move the host makes is not the person's choice of position.
    widget::mark_programmatic(moved);
    tracing::info!(
        x = moved.x,
        y = moved.y,
        screens = screens.len(),
        "the screens changed; the widget moved"
    );
    if let Err(error) = native.set_position(moved) {
        tracing::warn!(%error, "the widget could not be moved back onto a screen");
    }
}

/// Puts a shown widget back above the other windows while it is the view.
fn keep_on_top(app: &AppHandle) {
    let Some(native) = app.get_webview_window(widget::LABEL) else {
        return;
    };
    if !native.is_visible().unwrap_or(false)
        || saved_view(app).is_none_or(|(view, _)| view != AppView::Widget)
    {
        return;
    }
    if let Err(error) = widget::raise(&native) {
        tracing::warn!(
            code = error.diagnostic_code(),
            "the widget could not be put back on top"
        );
    }
}

/// The saved view and widget position, once the backend has read them.
fn saved_view(app: &AppHandle) -> Option<(AppView, Option<WidgetPosition>)> {
    let state = app.try_state::<AppState>()?;
    Some(tauri::async_runtime::block_on(async {
        let preferences = state.preferences_state.read().await;
        (preferences.view, preferences.widget_position)
    }))
}

/// Shows the widget again when it is the view but is not showing; `show`
/// logs where it went.
fn show_again(app: &AppHandle, saved: Option<WidgetPosition>) {
    if let Err(error) = widget::show(app, saved) {
        tracing::warn!(
            code = error.diagnostic_code(),
            "the widget could not be shown after the screens changed"
        );
    }
}

fn rect_of(position: PhysicalPosition<i32>, size: tauri::PhysicalSize<u32>) -> Rect {
    Rect {
        x: f64::from(position.x),
        y: f64::from(position.y),
        width: f64::from(size.width),
        height: f64::from(size.height),
    }
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

    /// Two screens side by side, each with a taskbar at the bottom.
    const LAPTOP: Rect = rect(0.0, 0.0, 1920.0, 1040.0);
    const MONITOR: Rect = rect(1920.0, 0.0, 2560.0, 1400.0);

    #[test]
    fn a_widget_wholly_on_a_screen_stays_where_it_is() {
        let widget = rect(4000.0, 100.0, 395.0, 145.0);
        assert_eq!(keep_inside(widget, &[LAPTOP, MONITOR]), None);
    }

    #[test]
    fn a_widget_cut_off_at_a_screens_edge_moves_just_inside_it() {
        // After a resolution change the work area shrank under it.
        let widget = rect(1700.0, 950.0, 395.0, 145.0);
        assert_eq!(
            keep_inside(widget, &[LAPTOP]),
            Some((1920.0 - 395.0, 1040.0 - 145.0))
        );
    }

    #[test]
    fn a_widget_left_on_an_unplugged_screen_moves_to_the_nearest_one() {
        let widget = rect(4000.0, 100.0, 395.0, 145.0);
        assert_eq!(
            keep_inside(widget, &[LAPTOP]),
            Some((1920.0 - 395.0, 100.0))
        );
    }

    #[test]
    fn a_widget_across_two_screens_moves_onto_the_one_it_overlaps_most() {
        let widget = rect(1800.0, 100.0, 395.0, 145.0);
        assert_eq!(
            keep_inside(widget, &[LAPTOP, MONITOR]),
            Some((1920.0, 100.0))
        );
    }

    #[test]
    fn a_widget_returns_to_its_saved_spot_when_its_screen_arrives() {
        // Placed on the laptop at sign-in, before the monitor was set up.
        let widget = rect(1501.0, 24.0, 395.0, 145.0);
        let screens = [LAPTOP, rect(1920.0, 0.0, 2560.0, 1440.0)];
        assert_eq!(
            resettle(widget, Some((4000.0, 100.0)), &screens, &[LAPTOP, MONITOR]),
            Some((4000.0, 100.0))
        );
    }

    #[test]
    fn a_saved_spot_still_off_every_screen_only_keeps_the_widget_inside() {
        let widget = rect(1700.0, 950.0, 395.0, 145.0);
        assert_eq!(
            resettle(widget, Some((4000.0, 100.0)), &[LAPTOP], &[LAPTOP]),
            keep_inside(widget, &[LAPTOP])
        );
    }

    #[test]
    fn a_widget_at_its_saved_spot_stays_there() {
        let widget = rect(4000.0, 100.0, 395.0, 145.0);
        assert_eq!(
            resettle(
                widget,
                Some((4000.0, 100.0)),
                &[MONITOR],
                &[LAPTOP, MONITOR]
            ),
            None
        );
    }

    #[test]
    fn no_screen_means_no_move() {
        assert_eq!(keep_inside(rect(0.0, 0.0, 10.0, 10.0), &[]), None);
    }
}
