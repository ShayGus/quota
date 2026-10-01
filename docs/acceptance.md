# Acceptance mapping

Which layer implements each specified acceptance case, and where its evidence lives.
`not implemented` means there is no source implementation. A source implementation does
not prove native runtime behaviour. This file records evidence; it does not claim that
every case passes.

Layers named below: `domain` (`crates/quota-domain`), `core` (`crates/quota-core`),
`providers` (`crates/quota-providers`), `persistence` (`crates/quota-persistence`),
`contracts` (`crates/quota-contracts`), `host` (`src-tauri`), `ui` (`src`).

The recorded local evidence covers compilation, strict linting, tests, and a WSLg launch
on 2026-10-01 showing ten fictional sample accounts with 19 readings. The Tauri native
build dependencies were installed for that run, and the window used software rendering.
This smoke check does not establish live-provider, tray, notification, or packaged-runtime
acceptance. Windows and macOS runs, Windows installers, Linux packaging, and the 72-hour
ten-account soak remain unverified. CI installs the Linux dependencies on a clean runner.

## AC-01 to AC-14: normalisation and presentation

| ID    | Layer      | Evidence                                                                                      | Status                                                                |
| ----- | ---------- | --------------------------------------------------------------------------------------------- | --------------------------------------------------------------------- |
| AC-01 | domain     | `percent::Percent::from_used_percent`; unit tests in `crates/quota-domain/src/percent.rs`     | implemented                                                           |
| AC-02 | domain     | `quota::window` window semantics; independent windows carry independent measurements          | implemented                                                           |
| AC-03 | domain     | `quota::window::QuotaCategory`; a missing category is absent, never synthesised               | implemented                                                           |
| AC-04 | domain     | `quota::measurement::UnavailableReason`; a missing field becomes an unavailable measurement   | implemented                                                           |
| AC-05 | domain     | `quota::money::MoneyMeasurement`; extra spend is a distinct role, not an included allowance   | implemented                                                           |
| AC-06 | domain     | `quota::measurement::QuantityMeasurement`; a bare amount has no denominator and no percentage | implemented                                                           |
| AC-07 | domain     | `quota::window::Enforcement`; unlimited is its own state                                      | implemented                                                           |
| AC-08 | domain     | `Percent::is_just_above_zero`                                                                 | implemented                                                           |
| AC-09 | domain     | `quota::window` not-entitled state; no division by a zero limit                               | implemented                                                           |
| AC-10 | domain     | `Percent` keeps the original evidence; `arc_fraction` clamps only the drawn arc               | implemented                                                           |
| AC-11 | core, host | Shared supervised polling with bounded concurrency; core tests and host worker source         | source and local launch exist; sustained native scheduling unverified |
| AC-12 | domain     | one window per scope, so one limit can replenish alone                                        | implemented                                                           |
| AC-13 | domain     | `quota::window::BoundaryKind` distinguishes a partial replenishment from a reset              | implemented                                                           |
| AC-14 | providers  | Codex, Claude, and OpenCode Go decoder mappings and provider tests                            | implemented                                                           |

## AC-45 to AC-61: UI and windowing

| ID             | Layer    | Evidence                                                | Status                                        |
| -------------- | -------- | ------------------------------------------------------- | --------------------------------------------- |
| AC-45 to AC-61 | ui, host | `src` renderer and `src-tauri/src/platform` window code | source implemented; native runtime unverified |

## AC-62 to AC-71: multi-account and scheduling

| ID             | Layer    | Evidence                                                                       | Status                                        |
| -------------- | -------- | ------------------------------------------------------------------------------ | --------------------------------------------- |
| AC-62          | core     | `accounts::AccountRegistry` holds one entry per connection                     | implemented                                   |
| AC-63          | core     | connection generation is stored beside each binding and compared on completion | implemented                                   |
| AC-64          | core     | `AccountRegistry` refuses a second binding for a verified principal            | implemented                                   |
| AC-65          | domain   | `QuotaPoolId` is a separate identity from the account                          | implemented                                   |
| AC-66          | core     | per-connection failure state; siblings are unaffected                          | implemented                                   |
| AC-67          | core     | `scheduler::ReadBudget` is shared scope state                                  | implemented                                   |
| AC-68 to AC-71 | ui, host | Text scaling, topmost control, and one shared supervisor in source             | source implemented; native runtime unverified |

## AC-75 to AC-84: scaffold, gates, and IPC

