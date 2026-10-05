# Dependency record

Resolved and verified on 1 October 2026 (spec section 7.6). Rust versions come from the
committed `Cargo.lock`; each was also checked against the crates.io sparse index. npm
versions come from the npm registry and are committed in
`the repository root/package.json` and `the repository root/bun.lock`.

## Toolchain

[`rust-toolchain.toml`](../src-tauri/rust-toolchain.toml) owns the compiler pin and
required components. [`Cargo.toml`](../src-tauri/Cargo.toml) owns the declared minimum
Rust version and resolver. Workspace members inherit that minimum; inheritance alone does
not demonstrate that the complete dependency graph builds with it. CI uses the pinned
compiler.

[`.bun-version`](../.bun-version) owns the Bun version used by CI. Bun produced the
frontend lockfile and replaces Node.js as the script runner. The package-manager decision
is recorded in
[the exception register](exceptions.md#3-deviations-from-the-stock-tauri-template).

## Rust dependencies

The committed [`Cargo.lock`](../src-tauri/Cargo.lock) owns resolved versions; workspace
and package manifests own requirements and enabled features. Inspect the desktop
dependency graph with `cargo tree -p quota-desktop --locked`. This includes the
window-state, single-instance, and autostart plugins, which the host registers during
bootstrap. `tauri-plugin-autostart` 2.7.0 and its renderer package
`@tauri-apps/plugin-autostart` 2.7.0 were added on 2 October 2026 for launch at login; the
two are pinned to the same release.

`tauri-plugin-updater` 2.13.1 and `semver` 1.0.28 were added on 4 October 2026 for in-app
updates, and are pinned with `=`: an update is code that replaces the installed program,
so none moves without a pull request. They are used from Rust only; no renderer package
(`@tauri-apps/plugin-updater`, `-dialog` or `-process`) was added, because the update flow
runs in the host and no window is granted an updater permission. The pop-up is Quota's own
window, so there is no dialog plugin. `AppHandle::restart` does what the process plugin's
`relaunch` does, so that plugin is not a dependency either. `xtask` gained `base64` 0.22.1
and `minisign-verify` 0.2.5, both already in the graph through the updater, to verify the
update list's signatures with the same library the application uses.

`tauri-plugin-clipboard-manager` 2.4.1 was added on 5 October 2026 so Report a bug can
copy its agent prompt, including from the tray menu, where no window exists to use the
browser clipboard. It brings `arboard` 3.6.1 and, on Linux, `x11rb` 0.13.2 and
`wl-clipboard-rs` 0.9.4. It is used from Rust only: the host writes the text it built
itself, so no renderer package was added and no window is granted a clipboard permission.
On Windows, `arboard` uses `clipboard-win` 5.4.1 and `error-code` 3.4.0, which are under
the Boost Software License (BSL-1.0). `deny.toml` allows that licence for those two crates
only, as named exceptions, rather than adding it to the allow list for the whole graph.

`keyring-core` 1.0.0 and one store crate per system were added on 3 October 2026 for the
credentials Quota owns itself, such as a pasted OpenRouter key:
`windows-native-keyring-store` 1.1.0 (Credential Manager),
`zbus-secret-service-keyring-store` 1.0.1 with `rt-tokio-crypto-rust` (the Secret Service,
pure Rust, no `libdbus`), and `apple-native-keyring-store` 1.0.2 with `keychain`. Each is
a target-specific dependency of `quota-providers`, so a system compiles only its own
store.

`quota-providers` also depends on the workspace's `sqlx` from 3 October 2026, to read the
Cursor app's sign-in from its SQLite state store, read-only. No new package entered the
graph: `sqlx` was already resolved for `quota-persistence`. It also depends on `ring`
0.17.14 directly, to sign Ollama requests with Ollama's Ed25519 key; `ring` was already in
the graph as rustls's crypto provider.

`libsqlite3-sys` is transitive through `sqlx-sqlite`; the linked engine version must be
recorded from the shipping artifact at release (spec 13.6), rather than inferred from the
crate version.

`cargo deny check advisories licenses sources` runs weekly against this set. See
`src-tauri/deny.toml` for the approved licence list and the allowed registry.

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

[`tauri-plugin-mcp`](https://github.com/P3GLEG/tauri-plugin-mcp) is a git dependency;
[`Cargo.toml`](../src-tauri/Cargo.toml) owns its audited revision. The
[inspection guide](inspecting-the-app.md) owns tool behavior, startup, Linux native
prerequisites, and release exclusion rules.

Two licence facts were checked rather than assumed, and both look like a mistake:

- The crate's `Cargo.toml` at the audited commit declares no `license` field, and the
  repository ships no `LICENSE` file (verified with `git ls-tree HEAD` in the pinned
  checkout, and against `main` on GitHub).
- The npm package `tauri-plugin-mcp` 0.3.1 declares `"license": "MIT"`, which is the
  licence the project publishes under.

The recorded clean `cargo deny check advisories licenses sources` result covers the
default graph, which excludes the optional inspection crate; it does not establish a
matched licence for that crate. `deny.toml` still holds `unknown-git = "deny"` and an
empty `allow-git`. A future change that made the crate part of the default graph would
fail the source check, and that is the review point.

## How to re-verify

```bash
cargo tree -p quota-desktop --locked        # resolved Rust graph
cargo deny check advisories licenses sources
cargo update --workspace --dry-run          # in-range upgrades available now
```

For npm, read `the repository root/bun.lock` after `bun install --frozen-lockfile`.
`cargo outdated` is not used: no pinned CI action ships it, and the weekly workflow uses
`cargo update --workspace --dry-run` instead.
