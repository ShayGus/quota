# Acceptance mapping

Which layer implements each specified acceptance case, and where its evidence
lives. `not implemented` means the case has no code or no test yet; the reason
is given. This file is a record, not a claim that every case passes.

Layers named below: `domain` (`crates/quota-domain`), `core` (`crates/quota-core`),
`providers` (`crates/quota-providers`), `persistence` (`crates/quota-persistence`),
`contracts` (`crates/quota-contracts`), `host` (`apps/desktop/src-tauri`), `ui`
(`apps/desktop/src`).

The native desktop host has never been compiled or run on this machine: it needs
`webkit2gtk` development packages and `pkg-config`, and neither is installed.
Every case whose evidence would come from the host or the UI is therefore
`not implemented` here, and no build or run is claimed for it.

## AC-01 to AC-14: normalisation and presentation

| ID | Layer | Evidence | Status |
|---|---|---|---|
| AC-01 | domain | `percent::Percent::from_used_percent`; unit tests in `crates/quota-domain/src/percent.rs` | implemented |
| AC-02 | domain | `quota::window` window semantics; independent windows carry independent measurements | implemented |
| AC-03 | domain | `quota::window::QuotaCategory`; a missing category is absent, never synthesised | implemented |
| AC-04 | domain | `quota::measurement::UnavailableReason`; a missing field becomes an unavailable measurement | implemented |
| AC-05 | domain | `quota::money::MoneyMeasurement`; extra spend is a distinct role, not an included allowance | implemented |
| AC-06 | domain | `quota::measurement::QuantityMeasurement`; a bare amount has no denominator and no percentage | implemented |
| AC-07 | domain | `quota::window::Enforcement`; unlimited is its own state | implemented |
| AC-08 | domain | `Percent::is_just_above_zero` | implemented |
| AC-09 | domain | `quota::window` not-entitled state; no division by a zero limit | implemented |
| AC-10 | domain | `Percent` keeps the original evidence; `arc_fraction` clamps only the drawn arc | implemented |
| AC-11 | core | `scheduler` boundary handling | partially: decision types exist, no supervisor run on this machine |
| AC-12 | domain | one window per scope, so one limit can replenish alone | implemented |
| AC-13 | domain | `quota::window::BoundaryKind` distinguishes a partial replenishment from a reset | implemented |
| AC-14 | providers | provider decoders must leave a missing `resets_at` absent | not implemented: no provider decoder is written yet |

## AC-45 to AC-61: UI and windowing

| ID | Layer | Evidence | Status |
|---|---|---|---|
| AC-45 to AC-61 | ui, host | native window and overview behaviour | not implemented: the desktop host and renderer cannot be built or run on this machine |

## AC-62 to AC-71: multi-account and scheduling

| ID | Layer | Evidence | Status |
|---|---|---|---|
| AC-62 | core | `accounts::AccountRegistry` holds one entry per connection | implemented |
| AC-63 | core | connection generation is stored beside each binding and compared on completion | implemented |
| AC-64 | core | `AccountRegistry` refuses a second binding for a verified principal | implemented |
| AC-65 | domain | `QuotaPoolId` is a separate identity from the account | implemented |
| AC-66 | core | per-connection failure state; siblings are unaffected | implemented |
| AC-67 | core | `scheduler::ReadBudget` is shared scope state | implemented |
| AC-68 to AC-71 | ui, host | text scaling, native topmost, and single scheduler behaviour in the real application | not implemented: no runnable desktop build on this machine |

## AC-75 to AC-84: scaffold, gates, and IPC

| ID | Layer | Evidence | Status |
|---|---|---|---|
| AC-75 | xtask | `cargo xtask check-architecture` checks inherited edition, Rust version, publish flag, and lint policy | implemented |
| AC-76 | CI | `.github/workflows/ci.yml` `rust` job runs clippy with `-D warnings` on the pinned toolchain | implemented |
| AC-77 | xtask | the same gate fails on a file over 400 code lines and on a forbidden dependency edge | implemented |
| AC-78 | docs | `docs/dependencies.md` records resolved versions and how each was verified | implemented |
| AC-79 | docs, xtask | `=2.0.0-rc.25` pins both release candidates; a mismatch fails the build, not a check | partially: the pin is in place; no update test exists |
| AC-80 | xtask | `cargo xtask bindings --check` fails on drift | partially: the command exists and reports the files it compares, but the IPC layer and the mirror do not exist yet |
| AC-81 | xtask, ui | the raw-IPC and duplicate-model rules in `check-architecture` | partially: the rules run; there is no renderer code to scan yet |
| AC-82 | domain, contracts | validated constructors plus Serde round-trip tests | implemented |
| AC-83, AC-84 | ui | snapshot revision handling in the renderer | not implemented: no renderer exists yet |

