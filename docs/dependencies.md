# Dependency record

Resolved and verified on 1 October 2026 (spec section 7.6). Rust versions come
from the committed `Cargo.lock`; each was also checked against the crates.io
sparse index. npm versions come from the npm registry and are committed in
`apps/desktop/package.json` and `apps/desktop/pnpm-lock.yaml`.

## Toolchain

| Tool | Version | Where declared | How verified |
|---|---|---|---|
| Rust compiler | 1.97.1 | `rust-toolchain.toml`, with `clippy` and `rustfmt` components | `rustc --version` on the build machine |
| Tested minimum Rust version (MSRV) | 1.90.0 | `[workspace.package] rust-version` in `Cargo.toml` | Chosen for edition 2024 support. Every crate inherits it with `rust-version.workspace = true`, so the gate in `cargo xtask check-architecture` fails if a member drops it. |
| Cargo resolver | 3 | `[workspace] resolver` | Required by a virtual workspace on edition 2024 |
| pnpm | 11.1.1 | `.github/workflows/ci.yml` | The client that produced `pnpm-lock.yaml` (`lockfileVersion: '9.0'`) |
| Node.js | 24 (LTS line) | `.github/workflows/ci.yml` | Compared against the `nodejs.org/dist/index.json` release list |

## Rust dependencies

Read from `Cargo.lock`. `tauri-plugin-window-state` is declared in the
workspace but is not yet a dependency of any package, so it has no lock entry.

| Crate | Resolved | Requirement in the workspace manifest | How verified |
|---|---|---|---|
| `tauri` | 2.12.1 | `2.12.1` | Lockfile entry plus `crates.io` version listing |
| `tauri-build` | 2.7.1 | `2.7.1` | Same |
| `tauri-plugin-notification` | 2.5.0 | `2.5.0` | Same |
| `tauri-plugin-opener` | 2.7.0 | `2.7.0` | Same |
| `tauri-plugin-positioner` | 2.4.0 | `2.4.0` | Same |
| `tauri-plugin-single-instance` | 2.5.1 | `2.5.1` | Same |
| `tauri-plugin-sql` | 2.5.0 | `2.5.0`, `sqlite` feature | Same, and `crates.io` dependency metadata |
| `tauri-plugin-store` | 2.5.0 | `2.5.0` | Same |
| `tauri-plugin-window-state` | — | `2.5.0` | Declared only; no package depends on it yet |
| `tauri-specta` | 2.0.0-rc.25 | `=2.0.0-rc.25` | Exact pin, see the prerelease exception below |
| `specta` | 2.0.0-rc.25 | `=2.0.0-rc.25` | Same |
| `specta-typescript` | 0.0.12 | `0.0.12` | Lockfile entry |
| `specta-serde` | 0.0.12 | `0.0.12` | Lockfile entry |
| `sqlx` | 0.8.6 | `0.8.6`, `sqlite`/`runtime-tokio`/`macros`/`json`/`uuid`/`time` | Lockfile entry; see the SQLx pin below |
| `libsqlite3-sys` | 0.30.1 | transitive, through `sqlx-sqlite` | Lockfile entry. Compiled against the system SQLite; the linked engine version must be recorded at release (spec 13.6) |
| `chrono` | 0.4.45 | `0.4.45`, `clock`/`serde`/`std` | Lockfile entry |
| `serde` | 1.0.229 | `1.0.229`, `derive` | Lockfile entry |
| `serde_json` | 1.0.151 | `1.0.151` | Lockfile entry |
| `thiserror` | 2.0.21 | `2.0.21` | Lockfile entry. `1.0.69` also resolves as a transitive version for crates that have not moved |
| `tokio` | 1.53.1 | `1.53.1`, minimal feature sets per crate | Lockfile entry |
| `tracing` | 0.1.44 | `0.1.44` | Lockfile entry |
| `uuid` | 1.26.1 | `1.26.1`, `serde`/`v4` | Lockfile entry |
| `proptest` | 1.11.0 | `1.11.0` (dev only) | Lockfile entry |
| `tracing-subscriber` | 0.3.20 | `0.3.20`, `env-filter`/`fmt` | Declared by `quota-desktop` |

