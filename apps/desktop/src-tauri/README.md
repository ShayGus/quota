# quota-desktop

The Tauri desktop host for Quota.

This package is an outer composition layer. It maps domain results onto the
typed IPC contracts in `quota-contracts`, mounts the Tauri Specta registry, and
owns the native window, tray, notification, and secret adapters. It holds no
quota rules of its own; every rule lives in `quota-domain` or `quota-core`.

## Layout

| Path | Responsibility |
|---|---|
| `src/main.rs` | Entrypoint only. No business logic. |
| `src/lib.rs` | Module exports and the run entrypoint. |
| `src/bootstrap.rs` | Plugin registration order, state restoration, supervisor start. |
| `src/state.rs` | The managed state the commands read. |
| `src/ipc/commands.rs` | Thin `#[tauri::command]` handlers. |
| `src/ipc/events.rs` | `tauri_specta::Event` wrappers and the typed emit path. |
| `src/ipc/bindings.rs` | The one registry used both to mount handlers and to export bindings. |
| `src/platform/` | Tray, overview window, notifications, secret storage. |

## Security model

Application commands are permissive by default unless the permission manifest
in `build.rs` assigns them. The manifest names every command, and the two
capability files grant only what each window needs. The settings window cannot
change native geometry; the overview window cannot change preferences.

No capability grants `store:*`, `sql:*`, `http:*`, `fs:*`, or `shell:*`. The
renderer never receives a raw IPC name: handwritten `invoke`, `listen`, and
`emit` calls are forbidden outside generated code, and `cargo xtask
check-architecture` fails the build when one appears.

## Build status

This package does not compile on the machine that produced the scaffold: Tauri
on Linux needs the WebKitGTK development packages and `pkg-config`, and neither
is present, and no package manager access is available. CI compiles it on a
runner that installs them. See `docs/exceptions.md`.