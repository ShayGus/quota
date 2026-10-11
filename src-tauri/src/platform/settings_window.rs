//! Where the settings window opens.
//!
//! Settings opens over the overview, on the overview's screen, so it covers
//! the window it configures. The settings window is owned by the overview (its
//! `parent` in `tauri.conf.json`), so it also stays above it in z-order. When
//! the overview is hidden, settings opens where the tray popover would, beside
//! the tray.
//!
//! The window is created when it is opened, not at launch, and closing it
//! destroys it, so its webview does not hold memory while settings is unused.
//! Its configuration stays in `tauri.conf.json`, marked `create: false`.

use quota_contracts::CommandError;
use tauri::{AppHandle, Manager, PhysicalPosition, WebviewUrl, WebviewWindow};

use super::tray_anchor::{self, Rect};
use super::window::{self, failed};

/// The settings window's label in `tauri.conf.json`.
pub const LABEL: &str = "settings";

/// The gap kept between settings and the edge of the work area.
const MARGIN: f64 = 12.0;

/// Where settings goes when the overview is on screen: centred over the
/// overview, then moved back inside the work area, `margin` from its edges.
/// A window larger than the work area keeps its top-left corner inside.
#[must_use]
pub fn over_window(work_area: Rect, overview: Rect, size: (f64, f64), margin: f64) -> (f64, f64) {
    let (width, height) = size;
    let x = overview.x + (overview.width - width) / 2.0;
    let y = overview.y + (overview.height - height) / 2.0;
    let right = work_area.x + work_area.width - width - margin;
    let bottom = work_area.y + work_area.height - height - margin;
    (
        x.min(right).max(work_area.x + margin),
        y.min(bottom).max(work_area.y + margin),
    )
}

/// Opens settings at `route`, a `#/settings` address, or where it already is
/// when `route` is `None`, then places, shows and focuses it over the
/// overview, or beside the tray when the overview is hidden.
///
/// A window moved onto a screen with another scale is resized by the system,
/// which keeps its top-left corner and so moves its centre. Settings is
/// therefore placed twice: once to reach the target screen, then again with
/// the size it has there.
pub fn show(app: &AppHandle, route: Option<&str>) -> Result<(), CommandError> {
    let settings = match app.get_webview_window(LABEL) {
        Some(settings) => {
            if let Some(route) = route {
                navigate(&settings, route)?;
            }
            settings
        }
        None => create(app, route.unwrap_or("#/settings"))?,
    };
    let overview = window::get(app, "overview")?;
    for _ in 0..2 {
        let size = settings
            .outer_size()
            .map_err(|_| failed("read_window_size"))?;
        let (x, y) = target(
            app,
            &overview,
            (f64::from(size.width), f64::from(size.height)),
        )?;
        settings
            .set_position(PhysicalPosition::new(x, y))
            .map_err(|_| failed("place_settings_window"))?;
    }
    settings
        .show()
        .map_err(|_| failed("show_settings_window"))?;
    settings
        .set_focus()
        .map_err(|_| failed("focus_settings_window"))
}

/// Creates the hidden settings window from its configuration, at `route`.
///
/// When it is destroyed, any connection its wizard left open is cancelled,
/// because the page's own cleanup does not run in a destroyed webview.
fn create(app: &AppHandle, route: &str) -> Result<WebviewWindow, CommandError> {
    let mut config = app
        .config()
        .app
        .windows
        .iter()
        .find(|config| config.label == LABEL)
        .cloned()
        .ok_or_else(|| failed("find_settings_configuration"))?;
    config.url = WebviewUrl::App(format!("index.html{route}").into());
    let settings = tauri::WebviewWindowBuilder::from_config(app, &config)
        .and_then(tauri::WebviewWindowBuilder::build)
        .map_err(|_| failed("create_settings_window"))?;
    let handle = app.clone();
    settings.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::Destroyed) {
            cancel_left_connections(&handle);
        }
    });
    Ok(settings)
}

/// Moves an open settings window to `route`.
///
/// The route is the host's own, its one renderer-given part percent-encoded,
/// and enters the script only as a JSON string literal.
fn navigate(settings: &WebviewWindow, route: &str) -> Result<(), CommandError> {
    let literal = serde_json::to_string(route).map_err(|_| failed("navigate_settings"))?;
    settings
        .eval(format!("window.location.hash = {literal};"))
        .map_err(|_| failed("navigate_settings"))
}

