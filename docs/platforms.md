# Platforms

Quota builds and passes its checks on Windows and Linux. macOS is not built yet, and the
code is arranged so that adding it means finishing the short list at the end of this page,
not rewriting anything.

## Where platform code lives

Everything outside these places is the same on every operating system:

| Place                                                                                                                                          | What differs                                                                  |
| ---------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| [`src-tauri/src/platform/`](../src-tauri/src/platform/)                                                                                        | Tray, popover window, login item, tray placement, height fit                  |
| [`src-tauri/crates/quota-providers/src/platform_paths.rs`](../src-tauri/crates/quota-providers/src/platform_paths.rs)                          | The user profile and application-data directories credentials are found under |
| [`src-tauri/crates/quota-providers/src/secrets.rs`](../src-tauri/crates/quota-providers/src/secrets.rs)                                        | System credential-store selection and Linux destruction outside Tokio         |
| [`src-tauri/build.rs`](../src-tauri/build.rs), [`src-tauri/crates/quota-persistence/build.rs`](../src-tauri/crates/quota-persistence/build.rs) | The Windows application manifest, on Windows only                             |
| [`src-tauri/tauri.conf.json`](../src-tauri/tauri.conf.json) `bundle`                                                                           | Installer formats and icons                                                   |

Two rules keep it that way:

- **Read the system, do not name it.** Geometry comes from what the system reports, not
  from `cfg(target_os)`. The tray popover is placed by
  [`tray_anchor`](../src-tauri/src/platform/tray_anchor.rs): the tray is on the screen
  edge nearest its icon (the bottom for a default Windows taskbar, the top for the macOS
  menu bar, any side for a moved taskbar or a Linux panel), or, when the system reports no
  icon rectangle (Linux), the side where the work area is inset. The popover sits against
  that edge, centred on the icon, and grows away from it. Its unit tests cover each of
  those desktops, including the macOS menu bar with the Dock.
- **When a branch is unavoidable, keep it in one of the places above**, behind a function
  the rest of the code calls the same way everywhere, as `user_profile` does.

The checks in [CONTRIBUTING](../CONTRIBUTING.md#3-run-the-checks) are plain `cargo` and
`bun` commands with no shell-specific steps, and [`.gitattributes`](../.gitattributes)
checks text out with LF everywhere, so they run unchanged on any of the three systems.

## Already portable

- Tray placement and the popover's height fit, as above.
- Closing a window hides it to the tray; Exit is in the tray menu.
- The settings window is owned by the overview (`"parent": "overview"` in
  `tauri.conf.json`), which keeps it above the overview on Windows, macOS (a child window)
  and Linux (a transient window).
  [`settings_window`](../src-tauri/src/platform/settings_window.rs) opens it centred over
  the overview on the overview's screen, or beside the tray when the overview is hidden,
  and places it again after a move to a screen with another scale. On macOS a child window
  also moves with its parent; confirm that this is wanted.
- Single instance (`tauri-plugin-single-instance`) and launch at login
  (`tauri-plugin-autostart`, a launch agent on macOS) support macOS as they are.
- Saved window geometry (`tauri-plugin-window-state`).
- Provider credentials are found under `HOME` on every system except Windows, where
  `USERPROFILE` comes first. Codex (`~/.codex/auth.json`) and OpenCode Go
  (`~/.local/share/opencode/auth.json`) use the same files on macOS.
- The renderer's font stack falls back to the macOS system font.
- The Windows-only build scripts and `windows_subsystem` attribute do nothing elsewhere.
- The bundle carries `icons/icon.icns`, which a macOS app bundle requires.

## Adding macOS

Each item names where the change goes. None of them is verified yet, because no Mac has
built Quota.

1. **Hide the Dock icon.** Quota is a tray app. In
   [`bootstrap.rs`](../src-tauri/src/bootstrap.rs) setup, call
   `app.set_activation_policy(tauri::ActivationPolicy::Accessory)` under
   `#[cfg(target_os = "macos")]`.
2. **Use a template image in the menu bar.** macOS menu-bar icons are monochrome and
   recoloured by the system. In [`tray.rs`](../src-tauri/src/platform/tray.rs), draw the
   mark in black and call `icon_as_template(true)`. A template image cannot carry the
   coloured attention dot, so the dot needs a monochrome design.
3. **Read Claude Code's credentials from the Keychain.** On macOS, Claude Code keeps its
   sign-in in the login Keychain instead of `~/.claude/.credentials.json`. The reader
   belongs in [`credentials.rs`](../src-tauri/crates/quota-providers/src/credentials.rs)
   beside the file reader, behind the same function. It needs a Keychain dependency, which
   is a separate pull request (see [CONTRIBUTING](../CONTRIBUTING.md)); confirm the
   Keychain item's service name on a Mac first.
4. **Sign and notarise the bundle.** `"targets": "all"` already produces an `.app` and a
   `.dmg`; distribution outside the App Store needs a Developer ID signing identity and
   notarisation in the release workflow.
5. **Run the checks on a `macos-latest` runner** in CI, with the same commands.
6. **Check the inspection guide.** The development inspection socket is
   `${TMPDIR:-/tmp}/tauri-mcp.sock` on macOS as on Linux; confirm it with
   [`inspecting-the-app.md`](inspecting-the-app.md) on the first run.
