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

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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

/// A position the host moved the widget to on purpose, packed as two halves,
/// never a position the person left the widget at.
static PROGRAMMATIC_POS: AtomicU64 = AtomicU64::new(0);
/// Whether a programmatic position is recorded.
static PROGRAMMATIC_SET: AtomicBool = AtomicBool::new(false);

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

/// The gap kept between a grown widget and the work area's edge.
const GROWTH_GAP: f64 = 8.0;

/// The smallest drawer that still shows its header and one row, in logical
/// pixels, when neither side has room for the whole drawer.
const MIN_DRAWER: f64 = 56.0;

/// The widget's current placement, in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    /// The top edge of the window.
    pub outer_y: i32,
    /// The window's full height.
    pub outer_height: u32,
    /// The height the renderer's content fills.
    pub inner_height: u32,
}

/// The work area's vertical extent, in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorkArea {
    /// The work area's top edge.
    pub top: i32,
    /// The work area's height.
    pub height: u32,
}

/// Where the widget should go: its content height in logical pixels, the side
/// it grows on, and its new top edge in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fitted {
    /// The fitted content height; below the asked height only when neither
    /// side had room and the drawer is capped.
    pub height: f64,
    /// The side the window grows or shrinks on.
    pub direction: quota_contracts::commands::WidgetGrowth,
    /// The window's new top edge.
    pub outer_y: i32,
    /// The free room above the window, in logical pixels.
    pub room_above: f64,
    /// The free room below the window, in logical pixels.
    pub room_below: f64,
}

