# Architecture

Quota is a Tauri 2 desktop application. Rust owns provider access, polling, identity,
normalisation, ranking, credentials, durable storage, notifications, and native lifecycle.
React owns presentation and transient interaction state. There is no server component.

## Crate dependency graph

```text
React 19 UI (src)
  └── generated commands + generated typed event listeners
                        │  Tauri Specta IPC
quota-desktop (src-tauri) — composition, thin commands, OS adapters
  ├── quota-contracts — Serde + Specta transport DTOs
  ├── quota-core       — accounts, snapshots, scheduler, alerts, ports
  │      └── quota-domain
  ├── quota-providers  — isolated provider clients, decoders, strategies
  │      └── quota-core ports + quota-domain
  ├── quota-persistence — typed Store and SQLite repositories, migrations
  │      └── quota-core ports + quota-domain
  └── platform modules — tray, popover window, login item
```

Operating-system differences are confined to the platform modules and a few named files;
[Platforms](platforms.md) lists them and what adding macOS takes.

`xtask` is a build tool, and nothing in the application depends on it.
[`src-tauri/xtask/Cargo.toml`](../src-tauri/xtask/Cargo.toml) owns its dependencies.

## Allowed dependency directions

| Crate               | May depend on                                    | Must not depend on                                                              |
| ------------------- | ------------------------------------------------ | ------------------------------------------------------------------------------- |
| `quota-domain`      | `serde`, `specta`, `chrono`, `uuid`, `thiserror` | Tauri, React, Tokio, any network client, any database driver                    |
| `quota-core`        | `quota-domain`, Tokio, `tracing`, `serde`        | Tauri, any concrete provider adapter, any storage plugin, any UI transport type |
| `quota-contracts`   | `quota-domain`, `serde`, `specta`                | business services, credential types, Tauri                                      |
| `quota-providers`   | `quota-core` ports, `quota-domain`               | Tauri, React, the desktop host                                                  |
| `quota-persistence` | `quota-core` ports, `quota-domain`, `sqlx`       | the renderer; runtime Tauri only behind the non-default `tauri-plugins` feature |
| `quota-desktop`     | everything above                                 | — it is the outermost composition layer                                         |

The graph is acyclic. Adapters implement interfaces the core declares; the core never
imports an adapter. `cargo xtask check-architecture` enforces the two edges that matter
most: no `tauri`, `tauri-plugin-*`, `react`, or `sqlx` dependency in `quota-domain` or
`quota-core`, and no package that skips the workspace edition, Rust version, publish flag,
or lint policy.

`quota-persistence` is deliberately an infrastructure package. It may depend on the Tauri
plugins, and that must not pull Tauri into the domain or the core, so the plugin
dependency sits behind the `tauri-plugins` feature, which is off by default.

## Platform seam and ports

Provider operating-system decisions live in one module,
[`src-tauri/crates/quota-providers/src/platform/`](../src-tauri/crates/quota-providers/src/platform/).
It holds four implementations — `windows.rs`, `linux.rs`, `macos.rs`, and `unsupported.rs`
— and one `cfg` selects the one the build runs on.

All four files compile on every host. A Linux CI runner therefore typechecks the Windows
and macOS path logic, and
[`platform_contract.rs`](../src-tauri/crates/quota-providers/tests/platform_contract.rs)
asserts every platform's paths on every runner. The suite builds each implementation
directly, so no case is skipped.

The only system-specific code left is each implementation's `open_credential_store`. On a
host that is not the platform under test it returns `SecretStoreError::Unavailable`, which
is what stops a store crate for a foreign system from ever being called. Everything else
the platform decides is a path rule: the profile variable, the user profile directory, and
the application-data directory.

`src-tauri/src/platform/` is a different thing. It holds Tauri window, tray, and autostart
code, and this seam does not touch it.

Callers await the async `SecretStore` port directly. The
[port contract](../src-tauri/crates/quota-core/src/ports/secrets.rs) owns the requirement
that implementations handle blocking work; the
[system adapter](../src-tauri/crates/quota-providers/src/secrets.rs) documents its
credential-store lifecycle constraints.

## Updates

[`src-tauri/src/updates/`](../src-tauri/src/updates/) keeps an installed copy current. It
is host code, in Rust, on a task of its own, because the windows of a tray application may
be hidden or closed for weeks; no window takes part in the check or the install, and none
is granted an updater permission. The Tauri updater plugin is used from Rust only.

The pop-up is the application's own interface, not an operating-system dialog: a small
frameless `update` window that the host opens when an update is found, drawn by the same
renderer from the settings window's header and the dialog's text and buttons
([`src/features/update/`](../src/features/update/)). It has two commands of its own, one
to read what it shows and one to say which button was pressed. Its full grants, including
reading the snapshot for the saved theme, are owned by the
[update capability](../src-tauri/capabilities/update-capability.json).

Its layers follow the platform seam's style, with one port:

- `policy` is the single function, `may_check_for_updates`, that says whether this build
  may check at all. Only the installed release may: a debug build, the development
  identity, the `sample-data` build and a build with agent inspection never do.
