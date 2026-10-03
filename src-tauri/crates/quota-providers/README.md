# quota-providers

Provider connectors for Quota. Each connector discovers a credential another client owns,
reads that provider's quota source through a bounded HTTP boundary, and normalises the
payload into `quota_domain::quota::window::QuotaWindow` values.

No connector starts a conversation, a turn, or a tool call. None writes to, refreshes, or
rotates an externally owned credential. None makes an inference or a model call to obtain
a quota reading.

## Verification status

**No live call was made by this crate or its tests.** Every endpoint below except
OpenRouter's is undocumented, and their field spellings were supplied as a specification,
not observed by this crate from a real login. The status column therefore separates what
an automated test proves from what only a real login can prove:

| Provider    | Endpoint behaviour | Field spellings | Live-login verification |
| ----------- | ------------------ | --------------- | ----------------------- |
| Codex       | ASSUMED            | ASSUMED         | Not performed           |
| Claude      | ASSUMED            | ASSUMED         | Not performed           |
| OpenCode Go | ASSUMED            | ASSUMED         | Not performed           |
| OpenRouter  | DOCUMENTED         | DOCUMENTED      | See the pull request    |
| Z.ai        | ASSUMED            | ASSUMED         | Not performed           |
| MiniMax     | ASSUMED            | ASSUMED         | Not performed           |
| Kimi        | ASSUMED            | ASSUMED         | Not performed           |
| Grok        | ASSUMED            | ASSUMED         | Not performed           |
| Muse Code   | ASSUMED            | ASSUMED         | Not performed           |
| Cursor      | ASSUMED            | ASSUMED         | Not performed           |
| Ollama      | ASSUMED            | ASSUMED         | Not performed           |

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

A body that is not a JSON object or is invalid JSON becomes `ProviderError::InvalidData`;
a shape that cannot deserialize into the supported wire types becomes
`ProviderError::UnsupportedSchema`. A recognised shape with no reported usage is rejected
according to the provider's mapping rules below. A value that is present but unusable
becomes `Measurement::Unavailable(UnavailableReason::InvalidResponse)` with a
`QuotaIssue`. No path turns a malformed payload into a zero.

## Codex

Reads the quota the Codex CLI reports for its own login.

- Endpoints: `GET https://chatgpt.com/backend-api/wham/usage`, then, when that yields
  nothing usable, `GET https://chatgpt.com/backend-api/codex/usage`. Both are
  undocumented. Authentication, authorization, and rate-limit failures stop immediately
  without trying the fallback. A decisive failure from the fallback takes precedence;
  otherwise, if both endpoints fail, the first error is retained.
