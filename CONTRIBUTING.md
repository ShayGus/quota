# Contributing

This file is the single clean-checkout path. Follow it in order; each step verifies the
one before it.

## 1. Set up the checkout

Prerequisites: `rustup`, and [Bun](https://bun.sh) for the frontend.

```bash
git clone https://github.com/ShayGus/quota.git
cd quota
bun install --frozen-lockfile          # the repository root is the frontend package
cd src-tauri                           # the Cargo workspace: every Rust package
rustup show active-toolchain          # installs the pinned compiler on first use
cargo build --workspace --locked
```

The repository root is the frontend; everything Rust lives in the Cargo workspace under
`src-tauri`: the host, `src-tauri/crates/`, `src-tauri/xtask/`, the lockfile, and the
Cargo and lint configuration. Run `cargo` from `src-tauri` and `bun` from the root.

`src-tauri/rust-toolchain.toml` owns the compiler pin and the required components. Do not
edit the version in a workflow file; it lives in that one file, so it cannot drift.

## 2. Verify the versions

From `src-tauri`:

```bash
rustc --version                       # must match rust-toolchain.toml
cargo tree -p quota-desktop --locked  # the resolved Rust graph
cargo deny check advisories licenses sources
cargo update --workspace --dry-run    # what could move inside existing ranges
```

`docs/dependencies.md` points to the version owners and explains the declared minimum Rust
version and the two compatibility holds: the Specta release candidate and SQLx 0.8.

For the frontend, read `bun.lock` at the repository root after
`bun install --frozen-lockfile`. Do not upgrade a dependency as a side effect of another
change: dependency moves are their own pull request, because they change the lockfile, the
advisory report, and sometimes the licence set.

## 3. Run the checks

From `src-tauri`:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --locked
cargo xtask check-architecture
cargo xtask check-release
cargo xtask bindings --check
```

From the repository root:

```bash
bun run typecheck && bun run lint && bun run format:check && bun run test && bun run check:release:renderer
bun run test:ui                        # interface tests, see section 7
bun run build:e2e && xvfb-run -a bun run test:e2e   # real-app tests, Linux, see section 8
```

CI runs exactly these commands. If a command passes locally and fails in CI, the
difference is the environment, not the command.

The same commands run on Windows and Linux, and are meant to run unchanged on macOS (see
[Platforms](docs/platforms.md)). On Windows, stop a running development app first, because
it holds `src-tauri\target\debug\quota.exe` open. Tauri needs the version 6 common
controls, which only the Windows application manifest selects, so `src-tauri/build.rs` and
`src-tauri/crates/quota-persistence/build.rs` hand `src-tauri/windows-app-manifest.xml` to
the linker for every binary they link, tests included. A crate whose tests start linking
Tauri needs the same build script; without it its test binary exits with
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

Two profile settings carry the weight, in the workspace manifest, `src-tauri/Cargo.toml`:

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

From `src-tauri`:

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

### Development identity

Every debug executable, including those produced by plain `bun tauri dev`,
`bun tauri build --debug`, and `cargo build`, applies the identity from
`src-tauri/tauri.dev.conf.json` to the Tauri context at startup before plugins and storage
initialize. Its runtime identifier is `app.quota.monitor.dev`, its product and autostart
name is `Quota Dev`, and all three window titles identify it as development. The runtime
identifier names the data directories, window-state file, credential service, and
single-instance lock. Window geometry, development server URLs, and capability overrides
remain as configured.

Use `bun run build:dev` for any development package. This command merges the overlay over
`src-tauri/tauri.conf.json` before packaging, giving bundles the executable name
`quota-dev`, product name `Quota Dev`, and identifier `app.quota.monitor.dev`. The overlay
repeats the three window definitions because the merge replaces arrays, and
`tests/dev-identity.test.ts` checks that every production window has a development twin.
Native identity tests exercise both build modes through Tauri's path resolver and the
credential service resolver.

```bash
bun tauri dev          # debug runtime identity is automatic
bun run build:dev      # development packages with the quota-dev executable
```

Release builds without the overlay retain the production identity and executable name.
`bun run build:dev` uses the development identity even though it compiles in release mode.

Plain `bun tauri build --debug` without the overlay still uses production bundle metadata
and the executable name `quota`. Its runtime identity is isolated; its raw debug bundle is
not for installing next to the production app. Use `bun run build:dev` for that.

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
- [The workspace lint tables](src-tauri/Cargo.toml) own Rust and Clippy lint levels;
  [clippy.toml](src-tauri/clippy.toml) owns thresholds and test-context allowances. CI
  runs Clippy with `-D warnings`, so a warn-level lint fails the build. Rewrite the site
  first; an `#[expect(..., reason = "...")]` is for the case where the rewrite is
  genuinely worse, and that reason belongs in
  [the exception register](docs/exceptions.md).
- An `#[expect(...)]` that stops firing is a warning, because
  `unfulfilled_lint_expectations = "warn"`. Do not add a blanket `#[allow(...)]`.

## 8. Changes to a command, a schema, or a provider mapping

Include the compatibility and migration effect, and the tests that cover it, in the same
change. A new enum variant breaks every handwritten exhaustive match by design; that is
the point of the typed contracts. Record any new exception in `docs/exceptions.md` with an
owner, a rationale, a scope, a review date, and a removal condition.

## 7. Interface tests

`bun run test:ui` loads the real built renderer (`bun run build`, served by
`vite preview`) in headless Chromium and drives it with Playwright. Only the Rust host is
replaced: a small faked backend, `tests/ui/fake-backend.ts`, answers the typed commands
and publishes the host's events, on top of Tauri's own `@tauri-apps/api/mocks`. It is
bundled into the page by `tests/ui/global-setup.ts` and never into `dist`, so nothing from
it can reach a release build.

The suite covers the popover with seven accounts in the states the interface has (healthy,
low, rate limited, offline, check failed, reconnect, monitoring off), the attention
filter, the account detail, the first-launch screen, a host that cannot be reached, every
settings panel, the add-account wizard through Provider, Connect and Verify, the privacy
aliases, the light and dark theme, the 440-pixel popover width, and the mini widget. It
does not start the native shell, so it cannot see window placement, the tray, the
single-instance lock, or anything the Rust host does; the faked host is only as faithful
as `tests/ui/fake-backend.ts`.

```bash
bun install --frozen-lockfile
bunx playwright install chromium        # once; add --with-deps on a clean Linux machine
bun run test:ui                         # about 15 s
bunx playwright show-report             # after a CI-style run, to browse the report
```

Screenshots of every state are written to `test-results/screenshots/`, and CI uploads them
with the traces of any failure as the `interface-test-results` artifact. A test fails if
the page logs an error or the renderer asks the faked host for a command it does not know,
so a new command needs an answer in `fake-backend.ts`.

### How this gates a release

The interface job is part of `.github/workflows/ci.yml`, not a workflow of its own. The
release workflow accepts a commit only when that file's CI run completed successfully, so
a release needs the unit tests, the interface tests and every other CI job green on the
exact commit. Keep new test layers inside `ci.yml` for the same reason.

## 8. Real-app tests (Linux)

The interface tests fake the Rust host. The real-app suite does not: `bun run build:e2e`
builds the actual desktop application in debug mode (twice: once as is, once with the
`sample-data` feature that seeds ten fixture accounts), and `bun run test:e2e` starts it
under a display and drives it through `tauri-driver`, Tauri's official WebDriver server,
which launches WebKitGTK's `WebKitWebDriver`. The client is a small W3C WebDriver client
in `tests/e2e/webdriver.ts`; Vitest is the runner (`vitest.e2e.config.ts`), so the suite
adds no dependency.

Every journey gets its own sandbox (`tests/e2e/sandbox.ts`): a temporary folder used as
`HOME` and as every `XDG_*` folder, and a private D-Bus session bus. The application can
neither read nor write the real user's data, the keyring, or another journey's
single-instance lock. The folder is deleted afterwards.

The journeys are: first launch with an empty profile; the main popover at 440 px; settings
opening from the popover and a preference surviving a restart through the tray's Quit; the
privacy aliases on the sample accounts; a second launch not starting a second app and
bringing the first forward; quitting from the tray menu (chosen over D-Bus, the way a
panel would) leaving no process and no lock behind; and the development identity: a debug
build uses `app.quota.monitor.dev`, writes only under that folder name, and never creates
the production one.

Not covered: adding an account against a fake local provider. The app has no seam for it:
every adapter calls its provider's real HTTPS address, and the address is a constant, so a
fake provider would need production code. The journey is listed as skipped in the output.
Also not covered: Windows, macOS, installed packages (these tests run the unpackaged debug
binary), and the tray icon's picture, which needs a panel.

To run it locally on Linux you need a display, `dbus-daemon`, `busctl`, `tauri-driver` and
`WebKitWebDriver`:

```bash
sudo apt install xvfb webkit2gtk-driver      # the package is webkitgtk-webdriver on newer Ubuntu
cargo install tauri-driver --version 2.1.0 --locked
bun run build:e2e                            # about 2 min cold, builds both binaries
xvfb-run -a bun run test:e2e                 # about 15 s
```

With a desktop session you can run `bun run test:e2e` directly and watch the windows;
`QUOTA_E2E_APP` and `QUOTA_E2E_SAMPLE_APP` point the suite at other binaries. Screenshots
and the logs of every journey are written to `test-results/e2e/`, and CI uploads them as
`real-app-results`.

### How this gates a release

Like the interface job, this job is in `.github/workflows/ci.yml`. The release workflow
accepts a commit only when that file's CI run succeeded, so a release needs the unit
tests, the interface tests and the real-app suite all green on that commit.
