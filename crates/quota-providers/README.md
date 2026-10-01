# quota-providers

Provider connectors for Quota. Each connector discovers a credential another client owns,
reads that provider's quota source through a bounded HTTP boundary, and normalises the
payload into `quota_domain::quota::window::QuotaWindow` values.

No connector starts a conversation, a turn, or a tool call. None writes to, refreshes, or
rotates an externally owned credential. None makes an inference or a model call to obtain
a quota reading.

## Verification status

**No live call was made by this crate or its tests.** Every endpoint below is
undocumented, and the field spellings were supplied as a specification, not observed by
this crate from a real login. The status column therefore separates what an automated test
proves from what only a real login can prove:

| Provider    | Endpoint behaviour | Field spellings | Live-login verification |
| ----------- | ------------------ | --------------- | ----------------------- |
| Codex       | ASSUMED            | ASSUMED         | Not performed           |
| Claude      | ASSUMED            | ASSUMED         | Not performed           |
| OpenCode Go | ASSUMED            | ASSUMED         | Not performed           |

What the tests do prove, over sanitized fixtures under `tests/fixtures/` and inline
payloads: the decoding and normalisation of each documented field spelling, the
missing-window and unusable-number rules, the identity and pool rules, and that no live
network call is needed to run them.

## Schema risk

Every endpoint here is undocumented. A field can be renamed, dropped, or added without
notice, and the shape can change between account plans. The decoders answer this in three
ways:

- every field is optional, so a partial payload decodes rather than fails;
- every documented spelling of every field is accepted as a `#[serde(alias)]`;
- unknown fields are ignored, so an added provider field can neither fail the parse nor
  change what a number means.

A payload that carries nothing usable becomes a typed failure:
`ProviderError::InvalidData` for a body that is not a JSON object,
`ProviderError::UnsupportedSchema` for a JSON object this build does not recognise. A
value that is present but unusable becomes
`Measurement::Unavailable(UnavailableReason::InvalidResponse)` with a `QuotaIssue`. No
path turns a malformed payload into a zero.

## Codex

Reads the quota the Codex CLI reports for its own login.

- Endpoints: `GET https://chatgpt.com/backend-api/wham/usage`, then, when that yields
  nothing usable, `GET https://chatgpt.com/backend-api/codex/usage`. Both are
  undocumented.
- Credential: `$CODEX_HOME/auth.json`, defaulting to `~/.codex/auth.json`. Read from
  `tokens.access_token`; the account identity is `account_id`.
- Credential owner: the Codex CLI. It owns and refreshes this file. This adapter never
  writes to, refreshes, or rotates it, and never runs the Codex CLI. An expired or
  rejected token becomes `ProviderError::Authentication` for the user to fix in Codex.
- Request: the bearer token plus the `ChatGPT-Account-Id` header.
- Decoded fields: `rate_limit`/`rateLimits`/`rate_limits` or the root object;
  `primary_window`/`primary` and `secondary_window`/`secondary`; `code_review_rate_limit`;
  `additional_rate_limits[]`; `rateLimitsByLimitId`. Used percent:
  `used_percent`/`usedPercent`, as a number or a numeric string. Reset:
  `reset_at`/`resetsAt` (epoch seconds or a date string) or `reset_after_seconds`.
  Duration: `limit_window_seconds` (seconds) or `windowDurationMins` (minutes). Plan:
  `plan_type`/`planType`. Optional `credits.balance`/`credits.unlimited`.
- Window mapping: exactly 18000 seconds maps to `QuotaCategory::Session`, exactly 604800
  seconds to `QuotaCategory::Weekly`, and any other duration keeps its own resource scope
  under `QuotaCategory::Custom`. Codex has no monthly allowance, so no monthly window is
  ever created for it.
- Roles: a window is `MetricRole::IncludedAllowance`. `credits` is
  `MetricRole::CreditBalance`, never included quota.
- Cadence: an event-assisted policy with a five-minute verification interval; the minimum
  interval is 300 seconds. HTTP 429 becomes `ProviderError::RateLimited` carrying the
  `Retry-After` deadline. The adapter does not retry: the shared supervisor owns that
  decision.

