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
  └── platform modules — tray, floating window, notifications, secure storage
```

`xtask` is a build tool. It depends on `quota-domain` and `serde_json` only, and nothing
in the application depends on it.

## Allowed dependency directions

| Crate               | May depend on                                    | Must not depend on                                                              |
| ------------------- | ------------------------------------------------ | ------------------------------------------------------------------------------- |
| `quota-domain`      | `serde`, `specta`, `chrono`, `uuid`, `thiserror` | Tauri, React, Tokio, any network client, any database driver                    |
| `quota-core`        | `quota-domain`, Tokio, `tracing`, `serde`        | Tauri, any concrete provider adapter, any storage plugin, any UI transport type |
| `quota-contracts`   | `quota-domain`, `serde`, `specta`                | business services, credential types, Tauri                                      |
| `quota-providers`   | `quota-core` ports, `quota-domain`               | Tauri, React, the desktop host                                                  |
| `quota-persistence` | `quota-core` ports, `quota-domain`, `sqlx`       | the renderer; Tauri only behind the non-default `tauri-plugins` feature         |
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

| State                                                                                                                                                                                                                                                                   | Durable owner                                                | Access path                                                                       |
| ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------ | --------------------------------------------------------------------------------- |
| Theme, indicator style, density, window mode, topmost preference, launch preference, privacy alias display mode, and other presentation defaults                                                                                                                        | `tauri-plugin-store`                                         | Rust `PreferenceRepository` over one versioned Serde document                     |
| Connections, accounts, pool bindings, latest validated measurements and source timestamps, scoped retry deadlines, alert baselines and outbox, enabled or paused state, effective polling policy, notification policy, history retention, and diagnostic-export privacy | `tauri-plugin-sql` with SQLite                               | Rust repositories and explicit transactions over the plugin-managed database      |
| Eligible native position, size, and maximised state for each persistent window                                                                                                                                                                                          | `tauri-plugin-window-state` behind the mode-aware controller | Native geometry adapter. No competing writes to these fields from Store or SQLite |
| App-owned tokens or other secret material                                                                                                                                                                                                                               | Operating-system secure storage                              | Rust secret broker only. Never a Store value, never a SQL column                  |
| Form drafts, hover, temporary search, open menus, and pending presentation order                                                                                                                                                                                        | React memory                                                 | Not durable                                                                       |

Two consequences follow.

A setting that must commit atomically with monitoring or alert state belongs in SQLite,
not in the Store. The Store plugin writes a JSON file; it is not a transaction log, and
enabling autosave does not make it one.

Suggested SQLite tables are `connections`, `accounts`, `account_pool_bindings`,
`quota_pools`, `quota_windows`, `latest_measurements`, `measurement_history`,
`alert_episodes`, `notification_outbox`, `refresh_backoff`, `monitoring_preferences`, and
versioned migration metadata. Presentation preferences and native geometry are not
duplicated into them.

## Trust boundaries

Rust is the contract source of truth. Commands and events are Rust structs and enums with
Serde and Specta derives; TypeScript definitions are generated from them. The renderer
calls generated typed wrappers only. Raw `invoke(`, `listen(`, and `emit(` are confined to
`src/generated/` and an audited integration wrapper under `src/shared/ipc/`.

An event is transient delivery, not a durable log and not a mutation authority. No backend
listener accepts a renderer-originated event as a command.

## Decision records

Small records, kept here rather than in separate files:

- **Workspace graph.** One root Cargo workspace, resolver 3, one `Cargo.lock`, one
  frontend lockfile. Members inherit edition, Rust version, publish flag, and lint levels;
  `cargo xtask check-architecture` fails a member that does not.
- **Persistence ownership.** The table above. Store holds presentation prefs, SQLite holds
  transactional state, the window-state plugin holds native geometry, and the OS keychain
  holds secrets.
- **IPC trust model.** Rust-owned DTOs, generated bindings, no generic
  `set_state(key, value)` command, no raw URL, path, or SQL argument.
- **Release feature set.** Release artifacts are built from an explicit audited feature
  list, never `--all-features`. The `test-fixtures` feature is non-default;
  `cargo xtask check-release` fails when it enters a default set.
- **Compiler exceptions.** React Compiler 1.0 is integrated through
  `@rolldown/plugin-babel` for `@vitejs/plugin-react` 6.x. See `docs/exceptions.md`.