- Credential discovery: see the
  [Codex credential reference](../../../docs/providers.md#codex).
- Credential owner: the Codex CLI. It owns and refreshes this file. This adapter never
  writes to, refreshes, or rotates it, and never runs the Codex CLI. An expired or
  rejected token becomes `ProviderError::Authentication` for the user to fix in Codex.
- Request: the bearer token, `Accept: application/json`, and `ChatGPT-Account-Id` only
  when the credential named an account. An empty header is not the same as an absent one.
- Decoded fields: `rate_limit`/`rateLimits`/`rate_limits` and the root object, both read,
  so a body that reports its review allowance at the root keeps it. The named container
  wins for a repeated bucket; within a container, the additional list wins over its map;
  `primary_window`/`primary` and `secondary_window`/`secondary`, either as a pair or as a
  window carried directly on the block; `code_review_rate_limit` as a pair or a window;
  `additional_rate_limits[]` with `limit_name`/`id`/`name` and a `rate_limit` pair or
  window, or the legacy `window` field; `rateLimitsByLimitId`. Used percent:
  `used_percent`/`usedPercent`, as a number or a numeric string. Reset:
  `reset_at`/`resetsAt` (epoch seconds or a date string) or `reset_after_seconds`.
  Duration: `limit_window_seconds` (seconds) or `windowDurationMins` (minutes). Plan:
  `plan_type`/`planType`. Optional `credits.balance`/`credits.unlimited`.
- A payload that carries only a non-negative numeric `credits.balance` or
  `credits.unlimited: true` is a reading: it connects with no allowance invented for it.
  Empty account, review, and additional-limit containers do not count as windows. Without
  a non-empty rate-limit window or a usable credit measurement, decoding returns
  `ProviderError::InvalidData`, including for an empty or unusable credits-only block.
  Used percentages and credit balances are decoded from the body only.
- Window mapping: exactly 18000 seconds maps to `QuotaCategory::Session`, exactly 604800
  seconds to `QuotaCategory::Weekly`, and any other duration keeps its own resource scope
  under `QuotaCategory::Custom`. Account-wide windows are the exception: any such window
  covering twenty days or more becomes `QuotaCategory::Monthly`, including in a pair. A
  lone first window covering at least one day has no fabricated second allowance; a
  shorter or unknown duration retains an unavailable secondary slot. A lone secondary
  window takes the primary slot. These rules follow the TaskbarQuota provider
  investigation report, section 12, Codex P1 row "Support credits-only and lone monthly
  responses", with acceptance evidence "Credits-only connects. No fabricated secondary
  allowance."
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
- Credential discovery: see the
  [Claude credential reference](../../../docs/providers.md#claude).
- Credential owner: Claude Code. It owns and refreshes this file. This adapter never
  refreshes the token, never writes to the file, and never runs Claude Code. The optional
  inference-based quota path is out of scope and does not exist in this crate.
- Identity: `account.uuid` from the profile route. Without it the reading is refused with
  `ProviderError::InvalidData`, so an account with no reported identity reads as
  unverified rather than inventing one. The address is masked in any label this crate
  produces.
- Decoded fields: `five_hour`, `seven_day`, and optional `seven_day_opus`,
  `seven_day_sonnet`, `seven_day_oauth_apps`, `seven_day_design`, and
  `seven_day_routines`, each with `utilization` (percent points used) and
  `resets_at`/`reset_at`; a `limits[]` array with `percent`, `group`/`kind`, `resets_at`,
  and optional `scope.model.id`/`display_name`; optional `extra_usage` with `is_enabled`,
  `monthly_limit`, `used_credits`, `decimal_places`.
- Window mapping: the fixed fields and the `limits[]` array are merged, never substituted
  for one another. A fixed field wins over a named counterpart for the same period and
  resource; other entries keep their own group and model scope. Only the two account-wide
  windows are expected: one the payload does not report becomes `Unavailable(NotReported)`
  and is recorded in `expected_but_missing`, unless a named limit with a reported
  percentage already describes that same account-wide period. Entries without a percentage
  are skipped. A model-specific or product allowance the payload does not mention is
  simply absent rather than missing.
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
- Credential discovery: see the
  [OpenCode Go credential reference](../../../docs/providers.md#opencode-go).
- Credential owner: the `OpenCode` login. Quota has no refresh path for this credential at
  all, and never writes to the file.
- Decoded fields: `usage` or the root object; `rollingUsage`/`rolling`,
  `weeklyUsage`/`weekly`, `monthlyUsage`/`monthly`. Used percent: `percent`,
  `percentUsed`, `usedPercent`, `usagePercent`; remaining percent: `percentRemaining`,
  `remainingPercent`. Reset: `resetsAt`, `resetAt`, `reset_at`, `nextResetTime` (a date
  string or epoch seconds) or `resetInSec`/`reset_in_sec`. Numbers or numeric strings.
- Window mapping: the rolling window is five hours (18000 seconds) and the weekly window
  is seven days (604800 seconds). The monthly window is mapped to `QuotaCategory::Monthly`
  and is never dropped.
- **Known limitation.** The usage response carries no account, workspace, or entitlement
  identity. This adapter does not invent one. It uses a stable local `QuotaPoolId` derived
  from the credential profile label and leaves the optional principal, workspace, and
  entitlement fields `None`, so the account reads as unverified. A binding whose principal
  is set is therefore refused on read. This is an accepted, honest limitation of this
  connector, not a defect to paper over.
- Cadence: a fixed interval with a 300-second minimum. HTTP 429 becomes `RateLimited` with
  the `Retry-After` deadline.

## OpenRouter

Reads the credits and the API key limit of a key the person pasted. Quota owns this
credential: there is no OpenRouter tool on the computer to read a sign-in from.

- Endpoints: `GET https://openrouter.ai/api/v1/key` and
  `GET https://openrouter.ai/api/v1/credits`, both documented, with a bearer key.
- Credential: kept by `secrets::SystemSecretStore` in the system credential store under
  the connection it signs in, and read from there on every read. During verification the
  host passes the pasted key through `discover_with` and `read_with` instead, so nothing
  is stored before the person adds the account. See the
  [OpenRouter reference](../../../docs/providers.md#openrouter).
- Decoded fields: `data.label`, `data.limit`, `data.limit_remaining`, `data.limit_reset`,
  `data.is_free_tier`; `data.total_credits`, `data.total_usage`. Amounts are US dollars,
  rounded to the cent.
- Window mapping: the key limit is an `ExtraSpendCap` in the period `limit_reset` names
  (`Custom` when it never resets), `Unlimited` when the key has no limit. The credit
  balance is a `CreditBalance` of what was bought less what was spent. Both are marked as
  read from a documented API.
- A key the credits endpoint refuses (401 or 403) still connects; only its key limit is
  shown.
- Cadence: a fixed interval with a 300-second minimum.

## Z.ai, MiniMax and Kimi

Three more providers signed in with a pasted key, through the same `keyed::KeyedSource` as
OpenRouter: the key comes from the system credential store on every read, and its
fingerprint (`decode::fingerprint`) is the connection's profile and pool seed. Kimi can
also read the Kimi CLI's own sign-in (`kimi::cli`), which it never refreshes; its
connection carries the profile `kimi-cli`. Endpoints, fields and window mapping are in
[the provider reference](../../../docs/providers.md#zai-glm-coding-plan).

## Cursor

Reads the plan's usage over the billing cycle with the sign-in the Cursor app keeps in its
SQLite state store (`cursor::app`), opened read-only on every read and never written; the
token is never refreshed. The account is the token's subject (`decode::jwt`). Endpoints
and fields are in [the provider reference](../../../docs/providers.md#cursor).

## Ollama Cloud

Reads `ollama.com/api/usage` with Ollama's own sign-in, signing each request with the
Ed25519 key Ollama keeps (`ollama::key`: an OpenSSH key parser and `ring`'s Ed25519), or
with a pasted API key through `keyed::KeyedSource`. See
[the provider reference](../../../docs/providers.md#ollama-cloud).

## Grok and Muse Code

Signed in through the browser with the OAuth device grant (`device::DeviceClient`), using
each provider's CLI's public client by the owner's decision, or with the CLI's own sign-in
(`grok::cli`, `muse::cli`), which Quota only reads. A granted token is kept as a
`device::StoredToken` in the system credential store; Grok's is refreshed by Quota when it
is about to expire and written back, Muse's does not expire. POST requests, which the
device grant and Muse's usage need, go through `post::PostRequest` at the same bounded
boundary as every GET, and a sign-in poll reads the OAuth refusal body
(`http::Answers::AnyJson`). Endpoints and fields are in
[the provider reference](../../../docs/providers.md#grok-supergrok).

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

`ProviderRegistry::production(secrets)` holds the eleven real adapters and the credential
store Quota's own sign-ins live in (`secrets::system_off_the_runtime(identifier)` opens
this system's store on a blocking worker and files every entry under the given application
identifier, so a development build keeps its own entries, or returns a store that refuses
every operation when there is none; the store is opened off the caller's runtime because
the Linux Secret Service connection drives a runtime of its own);
`ProviderRegistry::with_fixture(secrets)` adds the fixture when the feature is on.
`provider(id)` returns `None` for any provider this build does not contain, so the
application reports an explicit unsupported-provider state. There is no dynamic adapter
lookup, no downloaded parser, and no plugin loading.

## Transport

One pooled `reqwest` client per adapter, so connection pooling works and policy is
uniform: a 5-second connect deadline, a 10-second request deadline, the scheduler's own
deadline when it is shorter, a 256 KiB response ceiling, and a redirect rule that follows
a redirect only while it stays on the origin of the first request. Cookies are never
stored. Nothing in this crate logs a header, a body, a token, a cookie, an address, a
profile path, or a full URL; every read span records only the provider, an opaque
connection identity, and a profile label.

The bundled TLS backend is rustls with the `ring` provider, so the provider transport does
not require system TLS headers or `cmake`. The desktop host has separate native build
prerequisites in the root README. Only the endpoints named in this file are ever
contacted, and every request is HTTPS.

## Dependencies added by this crate

[The dependency record](../../../docs/dependencies.md) points to the authoritative
manifests and lockfiles. This crate's `Cargo.toml` declares the explicit rustls `ring`
backend; `http::ProviderHttp` installs that crypto provider once on first use.

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
synthetic values and example.invalid addresses. The one exception is the Linux test that
opens this system's own credential store to prove it opens from inside a running runtime:
it reads a generated connection and writes nothing, so a session with no Secret Service
behind it simply reports the store as unavailable.
