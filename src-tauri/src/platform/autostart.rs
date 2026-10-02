//! Launch at login.
//!
//! The autostart plugin registers Quota as a login item with one extra
//! argument, so a launch at sign-in can be told apart from a person opening the
//! app: it starts quietly in the tray, and when Quota is already running it
//! changes nothing.

/// The argument the login item passes.
const LAUNCHED_AT_LOGIN: &str = "--autostart";

/// The plugin that registers and removes the login item.
#[must_use]
pub fn plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_autostart::Builder::new()
        .args([LAUNCHED_AT_LOGIN])
        .build()
}

/// Whether a process with these arguments was started by the login item.
#[must_use]
pub fn launched_at_login(mut args: impl Iterator<Item = String>) -> bool {
    args.any(|arg| arg == LAUNCHED_AT_LOGIN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_login_item_starts_quietly() {
        let args = |list: &[&str]| list.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
        assert!(launched_at_login(
            args(&["quota.exe", "--autostart"]).into_iter()
        ));
        assert!(!launched_at_login(args(&["quota.exe"]).into_iter()));
    }
}
