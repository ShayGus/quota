# Quota

Quota is a desktop monitor for AI subscription allowances. It tracks several
accounts at once, including several accounts from the same provider, and shows
how much allowance is left in each quota window.

The application is Tauri 2 with a Rust backend and a React 19 frontend. Rust owns
provider access, polling, account identity, quota normalisation, ranking,
credentials, durable storage, notifications, and native lifecycle. React owns
presentation and transient interaction state. There is no server component.

Accounts are ordered closest to exhaustion first. The order uses the remaining
allowance, not the reset countdown, and a row keeps its position and its click
target while an update is applied.

## Status

The desktop app has production provider adapters for Codex, Claude, and
OpenCode Go. It restores account and monitoring state from SQLite, starts one
shared bounded refresh supervisor, and commits readings before it publishes
snapshots. SQLite stores account, history, backoff, monitoring, notification,
operational privacy, and polling state. The Tauri Store plugin stores
presentation settings.

The React renderer and Tauri host are present. The host compiles, lints, tests,
and launches on this WSL machine. The Tauri system libraries were installed on
2026-10-01. The app opens a window through WSLg; software rendering is used, so
the window is correct but not GPU-accelerated.

Windows installers, the 72-hour ten-account soak, macOS, and Linux packaging
remain unverified.

`docs/acceptance.md` maps the specified acceptance cases to a layer and records
the remaining checks.

## Run the app

One command builds the renderer and opens the desktop window:

```bash
cargo xtask dev
```

It prints every command it runs before running it. Under the hood it is:

```bash
pnpm --dir apps/desktop build
cargo run -p quota-desktop --features custom-protocol
```

`--features custom-protocol` makes the window load the built files in
`apps/desktop/dist` instead of the dev-server URL in `tauri.conf.json`, so no
vite server has to be running. Close the window, or press Ctrl-C, to stop it.

With no accounts connected the overview shows its empty state. To read your real
local logins, start the app the same way and add an account from the settings
window. Quota reads the credential files the owning CLI already wrote, and never
writes, refreshes, or deletes them:

| Provider    | File it reads                                                               | Owner       |
| ----------- | --------------------------------------------------------------------------- | ----------- |
| Codex       | `~/.codex/auth.json`, or `$CODEX_HOME/auth.json`                            | Codex CLI   |
| Claude      | `~/.claude/.credentials.json`, or `$CLAUDE_CONFIG_DIR/.credentials.json`    | Claude Code |
| OpenCode Go | `~/.local/share/opencode/auth.json`, or `$XDG_DATA_HOME/opencode/auth.json` | OpenCode    |

Expect one account row per verified principal, each row carrying every quota
window that provider reported, with a countdown to the next reset. If the file is
missing, the provider reads as not connected and the app still starts. Codex and
Claude do not report an account identity in their usage response, so Quota reads
the identity from the credential file; OpenCode Go reports none at all, so its
row is labelled by credential profile and is known-unverified.

The tray icon is the launcher. A left click shows the overview or raises it if it
is already open, the menu opens settings, refreshes, pauses monitoring, or quits,
and closing the window hides it rather than stopping the monitor.

## Sample data

The `sample-data` feature starts the app with ten fictional accounts and no real
credentials, so the full overview can be reviewed anywhere:

```bash
cargo run -p quota-desktop --features custom-protocol,sample-data
```

Each account reads from the deterministic local fixture adapter and carries a
fixed percentage per window, so the ten rows are stable between runs and need no
network. The feature is not a default. A build without it compiles no fixture
code and reads only the three real providers above.

Sample mode keeps its state away from the real one. The database, the preference
store, and the saved window geometry go under a `sample` child of the app config
directory, which is `~/.config/app.quota.monitor/sample/` on Linux. A normal
build never touches that directory and a sample build never touches its sibling.

## Developer setup from a clean checkout

Prerequisites: `rustup`, and a Node.js toolchain with `pnpm` for the frontend.

On Ubuntu, install the Tauri build packages before `cargo build`:

