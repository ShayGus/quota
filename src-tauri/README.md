# quota-desktop

The Tauri desktop host for Quota.

This package is an outer composition layer. It maps domain results onto the typed IPC
contracts in `quota-contracts`, mounts the Tauri Specta registry, and owns the native
window, tray, and login-item adapters, and registers the notification, single-instance,
autostart and updater plugins; the updater runs from Rust (`src/updates/`), and no window
is granted its commands. Its pop-up is a window of Quota's own. It holds no quota rules of
its own; every rule lives in `quota-domain` or `quota-core`.

## Layout

| Path                  | Responsibility                                                        |
| --------------------- | --------------------------------------------------------------------- |
| `src/main.rs`         | Entrypoint only. No business logic.                                   |
| `src/lib.rs`          | Module exports and the run entrypoint.                                |
| `src/bootstrap.rs`    | Plugin registration order, state restoration, supervisor start.       |
| `src/state.rs`        | The managed state the commands read.                                  |
| `src/updates/`        | The update check, its schedule, and the pop-up; see the architecture. |
| `src/ipc/commands.rs` | Thin `#[tauri::command]` handlers.                                    |
| `src/ipc/events.rs`   | `tauri_specta::Event` wrappers and the typed emit path.               |
| `src/ipc/bindings.rs` | The one registry used both to mount handlers and to export bindings.  |
| `src/platform/`       | Tray, popover window, and launch-at-login (`autostart.rs`) adapters.  |

## Lifecycle

[The tray icon](../docs/user-guide.md#the-tray-icon) and
[Updates](../docs/user-guide.md#updates) own the window and tray behaviour. Launch at
login registers a login item with an `--autostart` argument, so a launch at sign-in starts
quietly in the tray and a login launch that finds Quota already running changes nothing.

## Security model

Application commands are permissive by default unless the permission manifest in
`build.rs` assigns them. The manifest names every command, and the shipping
[capability files](capabilities/) own the command grants for each window; consult their
permission lists for allowed operations.

No capability grants `store:*`, `sql:*`, `http:*`, `fs:*`, or `shell:*`. The raw IPC call
boundary is owned by
[the architecture document](../docs/architecture.md#trust-boundaries). For
development-only inspection permissions, see
[the inspection guide](../docs/inspecting-the-app.md#what-an-agent-gets).

## Build status

[Acceptance mapping](../docs/acceptance.md) owns the local build and launch evidence and
its native verification limits. [README.md](../CONTRIBUTING.md#1-set-up-the-checkout)
lists the native build prerequisites.
