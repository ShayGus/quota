# Contributing

This file is the single clean-checkout path. Follow it in order; each step
verifies the one before it.

## 1. Set up the checkout

Prerequisites: `rustup`, and a Node.js toolchain with `pnpm`.

```bash
git clone https://github.com/ShayGus/quota.git
cd quota
rustup show active-toolchain          # installs the pinned compiler on first use
cd apps/desktop && pnpm install --frozen-lockfile && cd ../..
cargo build --workspace --locked
```

`rust-toolchain.toml` pins Rust 1.97.1 with the `clippy` and `rustfmt`
components. Do not edit the version in a workflow file; it lives in that one
file, so it cannot drift.

## 2. Verify the versions

```bash
rustc --version                       # must match rust-toolchain.toml
cargo tree -p quota-desktop --locked  # the resolved Rust graph
cargo deny check advisories licenses sources
cargo update --workspace --dry-run    # what could move inside existing ranges
```

Compare what you see with `docs/dependencies.md`. That file records the resolved
Rust and npm versions, the MSRV, the two compatibility holds (the Specta release
candidate and the SQLx 0.8 line), and how each version was verified.

For the frontend, read `apps/desktop/pnpm-lock.yaml` after
`pnpm install --frozen-lockfile`. Do not upgrade a dependency as a side effect of
another change: dependency moves are their own pull request, because they change
the lockfile, the advisory report, and sometimes the licence set.

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

From `apps/desktop`:

```bash
pnpm typecheck && pnpm lint && pnpm format:check && pnpm test && pnpm build
```

CI runs exactly these commands. If a command passes locally and fails in CI, the
difference is the environment, not the command.

## 4. Generated bindings

Rust is the contract source of truth. Commands and events are Rust structs and
enums with `serde` and `specta::Type` derives; the renderer's
`apps/desktop/src/generated/bindings.ts` mirrors them.

`cargo xtask bindings --check` compares the two. It reads the
`#[tauri::command]` function names and the event struct names under
`apps/desktop/src-tauri/src/ipc/` and asserts that each appears in
`bindings.ts`, and that `bindings.ts` invents no command or event name the Rust
side does not define.

**The check does not run `tauri-specta`.** `tauri-specta` needs `webkit2gtk`
development packages and `pkg-config`, and this build machine has neither, so
the exporter cannot be compiled here. The file is therefore a checked-in mirror,
and the check is the gate that keeps it honest. `docs/exceptions.md` records this
deviation, its owner, and the condition that removes it.

When you change a command, an argument type, a return type, an error variant, or
an event payload: update the Rust definition, update `bindings.ts` to match, and
run `cargo xtask bindings --check`. Do not edit `bindings.ts` alone.

## 5. Fixture tests

Provider work is tested against fixtures, never against a live paid account.

- Fixtures are sanitized recorded schemas. No token, cookie, authorization
  header, or account email belongs in a fixture.
- The `Fixture` provider is compiled only behind the non-default
  `test-fixtures` Cargo feature. A release artifact is built with an explicit
  audited feature list and never `--all-features`.
- `cargo xtask check-architecture` fails if `test-fixtures` enters a default
  feature set; `cargo xtask check-release` fails the same way.
- Tests use an injected clock and isolated temporary storage. They do not change
  process environment variables, and they do not depend on another test's
  ordering.
- A decoder test exercises decoding and normalisation together, not a mocked
  normalised snapshot.

```bash
cargo test -p quota-domain --locked
cargo test -p quota-providers --locked --features test-fixtures
```

## 6. Native run and build

```bash
cd apps/desktop
pnpm dev          # Vite renderer only, no native shell
pnpm build        # renderer bundle

cd src-tauri
cargo tauri dev   # native host in development
cargo tauri build # packaged application
```

**These native commands cannot run on the machine this project was bootstrapped
on.** That machine has no `webkit2gtk` development packages and no `pkg-config`,
so the Tauri host does not compile. Run them on a machine with the Tauri 2
prerequisites for your target. Until someone does, the desktop host and every
native acceptance case have no evidence; `docs/acceptance.md` marks them
accordingly, and `README.md` states the same limit.

Windows installer work, the 72-hour soak, macOS, and Linux packaging are later
stages.

## 7. Rules that the gates enforce

- Every first-party Cargo package inherits edition, Rust version, publish flag,
  and lint policy from the workspace. `cargo xtask check-architecture` fails a
  member that does not.
- `quota-domain` and `quota-core` may not depend on `tauri`, `tauri-plugin-*`,
  `react`, or `sqlx`.
- A first-party production file over 400 non-comment, non-blank lines fails the
  architecture gate. The guideline is 250.
- Raw `invoke(`, `listen(`, and `emit(` are confined to
  `apps/desktop/src/generated/` and `apps/desktop/src/shared/ipc/`.
- Every `uses:` in `.github/workflows/` is pinned to a full 40-character commit
  SHA. A tag or a branch fails `cargo xtask check-release`, and a `run:` step
  must not interpolate event text such as a pull-request title or a branch name.
- Production code denies `unwrap`, `expect`, `panic!`, `todo!`, and
  `unimplemented!`. Tests may use them; see `docs/exceptions.md`.
- An `#[expect(...)]` that stops firing is a warning, because
  `unfulfilled_lint_expectations = "warn"`. Do not add a blanket `#[allow(...)]`.

## 8. Changes to a command, a schema, or a provider mapping

Include the compatibility and migration effect, and the tests that cover it, in
the same change. A new enum variant breaks every handwritten exhaustive match by
design; that is the point of the typed contracts. Record any new exception in
`docs/exceptions.md` with an owner, a rationale, a scope, a review date, and a
removal condition.
