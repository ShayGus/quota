//! The tray controller.
//!
//! The tray exists in Rust so it survives renderer closure. Closing a window
//! hides it to the tray; a left click on the icon always opens the app, and the
//! menu offers Settings, Show App, and Exit. Only Exit ends the process.

use quota_domain::account::{ConnectionState, FetchState};
use quota_domain::provider::ProviderId;
use quota_domain::quota::QuotaCategory;
use quota_domain::ranking::{AccountOrder, UnrankedReason};
use quota_domain::snapshot::{AccountSnapshot, AppSnapshot, MonitoringState};

use tauri::menu::{IconMenuItem, Menu, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};

use crate::platform::window;

/// Installs the native tray icon and its actions.
pub fn install(app: &AppHandle) -> tauri::Result<()> {
    let dark = system_is_dark(app);
    let item = |id: &str, text: &str, icon: MenuIcon| {
        IconMenuItem::with_id(
            app,
            id,
            text,
            true,
            Some(menu_icon(icon, dark)),
            None::<&str>,
        )
    };
    let settings = item("settings", "Settings", MenuIcon::Settings)?;
    let show = item("show", "Show App", MenuIcon::Donut)?;
    let exit = item("exit", "Exit", MenuIcon::Power)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&settings, &show, &separator, &exit])?;
    TrayIconBuilder::with_id("quota")
        .icon(tray_image(false, system_is_dark(app)))
        .tooltip("Quota")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_tray_icon_event(move |tray, event| {
            let app = tray.app_handle();
            tauri_plugin_positioner::on_tray_event(app, &event);
            // A saved Tray mode whose startup anchor failed, because the icon
            // was not placed yet, is completed by the next tray event.
            // The lock is only inspected here, never held, so a busy controller
            // defers the anchor to the next tray event instead of stalling the
            // main thread behind an in-flight read.
            let mut deferred = false;
            if let Some(state) = app.try_state::<crate::state::AppState>()
                && let Ok(controller) = state.window.try_lock()
            {
                deferred = controller.state().mode == quota_domain::preferences::OverviewMode::Tray;
            }
            if deferred && let Err(error) = window::anchor_to_tray(app) {
                tracing::warn!(
                    code = error.diagnostic_code(),
                    "overview could not anchor to the tray"
                );
            }
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                activate_from_tray(app);
            }
        })
        .on_menu_event(handle_menu_event)
        .build(app)?;
    Ok(())
}

/// The wireframe's menu icons, drawn from its line-icon paths.
#[derive(Clone, Copy)]
enum MenuIcon {
    Donut,
    Settings,
    Power,
}

/// One menu icon in the ink of the system theme.
fn menu_icon(icon: MenuIcon, dark: bool) -> tauri::image::Image<'static> {
    let bytes: &'static [u8] = match (icon, dark) {
        (MenuIcon::Donut, false) => include_bytes!("../../icons/menu/donut-light.png"),
        (MenuIcon::Donut, true) => include_bytes!("../../icons/menu/donut-dark.png"),
        (MenuIcon::Settings, false) => include_bytes!("../../icons/menu/settings-light.png"),
        (MenuIcon::Settings, true) => include_bytes!("../../icons/menu/settings-dark.png"),
        (MenuIcon::Power, false) => include_bytes!("../../icons/menu/power-light.png"),
        (MenuIcon::Power, true) => include_bytes!("../../icons/menu/power-dark.png"),
    };
    // The bytes are PNGs committed beside the other icons; a decode failure
    // would be a packaging fault, and an empty image keeps the menu usable.
    tauri::image::Image::from_bytes(bytes)
        .unwrap_or_else(|_| tauri::image::Image::new_owned(vec![0; 4], 1, 1))
}

/// The accent the wireframe draws the tray mark in on a light taskbar.
const MARK_LIGHT: [u8; 3] = [0x18, 0x77, 0x5f];
/// The wireframe's dark-theme accent, for a dark taskbar.
const MARK_DARK: [u8; 3] = [0x72, 0xd9, 0xb5];
/// The attention dot's colour.
const DOT: [u8; 3] = [0xbd, 0x70, 0x13];