- `schedule` computes when a check is due from monotonic and wall clocks, counting time
  spent asleep without accumulating missed checks. The
  [user-facing schedule](../README.md#updates) includes the pop-up interaction.
- `flow` is one check cycle over the `UpdateHost` port: check, ask, install, relaunch, and
  the in-memory record of the version already put to the person. It never starts a second
  check or a second pop-up, because it runs them one after the other.
- `prompt` holds what the pop-up shows and who waits for its answer. Only an answer that
  fits what is on screen is accepted, and nothing is accepted while an install runs.
- `host` is the port's real implementation over the updater plugin and the pop-up window.
  It names no address and no key; both come from `plugins.updater` in `tauri.conf.json`,
  and `cargo xtask check-release` keeps every other route to them closed.

The tests drive `flow` through a scripted host and a fake clock, so each behaviour is
checked without a network, a window or a restart. The pop-up itself is tested in Chromium
against a faked host, with screenshots in both themes beside the settings window's. The
release side is described in [Releasing](RELEASING.md).

## Persisted-state ownership

Each kind of durable state has exactly one owner. A typed `Preferences` value returned to
the renderer is assembled from these owners; it is not an instruction to write the same
object into three stores.

| State                                                                                                                                                                                                                                                                   | Durable owner                                           | Access path                                                                                                                           |
| ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| Theme, indicator style, window mode, topmost preference, launch preference, privacy alias display mode, and other presentation defaults                                                                                                                                 | `tauri-plugin-store`                                    | Rust `PreferenceRepository` over one versioned Serde document                                                                         |
| Connections, accounts, pool bindings, latest validated measurements and source timestamps, scoped retry deadlines, alert baselines and outbox, enabled or paused state, effective polling policy, notification policy, history retention, and diagnostic-export privacy | `tauri-plugin-sql` with SQLite                          | Rust repositories and explicit transactions over the plugin-managed database                                                          |
| Eligible native position, size, and maximised state for each persistent window                                                                                                                                                                                          | `tauri-plugin-window-state`                             | Native plugin lifecycle; explicit changes go through the overview controller                                                          |
| Launch-at-login registration (the login item)                                                                                                                                                                                                                           | The operating system, through `tauri-plugin-autostart`  | Read back from the system and never mirrored into a store                                                                             |
| Credentials Quota owns itself, such as a pasted API key                                                                                                                                                                                                                 | Operating-system secure storage, through `keyring-core` | Written when the account is added and deleted on disconnect, one entry per connection; never in SQLite, the Store, logs, or snapshots |
| Form drafts, hover, toasts, open dialogs, expanded limits, and pending presentation order                                                                                                                                                                               | React memory                                            | Not durable                                                                                                                           |

Two consequences follow.

A setting that must commit atomically with monitoring or alert state belongs in SQLite,
not in the Store. The Store plugin writes a JSON file; it is not a transaction log, and
enabling autosave does not make it one.

The schema is owned by the embedded migrations in
[`src-tauri/crates/quota-persistence/migrations/`](../src-tauri/crates/quota-persistence/migrations/),
ordered by `sqlite::MIGRATIONS`. Presentation preferences and native geometry are not
duplicated into those tables.

## Trust boundaries

Rust is the contract source of truth. Commands and events are Rust structs and enums with
Serde and Specta derives; TypeScript definitions are generated from them. The renderer
uses generated typed wrappers for application commands. Raw `invoke`, `listen`, `emit`,
and `emitTo` calls, including their generic forms, are confined to `src/generated/` and
audited integration wrappers under `src/shared/ipc/`. One of those wrappers,
`navigation.ts`, carries window-to-window presentation routing (settings asking the
popover to show a surface); it holds no mutation.

An event is transient delivery, not a durable log and not a mutation authority. No backend
listener accepts a renderer-originated event as a command.

Development-only agent inspection has guest event handlers; their window authorization
boundary is documented in
[the inspection guide](inspecting-the-app.md#what-an-agent-gets).

## Decision records

Small records, kept here rather than in separate files:

- **Workspace graph.** One Cargo workspace, rooted at `src-tauri` with every Rust package
  beneath it, resolver 3, one `Cargo.lock`, and one frontend lockfile at the repository
  root. Members inherit edition, Rust version, publish flag, and lint levels;
  `cargo xtask check-architecture` fails a member that does not.
- **Persistence ownership.** The table above owns the durable-state boundaries.
- **IPC trust model.** Rust-owned DTOs, generated bindings, no generic
  `set_state(key, value)` command, no raw URL, path, or SQL argument.
- **Updates are host-side.** The update flow runs in Rust, not in a window, so it keeps
  running while every window is hidden, and the renderer needs no updater permission.
- **Release feature set.** Release artifacts are built from an explicit audited feature
  list, never `--all-features`. The `test-fixtures` feature is non-default;
  `cargo xtask check-release` fails when it enters a default set.
- **React Compiler integration.** See
  [the dependency record](dependencies.md#npm-dependencies) for the supported integration
  route and its compatibility constraint.