| ID           | Layer             | Evidence                                                                                               | Status                                                |
| ------------ | ----------------- | ------------------------------------------------------------------------------------------------------ | ----------------------------------------------------- |
| AC-75        | xtask             | `cargo xtask check-architecture` checks inherited edition, Rust version, publish flag, and lint policy | implemented                                           |
| AC-76        | CI                | `.github/workflows/ci.yml` `rust` job runs clippy with `-D warnings` on the pinned toolchain           | implemented                                           |
| AC-77        | xtask             | the same gate fails on a file over 400 code lines and on a forbidden dependency edge                   | implemented                                           |
| AC-78        | docs              | `docs/dependencies.md` records resolved versions and how each was verified                             | implemented                                           |
| AC-79        | docs, xtask       | `=2.0.0-rc.25` pins both release candidates; a mismatch fails the build, not a check                   | partially: the pin is in place; no update test exists |
| AC-80        | xtask, ui         | `cargo xtask bindings --check`; current Rust commands and events are mirrored                          | implemented                                           |
| AC-81        | xtask, ui         | `check-architecture` scans raw IPC calls and duplicate models in the renderer                          | implemented                                           |
| AC-82        | domain, contracts | validated constructors plus Serde round-trip tests                                                     | implemented                                           |
| AC-83, AC-84 | ui                | snapshot revision handling and subscription tests in the renderer                                      | implemented                                           |

## AC-92 to AC-100: persistence

| ID              | Layer       | Evidence                                                                  | Status                                                                         |
| --------------- | ----------- | ------------------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| AC-92 to AC-100 | persistence | Typed Store codec, SQLite repositories, migrations, and persistence tests | storage tests and local launch exist; native persistence acceptance unverified |

## AC-105 to AC-111: hardening

| ID             | Layer           | Evidence                                                                                                                            | Status                                                                        |
| -------------- | --------------- | ----------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| AC-105         | domain          | private fields with validating constructors on every identifier and measurement; deserialisation goes through the constructor       | implemented                                                                   |
| AC-106         | core, xtask     | no process environment mutation; the architecture gate rejects the pattern when it appears                                          | implemented; core tests run locally                                           |
| AC-107         | CI, clippy.toml | `unfulfilled_lint_expectations = "warn"` with `-D warnings`; `clippy.toml` allows panic and expect only in recognised test contexts | implemented                                                                   |
| AC-108         | CI              | `[profile.release] panic = "unwind"` in the root manifest                                                                           | partially: the profile is set; the desktop release profile has not been built |
| AC-109         | xtask           | `cargo xtask check-release` rejects a test-only feature in the default set                                                          | implemented                                                                   |
| AC-110, AC-111 | ui              | strict TypeScript projects and the typed lint configuration                                                                         | implemented; typecheck and lint pass locally                                  |

## AC-112 to AC-142

| ID               | Layer           | Evidence                                                                                  | Status                                                                     |
| ---------------- | --------------- | ----------------------------------------------------------------------------------------- | -------------------------------------------------------------------------- |
| AC-112           | contracts, ui   | exhaustive typed unions and renderer matches                                              | implemented in source; native runtime unverified                           |
| AC-113           | domain, ui      | nullish handling that preserves a real zero                                               | implemented; renderer tests run locally                                    |
| AC-114           | xtask           | `bindings --check` after an exporter upgrade                                              | not implemented: no exporter upgrade test exists                           |
| AC-115 to AC-130 | ui, host        | renderer lifecycle, React Strict Mode, ACL, CSP, and navigation                           | source implemented; native runtime unverified                              |
| AC-131, AC-132   | persistence     | pooled connection settings and the linked SQLite version                                  | partially: pool settings are tested; no packaged artifact exists           |
| AC-133, AC-134   | persistence     | crash, checkpoint, and backup recovery                                                    | partially: Store recovery tests exist; SQLite crash recovery is unverified |
| AC-135           | core, host      | reconciled non-idempotent mutation and connection-generation checks                       | source and core tests exist; native runtime unverified                     |
| AC-136, AC-137   | host, CI        | native WebDriver lane and shipping-artifact inspection                                    | not implemented: neither the driver lane nor the artifact exists           |
| AC-138           | domain          | property tests in `crates/quota-domain/tests/ranking.rs` reject shared globals            | implemented                                                                |
| AC-139           | persistence     | SQLx offline metadata drift                                                               | not implemented: no query metadata yet                                     |
| AC-140           | docs            | `CONTRIBUTING.md` documents one clean-checkout path and the exact commands                | implemented                                                                |
| AC-141           | CI              | `.github/workflows/` pins every action to a commit SHA and asks for read-only permissions | implemented                                                                |
| AC-142           | release process | release evidence bundle                                                                   | not implemented: no release has been cut                                   |

## Cases with no line above

The remaining cases (AC-15 to AC-44, AC-72 to AC-74, AC-85 to AC-91, AC-101 to AC-104)
have no acceptance evidence recorded here. They include live provider credentials,
notification delivery, and the ten-account soak. Existing connector or notification source
does not establish those cases; verification requires the relevant signed-in provider or
long-running packaged desktop run.

## Repository checks

[CONTRIBUTING.md](../CONTRIBUTING.md#3-run-the-checks) owns the check commands. The
workspace checks include the desktop host; no local host exclusion is required.