/// Fits the widget to `content` logical pixels inside the work area.
///
/// A shrink keeps the drawer's side: downward keeps the top edge, and upward
/// moves the top down by what the height loses, so the tiles keep their
/// screen position. A growth takes the preferred side while it fits there,
/// else the other side while that fits, else the larger side with the drawer
/// capped at its room, so its rows scroll instead of leaving the screen.
#[must_use]
pub fn fit(
    content: f64,
    preferred: quota_contracts::commands::WidgetGrowth,
    placement: Placement,
    area: WorkArea,
    scale: f64,
) -> Fitted {
    use quota_contracts::commands::WidgetGrowth;
    let scale = if scale.is_finite() && scale > 0.0 {
        scale
    } else {
        1.0
    };
    let current = f64::from(placement.inner_height) / scale;
    let content = if content.is_finite() {
        content.max(1.0)
    } else {
        current
    };
    let top = f64::from(area.top);
    let bottom = top + f64::from(area.height);
    let current_top = f64::from(placement.outer_y);
    let current_bottom = current_top + f64::from(placement.outer_height);
    let room_above = (current_top - top) / scale;
    let room_below = (bottom - current_bottom) / scale;
    let growth = content - current;
    let (height, direction) = if growth <= 0.0 {
        (content, preferred)
    } else {
        let fits_below = room_below >= growth + GROWTH_GAP;
        let fits_above = room_above >= growth + GROWTH_GAP;
        let direction = if preferred == WidgetGrowth::Down && fits_below {
            WidgetGrowth::Down
        } else if preferred == WidgetGrowth::Up && fits_above {
            WidgetGrowth::Up
        } else if fits_below {
            WidgetGrowth::Down
        } else if fits_above {
            WidgetGrowth::Up
        } else if room_below >= room_above {
            WidgetGrowth::Down
        } else {
            WidgetGrowth::Up
        };
        let fits = if direction == WidgetGrowth::Down {
            fits_below
        } else {
            fits_above
        };
        let grown = if fits {
            growth
        } else {
            let room = if direction == WidgetGrowth::Down {
                room_below
            } else {
                room_above
            };
            growth.min((room - GROWTH_GAP).max(MIN_DRAWER))
        };
        (current + grown, direction)
    };
    let outer_y = if direction == WidgetGrowth::Up {
        current_bottom - height * scale
    } else {
        current_top
    };
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the position is rounded from physical pixels already inside an i32 range"
    )]
    let outer_y = outer_y.round() as i32;
    Fitted {
        height,
        direction,
        outer_y,
        room_above,
        room_below,
    }
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
/// its end and not every step on the way. A position the host moved the
/// widget to on purpose, as an upward drawer does, is never saved.
pub fn remember(app: &AppHandle, position: PhysicalPosition<i32>) {
    if PROGRAMMATIC_SET.load(Ordering::SeqCst)
        && PROGRAMMATIC_POS.load(Ordering::SeqCst) == pack(position)
    {
        return;
    }
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

/// Records a position the host moved the widget to on purpose, so `remember`
/// never saves it as where the person left the widget.
pub fn mark_programmatic(position: PhysicalPosition<i32>) {
    PROGRAMMATIC_POS.store(pack(position), Ordering::SeqCst);
    PROGRAMMATIC_SET.store(true, Ordering::SeqCst);
}

/// Saves the resting position an upward close restored, so a restart puts the
/// widget where its tiles are. A drag that is still settling must not
/// overwrite it afterwards, so pending delayed saves are discarded first.
pub async fn save_resting_position(state: &AppState, position: WidgetPosition) {
    MOVES.fetch_add(1, Ordering::SeqCst);
    if state.preferences_state.read().await.widget_position == Some(position) {
        return;
    }
    if let Err(error) = crate::ipc::commands_prefs::change_preferences(state, |preferences| {
        preferences.widget_position = Some(position);
    })
    .await
    {
        tracing::warn!(
            code = error.diagnostic_code(),
            "the widget position could not be saved"
        );
    }
}

/// Packs a position into one atomic word.
fn pack(position: PhysicalPosition<i32>) -> u64 {
    (u64::from(position.x.cast_unsigned()) << 32) | u64::from(position.y.cast_unsigned())
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
    use quota_contracts::commands::WidgetGrowth;

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

    const AREA: WorkArea = WorkArea {
        top: 0,
        height: 1032,
    };

    const fn placed(y: i32, height: u32) -> Placement {
        Placement {
            outer_y: y,
            outer_height: height,
            inner_height: height,
        }
    }

    #[test]
    fn a_growth_with_room_below_keeps_the_top_edge() {
        let fitted = fit(256.0, WidgetGrowth::Down, placed(24, 172), AREA, 1.0);
        assert!((fitted.height - 256.0).abs() < f64::EPSILON);
        assert_eq!(fitted.direction, WidgetGrowth::Down);
        assert_eq!(fitted.outer_y, 24);
        assert!((fitted.room_above - 24.0).abs() < f64::EPSILON);
        assert!((fitted.room_below - 836.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_growth_without_room_below_opens_upward_and_keeps_the_bottom_edge() {
        let fitted = fit(256.0, WidgetGrowth::Down, placed(850, 172), AREA, 1.0);
        assert!((fitted.height - 256.0).abs() < f64::EPSILON);
        assert_eq!(fitted.direction, WidgetGrowth::Up);
        // The bottom edge stays at 1022, so the tiles keep their place.
        assert_eq!(fitted.outer_y, 766);
        assert!((fitted.room_below - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_downward_close_keeps_the_top_edge() {
        let fitted = fit(172.0, WidgetGrowth::Down, placed(24, 256), AREA, 1.0);
        assert!((fitted.height - 172.0).abs() < f64::EPSILON);
        assert_eq!(fitted.direction, WidgetGrowth::Down);
        assert_eq!(fitted.outer_y, 24);
    }

    #[test]
    fn an_upward_close_moves_the_top_down_to_the_resting_position() {
        let fitted = fit(172.0, WidgetGrowth::Up, placed(766, 256), AREA, 1.0);
        assert!((fitted.height - 172.0).abs() < f64::EPSILON);
        assert_eq!(fitted.direction, WidgetGrowth::Up);
        assert_eq!(fitted.outer_y, 850);
    }

    #[test]
    fn a_taller_switch_upward_stays_upward() {
        let fitted = fit(296.0, WidgetGrowth::Up, placed(766, 256), AREA, 1.0);
        assert!((fitted.height - 296.0).abs() < f64::EPSILON);
        assert_eq!(fitted.direction, WidgetGrowth::Up);
        assert_eq!(fitted.outer_y, 726);
    }

    #[test]
    fn without_room_on_either_side_the_larger_side_wins_capped() {
        let area = WorkArea {
            top: 0,
            height: 200,
        };
        let fitted = fit(256.0, WidgetGrowth::Down, placed(10, 172), area, 1.0);
        assert_eq!(fitted.direction, WidgetGrowth::Down);
        // 10 px of room cannot show the drawer, so it is capped at the
        // header with one row and scrolls the rest.
        assert!((fitted.height - 228.0).abs() < f64::EPSILON);
        assert_eq!(fitted.outer_y, 10);
    }

    #[test]
    fn room_is_measured_in_logical_pixels() {
        let area = WorkArea {
            top: 0,
            height: 2064,
        };
        let fitted = fit(256.0, WidgetGrowth::Down, placed(48, 344), area, 2.0);
        assert!((fitted.height - 256.0).abs() < f64::EPSILON);
        assert_eq!(fitted.direction, WidgetGrowth::Down);
        assert_eq!(fitted.outer_y, 48);
        assert!((fitted.room_below - 836.0).abs() < f64::EPSILON);
    }
}