/// Distance from a point to the segment `a..b`.
fn segment_distance(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let t = (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    (p.0 - (a.0 + t * dx)).hypot(p.1 - (a.1 + t * dy))
}

/// The coverage of one pixel by the Quota mark, `0..=1`, and whether the
/// covering stroke is the faint full ring rather than the arc or the tail.
///
/// The geometry is the wireframe logo's 32-unit drawing at 24 pixels: a faint
/// full ring, an arc from twelve o'clock clockwise to about ten o'clock, and a
/// tail at the lower right.
fn mark_coverage(x: f64, y: f64) -> (f64, bool) {
    const SCALE: f64 = 24.0 / 32.0;
    let (cx, cy, r, half) = (15.0 * SCALE, 15.0 * SCALE, 10.0 * SCALE, 1.8 * SCALE);
    let ring = (half + 0.5 - ((x - cx).hypot(y - cy) - r).abs()).clamp(0.0, 1.0);
    // Clockwise angle from twelve o'clock, in degrees.
    let angle = (x - cx).atan2(cy - y).to_degrees().rem_euclid(360.0);
    let on_arc = angle <= 288.0;
    let tail = (half + 0.5
        - segment_distance(
            (x, y),
            (21.0 * SCALE, 22.0 * SCALE),
            (26.0 * SCALE, 27.0 * SCALE),
        ))
    .clamp(0.0, 1.0);
    let strong = if on_arc { ring.max(tail) } else { tail };
    if strong > 0.0 {
        (strong, false)
    } else {
        (ring, true)
    }
}

/// Whether the system draws dark chrome, so the mark uses the dark accent.
fn system_is_dark(app: &AppHandle) -> bool {
    app.get_webview_window("overview")
        .and_then(|window| window.theme().ok())
        .is_some_and(|theme| theme == tauri::Theme::Dark)
}

/// The tray mark, with the wireframe's attention dot when something needs it.
fn tray_image(attention: bool, dark: bool) -> tauri::image::Image<'static> {
    const SIZE: usize = 24;
    let side = u32::try_from(SIZE).unwrap_or(0);
    let mut rgba = vec![0; SIZE * SIZE * 4];
    for (y, row) in rgba.chunks_exact_mut(SIZE * 4).enumerate() {
        for (x, pixel) in row.chunks_exact_mut(4).enumerate() {
            let (px, py) = (
                f64::from(u32::try_from(x).unwrap_or(0)) + 0.5,
                f64::from(u32::try_from(y).unwrap_or(0)) + 0.5,
            );
            let (coverage, faint) = mark_coverage(px, py);
            let mut alpha = coverage * if faint { 0.2 } else { 1.0 };
            let mut colour = if dark { MARK_DARK } else { MARK_LIGHT };
            if attention {
                let dot = (2.6 - (px - 19.5).hypot(py - 4.5)).clamp(0.0, 1.0);
                if dot > 0.0 {
                    colour = DOT;
                    alpha = alpha.max(dot);
                }
            }
            // The smallest byte whose value covers the alpha, so no float is
            // cast; alpha is clamped to 0..=1 by its construction.
            let a = (0..=u8::MAX)
                .find(|value| f64::from(*value) + 0.5 > alpha * 255.0)
                .unwrap_or(u8::MAX);
            pixel.copy_from_slice(&[colour[0], colour[1], colour[2], a]);
        }
    }
    tauri::image::Image::new_owned(rgba, side, side)
}

/// What the tray says about the accounts: whether to show the attention dot,
/// and the tooltip, in the wireframe's words.
#[must_use]
pub fn tray_summary(snapshot: &AppSnapshot) -> (bool, String) {
    if matches!(snapshot.monitoring_state, MonitoringState::Paused) {
        return (false, "Quota · monitoring paused".to_owned());
    }
    let attention = snapshot
        .accounts
        .iter()
        .filter(|account| account.monitoring_enabled)
        .find_map(|account| account_attention(account).map(|text| (account, text)));
    match attention {
        Some((account, text)) => (
            true,
            format!("Quota · {} · {text}", provider_name(account.provider_id)),
        ),
        None if snapshot.accounts.is_empty() => (false, "Quota · no accounts yet".to_owned()),
        None => (false, "Quota · all accounts current".to_owned()),
    }
}

