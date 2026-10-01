//! Native overview window commands.
//!
//! Mode, geometry, and topmost are separate controls. No handler here moves,
//! resizes, or hides the window as a side effect of an unrelated change.

use quota_contracts::CommandError;
use quota_contracts::commands::WindowModeChange;
use quota_domain::preferences::OverviewMode;
use quota_domain::provider::ProviderId;
use tauri::State;

use crate::platform::tray::activation_for;
use crate::platform::window::OverviewWindowState;
use crate::state::AppState;

/// Moves the overview between floating and tray anchoring.
#[tauri::command]
#[specta::specta]
pub async fn set_overview_mode(
    state: State<'_, AppState>,
    mode: OverviewMode,
) -> Result<WindowModeChange, CommandError> {
    let mut window = state.window.lock().await;
    let confirmed = window.set_mode(mode);
    Ok(WindowModeChange::Applied(confirmed.mode))
}

/// Changes only the native topmost flag.
///
/// The specification forbids changing geometry, mode, visibility, or
/// monitoring here. The controller enforces that by construction: this handler
/// touches `always_on_top` and nothing else.
#[tauri::command]
#[specta::specta]
pub async fn set_overview_always_on_top(
    state: State<'_, AppState>,
    always_on_top: bool,
) -> Result<OverviewWindowState, CommandError> {
    let mut window = state.window.lock().await;
    Ok(window.set_always_on_top(always_on_top))
}

/// Widens the overview to the account comparison layout.
///
/// This is an explicit user action. No reading, label change, or added account
/// may resize or relocate the window on its own.
#[tauri::command]
#[specta::specta]
pub async fn fit_overview_to_accounts(
    state: State<'_, AppState>,
) -> Result<OverviewWindowState, CommandError> {
    let account_count = state.registry.read().await.len();
    if account_count == 0 {
        return Err(CommandError::ValidationFailed {
            field: "accounts".into(),
            reason: "there are no accounts to fit".into(),
        });
    }
    let mut window = state.window.lock().await;
    Ok(window.set_visible(true))
}

/// Restores a position known to be inside a surviving monitor's work area.
#[tauri::command]
#[specta::specta]
pub async fn reset_overview_position(
    state: State<'_, AppState>,
) -> Result<OverviewWindowState, CommandError> {
    let mut window = state.window.lock().await;
    let restored = window.detach_to_floating();
    Ok(restored)
}

/// Raises or shows the overview in response to a tray activation.
#[tauri::command]
#[specta::specta]
pub async fn activate_overview(
    state: State<'_, AppState>,
    repeated_click: bool,
) -> Result<OverviewWindowState, CommandError> {
    let mut window = state.window.lock().await;
    let activation = activation_for(window.state().mode, window.state().visible, repeated_click);
    let confirmed = match activation {
        crate::platform::tray::TrayActivation::Show => window.set_visible(true),
        crate::platform::tray::TrayActivation::Raise => window,
        crate::platform::tray::TrayActivation::Dismiss => window.set_visible(false),
        crate::platform::tray::TrayActivation::Hide => window.set_visible(false),
        crate::platform::tray::TrayActivation::Quit => return Err(CommandError::Cancelled),
    };
    Ok(confirmed)
}

/// Opens one allowlisted provider usage page in the external browser.
///
/// The renderer cannot supply a URL. It names a provider, and the host owns the
/// address. An unknown provider is refused rather than guessed.
#[tauri::command]
#[specta::specta]
pub fn open_provider_usage_page(
    app: tauri::AppHandle,
    provider_id: ProviderId,
) -> Result<(), CommandError> {
    let Some(url) = crate::bootstrap_helpers::usage_page_of(provider_id) else {
        return Err(CommandError::UnsupportedProvider { provider_id });
    };
    crate::bootstrap_helpers::open_external(&app, url)
}
