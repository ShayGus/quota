//! Build-time configuration: the application-command permission manifest.
//!
//! Tauri commands registered with `invoke_handler` are callable from every
//! window by default unless application-command permissions are configured
//! here. Registering the manifest below is what makes the capability files in
//! `capabilities/` meaningful; without it a narrow capability file alone does
//! not restrict a custom command.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest = tauri_build::AppManifest::new().commands(&[
        "get_snapshot",
        "list_provider_capabilities",
        "get_connection_progress",
        "begin_connection",
        "cancel_connection",
        "confirm_connection",
        "reconnect_account",
        "set_account_enabled",
        "rename_account",
        "disconnect_account",
        "refresh_accounts",
        "set_monitoring_state",
        "set_polling_preferences",
        "update_preferences",
        "set_indicator_style",
        "set_overview_mode",
        "set_overview_always_on_top",
        "fit_overview_to_accounts",
        "fit_overview_height",
        "reset_overview_position",
        "open_provider_usage_page",
        "open_settings_window",
        "clear_local_history",
        "export_sanitized_diagnostics",
    ]);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))?;
    Ok(())
}