fn provider_name(provider: ProviderId) -> &'static str {
    match provider {
        ProviderId::Codex => "Codex",
        ProviderId::Claude => "Claude",
        ProviderId::OpenCodeGo => "OpenCode Go",
        ProviderId::Fixture => "Fixture",
    }
}

/// The badge words for an account that needs attention, or `None`.
///
/// The order matches the renderer's status: connection state first, then the
/// fetch state, then the ranking, so the tray and the popover never disagree
/// about which accounts need attention.
fn account_attention(account: &AccountSnapshot) -> Option<String> {
    connection_attention(account.connection_state)
        .or_else(|| fetch_attention(account.fetch_state))
        .map(str::to_owned)
        .or_else(|| order_attention(account))
}

/// A connection state that needs the person, in the renderer's words.
const fn connection_attention(state: ConnectionState) -> Option<&'static str> {
    match state {
        ConnectionState::ReauthenticationRequired => Some("Reconnect"),
        ConnectionState::Disconnected => Some("Disconnected"),
        ConnectionState::Connecting => Some("Connecting"),
        ConnectionState::NeverConnected
        | ConnectionState::Connected
        | ConnectionState::Unsupported => None,
    }
}

/// A fetch state that means the reading is not being kept current.
const fn fetch_attention(state: FetchState) -> Option<&'static str> {
    match state {
        FetchState::Backoff => Some("Rate limited"),
        FetchState::Offline => Some("Offline"),
        FetchState::Error => Some("Check failed"),
        FetchState::Idle | FetchState::Fetching => None,
    }
}

/// The ranking's words: an unranked reason, or a low or exhausted allowance.
fn order_attention(account: &AccountSnapshot) -> Option<String> {
    let order = match &account.order {
        AccountOrder::Unranked(order) => {
            return match order.reason {
                UnrankedReason::Stale => Some("Stale"),
                UnrankedReason::ResetPending => Some("Verifying reset"),
                UnrankedReason::Incomplete => Some("Partially reported"),
                UnrankedReason::NativeUnitsOnly => Some("Native units only"),
                UnrankedReason::NoIncludedAllowance => Some("No included allowance"),
                UnrankedReason::ReconnectRequired => Some("Reconnect"),
                UnrankedReason::UnlimitedOnly
                | UnrankedReason::Disabled
                | UnrankedReason::MonitoringPaused => None,
            }
            .map(str::to_owned);
        }
        AccountOrder::Ranked(order) => order,
    };
    let remaining = order.remaining_percent.value();
    if remaining > 20.0 {
        return None;
    }
    let name = account
        .windows
        .iter()
        .find(|window| window.id == order.controlling_window_id)
        .map_or("Allowance", |window| match window.category {
            QuotaCategory::Session => "5h",
            QuotaCategory::Daily => "Daily",
            QuotaCategory::Weekly => "Weekly",
            QuotaCategory::Monthly => "Monthly",
            QuotaCategory::Custom => "Allowance",
        });
    Some(if remaining <= 0.0 {
        format!("{name} exhausted")
    } else {
        format!("{name} low")
    })
}

/// Shows the current attention state on the tray icon and in its tooltip.
pub fn reflect_snapshot(app: &AppHandle, snapshot: &AppSnapshot) {
    let (attention, tooltip) = tray_summary(snapshot);
    let Some(tray) = app.tray_by_id("quota") else {
        return;
    };
    warn_on_failure(
        tray.set_icon(Some(tray_image(attention, system_is_dark(app)))),
        "icon",
    );
    warn_on_failure(tray.set_tooltip(Some(tooltip)), "tooltip");
}

/// Logs a tray update the platform refused; the previous state stays on screen.
fn warn_on_failure(result: tauri::Result<()>, part: &'static str) {
    if let Err(error) = result {
        tracing::warn!(%error, part, "tray could not be updated");
    }
}

/// Opens the app: shows the overview, or brings it forward when it is already open.
fn activate_from_tray(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let Some(state) = app.try_state::<crate::state::AppState>() else {
            if let Err(error) = window::set_visible(&app, "overview", true, true) {
                tracing::warn!(
                    code = error.diagnostic_code(),
                    "overview could not be shown from the tray"
                );
            }
            return;
        };
        let state = state.inner().clone();
        let mut controller = state.window.lock().await;
        if let Ok(confirmed) = window::set_visible(&app, "overview", true, true) {
            let confirmed = controller.set_visible(confirmed);
            window::publish_state(&app, &state.app_instance_id, confirmed);
        }
    });
}