`cargo deny check advisories licenses sources` runs weekly against this set. See
`deny.toml` for the approved licence list and the allowed registry.

## npm dependencies

Read from `apps/desktop/package.json`; each version was checked with
`npm view <package> version`.

| Package | Resolved | Note |
|---|---|---|
| `react`, `react-dom` | 19.3.0 | Major 19 is fixed by the specification |
| `@types/react`, `@types/react-dom` | 19.3.0 | Matched to the runtime release |
| `typescript` | 6.0.3 | Newest release `typescript-eslint` 8.71.0 accepts. Its peer range is `>=4.8.4 <6.1.0`, so TypeScript 7.0.2 is outside it |
| `typescript-eslint` | 8.71.0 | Type-aware flat configuration |
| `eslint` | 10.11.0 | |
| `@eslint/js` | 10.0.1 | |
| `eslint-plugin-react-hooks` | 7.1.1 | Includes the React Compiler diagnostics |
| `globals` | 17.12.0 | |
| `prettier` | 3.9.9 | The single formatter |
| `vite` | 8.3.1 | |
| `@vitejs/plugin-react` | 6.1.1 | 6.x and later, so the React Compiler preset comes from `@rolldown/plugin-babel` |
| `@rolldown/plugin-babel` | 0.2.4 | |
| `babel-plugin-react-compiler` | 1.0.0 | Compiler 1.0, stable |
| `@babel/core` | 8.0.6 | |
| `vitest` | 5.0.3 | |
| `jsdom` | 30.1.1 | |
| `@testing-library/react` | 16.3.3 | |
| `@testing-library/dom` | 10.4.2 | |
| `@testing-library/user-event` | 14.6.7 | |
| `@testing-library/jest-dom` | 7.0.1 | |
| `@types/node` | 26.6.3 | Tooling project only |
| `@tauri-apps/api` | 2.12.1 | Matched to the `tauri` 2.12.1 crate |

## Prerelease exception: the Specta v2 release candidate

There is no stable `tauri-specta` 2.x. Tauri Specta's maintained compatibility
table pairs Tauri 2 with Specta 2 and Tauri Specta 2, and the only published
members of that family are release candidates. This build therefore uses the
documented Specta v2 release-candidate stack:

```toml
tauri-specta = "=2.0.0-rc.25"
specta = "=2.0.0-rc.25"
specta-typescript = "0.0.12"
specta-serde = "0.0.12"
```

Both release candidates are pinned with `=`, so no update can be taken by
accident. `specta-typescript` and `specta-serde` are still 0.0.x, which Cargo
already treats as breaking between releases. The four move together: an update
is one change that regenerates the bindings and re-runs
`cargo xtask bindings --check`.

The alternative would be Specta v1, which the specification rejects: it does not
match Tauri 2, and the specification names falling back to v1 as
unacceptable. `docs/exceptions.md` records the owner and the removal condition.

## Why SQLx is 0.8.6 and not 0.9

`tauri-plugin-sql` 2.5.0 declares `sqlx ^0.8` as a non-optional normal
dependency (verified through the crates.io dependency metadata for that exact
release). The plugin's pool type is part of its public Rust API, and the typed
repositories receive that pool. A second, incompatible SQLx version in the same
process would give two `Pool` types that do not convert.

The workspace therefore pins `sqlx 0.8.6`. Moving to the 0.9 line is blocked
until `tauri-plugin-sql` declares it; forcing an override would create the
incompatible copy the specification forbids. This is a compatibility hold with a
known unlock condition, not a preference.

## How to re-verify

```bash
cargo tree -p quota-desktop --locked        # resolved Rust graph
cargo deny check advisories licenses sources
cargo update --workspace --dry-run          # in-range upgrades available now
```

For npm, read `apps/desktop/pnpm-lock.yaml` after `pnpm install --frozen-lockfile`.
`cargo outdated` is not used: no pinned CI action ships it, and the weekly
workflow uses `cargo update --workspace --dry-run` instead.
