# Contributing

This file is the single clean-checkout path. Follow it in order; each step verifies the
one before it.

## 1. Set up the checkout

Prerequisites: `rustup`, and [Bun](https://bun.sh) for the frontend.

```bash
git clone https://github.com/ShayGus/quota.git
cd quota
rustup show active-toolchain          # installs the pinned compiler on first use
bun install --frozen-lockfile          # the repository root is the frontend package
cargo build --workspace --locked
```

`rust-toolchain.toml` owns the compiler pin and the required components. Do not edit the
version in a workflow file; it lives in that one file, so it cannot drift.

## 2. Verify the versions

```bash
rustc --version                       # must match rust-toolchain.toml
cargo tree -p quota-desktop --locked  # the resolved Rust graph
cargo deny check advisories licenses sources
cargo update --workspace --dry-run    # what could move inside existing ranges
```

`docs/dependencies.md` points to the version owners and explains the declared minimum Rust
version and the two compatibility holds: the Specta release candidate and SQLx 0.8.

For the frontend, read `the repository root/bun.lock` after
`bun install --frozen-lockfile`. Do not upgrade a dependency as a side effect of another
change: dependency moves are their own pull request, because they change the lockfile, the
advisory report, and sometimes the licence set.

## 3. Run the checks

From the repository root:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
cargo xtask check-architecture
cargo xtask check-release
cargo xtask bindings --check
```

From `the repository root`:

```bash
bun run typecheck && bun run lint && bun run format:check && bun run test && bun run check:release:renderer
```

CI runs exactly these commands. If a command passes locally and fails in CI, the
difference is the environment, not the command.

The same commands run on Windows. Stop a running development app first, because it holds
`target\debug\quota.exe` open. Tauri needs the version 6 common controls, which only the
Windows application manifest selects, so `src-tauri/build.rs` and
`crates/quota-persistence/build.rs` hand `src-tauri/windows-app-manifest.xml` to the
linker for every binary they link, tests included. A crate whose tests start linking Tauri
needs the same build script; without it its test binary exits with
`STATUS_ENTRYPOINT_NOT_FOUND` before any test runs.

The inspection checks are described in
[the inspection guide](docs/inspecting-the-app.md#why-it-cannot-reach-a-release).

### Measured times

Measured on this machine, 32 cores, dev profile, warm tree. These are here so a slow run
can be compared against a known baseline rather than guessed at.

| Command                                                          | Warm   | Cold, desktop package |
| ---------------------------------------------------------------- | ------ | --------------------- |
| `cargo fmt --all -- --check`                                     | 0.5 s  | —                     |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 0.7 s  | 79.6 s                |
| `cargo test --workspace --locked`                                | 8.1 s  | —                     |
| `cargo xtask check-architecture`                                 | 0.2 s  | —                     |
| `cargo xtask check-release`                                      | 0.3 s  | —                     |
| `cargo xtask bindings --check`                                   | 0.2 s  | —                     |
| `bun run typecheck`                                              | 20.8 s | —                     |
| `bun run lint`                                                   | 9.1 s  | —                     |
| `bun run test`                                                   | 3.2 s  | —                     |
| `bun run build`                                                  | 1.4 s  | —                     |

The cold desktop build compiles 645 units and is bound by its dependency tree, not by
linking: the final link alone is 6.1 s of the 79.6 s. Installing `lld` or `mold` would
therefore buy about 5 s and was deliberately not done.

Two profile settings carry the weight, in the root `Cargo.toml`:

- `[profile.dev] debug = "line-tables-only"` and
  `[profile.dev.package."*"] debug = false`. Before, a cold desktop build wrote 5.7 GB of
  artifacts; after, 2.6 GB. Cold build time went from 88.8 s to 79.6 s.
- Incremental compilation is left on. Turning it off was measured, not assumed: an edit to
  `quota-core` followed by a rebuild went from 6.5 s to 13.2 s, which doubles the most
  repeated operation in a fix round.

None of these settings weaken a gate. Backtraces stay line-accurate, every check in the
two blocks above runs unchanged, and the test count is unaffected.

## 4. Generated bindings

Rust is the contract source of truth. Commands and events are Rust structs and enums with
`serde` and `specta::Type` derives, and `tauri-specta` writes `src/generated/bindings.ts`
from them. The file is machine output and is never edited by hand.

`cargo xtask bindings --check` regenerates the file and compares it with the committed
one, so a hand edit, a changed signature, or a renamed event fails the gate. The
regeneration lives in `src-tauri/tests/bindings.rs` because only the host crate can run
the exporter.

When you change a command, an argument type, a return type, an error variant, or an event
payload, change the Rust definition and run the check. If it fails, the test writes the
regenerated file next to the committed one; move that file over it. Do not edit
`src/generated/bindings.ts` itself.

One consequence shapes the types: the pinned exporter refuses 64-bit integers outright,
because `JSON.parse` would silently lose precision. Every counter in the contract is
therefore a `u32`, and monetary minor units are declared as numbers at the type level,
bounded by `MAX_SAFE_MINOR_UNITS`. Do not widen a counter back to 64 bits without changing
how it crosses the wire.

## 5. Fixture tests

Provider work is tested against fixtures, never against a live paid account.

- Fixtures are sanitized recorded schemas. No token, cookie, authorization header, or
  account email belongs in a fixture.
- The `Fixture` provider is compiled only behind the non-default `test-fixtures` Cargo
  feature. A release artifact is built with an explicit audited feature list and never
  `--all-features`.
- `cargo xtask check-architecture` fails if `test-fixtures` enters a default feature set;
  `cargo xtask check-release` fails the same way.
- Tests use an injected clock and isolated temporary storage. They do not change process
  environment variables, and they do not depend on another test's ordering.
- A decoder test exercises decoding and normalisation together, not a mocked normalised
  snapshot.

```bash
cargo test -p quota-domain --locked
cargo test -p quota-providers --locked --features test-fixtures
```

## 6. Native run and build

The Tauri CLI owns running, building and packaging the application. `cargo xtask` runs the
repository gates and never starts the app.

```bash
bun install            # once, from the repository root
bun tauri dev          # Vite on 1420, then the native host
bun run dev            # Vite alone, no native shell
bun tauri build        # release bundles
```

## 7. Rules that the gates enforce

- Every first-party Cargo package inherits edition, Rust version, publish flag, and lint
  policy from the workspace. `cargo xtask check-architecture` fails a member that does
  not.
- `quota-domain` and `quota-core` may not depend on `tauri`, `tauri-plugin-*`, `react`, or
  `sqlx`.
- A first-party production file over 400 non-comment, non-blank lines fails the
  architecture gate. The guideline is 250.
- Raw `invoke`, `listen`, `emit`, and `emitTo` calls, including their generic forms, are
  confined to `src/generated/` and `src/shared/ipc/`.
- Every `uses:` in `.github/workflows/` is pinned to a full 40-character commit SHA. A tag
  or a branch fails `cargo xtask check-release`, and a `run:` step must not interpolate
  event text such as a pull-request title or a branch name.
- [The workspace lint tables](Cargo.toml) own Rust and Clippy lint levels;
  [clippy.toml](clippy.toml) owns thresholds and test-context allowances. CI runs Clippy
  with `-D warnings`, so a warn-level lint fails the build. Rewrite the site first; an
  `#[expect(..., reason = "...")]` is for the case where the rewrite is genuinely worse,
  and that reason belongs in [the exception register](docs/exceptions.md).
- An `#[expect(...)]` that stops firing is a warning, because
  `unfulfilled_lint_expectations = "warn"`. Do not add a blanket `#[allow(...)]`.

## 8. Changes to a command, a schema, or a provider mapping

Include the compatibility and migration effect, and the tests that cover it, in the same
change. A new enum variant breaks every handwritten exhaustive match by design; that is
the point of the typed contracts. Record any new exception in `docs/exceptions.md` with an
owner, a rationale, a scope, a review date, and a removal condition.
