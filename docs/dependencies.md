# Dependency record

Resolved and verified on 1 October 2026 (spec section 7.6). Rust versions come from the
committed `Cargo.lock`; each was also checked against the crates.io sparse index. npm
versions come from the npm registry and are committed in
`the repository root/package.json` and `the repository root/bun.lock`.

## Toolchain

[`rust-toolchain.toml`](../rust-toolchain.toml) owns the compiler pin and required
components. [`Cargo.toml`](../Cargo.toml) owns the declared minimum Rust version and
resolver. Workspace members inherit that minimum; inheritance alone does not demonstrate
that the complete dependency graph builds with it. CI uses the pinned compiler.

[`.bun-version`](../.bun-version) owns the Bun version used by CI. Bun produced the
frontend lockfile and replaces Node.js as the script runner. The package-manager decision
is recorded in
[the exception register](exceptions.md#3-deviations-from-the-stock-tauri-template).

## Rust dependencies

The committed [`Cargo.lock`](../Cargo.lock) owns resolved versions; workspace and package
manifests own requirements and enabled features. Inspect the desktop dependency graph with
`cargo tree -p quota-desktop --locked`. This includes the window-state plugin, which the
host registers during bootstrap.

`libsqlite3-sys` is transitive through `sqlx-sqlite`; the linked engine version must be
recorded from the shipping artifact at release (spec 13.6), rather than inferred from the
crate version.

`cargo deny check advisories licenses sources` runs weekly against this set. See
`deny.toml` for the approved licence list and the allowed registry.

## npm dependencies

[`package.json`](../package.json) owns direct dependency pins; [`bun.lock`](../bun.lock)
owns the resolved frontend graph.

React 19 is fixed by the specification. TypeScript must stay inside the installed
`typescript-eslint` peer range. The renderer's React Compiler runs through
`@vitejs/plugin-react`'s `compiler` option and `oxc-transform-react`, as configured in
[`vite.config.ts`](../vite.config.ts). The Babel route was rejected because
`@rolldown/plugin-babel` 0.2.4's declarations failed with `skipLibCheck: false` against
both evaluated Babel type stacks. It is not a dependency of this application.

## Prerelease exception: the Specta v2 release candidate

There is no stable `tauri-specta` 2.x. Tauri Specta's maintained compatibility table pairs
Tauri 2 with Specta 2 and Tauri Specta 2, and the only published members of that family
are release candidates. This build therefore uses the documented Specta v2
release-candidate stack:

```toml
tauri-specta = "=2.0.0-rc.25"
specta = "=2.0.0-rc.25"
specta-typescript = "0.0.12"
specta-serde = "0.0.12"
```

Both release candidates are pinned with `=`, so no update can be taken by accident.
`specta-typescript` and `specta-serde` are still 0.0.x, which Cargo already treats as
breaking between releases. The four move together: an update is one change that
regenerates the bindings and re-runs `cargo xtask bindings --check`.

The alternative would be Specta v1, which the specification rejects: it does not match
Tauri 2, and the specification names falling back to v1 as unacceptable.
`docs/exceptions.md` records the owner and the removal condition.

## Why SQLx is 0.8.6 and not 0.9

`tauri-plugin-sql` 2.5.0 declares `sqlx ^0.8` as a non-optional normal dependency
(verified through the crates.io dependency metadata for that exact release). The plugin's
pool type is part of its public Rust API, and the typed repositories receive that pool. A
second, incompatible SQLx version in the same process would give two `Pool` types that do
not convert.

The workspace therefore pins `sqlx 0.8.6`. Moving to the 0.9 line is blocked until
`tauri-plugin-sql` declares it; forcing an override would create the incompatible copy the
specification forbids. This is a compatibility hold with a known unlock condition, not a
preference.

## Development-only agent inspection

[`tauri-plugin-mcp`](https://github.com/P3GLEG/tauri-plugin-mcp) lets an AI agent read the
running app: screenshots, the DOM, the console log, and the IPC calls the renderer makes.
It is not published to crates.io, so it is a git dependency pinned to the full commit
`c7d271a06469bdf4744bfdeadca7458a1f3d02e5` in the root manifest.
`cargo xtask check-release` fails if that ever becomes a branch or a tag.

It is an `optional` dependency behind the non-default `agent-inspection` feature, and
`src-tauri/src/bootstrap.rs` registers it only when both that feature and
`debug_assertions` are set. `docs/inspecting-the-app.md` explains the workflow, the three
stops that keep it out of a release artifact, and the two native libraries a Linux debug
build needs.

Two licence facts were checked rather than assumed, and both look like a mistake:

- The crate's `Cargo.toml` at that commit declares no `license` field, and the repository
  ships no `LICENSE` file (verified with `git ls-tree HEAD` in the pinned checkout, and
  against `main` on GitHub).
- The npm package `tauri-plugin-mcp` 0.3.1 declares `"license": "MIT"`, which is the
  licence the project publishes under.

`cargo deny check advisories licenses sources` reports clean. It does so because the crate
is an unpublished git source and cargo-deny treats an unpublished crate as private and
exempts it from licence resolution, not because a licence was matched. `deny.toml`
therefore still holds `unknown-git = "deny"` and an empty `allow-git`: the crate never
enters the graph cargo-deny inspects, because no default feature selects it. A future
change that made it part of the default graph would fail the source check, and that is the
review point.

## How to re-verify

```bash
cargo tree -p quota-desktop --locked        # resolved Rust graph
cargo deny check advisories licenses sources
cargo update --workspace --dry-run          # in-range upgrades available now
```

For npm, read `the repository root/bun.lock` after `bun install --frozen-lockfile`.
`cargo outdated` is not used: no pinned CI action ships it, and the weekly workflow uses
`cargo update --workspace --dry-run` instead.
