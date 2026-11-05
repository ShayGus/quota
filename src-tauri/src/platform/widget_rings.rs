//! The rings that spread from the mini widget when it is pointed out. They are
//! a window of their own that clicks pass through, so the widget itself never
//! moves or resizes, and that page has no capability, so it can call nothing.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use quota_contracts::CommandError;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize};

use super::widget::{LABEL, on_wayland};
use super::window::failed;

/// The window the rings around the widget are drawn in.
pub const HALO_LABEL: &str = "widget-halo";

/// How far the rings reach beyond the widget, in logical pixels, as
/// `widget-halo.html` draws them.
const HALO_REACH: f64 = 46.0;

/// How long the rings last, as `widget-halo.html` animates them.
const HALO_LASTS: Duration = Duration::from_millis(2900);

/// Counts the rings drawn, so an older pair's timer leaves newer rings alone.
static HALOS: AtomicU64 = AtomicU64::new(0);

/// Draws the rings around the widget for a moment. They are a window of their
/// own that clicks pass through, so nothing under them stops working.
///
/// # Errors
/// Returns the native failure when the window refuses.
pub fn draw_halo(app: &AppHandle) -> Result<(), CommandError> {
    if on_wayland() {
        return Ok(());
    }
    let (position, size, reach) = halo_bounds(app)?;
    if let Some(old) = app.get_webview_window(HALO_LABEL) {
        old.destroy().map_err(|_| failed("clear_widget_halo"))?;
    }
    let halo = new_halo_window(app)?;
    if place_halo(&halo, position, size, reach).is_err() {
        clear_halo(&halo);
        return Err(failed("place_widget_halo"));
    }
    let this_halo = HALOS.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(HALO_LASTS).await;
        if HALOS.load(Ordering::SeqCst) == this_halo {
            clear_halo(&halo);
        }
    });
    Ok(())
}

/// Puts the rings around the widget, lets clicks through them, and shows them.
fn place_halo(
    halo: &tauri::WebviewWindow,
    position: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
    reach: u32,
) -> tauri::Result<()> {
    let offset = i32::try_from(reach).unwrap_or(0);
    // In this order: a window moved to a screen of another scale is rescaled by
    // the system, so a size set first would come out wrong there.
    halo.set_position(PhysicalPosition::new(
        position.x.saturating_sub(offset),
        position.y.saturating_sub(offset),
    ))?;
    halo.set_size(PhysicalSize::new(
        size.width.saturating_add(reach.saturating_mul(2)),
        size.height.saturating_add(reach.saturating_mul(2)),
    ))?;
    halo.set_ignore_cursor_events(true)?;
    halo.show()
}

/// Where the widget is, how big, and how far the rings reach from it.
type HaloBounds = (PhysicalPosition<i32>, PhysicalSize<u32>, u32);

/// Where the widget is, how big, and how far the rings reach from it, all in
/// physical pixels.
fn halo_bounds(app: &AppHandle) -> Result<HaloBounds, CommandError> {
    let native = super::window::get(app, LABEL)?;
    let position = native
        .outer_position()
        .map_err(|_| failed("read_window_position"))?;
    let size = native
        .outer_size()
        .map_err(|_| failed("read_widget_size"))?;
    let scale = native.scale_factor().map_err(|_| failed("read_scale"))?;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the reach is a small positive number of pixels"
    )]
    let reach = (HALO_REACH * scale).round() as u32;
    Ok((position, size, reach))
}

/// The hidden window the rings are drawn in. No capability names it, so its
/// page can call nothing in the app.
fn new_halo_window(app: &AppHandle) -> Result<tauri::WebviewWindow, CommandError> {
    tauri::WebviewWindowBuilder::new(
        app,
        HALO_LABEL,
        tauri::WebviewUrl::App("widget-halo.html".into()),
    )
    .title("Quota widget rings")
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .resizable(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .focused(false)
    .visible(false)
    .build()
    .map_err(|_| failed("create_widget_halo"))
}

/// Takes the rings away.
fn clear_halo(halo: &tauri::WebviewWindow) {
    if halo.destroy().is_err() {
        tracing::warn!("the rings around the widget could not be cleared");
    }
}