/// Discards the connection attempts a closed settings window left behind.
fn cancel_left_connections(app: &AppHandle) {
    let Some(state) = app.try_state::<crate::state::AppState>() else {
        return;
    };
    let monitor = state.monitor.clone();
    tauri::async_runtime::spawn(async move {
        monitor.cancel_all_connections().await;
    });
}

/// Where a settings window of `size` physical pixels goes.
fn target(
    app: &AppHandle,
    overview: &tauri::WebviewWindow,
    size: (f64, f64),
) -> Result<(f64, f64), CommandError> {
    let visible = overview
        .is_visible()
        .map_err(|_| failed("read_window_visibility"))?;
    if !visible {
        let layout = window::tray_layout(app, overview)?;
        return Ok(tray_anchor::place(
            layout.work_area,
            layout.edge,
            layout.icon,
            size,
            MARGIN * layout.scale,
        ));
    }
    let monitor = overview
        .current_monitor()
        .map_err(|_| failed("read_current_monitor"))?
        .or(overview
            .primary_monitor()
            .map_err(|_| failed("read_primary_monitor"))?)
        .ok_or_else(|| failed("find_display"))?;
    let area = monitor.work_area();
    let position = overview
        .outer_position()
        .map_err(|_| failed("read_window_position"))?;
    let outer = overview
        .outer_size()
        .map_err(|_| failed("read_window_size"))?;
    Ok(over_window(
        rect(area.position, area.size),
        rect(position, outer),
        size,
        MARGIN * monitor.scale_factor(),
    ))
}

/// A physical rectangle from a position and size.
fn rect(position: PhysicalPosition<i32>, size: tauri::PhysicalSize<u32>) -> Rect {
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

    #[track_caller]
    fn assert_at(actual: (f64, f64), expected: (f64, f64)) {
        assert!(
            (actual.0 - expected.0).abs() < 1e-3 && (actual.1 - expected.1).abs() < 1e-3,
            "placed at {actual:?}, expected {expected:?}"
        );
    }

    /// Settings is created when opened, in every build's configuration.
    #[test]
    fn no_configuration_creates_settings_at_launch() {
        for (name, text) in [
            ("tauri.conf.json", include_str!("../../tauri.conf.json")),
            (
                "tauri.dev.conf.json",
                include_str!("../../tauri.dev.conf.json"),
            ),
        ] {
            let config: serde_json::Value = serde_json::from_str(text).expect("the configuration");
            let windows = config["app"]["windows"].as_array().expect("the windows");
            let settings = windows
                .iter()
                .find(|window| window["label"] == LABEL)
                .expect("the settings window");
            assert_eq!(
                settings["create"], false,
                "{name} creates settings at launch"
            );
        }
    }

    const WORK_AREA: Rect = rect(0.0, 0.0, 1920.0, 1032.0);
    const SETTINGS: (f64, f64) = (780.0, 600.0);

    #[test]
    fn settings_opens_centred_over_the_overview() {
        let overview = rect(600.0, 200.0, 440.0, 600.0);
        assert_at(
            over_window(WORK_AREA, overview, SETTINGS, 12.0),
            (430.0, 200.0),
        );
    }

    #[test]
    fn settings_stays_inside_the_work_area_beside_a_corner_overview() {
        let overview = rect(1468.0, 420.0, 440.0, 600.0);
        assert_at(
            over_window(WORK_AREA, overview, SETTINGS, 12.0),
            (1128.0, 420.0),
        );
        let top_left = rect(0.0, 0.0, 440.0, 300.0);
        assert_at(
            over_window(WORK_AREA, top_left, SETTINGS, 12.0),
            (12.0, 12.0),
        );
    }

    #[test]
    fn settings_follows_the_overview_to_another_screen() {
        let second = rect(1920.0, 0.0, 2560.0, 1380.0);
        let overview = rect(3000.0, 500.0, 440.0, 400.0);
        assert_at(
            over_window(second, overview, SETTINGS, 12.0),
            (2830.0, 400.0),
        );
    }
}
