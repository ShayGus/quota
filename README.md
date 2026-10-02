# Quota

Quota is a desktop monitor for AI subscription allowances. It tracks accounts across
Codex, Claude, and OpenCode Go and shows their remaining allowances. Production adapters
use one local credential profile per provider; see
[provider connection limits](docs/providers.md). Same-provider account isolation is
covered by the fictional fixture provider.

The application is Tauri 2 with a Rust backend and a React 19 frontend. Rust owns provider
access, polling, account identity, quota normalisation, ranking, credentials, durable
storage, notifications, and native lifecycle. React owns presentation and transient
interaction state. There is no server component.

Accounts are ordered closest to exhaustion first. The order uses the remaining allowance,
not the reset countdown, and a row keeps its position and its click target while an update
is applied.

## Status

The desktop app has production provider adapters for Codex, Claude, and OpenCode Go. It
restores account and monitoring state from SQLite, starts one shared bounded refresh
supervisor, and commits readings before it publishes snapshots. SQLite stores account,
history, backoff, monitoring, notification, operational privacy, and polling state. The
Tauri Store plugin stores presentation settings.

[Acceptance mapping](docs/acceptance.md) owns the native verification status, evidence
limits, and remaining checks.

## Run the app

Install the frontend dependencies once, then start the application:

```bash
bun install
bun tauri dev
```

`bun tauri dev` starts the Vite dev server on port 1420 itself, compiles the Rust host,
and opens only the overview. Open Settings with the overview's settings button or the tray
menu's Settings action; it stays hidden at launch even if it was open when you last quit.
Launching Quota again brings the running overview forward. Closing a window hides it while
monitoring continues. Use Quit from the tray menu, or press Ctrl-C in the terminal, to
stop the application.

For development-only AI agent inspection, see
[Inspecting the app](docs/inspecting-the-app.md).

To produce a release build:

```bash
bun tauri build
```

The Tauri CLI owns starting, building and packaging the application; `cargo xtask` only
runs the repository gates.

## Connect and refresh accounts

In Settings → Accounts, choose a provider, optionally enter a label, and press Connect.
Quota reads an existing credential from the provider's client; it does not sign you in.
The button stays busy until verification finishes, then shows the result. If a credential
is missing or rejected, follow the provider-specific recovery guidance:

- Codex: run `codex login` in a terminal.
- Claude: run `claude` in a terminal and sign in to Claude Code.
- OpenCode Go: sign in through OpenCode, or set `OPENCODE_API_KEY` in Quota's process
  environment.

After signing in, press Connect again. Dismiss clears a finished result.
[Provider credential discovery](docs/providers.md) owns the supported locations and
overrides, including Windows defaults that work without `HOME`.

Ordinary refreshes, including manual requests, wait for the polling interval and any
backoff deadline. Checks that do not send a request leave the due time unchanged. The
overview and Accounts panel show a formatted countdown to the next eligible read when a
manual refresh is deferred; a short provider retry delay cannot shorten the policy's
minimum interval. With Hide account labels enabled, these notices use the same replacement
labels as the overview rows.

Reconnect immediately verifies the account once, even while monitoring is paused. If
verification fails, subsequent ordinary retries respect the interval, backoff, and
monitoring pause. A countdown names eligibility, not a guarantee of immediate completion.

## Developer setup from a clean checkout

Prerequisites: `rustup`, and [Bun](https://bun.sh) for the frontend.

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

# 2. The frontend toolchain. The repository root is the frontend package, so
#    the lockfile and the install are both here.
bun install --frozen-lockfile

# 3. The Rust build.
cargo build --workspace --locked
```

`rust-toolchain.toml` owns the compiler pin; `Cargo.toml` owns the declared minimum Rust
version. [Dependency record](docs/dependencies.md) explains how to inspect the resolved
versions and compatibility constraints.

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
| `cargo xtask bindings --check`                                        | `src/generated/bindings.ts` against the Rust IPC layer                                                                        |
| `cargo deny check advisories licenses sources`                        | Advisories, licence allow list, allowed sources                                                                               |

`cargo deny` is not a workspace tool, so install it once with `cargo install cargo-deny`
before that line runs locally. CI runs the same check through
`EmbarkStudios/cargo-deny-action`, pinned by commit in `.github/workflows/`.

The frontend check commands are owned by
[CONTRIBUTING.md](CONTRIBUTING.md#3-run-the-checks).

`cargo xtask --help` lists the subcommands. An unknown subcommand prints the usage text
and exits non-zero.

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
├── the repository root/           # React 19 renderer and the Tauri host
├── xtask/                  # Repository gates
└── docs/                   # Dependency record, acceptance map, architecture,
                            # exceptions, providers
```

`docs/architecture.md` has the dependency graph, the allowed dependency directions, and
the persisted-state ownership table. `docs/providers.md` records each provider's access
method, identity source, credential owner, schema, and risk. `docs/exceptions.md` is the
exception register.

## Documentation

- `docs/architecture.md` — the crate graph, allowed directions, and state ownership.
- `docs/dependencies.md` — resolved versions, the MSRV, and how each was verified.
- `docs/acceptance.md` — the acceptance-case map, including what is not implemented.
- `docs/exceptions.md` — every deviation, with an owner and a removal condition.
- `docs/providers.md` — per-provider connector facts.
- `CONTRIBUTING.md` — the contributor path, binding generation, and fixture tests.

The specification and the HTML wireframe are maintained outside this repository. They are
not copied here.

## Licence

MIT. See `LICENSE`.