fn handle_menu_event(app: &AppHandle, event: tauri::menu::MenuEvent) {
    let id = event.id;
    match id.as_ref() {
        "settings" => {
            if let Err(error) = super::settings_window::show(app) {
                tracing::warn!(
                    code = error.diagnostic_code(),
                    "settings could not be shown from the tray"
                );
            }
        }
        "show" => activate_from_tray(app),
        "exit" => app.exit(0),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quota_domain::Percent;
    use quota_domain::ids::{AccountId, AppInstanceId, ConnectionId, QuotaWindowId};
    use quota_domain::ranking::{RankedOrder, UnrankedOrder};
    use quota_domain::snapshot::{PersistenceStatus, SNAPSHOT_SCHEMA_VERSION};

    fn account(id: &str, order: AccountOrder) -> AccountSnapshot {
        AccountSnapshot {
            account_id: AccountId::new(id).unwrap(),
            connection_id: ConnectionId::new("c").unwrap(),
            connection_generation: 1,
            provider_id: ProviderId::Claude,
            nickname: "Personal".into(),
            identity: None,
            connection_ordinal: 1,
            monitoring_enabled: true,
            connection_state: ConnectionState::Connected,
            fetch_state: FetchState::Idle,
            last_attempt_at: None,
            last_success_at: None,
            next_attempt_at: None,
            windows: Vec::new(),
            expected_but_missing_window_ids: Vec::new(),
            order,
        }
    }

    fn ranked(remaining: f64) -> AccountOrder {
        AccountOrder::Ranked(RankedOrder {
            remaining_percent: Percent::new(remaining).unwrap(),
            controlling_window_id: QuotaWindowId::new("w").unwrap(),
            scope_label: "Subscription".into(),
            rule_version: 1,
        })
    }

    fn snapshot(accounts: Vec<AccountSnapshot>, monitoring: MonitoringState) -> AppSnapshot {
        AppSnapshot {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            app_instance_id: AppInstanceId::new("i").unwrap(),
            revision: 1,
            generated_at: chrono::DateTime::UNIX_EPOCH,
            monitoring_state: monitoring,
            persistence_status: PersistenceStatus::Available,
            connections: Vec::new(),
            accounts,
            order: Vec::new(),
        }
    }

    #[test]
    fn a_low_allowance_raises_the_attention_dot_and_names_it() {
        let summary = tray_summary(&snapshot(
            vec![account("a", ranked(64.0)), account("b", ranked(18.0))],
            MonitoringState::Running,
        ));
        assert_eq!(summary, (true, "Quota · Claude · Allowance low".to_owned()));
    }

    #[test]
    fn current_accounts_and_a_paused_view_raise_no_dot() {
        let current = snapshot(vec![account("a", ranked(64.0))], MonitoringState::Running);
        assert!(!tray_summary(&current).0);
        let paused = snapshot(vec![account("a", ranked(5.0))], MonitoringState::Paused);
        assert_eq!(
            tray_summary(&paused),
            (false, "Quota · monitoring paused".to_owned())
        );
    }

    #[test]
    fn a_rate_limited_account_is_not_reported_current() {
        let mut limited = account("a", ranked(64.0));
        limited.fetch_state = FetchState::Backoff;
        assert_eq!(
            tray_summary(&snapshot(vec![limited], MonitoringState::Running)),
            (true, "Quota · Claude · Rate limited".to_owned())
        );
    }

    #[test]
    fn a_disabled_account_never_asks_for_attention() {
        let mut disabled = account(
            "a",
            AccountOrder::Unranked(UnrankedOrder {
                reason: UnrankedReason::Stale,
                rule_version: 1,
            }),
        );
        disabled.monitoring_enabled = false;
        assert!(!tray_summary(&snapshot(vec![disabled], MonitoringState::Running)).0);
    }

    #[test]
    fn the_mark_covers_its_arc_but_not_its_centre() {
        assert!(mark_coverage(11.25, 4.1).0 > 0.9);
        assert!(mark_coverage(11.25, 11.25).0 < f64::EPSILON);
    }
}
