//! The mini widget window.
//!
//! The widget is the compact way to present Quota: a small window with every
//! account at a glance. While it is the chosen view it is always on top and
//! always on screen; closing it does nothing, and only choosing the full window
//! puts it away (see `app_view`).
//!
//! Its position is the app's own, saved in the preferences whenever it moves,
//! not left to the window-state plugin, which saves only when the app exits
//! cleanly. A saved position is used while it is still on a screen; otherwise,
//! as on the first run or after a screen is unplugged, the widget goes to the
//! top-right corner of the main screen.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use quota_contracts::CommandError;
use quota_domain::preferences::WidgetPosition;
use tauri::menu::CheckMenuItem;
use tauri::{AppHandle, Manager, PhysicalPosition, Wry};

use super::tray_anchor::Rect;
use super::window::{self, failed};
use crate::state::AppState;

/// The widget window's label in `tauri.conf.json`.
pub const LABEL: &str = "widget";

/// The gap kept between a placed widget and the edge of the work area.
const MARGIN: f64 = 24.0;

/// How much of the widget's top edge must be on a screen for a saved position
/// to be used, in physical pixels.
const VISIBLE_GRIP: f64 = 32.0;

/// How long the widget must rest before a move is saved.
const SAVE_DELAY: Duration = Duration::from_millis(500);

/// The tray menu's widget item, kept so a change from any window checks it.
pub struct WidgetMenuItem(pub CheckMenuItem<Wry>);

/// Counts moves, so only the last move of a drag is saved.
static MOVES: AtomicU64 = AtomicU64::new(0);

/// Where the widget goes: where it was left, while its top edge is still on a
/// screen, else the top-right corner of the main screen's work area.
#[must_use]
pub fn place(
    saved: Option<WidgetPosition>,
    size: (f64, f64),
    screens: &[Rect],
    main_work_area: Rect,
    margin: f64,
) -> (f64, f64) {
    if let Some(saved) = saved {
        let (x, y) = (f64::from(saved.x), f64::from(saved.y));
        let grip = (x + size.0 / 2.0, y + VISIBLE_GRIP.min(size.1) / 2.0);
        let on_screen = screens.iter().any(|screen| {
            grip.0 >= screen.x
                && grip.0 < screen.x + screen.width
                && grip.1 >= screen.y
                && grip.1 < screen.y + screen.height
        });
        if on_screen {
            return (x, y);
        }
    }
    (
        main_work_area.x + main_work_area.width - size.0 - margin,
        main_work_area.y + margin,
    )
}

/// Places the widget, keeps it above other windows, and shows it without
/// taking focus from what the person is doing.
///
/// # Errors
/// Returns the native failure when the window refuses.
pub fn show(app: &AppHandle, saved: Option<WidgetPosition>) -> Result<(), CommandError> {
    let native = window::get(app, LABEL)?;
    let size = native
        .outer_size()
        .map_err(|_| failed("read_widget_size"))?;
    let screens = native
        .available_monitors()
        .map_err(|_| failed("read_monitors"))?
        .iter()
        .map(|monitor| physical_rect(*monitor.position(), *monitor.size()))
        .collect::<Vec<_>>();
    let main = native
        .primary_monitor()
        .map_err(|_| failed("read_primary_monitor"))?
        .ok_or_else(|| failed("find_display"))?;
    let area = main.work_area();
    let (x, y) = place(
        saved,
        (f64::from(size.width), f64::from(size.height)),
        &screens,
        physical_rect(area.position, area.size),
        MARGIN * main.scale_factor(),
    );
    native
        .set_position(PhysicalPosition::new(x, y))
        .map_err(|_| failed("place_widget"))?;
    native
        .set_always_on_top(true)
        .map_err(|_| failed("set_always_on_top"))?;
    native.show().map_err(|_| failed("show_widget"))
}

/// Puts the widget away, when the full window becomes the view.
///
/// # Errors
/// Returns the native failure when the window refuses.
pub fn hide(app: &AppHandle) -> Result<(), CommandError> {
    window::get(app, LABEL)?
        .hide()
        .map_err(|_| failed("hide_widget"))
}

/// Saves where the widget was moved to, once it has rested, so a drag saves
/// its end and not every step on the way.
pub fn remember(app: &AppHandle, position: PhysicalPosition<i32>) {
    let this_move = MOVES.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(SAVE_DELAY).await;
        if MOVES.load(Ordering::SeqCst) != this_move {
            return;
        }
        let Some(state) = app.try_state::<AppState>() else {
            return;
        };
        let state = state.inner().clone();
        let position = WidgetPosition {
            x: position.x,
            y: position.y,
        };
        if state.preferences_state.read().await.widget_position == Some(position) {
            return;
        }
        if let Err(error) = crate::ipc::commands_prefs::change_preferences(&state, |preferences| {
            preferences.widget_position = Some(position);
        })
        .await
        {
            tracing::warn!(
                code = error.diagnostic_code(),
                "the widget position could not be saved"
            );
        }
    });
}

/// Checks the tray item while the widget is the view.
pub fn reflect(app: &AppHandle, widget: bool) {
    if let Some(item) = app.try_state::<WidgetMenuItem>()
        && let Err(error) = item.0.set_checked(widget)
    {
        tracing::warn!(%error, "the tray widget item could not be updated");
    }
}

/// A monitor rectangle as the placement takes it.
fn physical_rect(position: tauri::PhysicalPosition<i32>, size: tauri::PhysicalSize<u32>) -> Rect {
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

    const MAIN: Rect = rect(0.0, 0.0, 1920.0, 1080.0);
    const MAIN_WORK_AREA: Rect = rect(0.0, 0.0, 1920.0, 1032.0);
    const LEFT: Rect = rect(-2560.0, -200.0, 2560.0, 1440.0);
    const SIZE: (f64, f64) = (395.0, 128.0);

    #[test]
    fn a_first_run_puts_the_widget_in_the_top_right_corner() {
        assert_eq!(
            place(None, SIZE, &[MAIN], MAIN_WORK_AREA, 24.0),
            (1501.0, 24.0)
        );
    }

    #[test]
    fn a_saved_position_on_any_screen_is_kept_exactly() {
        let saved = WidgetPosition { x: -1800, y: 40 };
        assert_eq!(
            place(Some(saved), SIZE, &[MAIN, LEFT], MAIN_WORK_AREA, 24.0),
            (-1800.0, 40.0)
        );
        let edge = WidgetPosition { x: 1700, y: 900 };
        assert_eq!(
            place(Some(edge), SIZE, &[MAIN], MAIN_WORK_AREA, 24.0),
            (1700.0, 900.0)
        );
    }

    #[test]
    fn a_position_on_a_screen_that_is_gone_falls_back_to_the_corner() {
        let saved = WidgetPosition { x: -1800, y: 40 };
        assert_eq!(
            place(Some(saved), SIZE, &[MAIN], MAIN_WORK_AREA, 24.0),
            (1501.0, 24.0)
        );
    }

    #[test]
    fn a_widget_whose_top_edge_is_off_screen_falls_back_to_the_corner() {
        let above = WidgetPosition { x: 400, y: -100 };
        assert_eq!(
            place(Some(above), SIZE, &[MAIN], MAIN_WORK_AREA, 24.0),
            (1501.0, 24.0)
        );
    }
}
