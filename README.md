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

This repository is at the foundation stage. The domain model is written, and the
repository gates, the dependency record, and the CI workflows exist. The
following are **not** implemented yet:

- the provider adapters (`quota-providers` is a stub) and therefore every live
  reading;
- the application services beyond their types (`quota-core` has the scheduler
  and account types, no running supervisor);
- persistence (`quota-persistence` holds only `lib.rs`);
- the desktop host and the renderer;
- Windows installers, the 72-hour ten-account soak, macOS, and Linux packaging.

`docs/acceptance.md` maps every specified acceptance case to a layer and marks
the unimplemented ones with a reason. It is the honest status list.

## The desktop host is not buildable on this machine

`apps/desktop/src-tauri` has **not** been compiled or run here. Building a Tauri
host needs the `webkit2gtk` development packages and `pkg-config`, and this Linux
machine has neither. Do not read a green Rust check as evidence that the desktop
application works. Nothing about the tray, the floating window, native
notifications, or the packaged installer has been observed.

The Rust library crates and `xtask` are ordinary Rust. They are formatted, linted,
and tested on this machine.

## Developer setup from a clean checkout

Prerequisites: `rustup`, and a Node.js toolchain with `pnpm` for the frontend.

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

| Command | What it checks |
|---|---|
| `cargo fmt --all -- --check` | Formatting |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Lint policy; warning-level findings fail |
| `cargo test --workspace --locked` | Unit and integration tests |
| `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked` | Documentation links and missing docs |
| `cargo xtask check-architecture` | Workspace inheritance, forbidden dependency edges, file size, raw IPC calls, duplicated IPC models, provider feature defaults |
| `cargo xtask check-release` | Release feature set, licence allow list, workflow action pins, Tauri devtools and content security policy |
| `cargo xtask bindings --check` | `apps/desktop/src/generated/bindings.ts` against the Rust IPC layer |
| `cargo deny check advisories licenses sources` | Advisories, licence allow list, allowed sources |

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
