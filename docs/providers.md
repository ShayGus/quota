# Providers

One section per provider. Facts here come from reading a real local quota
reporter's implementation and from the specification's provider notes. Anything
not observed there is marked as unverified rather than filled in.

Each adapter declares its supported connection methods, its identity scope, its
polling policy, and whether Quota may refresh credentials. Quota never owns a
credential it did not create, and no adapter returns a token, a cookie, an
authorization header, or unrelated account content to the UI.

## Codex

| Property | Value |
|---|---|
| Access method | Bearer-authenticated HTTPS reads: `https://chatgpt.com/backend-api/wham/usage`, then `https://chatgpt.com/backend-api/codex/usage`. An alternative documented path is the installed CLI's app server, `codex -s read-only -a never app-server`, with the JSON-RPC methods `initialize`, `account/read`, and `account/rateLimits/read`. |
| Identity source | `account_id` in `~/.codex/auth.json`, sent as the `ChatGPT-Account-Id` header |
| Credential path and owner | `~/.codex/auth.json`. The Codex CLI owns and refreshes it. Quota must never refresh it, must never overwrite it, and must not call logout when Quota disconnects. |
| Response fields | `primary_window` and `secondary_window`; `used_percent` in percentage points used; `reset_at` as epoch seconds or `reset_after_seconds`; `limit_window_seconds`. Exactly 18000 s maps to the session window and 604800 s to the weekly window. There is no monthly window. |
| Polling cadence | 300 s default |
| Multi-account support | Only through distinct supported CLI profiles. Quota must not rotate the user's active login, and a profile-isolation path must be verified before independent accounts are advertised. |
| Live verification | Not performed. No Codex login was read on this machine. |
| Undocumented-schema risk | High. The `/backend-api` routes are undocumented and carry no pinned schema version, so a field rename or a new wrapper object must degrade to a typed "unsupported" state rather than a fabricated zero. |

The app-server path is the preferred route in the specification because it is a
documented interface. Before it is adopted, its startup behaviour must be
verified for each supported version, and it must run with an isolated
configuration and a neutral working directory so that launching it cannot
activate project tools or user MCP servers. If a quota-only, read-only operation
cannot be demonstrated, that path does not ship. The allowlist is quota and
auth-status methods only: never start a conversation or a turn, never consume a
reset credit, never send a credit-request email, and never invoke a tool to
obtain a reading.

## Claude

| Property | Value |
|---|---|
| Access method | Bearer-authenticated read to `https://api.anthropic.com/api/oauth/usage`. Identity comes from `https://api.anthropic.com/api/oauth/profile`. |
| Identity source | `account.uuid` from the profile response |
| Credential path and owner | `~/.claude/.credentials.json`, or `$CLAUDE_CONFIG_DIR/.credentials.json` when that variable is set. Claude Code owns and refreshes it. |
| Response fields | `five_hour`, `seven_day`, and an optional `seven_day_opus`. Each carries a numeric `utilization` in percentage points used and `resets_at`. A `limits[]` array carries `percent`, `group`, `resets_at`, and an optional `scope.model.id`. An optional `extra_usage` object is a monthly extra-spend cap with `monthly_limit`, `used_credits`, and `decimal_places`. |
| Polling cadence | 300 s default |
| Multi-account support | Not established. The credential file is single-profile, so independent accounts require a supported isolation path. Until one is verified, extra connections are refused rather than served by cycling the active login. |
| Live verification | Not performed. No Claude login was read on this machine. |
| Undocumented-schema risk | High. The route is an OAuth endpoint that also requires the `anthropic-beta: oauth-2025-04-20` header, and it has no pinned schema version. |

`extra_usage` is an extra-spend role, not included quota. It is displayed as its
own labelled cap and never added to a percentage of a plan allowance. A paid
plan's API key is not assumed to expose subscription usage; a documented public
monitor integration takes precedence if one appears.

Where an external client owns refresh-token rotation, Quota does not rotate or
overwrite that token. It either delegates through a supported owner interface,
obtains app-owned authorization, or asks the user to reconnect through the owning
client. Automatic cookie decryption, browser-profile scanning, challenge
bypasses, and prompt-based CLI probes are excluded from version 1.

## OpenCode Go

| Property | Value |
|---|---|
| Access method | `GET https://opencode.ai/zen/go/v1/usage` with a bearer key |
| Identity source | None in the usage response. The response carries usage only, so identity must come from somewhere else, and it may remain unverified. This is a known gap, not a solved case. |
| Credential path and owner | `~/.local/share/opencode/auth.json`, under the `opencode-go` or `opencode` entry. OpenCode owns it. Quota has no refresh path. |
| Response fields | Usage appears under `usage` or at the root. Windows are `rollingUsage`, `weeklyUsage`, and `monthlyUsage`. The used percentage is one of the aliases `percent`, `percentUsed`, `usedPercent`, `usagePercent`. The reset field is one of `resetsAt`, `resetAt`, `reset_at`, `nextResetTime`, or `resetInSec`. |
| Polling cadence | 300 s default |
| Multi-account support | Not supported. A single key with no account identity in the response cannot be split into independent accounts. |
| Live verification | Not performed. No OpenCode credential was read, and no usage response was decoded. |
| Undocumented-schema risk | High. There is no vendor schema pin, and the alias lists above mean the decoder must accept several spellings of the same field. A missing alias is an unavailable measurement, not a zero. |

This is the only connector in scope that reports a monthly window. Because its
identity is not present in the usage response, an account row for this provider
carries a locally issued identity and must be labelled as unverified. A release
may claim monthly support only after one production-approved monthly connector
passes acceptance; a fixture demonstrates the renderer and proves nothing about
integration.

## Fixture

| Property | Value |
|---|---|
| Access method | Local, in-process. No network call. |
| Identity source | Fixed identities declared by the fixture itself |
| Credential path and owner | None |
| Response fields | A deterministic subset of the shapes above, chosen by the test |
| Polling cadence | Test-controlled, with an injected clock |
| Multi-account support | Yes: a test can declare several accounts, including several of the same provider |
| Live verification | Not applicable; the fixture is synthetic by definition |
| Undocumented-schema risk | None |

The fixture provider compiles only under the non-default `test-fixtures` Cargo
feature and never in a release build. `cargo xtask check-architecture` fails if
`test-fixtures` enters a default feature set, and `cargo xtask check-release`
fails the same way, so a shipping artifact cannot serve fixture data. Automated
tests use sanitized recorded schemas and a bounded local fake transport; they
never require a live paid account.

## Decoder rules common to all four

A decoder maps provider fields into domain types and nothing else. It does not
decide display text, and it does not invent a value. Specifically:

- a missing window is absent, never a zero;
- a missing reset timestamp stays absent, so no reset date is guessed;
- a percentage above 100 % used keeps its original value, and only the drawn arc
  is clamped;
- malformed data or an HTML error page produces a typed unsupported or invalid
  state, never fabricated zeros;
- the documented multi-bucket representation takes precedence over a legacy
  single-bucket view, and the two are never shown as duplicates.
