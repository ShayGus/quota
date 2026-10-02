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
[`xtask/Cargo.toml`](../xtask/Cargo.toml) owns its dependencies.

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

## Persisted-state ownership

Each kind of durable state has exactly one owner. A typed `Preferences` value returned to
the renderer is assembled from these owners; it is not an instruction to write the same
object into three stores.

| State                                                                                                                                                                                                                                                                   | Durable owner                                          | Access path                                                                   |
| ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------ | ----------------------------------------------------------------------------- |
| Theme, indicator style, density, window mode, topmost preference, launch preference, privacy alias display mode, and other presentation defaults                                                                                                                        | `tauri-plugin-store`                                   | Rust `PreferenceRepository` over one versioned Serde document                 |
| Connections, accounts, pool bindings, latest validated measurements and source timestamps, scoped retry deadlines, alert baselines and outbox, enabled or paused state, effective polling policy, notification policy, history retention, and diagnostic-export privacy | `tauri-plugin-sql` with SQLite                         | Rust repositories and explicit transactions over the plugin-managed database  |
| Eligible native position, size, and maximised state for each persistent window                                                                                                                                                                                          | `tauri-plugin-window-state`                            | Native plugin lifecycle; explicit changes go through the overview controller  |
| Launch-at-login registration (the login item)                                                                                                                                                                                                                           | The operating system, through `tauri-plugin-autostart` | Read back from the system and never mirrored into a store                     |
| Future app-owned tokens or other secret material                                                                                                                                                                                                                        | Operating-system secure storage                        | Reserved boundary; no app-owned authorization or secret broker is implemented |
| Form drafts, hover, toasts, open dialogs, expanded limits, and pending presentation order                                                                                                                                                                               | React memory                                           | Not durable                                                                   |

Two consequences follow.

A setting that must commit atomically with monitoring or alert state belongs in SQLite,
not in the Store. The Store plugin writes a JSON file; it is not a transaction log, and
enabling autosave does not make it one.

The schema is owned by the embedded migrations in
[`crates/quota-persistence/migrations/`](../crates/quota-persistence/migrations/), ordered
by `sqlite::MIGRATIONS`. Presentation preferences and native geometry are not duplicated
into those tables.

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

- **Workspace graph.** One root Cargo workspace, resolver 3, one `Cargo.lock`, one
  frontend lockfile. Members inherit edition, Rust version, publish flag, and lint levels;
  `cargo xtask check-architecture` fails a member that does not.
- **Persistence ownership.** The table above owns the durable-state boundaries.
- **IPC trust model.** Rust-owned DTOs, generated bindings, no generic
  `set_state(key, value)` command, no raw URL, path, or SQL argument.
- **Release feature set.** Release artifacts are built from an explicit audited feature
  list, never `--all-features`. The `test-fixtures` feature is non-default;
  `cargo xtask check-release` fails when it enters a default set.
- **React Compiler integration.** See
  [the dependency record](dependencies.md#npm-dependencies) for the supported integration
  route and its compatibility constraint.