## AC-92 to AC-100: persistence

| ID | Layer | Evidence | Status |
|---|---|---|---|
| AC-92 to AC-100 | persistence | typed Store codec and typed SQLite repositories | not implemented: `crates/quota-persistence` holds only `lib.rs` at this stage |

## AC-105 to AC-111: hardening

| ID | Layer | Evidence | Status |
|---|---|---|---|
| AC-105 | domain | private fields with validating constructors on every identifier and measurement; deserialisation goes through the constructor | implemented |
| AC-106 | core, xtask | no process environment mutation; the architecture gate rejects the pattern when it appears | partially: the rule exists, and the core is written this way, but nothing is executed |
| AC-107 | CI, clippy.toml | `unfulfilled_lint_expectations = "warn"` with `-D warnings`; `clippy.toml` allows panic and expect only in recognised test contexts | implemented |
| AC-108 | CI | `[profile.release] panic = "unwind"` in the root manifest | partially: the profile is set; the desktop release profile has not been built |
| AC-109 | xtask | `cargo xtask check-release` rejects a test-only feature in the default set | implemented |
| AC-110, AC-111 | ui | strict TypeScript projects and the typed lint configuration | not implemented: no renderer code exists yet |

## AC-112 to AC-142

| ID | Layer | Evidence | Status |
|---|---|---|---|
| AC-112 | contracts | exhaustive typed unions; a new variant breaks handwritten matches | partially: the contract types exist, the UI that consumes them does not |
| AC-113 | domain, ui | nullish handling that preserves a real zero | not implemented: the renderer does not exist |
| AC-114 | xtask | `bindings --check` after an exporter upgrade | not implemented: the exporter cannot be compiled on this machine, see `docs/exceptions.md` |
| AC-115 to AC-130 | ui, host | renderer lifecycle, React Strict Mode, ACL, CSP, and navigation | not implemented: no runnable desktop build on this machine |
| AC-131, AC-132 | persistence | pooled connection settings and the linked SQLite version | not implemented: no repository code and no packaged artifact yet |
| AC-133, AC-134 | persistence | crash, checkpoint, and backup recovery | not implemented: no repository code yet |
| AC-135 | core | reconciled non-idempotent mutation | partially: the attempt identity type exists; no service implements it |
| AC-136, AC-137 | host, CI | native WebDriver lane and shipping-artifact inspection | not implemented: neither the driver lane nor the artifact exists |
| AC-138 | domain | property tests in `crates/quota-domain/tests/ranking.rs` reject shared globals | implemented |
| AC-139 | persistence | SQLx offline metadata drift | not implemented: no query metadata yet |
| AC-140 | docs | `CONTRIBUTING.md` documents one clean-checkout path and the exact commands | implemented |
| AC-141 | CI | `.github/workflows/` pins every action to a commit SHA and asks for read-only permissions | implemented |
| AC-142 | release process | release evidence bundle | not implemented: no release has been cut |

## Cases with no line above

Every remaining case (AC-15 to AC-44, AC-72 to AC-74, AC-85 to AC-91, AC-101 to
AC-104) is `not implemented`. They cover live provider credentials, notification
delivery, the native tray and window controller, the ten-account soak, and the
renderer's subscription lifecycle. Each needs either a signed-in provider
account, a packaged desktop build, or a real operating-system session, and none
of those exists in this stage.

## What runs today

| Command | Meaning |
|---|---|
| `cargo test -p quota-domain --locked` | Domain arithmetic, invariants, and ranking |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Lint policy for every crate that compiles |
| `cargo xtask check-architecture` | Package, dependency, file size, and IPC rules |
| `cargo xtask check-release` | Release feature set, licence allow list, action pins |
| `cargo xtask bindings --check` | The checked-in mirror against the Rust IPC layer |
