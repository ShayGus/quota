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
        OverviewMode::Tray if click_is_repeated => TrayActivation::Dismiss,
        OverviewMode::Floating | OverviewMode::Tray => TrayActivation::Raise,
    }
}

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

use crate::platform::window;

/// Installs the native tray icon and its actions.
pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Open Quota", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", "Refresh now", true, None::<&str>)?;
    let pause = MenuItem::with_id(app, "pause", "Pause monitoring", true, None::<&str>)?;
    let resume = MenuItem::with_id(app, "resume", "Resume monitoring", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Quota", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[
            &open, &settings, &separator, &refresh, &pause, &resume, &quit,
        ],
    )?;
    let last_click = Arc::new(Mutex::new(None::<Instant>));
    let click_clock = last_click.clone();
    TrayIconBuilder::with_id("quota")
        .icon(tray_image())
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(move |tray, event| {
            let app = tray.app_handle();
            tauri_plugin_positioner::on_tray_event(app, &event);
            // This event carries the tray geometry a deferred startup anchor was
            // waiting for, so a saved Tray mode is completed here if not before.
            // The lock is only inspected here, never held, so a busy controller
            // defers the anchor to the next tray event instead of stalling the
            // main thread behind an in-flight read.
            let mut deferred = false;
            if let Some(state) = app.try_state::<crate::state::AppState>()
                && let Ok(controller) = state.window.try_lock()
            {
                deferred = controller.state().mode == quota_domain::preferences::OverviewMode::Tray;
            }
            if deferred {
                let _ = window::anchor_to_tray(app);
            }
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let now = Instant::now();
                let repeated = {
                    let mut last = click_clock
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    let repeated = last.is_some_and(|previous| {
                        now.duration_since(previous) <= Duration::from_millis(450)
                    });
                    *last = Some(now);
                    repeated
                };
                activate_from_tray(app, repeated);
            }
        })
        .on_menu_event(handle_menu_event)
        .build(app)?;
    Ok(())
}

fn tray_image() -> tauri::image::Image<'static> {
    const SIZE: usize = 24;
    let side = u32::try_from(SIZE).unwrap_or(0);
    let mut rgba = vec![0; SIZE * SIZE * 4];
    for y in 0..SIZE {
        for x in 0..SIZE {
            let index = (y * SIZE + x) * 4;
            let dx = i32::try_from(x).unwrap_or(0) - 12;
            let dy = i32::try_from(y).unwrap_or(0) - 12;
            if dx * dx + dy * dy <= 121 {
                let bar = (5..=7).contains(&x) && (10..=17).contains(&y)
                    || (10..=12).contains(&x) && (7..=17).contains(&y)
                    || (15..=17).contains(&x) && (4..=17).contains(&y);
                rgba[index..index + 4].copy_from_slice(if bar {
                    &[255, 255, 255, 255]
                } else {
                    &[35, 112, 230, 255]
                });
            }
        }
    }
    tauri::image::Image::new_owned(rgba, side, side)
}

fn activate_from_tray(app: &AppHandle, repeated: bool) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Some(state) = app.try_state::<crate::state::AppState>() else {
            let _ = window::set_visible(&app, "overview", true, true);
            return;
        };
        let state = state.inner().clone();
        let native_visible = window::get(&app, "overview")
            .and_then(|native| {
                native
                    .is_visible()
                    .map_err(|_| window::failed("read_window_visibility"))
            })
            .unwrap_or(false);
        let mut controller = state.window.lock().await;
        let action = activation_for(controller.state().mode, native_visible, repeated);
        let visible = matches!(action, TrayActivation::Show | TrayActivation::Raise);
        if matches!(action, TrayActivation::Quit) {
            app.exit(0);
            return;
        }
        if let Ok(confirmed) = window::set_visible(&app, "overview", visible, visible) {
            let confirmed = controller.set_visible(confirmed);
            window::publish_state(&app, &state.app_instance_id, confirmed);
        }
    });
}

fn handle_menu_event(app: &AppHandle, event: tauri::menu::MenuEvent) {
    let id = event.id;
    match id.as_ref() {
        "open" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Ok(visible) = window::set_visible(&app, "overview", true, true)
                    && let Some(state) = app.try_state::<crate::state::AppState>()
                {
                    let state = state.inner().clone();
                    let confirmed = state.window.lock().await.set_visible(visible);
                    window::publish_state(&app, &state.app_instance_id, confirmed);
                }
            });
        }
        "settings" => {
            let _ = window::set_visible(app, "settings", true, true);
        }
        "refresh" => {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                if let Some(state) = app.try_state::<crate::state::AppState>()
                    && let Err(error) = state
                        .monitor
                        .request_all(crate::monitoring::RefreshReason::UserRequested)
                        .await
                {
                    tracing::warn!(code = error.diagnostic_code(), "tray refresh was refused");
                }
            });
        }
        "pause" | "resume" => {
            let app = app.clone();
            let paused = id.as_ref() == "pause";
            tauri::async_runtime::spawn(async move {
                if let Err(error) =
                    crate::ipc::commands::set_monitoring_state(app.state(), paused).await
                {
                    tracing::warn!(
                        code = error.diagnostic_code(),
                        "tray monitoring update failed"
                    );
                }
            });
        }
        "quit" => app.exit(0),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_click_raises_a_covered_floating_window_instead_of_hiding_it() {
        assert_eq!(
            activation_for(OverviewMode::Floating, true, false),
            TrayActivation::Raise
        );
    }

    #[test]
    fn a_click_shows_a_hidden_window() {
        assert_eq!(
            activation_for(OverviewMode::Floating, false, false),
            TrayActivation::Show
        );
    }

    #[test]
    fn a_repeated_click_toggles_the_tray_view() {
        assert_eq!(
            activation_for(OverviewMode::Tray, true, true),
            TrayActivation::Dismiss
        );
        assert_eq!(
            activation_for(OverviewMode::Tray, true, false),
            TrayActivation::Raise
        );
    }
}