## Claude

Reads Claude subscription usage for the Claude Code login.

- Endpoints: `GET https://api.anthropic.com/api/oauth/usage` for the reading and
  `GET https://api.anthropic.com/api/oauth/profile` for the identity. Both are
  undocumented, and both require the `anthropic-beta: oauth-2025-04-20` header.
- Credential: `$CLAUDE_CONFIG_DIR/.credentials.json`, defaulting to
  `~/.claude/.credentials.json`. Read from `claudeAiOauth.accessToken` or
  `claudeAiOauth.access_token`.
- Credential owner: Claude Code. It owns and refreshes this file. This adapter never
  refreshes the token, never writes to the file, and never runs Claude Code. The optional
  inference-based quota path is out of scope and does not exist in this crate.
- Identity: `account.uuid` from the profile route. Without it the reading is refused with
  `ProviderError::InvalidData`, so an account with no reported identity reads as
  unverified rather than inventing one. The address is masked in any label this crate
  produces.
- Decoded fields: `five_hour`, `seven_day`, and optional `seven_day_opus`, each with
  `utilization` (percent points used) and `resets_at`/`reset_at`; a `limits[]` array with
  `percent`, `group`/`kind`, `resets_at`, and optional `scope.model.id`/`display_name`;
  optional `extra_usage` with `is_enabled`, `monthly_limit`, `used_credits`,
  `decimal_places`.
- Window mapping: a `limits[]` array that carries a percentage replaces the three fixed
  windows outright, and each entry keeps its own group and model scope. A fixed window the
  source did not report becomes `Unavailable(NotReported)` and is recorded in
  `expected_but_missing`. The Opus weekly window is optional: a plan without it simply has
  no such window.
- Extra usage: `MetricRole::ExtraSpendCap`, never included quota, and never ranked. It is
  a `MoneyMeasurement`. The reported amounts are read as minor units with `decimal_places`
  as their scale, so `monthly_limit: 5000` with `decimal_places: 2` is 50.00; the scale
  travels with the amount. The currency is taken from the payload when it names one, and
  otherwise assumed to be USD, because this route reports United States dollar amounts.
  Both readings are ASSUMED and are worth confirming against a live payload.
- Cadence: a fixed interval with a 300-second minimum, 300 seconds while the overview is
  visible, and 900 seconds in battery saver. HTTP 429 becomes `RateLimited` with the
  `Retry-After` deadline, 401 becomes `Authentication`, and 403 alone becomes
  `Authorization` — never a network retry.

## OpenCode Go

Reads the `OpenCode` Zen Go usage the local `OpenCode` login authorizes.

- Endpoint: `GET https://opencode.ai/zen/go/v1/usage` with a bearer key and a JSON accept
  header. It is undocumented.
- Credential: `$XDG_DATA_HOME/opencode/auth.json`, defaulting to
  `~/.local/share/opencode/auth.json`. The `opencode-go` entry is preferred, then
  `opencode`, taking a literal `key`, `apiKey`, `api_key`, `access`, or `token`.
- Credential owner: the `OpenCode` login. Quota has no refresh path for this credential at
  all, and never writes to the file.
- Decoded fields: `usage` or the root object; `rollingUsage`/`rolling`,
  `weeklyUsage`/`weekly`, `monthlyUsage`/`monthly`. Used percent: `percent`,
  `percentUsed`, `usedPercent`, `usagePercent`; remaining percent: `percentRemaining`,
  `remainingPercent`. Reset: `resetsAt`, `resetAt`, `reset_at`, `nextResetTime` (a date
  string or epoch seconds) or `resetInSec`/`reset_in_sec`. Numbers or numeric strings.
- Window mapping: the rolling window is five hours (18000 seconds) and the weekly window
  is seven days (604800 seconds). The monthly window is mapped to `QuotaCategory::Monthly`
  and is never dropped: this is the only connector in scope that reports one.