```bash
sudo apt-get update
sudo apt-get install --no-install-recommends -y \
  build-essential curl file libayatana-appindicator3-dev libdbus-1-dev \
  librsvg2-dev libssl-dev libwebkit2gtk-4.1-dev libxdo-dev patchelf \
  pkg-config wget
```

```bash
git clone https://github.com/ShayGus/quota.git
cd quota

# 1. The pinned compiler, with rustfmt and clippy. rustup reads
#    rust-toolchain.toml and installs the exact version listed there.
rustup show active-toolchain

# 2. The frontend toolchain. The desktop package carries its own lockfile.
cd apps/desktop
pnpm install --frozen-lockfile
cd ../..

# 3. The Rust build.
cargo build --workspace --locked
```

`rust-toolchain.toml` pins Rust 1.97.1. The workspace's `rust-version` is
1.90.0, the tested minimum. `docs/dependencies.md` lists every resolved version
and how it was verified.

## Commands

Run every check from the repository root.

| Command                                                               | What it checks                                                                                                                |
| --------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| `cargo fmt --all -- --check`                                          | Formatting                                                                                                                    |
| `cargo clippy --workspace --all-targets --locked -- -D warnings`      | Lint policy; warning-level findings fail                                                                                      |
| `cargo test --workspace --locked`                                     | Unit and integration tests                                                                                                    |
| `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked` | Documentation links and missing docs                                                                                          |
| `cargo xtask check-architecture`                                      | Workspace inheritance, forbidden dependency edges, file size, raw IPC calls, duplicated IPC models, provider feature defaults |
| `cargo xtask check-release`                                           | Release feature set, licence allow list, workflow action pins, Tauri devtools and content security policy                     |
| `cargo xtask bindings --check`                                        | `apps/desktop/src/generated/bindings.ts` against the Rust IPC layer                                                           |
| `cargo deny check advisories licenses sources`                        | Advisories, licence allow list, allowed sources                                                                               |

The frontend checks run in `apps/desktop`:

```bash
cd apps/desktop
pnpm typecheck && pnpm lint && pnpm format:check && pnpm test && pnpm build
```

`cargo xtask --help` lists the subcommands. An unknown subcommand prints the
usage text and exits non-zero.

## Project layout

```text
quota/
├── Cargo.toml              # Virtual workspace, resolver 3
├── Cargo.lock              # The one committed Rust lockfile
├── rust-toolchain.toml     # Pinned compiler
├── rustfmt.toml, clippy.toml
├── deny.toml               # cargo-deny policy
├── .github/workflows/      # ci.yml, dependencies.yml
├── crates/
│   ├── quota-domain/       # Validated values, quota windows, ranking
│   ├── quota-core/         # Accounts, snapshots, scheduler, alerts, ports
│   ├── quota-contracts/    # Serde + Specta IPC transport DTOs
│   ├── quota-providers/    # Provider clients, decoders, strategies
│   └── quota-persistence/  # Typed Store and SQLite repositories
├── apps/desktop/           # React 19 renderer and the Tauri host
├── xtask/                  # Repository gates
└── docs/                   # Dependency record, acceptance map, architecture,
                            # exceptions, providers
```

`docs/architecture.md` has the dependency graph, the allowed dependency
directions, and the persisted-state ownership table. `docs/providers.md` records
each provider's access method, identity source, credential owner, schema, and
risk. `docs/exceptions.md` is the exception register.

## Documentation

- `docs/architecture.md` — the crate graph, allowed directions, and state ownership.
- `docs/dependencies.md` — resolved versions, the MSRV, and how each was verified.
- `docs/acceptance.md` — the acceptance-case map, including what is not implemented.
- `docs/exceptions.md` — every deviation, with an owner and a removal condition.
- `docs/providers.md` — per-provider connector facts.
- `CONTRIBUTING.md` — the contributor path, binding generation, and fixture tests.

The specification and the HTML wireframe are maintained outside this repository.
They are not copied here.

## Licence

MIT. See `LICENSE`.
