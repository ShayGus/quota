//! Build-time configuration: the application-command permission manifest, and
//! the Windows application manifest.
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
        "set_key_limit_shown",
        "create_account_group",
        "set_account_group",
        "rename_account_group",
        "disconnect_account",
        "refresh_accounts",
        "set_monitoring_state",
        "set_polling_preferences",
        "update_preferences",
        "set_indicator_style",
        "set_overview_mode",
        "set_overview_always_on_top",
        "fit_overview_height",
        "fit_widget",
        "set_app_view",
        "open_provider_usage_page",
        "open_settings_window",
        "open_bug_report_issue",
        "copy_bug_report_prompt",
        "clear_local_history",
        "export_sanitized_diagnostics",
        "get_update_prompt",
        "respond_to_update_prompt",
    ]);
    // Tauri embeds its Windows application manifest as a resource linked into
    // the application binary only. Test binaries then load the version 5 common
    // controls, which lack entry points Tauri imports, and exit with
    // STATUS_ENTRYPOINT_NOT_FOUND before any test runs. The same manifest is
    // therefore handed to the linker for every target the package links.
    let windows = tauri_build::WindowsAttributes::new_without_app_manifest();
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(manifest)
            .windows_attributes(windows),
    )?;
    embed_windows_manifest();
    Ok(())
}

/// Embeds `windows-app-manifest.xml` into every binary the MSVC linker links:
/// the application, its tests, and its examples.
fn embed_windows_manifest() {
    let windows = std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "windows");
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").is_ok_and(|env| env == "msvc");
    if !(windows && msvc) {
        return;
    }
    let manifest =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("windows-app-manifest.xml");
    println!("cargo:rerun-if-changed=windows-app-manifest.xml");
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
}