- **Known limitation.** The usage response carries no account, workspace, or entitlement
  identity. This adapter does not invent one. It uses a stable local `QuotaPoolId` derived
  from the credential profile label and leaves the optional principal, workspace, and
  entitlement fields `None`, so the account reads as unverified. A binding whose principal
  is set is therefore refused on read. This is an accepted, honest limitation of this
  connector, not a defect to paper over.
- Cadence: a fixed interval with a 300-second minimum. HTTP 429 becomes `RateLimited` with
  the `Retry-After` deadline.

## Fixture

A deterministic local provider for tests and developer runs. It performs no I/O at all:
every reading is derived from the binding and the receipt instant.

It is compiled **only** under the non-default `test-fixtures` feature. The module itself
does not exist when the feature is off, and `ProviderRegistry` does not register it, so a
release build contains no code that can serve a fixture reading.
`cargo build -p quota-providers --no-default-features` is the proof, and the architecture
gate (`cargo xtask check-architecture`) rejects a manifest that puts the feature in the
default set.

`cargo test -p quota-providers` selects the feature through this crate's own
`[dev-dependencies]` table, which a plain `cargo build` never reads.

It supports several independent accounts of one provider, each with its own pool and its
own reading: a healthy account, an account whose monthly window is exhausted while its
session window is healthy, an account whose boundary has passed, an account that reports
only native units with no denominator, and an account with an unlimited window.

## Registry

`ProviderRegistry::production()` holds the three real adapters;
`ProviderRegistry::with_fixture()` adds the fixture when the feature is on. `provider(id)`
returns `None` for any provider this build does not contain, so the application reports an
explicit unsupported-provider state. There is no dynamic adapter lookup, no downloaded
parser, and no plugin loading.

## Transport

One pooled `reqwest` client per adapter, so connection pooling works and policy is
uniform: a 5-second connect deadline, a 10-second request deadline, the scheduler's own
deadline when it is shorter, a 256 KiB response ceiling, and a redirect rule that follows
a redirect only while it stays on the origin of the first request. Cookies are never
stored. Nothing in this crate logs a header, a body, a token, a cookie, an address, a
profile path, or a full URL; every read span records only the provider, an opaque
connection identity, and a profile label.

The bundled TLS backend is rustls with the `ring` provider, selected explicitly so the
build needs no system TLS headers and no `cmake`. Only the endpoints named in this file
are ever contacted, and every request is HTTPS.

## Dependencies added by this crate

Versions were resolved from the crates.io sparse index
(`https://index.crates.io/<a>/<b>/<name>`) on 2026-10-01, and each one resolves to the
version already pinned in the workspace `Cargo.lock`.

| Dependency | Version | Resolved from                           | Date       |
| ---------- | ------- | --------------------------------------- | ---------- |
| `reqwest`  | 0.13.5  | `https://index.crates.io/re/qw/reqwest` | 2026-10-01 |
| `rustls`   | 0.23.45 | `https://index.crates.io/ru/st/rustls`  | 2026-10-01 |

`reqwest` is used with `default-features = false, features = ["rustls-no-provider"]`;
`rustls` is used with `default-features = false, features = ["ring", "std", "tls12"]`. The
default backend (`aws-lc-rs`) needs `cmake`, and the `native-tls` backend needs
`pkg-config`; neither is installed on this build machine, so the `ring` provider is
selected deliberately and installed once at process start.

## Tests

| File                           | What it proves                                                                                                     |
| ------------------------------ | ------------------------------------------------------------------------------------------------------------------ |
| `src/**` unit tests            | transport classification, credential shape and path rules, identity derivation, fixture profiles, offline decoding |
| `tests/codex_mapping.rs`       | Codex decoding: durations, categories, overspend, missing windows, unusable numbers                                |
| `tests/claude_mapping.rs`      | Claude decoding: fixed windows, named limits, model scope, the extra-spend cap, boundaries                         |
| `tests/opencode_go_mapping.rs` | `OpenCode` Go decoding, including the monthly window                                                               |
| `tests/fixture_isolation.rs`   | multi-account isolation through the compiled fixture adapter                                                       |

The suite never makes a live network call, never reads a real credential file, and never
needs one. Fixtures are sanitized payloads written for this crate, using obviously
synthetic values and example.invalid addresses.
