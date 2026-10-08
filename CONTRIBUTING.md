# Contributing

This file is the single clean-checkout path. Follow it in order; each step verifies the
one before it.

## 1. Set up the checkout

Prerequisites: `rustup`, and [Bun](https://bun.sh) for the frontend.

On Ubuntu, install the Tauri build packages first:

```bash
sudo apt-get update
sudo apt-get install --no-install-recommends -y   build-essential curl file libayatana-appindicator3-dev libdbus-1-dev   librsvg2-dev libssl-dev libwebkit2gtk-4.1-dev libxdo-dev patchelf   pkg-config wget
```

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
`src-tauri/Cargo.toml` owns the declared minimum Rust version.

### Project layout

```text
quota/
├── package.json, src/      # The React 19 renderer, at the repository root
├── tests/                  # Renderer, interface and real-app tests
├── tools/                  # Build helpers for the tests and the docs screenshots
├── .github/workflows/      # CI, dependency and release workflows
├── docs/                   # User guide, troubleshooting, providers, architecture,
│                           # platforms, releasing, acceptance, exceptions
└── src-tauri/              # Everything Rust: the Cargo workspace
    ├── Cargo.toml          # The workspace (resolver 3) and the Tauri host
    ├── Cargo.lock          # The one committed Rust lockfile
    ├── rust-toolchain.toml # Pinned compiler
    ├── rustfmt.toml, clippy.toml
    ├── deny.toml           # cargo-deny policy
    ├── src/                # The Tauri host
    ├── crates/
    │   ├── quota-domain/       # Validated values, quota windows, ranking
    │   ├── quota-core/         # Accounts, snapshots, scheduler, alerts, ports
    │   ├── quota-contracts/    # Serde + Specta IPC transport DTOs
    │   ├── quota-providers/    # Provider clients, decoders, strategies
    │   └── quota-persistence/  # Typed Store and SQLite repositories
    └── xtask/              # Repository gates
```

Rust owns provider access, polling, account identity, quota normalisation, ranking,
credentials, durable storage, notifications, and native lifecycle. React owns presentation
and transient interaction state. SQLite stores account, history, backoff, monitoring,
notification, operational privacy, and polling state; the Tauri Store plugin stores
presentation settings. [Architecture](docs/architecture.md) has the dependency graph and
the state ownership table.

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
bun run test:ui                        # interface tests, see section 9
bun run build:e2e && xvfb-run -a bun run test:e2e   # Linux real-app tests, see section 10
bun run build:e2e && bun run test:e2e               # Windows real-app tests, see section 10
```

CI runs the relevant commands for each operating system. Linux uses `xvfb-run` for the
real-app suite. Windows runs that suite directly.

The frontend and Rust commands run on Windows and Linux. They are meant to run unchanged
on macOS (see [Platforms](docs/platforms.md)). On Windows, stop a running development app
first. It holds `src-tauri\target\debug\quota.exe` open. Tauri needs the version 6 common
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
bun tauri build        # release bundles; needs TAURI_SIGNING_PRIVATE_KEY, see below
```

Packaging signs every installer for the updater, because `bundle.createUpdaterArtifacts`
is on, so `bun tauri build` with bundles stops without `TAURI_SIGNING_PRIVATE_KEY` (and
its password in `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`). A local build that only needs the
binary runs `bun tauri build --no-bundle`, which signs nothing, and `bun run build:dev`
switches signing off for development packages. The real key is the maintainer's and lives
in the repository secrets; see [Releasing](docs/RELEASING.md#the-update-key). Never put a
private key in a file in the checkout.

`bun tauri dev` starts the Vite dev server on port 1420, compiles the Rust host, and opens
only the overview; Settings stays hidden at launch. The native host writes to standard
output and, once startup resolves the application log directory, to `quota.log` there. It
logs warnings for failed close-time hiding, tray anchoring, second-launch focusing, and
refused initial or periodic refresh requests. Browser-launch, provider sign-in, and
filesystem export failures name the log in their messages; if the log cannot be opened or
written, the message says logging is unavailable and gives the expected path when it can
be resolved. `cargo deny` is not a workspace tool: install it once with
`cargo install cargo-deny`.

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

## 9. Interface tests

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
aliases, the light and dark theme, the 440-pixel popover width, the mini widget, and the
update pop-up (the offer, OK with its busy state, Cancel, and a failed install, with
screenshots in both themes beside the settings window's, in `docs/update-popup/`). It does
not start the native shell, so it cannot see window placement, the tray, the
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

### Display modes covered

What is on screen is decided by three confirmed preferences: `view` (full window or mini
widget; never both), `overview_mode` (the full window docked to the tray or floating), and
`indicator_style` (rings or bars). In the widget the indicator style picks the look: rings
give the ring strip, bars give the mini cards. There is no third widget variant, and the
widget ignores `overview_mode`. `tests/ui/matrix.ts` states this, and
`tests/ui/modes.spec.ts` runs every combination that exists. The names the owner uses map
to the code like this:

| Owner's name   | In the code                                     | Test surface                                                                 |
| -------------- | ----------------------------------------------- | ---------------------------------------------------------------------------- |
| Full window    | `view: overview`, docked (`tray`) or `floating` | `full-tray-ring`, `full-tray-bar`, `full-floating-ring`, `full-floating-bar` |
| Floating bar   | `view: widget` with rings: the ring strip       | `widget-strip`                                                               |
| Floating cards | `view: widget` with bars: the mini cards        | `widget-cards`                                                               |

Every surface is run against each of these in both themes:

| Axis            | Values                                                                                                                                          |
| --------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| Account count   | 0, 1, 2, 3 (odd: the last mini card is full width), 7; for the strip, rows of at most four, as even as they can be                              |
| Account state   | healthy, low, rate limited, offline, check failed, reconnect needed                                                                             |
| Theme           | light, dark                                                                                                                                     |
| Privacy aliases | off, on (no account name anywhere in the page, accessible names included)                                                                       |
| Long names      | two accounts of one provider with 100-character nicknames: nothing is wider than the window                                                     |
| Widget size     | 316 px wide, with 0 to 12 accounts in the renderer size scenarios; native fitting follows [The mini widget](docs/user-guide.md#the-mini-widget) |
| Switching       | full window to widget and back, Expand from the widget, the Settings "Mini widget" switch, the pin (docked and floating), the layout buttons    |

Counts by theme by aliases are 6 surfaces x 5 x 2 x 2 = 120 tests, states by theme are 6 x
6 x 2 = 72, long names 12, sizes 3, switching 5. `test-results/screenshots/index.html` is
a contact sheet of every screenshot, grouped by folder.

The README's and the user guide's pictures in `docs/images/` come from the same run, so
they show the fictional accounts of the faked host. A test takes one with
`host.screenshotFull(name)`, which grows the window until nothing in it scrolls, so the
picture is never cut off, and writes it to `test-results/screenshots/docs/`;
`tests/ui/docs.spec.ts` takes the README's overview pictures. After an interface change,
refresh them with `bun run test:ui && bun run docs:screenshots`;
`tools/docs/screenshots.mjs` lists which capture each picture comes from.

What the screenshots show about the mini widget, as the code stands: a rate-limited,
offline, check-failed, paused or connecting account keeps its last readings in both the
ring strip and the mini cards, muted (and in the strip its rings faded), as the full
window keeps them; the peek and the drawer name the status. Only an account with no
reading, or one that must be reconnected, shows its status ("Reconnect") instead of
numbers.

### How this gates a release

Keep new test layers in `.github/workflows/ci.yml`, alongside the interface job, so the
release preflight covers them. See
[Which tests a release needs](docs/RELEASING.md#which-tests-a-release-needs) for the
preflight requirement.

## 10. Real-app tests (Linux and Windows)

The interface tests fake the Rust host. The real-app suite does not: `bun run build:e2e`
builds the actual desktop application in debug mode (twice: once as is, once as the test
launcher with the `sample-data` feature, which seeds ten fixture accounts).
`bun run test:e2e` starts it through Tauri's official WebDriver server and drives it with
the small W3C WebDriver client in `tests/e2e/webdriver.ts`.

On Linux, the app runs under a display and uses WebKitGTK's `WebKitWebDriver`. Each
journey gets a temporary `HOME`, isolated `XDG_*` folders, and a private D-Bus session
bus. On Windows, the app uses Microsoft Edge WebDriver. The harness redirects the current
user's `AppData` and `Local AppData` folders to the sandbox, then restores their settings
after each journey. It sets a temporary `USERPROFILE` for sign-in files. The Windows job
runs on `windows-2022`. The OS credential store is not isolated.

The journeys in [`tests/e2e/journeys.e2e.ts`](tests/e2e/journeys.e2e.ts) cover: first
launch with an empty profile; the main popover at 440 px; settings opening from the
popover and a preference surviving a restart; diagnostics export reporting a path and
producing a JSON file there; privacy aliases on sample accounts; a second launch not
starting a second app and bringing the first forward; quitting from the tray menu and
leaving no process or lock behind; using the development identity without creating the
production identity; and adding an account against a fake provider; and a development or
test build never checking for updates, using a fake update server that offers a far newer
version and asserting it receives no request. On Windows, the settings journey ends its
WebDriver session before restarting. Windows runs the journeys that WebDriver can drive.
It marks the tray-menu quit journey as skipped because the native Windows tray menu is
outside the WebDriver interface.

The Windows job also runs the host's library tests and
[`browser_launch.rs`](src-tauri/tests/browser_launch.rs) before the real-app journeys. The
browser probes record the opener's handover without launching a real browser: Linux uses
sandbox launchers, while Windows temporarily registers an HTTPS recorder only when there
is no per-user HTTPS registration or UserChoice. Windows queries the effective handler
before installation and again before launching; it prints a skip reason if interception
cannot be established. These probes do not establish live provider approval or a completed
Muse Code connection.

Adding an account works against a fake provider, through the transport seam the provider
crate offers to test builds. `bun run build:e2e` also builds
`src-tauri/examples/quota_e2e.rs`, a launcher that reads `QUOTA_E2E_PROVIDER_BASE` and
calls `quota_providers::retarget` before starting the app, and reads
`QUOTA_E2E_UPDATE_ENDPOINT` for the update-check journey (the updater never reads the
environment, and `cargo xtask check-release` keeps it that way). It exists only with the
non-default `sample-data` feature (which compiles `test-fixtures`), so no default or
release build contains it, and the provider transport itself still reads nothing from the
environment (`cargo xtask check-release` checks this). The journey writes a Codex sign-in
into the sandbox's home, starts a loopback fake provider that answers the Codex usage
endpoint with the provider crate's own sanitized fixture, adds the account through the
wizard, asserts the fake provider received the request with the sandbox's token, and
checks the card shows the fixture's 72% left. The fake provider is only used by that
journey, which therefore runs the launcher build (with its ten seeded sample accounts);
every other journey uses the ordinary debug build or, for the privacy aliases, the same
launcher.

Not covered: installing or upgrading an installer; SmartScreen; the real Windows
credential store; the Windows tray-menu quit journey; macOS; and the tray icon's picture,
which needs a panel. The sandboxed account-add journey uses a synthetic Codex file and a
fake local provider. It does not verify system credential-store behavior.

To run it locally on Linux you need a display, `dbus-daemon`, `busctl`, `tauri-driver` and
`WebKitWebDriver`:

```bash
sudo apt install xvfb webkit2gtk-driver      # the package is webkitgtk-webdriver on newer Ubuntu
cargo install tauri-driver --version 2.1.0 --locked
bun run build:e2e                            # builds both binaries
xvfb-run -a bun run test:e2e
```

On Windows, install a WebDriver for the installed WebView2 runtime, then run:

```powershell
Push-Location src-tauri
cargo install --git https://github.com/chippers/msedgedriver-tool --rev 8c4b34f51b45f5cf08013366d703de464ab871d1 --locked
& "$env:USERPROFILE\.cargo\bin\msedgedriver-tool.exe"
$env:PATH = "$($PWD.Path);$env:USERPROFILE\.cargo\bin;$env:PATH"
cargo install tauri-driver --version 2.1.0 --locked
Pop-Location
bun run build:e2e
bun run test:e2e
```

With a desktop session on Linux, `bun run test:e2e` opens the application windows.
`QUOTA_E2E_APP` and `QUOTA_E2E_SAMPLE_APP` point the suite at other binaries. Screenshots
and logs go to `test-results/e2e/`. CI uploads `real-app-results` and
`real-app-windows-results`.

### How this gates a release

Like the interface job, this job is in `.github/workflows/ci.yml`. See
[Which tests a release needs](docs/RELEASING.md#which-tests-a-release-needs) for the
release preflight requirement.
