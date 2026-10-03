# Providers

One section per provider. Facts here come from reading a real local quota reporter's
implementation and from the specification's provider notes. Anything not observed there is
marked as unverified rather than filled in.

Each adapter declares its supported connection methods, its identity scope, its polling
policy, and whether Quota may refresh credentials. Quota never owns a credential it did
not create, and no adapter returns a token, a cookie, an authorization header, or
unrelated account content to the UI.

For default credential paths, `<user profile>` means the first non-blank `USERPROFILE`
value on Windows, falling back to `HOME`; other platforms use `HOME`. Explicit provider
variables take precedence over that profile, and blank environment values are ignored. No
profile path is guessed when neither a profile nor an override is available.

## Codex

| Property                  | Value                                                                                                                                                                                                                                                                                                                                                              |
| ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Access method             | Bearer-authenticated HTTPS reads; [the Codex adapter reference](../src-tauri/crates/quota-providers/README.md#codex) owns endpoint selection and request rules. An alternative documented path is the installed CLI's app server, `codex -s read-only -a never app-server`, with the JSON-RPC methods `initialize`, `account/read`, and `account/rateLimits/read`. |
| Identity source           | `tokens.account_id`, falling back to root-level `account_id`, in the Codex `auth.json`.                                                                                                                                                                                                                                                                            |
| Credential path and owner | `$CODEX_HOME/auth.json`, otherwise `<user profile>/.codex/auth.json`. The access token is read from `tokens.access_token`, falling back to root-level `access_token`. The Codex CLI owns and refreshes the file. Quota must never refresh or overwrite it, or call logout when Quota disconnects.                                                                  |
| Response fields           | See the [Codex decoder reference](../src-tauri/crates/quota-providers/README.md#codex) for wire fields, window mapping, and merge rules.                                                                                                                                                                                                                           |
| Polling cadence           | 300 s default                                                                                                                                                                                                                                                                                                                                                      |
| Multi-account support     | SingleProfile: one local credential profile. Independent production accounts remain unsupported until a supported profile-isolation path is verified; Quota never rotates the active login.                                                                                                                                                                        |
| Live verification         | Performed on 2026-10-03 on Windows: the owner connected their signed-in Codex and the weekly window and credits were read from `wham/usage`. That body sends `null` for empty lists, which the decoder now accepts. `codex/usage` answered with a web firewall page.                                                                                               |
| Undocumented-schema risk  | High. The `/backend-api` routes are undocumented and carry no pinned schema version, so a field rename or a new wrapper object must degrade to a typed "unsupported" state rather than a fabricated zero.                                                                                                                                                          |

The app-server path is the preferred route in the specification because it is a documented
interface. Before it is adopted, its startup behaviour must be verified for each supported
version, and it must run with an isolated configuration and a neutral working directory so
that launching it cannot activate project tools or user MCP servers. If a quota-only,
read-only operation cannot be demonstrated, that path does not ship. The allowlist is
quota and auth-status methods only: never start a conversation or a turn, never consume a
reset credit, never send a credit-request email, and never invoke a tool to obtain a
reading.

## Claude

| Property                  | Value                                                                                                                                                                                                                                                                             |
| ------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Access method             | Bearer-authenticated read to `https://api.anthropic.com/api/oauth/usage`. Identity comes from `https://api.anthropic.com/api/oauth/profile`.                                                                                                                                      |
| Identity source           | `account.uuid` from the profile response                                                                                                                                                                                                                                          |
| Credential path and owner | `$CLAUDE_CONFIG_DIR/.credentials.json`, otherwise `<user profile>/.claude/.credentials.json`. The token is read first from `claudeAiOauth.accessToken` or `claudeAiOauth.access_token`, then root-level `accessToken` or `access_token`. Claude Code owns and refreshes the file. |
| Response fields           | See the [Claude decoder reference](../src-tauri/crates/quota-providers/README.md#claude) for wire fields, window mapping, and merge rules.                                                                                                                                        |
| Polling cadence           | 300 s default                                                                                                                                                                                                                                                                     |
| Multi-account support     | Not established. The credential file is single-profile, so independent accounts require a supported isolation path. Until one is verified, extra connections are refused rather than served by cycling the active login.                                                          |
| Live verification         | Performed on 2026-10-02 on Windows: a real verification of the owner's signed-in Claude Code read the 5-hour, weekly, and extra-usage windows.                                                                                                                                    |
| Undocumented-schema risk  | High. The route is an OAuth endpoint that also requires the `anthropic-beta: oauth-2025-04-20` header, and it has no pinned schema version.                                                                                                                                       |

`extra_usage` is an extra-spend role, not included quota. It is displayed as its own
labelled cap and never added to a percentage of a plan allowance. A paid plan's API key is
not assumed to expose subscription usage; a documented public monitor integration takes
precedence if one appears.

Where an external client owns refresh-token rotation, Quota does not rotate or overwrite
that token. It either delegates through a supported owner interface, obtains app-owned
authorization, or asks the user to reconnect through the owning client. Automatic cookie
decryption, browser-profile scanning, challenge bypasses, and prompt-based CLI probes are
excluded from version 1.

## OpenCode Go

| Property                  | Value                                                                                                                                                                                                                                                                                                                                             |
| ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Access method             | `GET https://opencode.ai/zen/go/v1/usage` with a bearer key                                                                                                                                                                                                                                                                                       |
| Identity source           | None in the usage response. The response carries usage only, so identity must come from somewhere else, and it may remain unverified. This is a known gap, not a solved case.                                                                                                                                                                     |
| Credential path and owner | `OPENCODE_API_KEY` when set, otherwise `$XDG_DATA_HOME/opencode/auth.json`, otherwise `<user profile>/.local/share/opencode/auth.json`. Only the `opencode-go` entry is read, using `key`, `apiKey`, `api_key`, `access`, or `token`. The generic `opencode` entry belongs to another product. OpenCode owns the file. Quota has no refresh path. |
| Response fields           | Usage appears under `usage` or at the root. Windows are `rollingUsage`, `weeklyUsage`, and `monthlyUsage`. The used percentage is one of the aliases `percent`, `percentUsed`, `usedPercent`, `usagePercent`. The reset field is one of `resetsAt`, `resetAt`, `reset_at`, `nextResetTime`, or `resetInSec`.                                      |
| Polling cadence           | 300 s default                                                                                                                                                                                                                                                                                                                                     |
| Multi-account support     | Not supported. A single key with no account identity in the response cannot be split into independent accounts.                                                                                                                                                                                                                                   |
| Live verification         | Not performed. No live OpenCode credential or usage response was read; synthetic payloads are decoded by fixture tests.                                                                                                                                                                                                                           |
| Undocumented-schema risk  | High. There is no vendor schema pin, and the alias lists above mean the decoder must accept several spellings of the same field. A missing alias is an unavailable measurement, not a zero.                                                                                                                                                       |

Monthly window mappings are defined in the
[decoder reference](../src-tauri/crates/quota-providers/README.md). Because OpenCode Go's
identity is not present in the usage response, an account row for this provider carries a
locally issued identity and must be labelled as unverified. A release may claim monthly
support only after one production-approved monthly connector passes acceptance; a fixture
demonstrates the renderer and proves nothing about integration.

## OpenRouter

| Property                  | Value                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| ------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Access method             | `GET https://openrouter.ai/api/v1/key` and `GET https://openrouter.ai/api/v1/credits`, each with `Authorization: Bearer <key>`. Both are documented in OpenRouter's API reference.                                                                                                                                                                                                                                                                       |
| Identity source           | The key's own `label`, which OpenRouter writes as a masked form of the key, and `is_free_tier` as the plan. The response names no account, so each key is its own connection.                                                                                                                                                                                                                                                                            |
| Credential path and owner | Quota owns it. The person pastes an API key from openrouter.ai/settings/keys on the add-account page. It is held in memory while the account is verified, written to the system credential store (Windows Credential Manager, the Secret Service, or the macOS Keychain, service `app.quota.monitor`, entry named for the connection) only when the account is added, and deleted on disconnect. It never reaches SQLite, the Store, logs, or snapshots. |
| Response fields           | `data.limit`, `data.limit_remaining` and `data.limit_reset` (`daily`, `weekly`, `monthly` or `null`) become the key's spend cap; a `null` limit is unlimited. `data.total_credits` less `data.total_usage` is the credit balance. Amounts are US dollars, rounded to the cent.                                                                                                                                                                           |
| Polling cadence           | 300 s                                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| Multi-account support     | Yes: each pasted key is an independent connection.                                                                                                                                                                                                                                                                                                                                                                                                       |
| Live verification         | The sign-in flow, the credential store, and a refused key were checked in the running app on Windows; a reading with a real key is listed in the pull request.                                                                                                                                                                                                                                                                                           |
| Undocumented-schema risk  | Low. Both endpoints are documented. OpenRouter documents the credits endpoint for management keys, so an ordinary key that is refused there still connects, with its key limit only.                                                                                                                                                                                                                                                                     |

OpenRouter is pay as you go, so neither window is included quota: the balance is a credit
balance and the key limit is a spend cap. An account with no included allowance draws its
balance and caps on its card, balance first, and they never take part in the
least-remaining ranking.

## Z.ai GLM Coding Plan

| Property                  | Value                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Access method             | `GET https://api.z.ai/api/monitor/usage/quota/limit` with the key itself as the `Authorization` header (no scheme). Undocumented.                                                                                                                                                                                                                                                                                                     |
| Identity source           | None in the answer; the plan comes from `data.level` (`lite`, `pro`, `max`).                                                                                                                                                                                                                                                                                                                                                          |
| Credential path and owner | Quota owns it, as it does OpenRouter's: pasted on the add-account page, verified before anything is stored, kept in the system credential store under the connection, and deleted on disconnect. Each connection's profile is a fingerprint of its key, so two keys are told apart and a read checks the stored key is still the one confirmed, without keeping any of the key's text. Keys come from z.ai/manage-apikey/apikey-list. |
| Response fields           | `success` must be `true`. Each `data.limits[]` entry is a meter (`TOKENS_LIMIT`, `CREDIT_LIMIT`, or `TIME_LIMIT` for web tool requests) over a window from `unit` (3 hours, 4 days, 5 months, 6 weeks) and `number`: `usage` is the allowance, `currentValue` the use, `remaining`, `percentage` (server-rounded), and `nextResetTime` in epoch milliseconds.                                                                         |
| Polling cadence           | 300 s                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| Multi-account support     | Yes: each pasted key is an independent connection.                                                                                                                                                                                                                                                                                                                                                                                    |
| Live verification         | Not performed; no Z.ai key was available. Fixture tests cover the documented shapes.                                                                                                                                                                                                                                                                                                                                                  |
| Undocumented-schema risk  | Medium. omp and CodexBar read the same shape; CodexBar sends the key as a bearer token, omp without a scheme, which is what Quota does.                                                                                                                                                                                                                                                                                               |

Five hours is the session window, one week the weekly and one month the monthly; a count
against an allowance is kept in its unit, and the rounded `percentage` is used only
without one. Web tool requests are their own allowance, scoped `web-tools`.

## MiniMax Coding Plan

| Property                  | Value                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| ------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Access method             | `GET https://api.minimax.io/v1/token_plan/remains` with a bearer key. Undocumented.                                                                                                                                                                                                                                                                                                                                                                       |
| Identity source           | None in the answer; the plan comes from `current_subscribe_title`, `plan_name` or `combo_title` when present.                                                                                                                                                                                                                                                                                                                                             |
| Credential path and owner | Quota owns it, as it does OpenRouter's: pasted on the add-account page, verified before anything is stored, kept in the system credential store under the connection, and deleted on disconnect. Each connection's profile is a fingerprint of its key, so two keys are told apart and a read checks the stored key is still the one confirmed, without keeping any of the key's text. Coding Plan keys start `sk-cp-` and come from platform.minimax.io. |
| Response fields           | HTTP 200 even for a refused key: `base_resp.status_code` must be `0` (`1002` is a rate limit). `model_remains[]`, at the root or under `data`, holds one bucket per plan quota: `start_time`/`end_time` and `current_interval_remaining_percent`/`current_interval_status` for the rolling window, `weekly_end_time`, `current_weekly_remaining_percent` and `current_weekly_status` for the weekly one. Status `2` is exhausted and `3` unlimited.       |
| Polling cadence           | 300 s                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| Multi-account support     | Yes: each pasted key is an independent connection.                                                                                                                                                                                                                                                                                                                                                                                                        |
| Live verification         | Not performed; no MiniMax key was available. Fixture tests cover the documented shapes.                                                                                                                                                                                                                                                                                                                                                                   |
| Undocumented-schema risk  | Medium. The shape follows omp and MiniMax's own CLI.                                                                                                                                                                                                                                                                                                                                                                                                      |

The `general` bucket is the plan-wide quota; another bucket, such as `video`, is its own
allowance. A bucket reported unlimited with zero totals in both windows is a model outside
the plan and is left out. An exhausted status outranks a stale percentage.

## Kimi for Coding

| Property                  | Value                                                                                                                                                                                                                                                                                                                                                                                          |
| ------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Access method             | `GET https://api.kimi.com/coding/v1/usages` with a bearer token. Undocumented.                                                                                                                                                                                                                                                                                                                 |
| Identity source           | None in the answer; the plan comes from `user.membership.level`. The verified identity says which sign-in the connection uses.                                                                                                                                                                                                                                                                 |
| Credential path and owner | A pasted key from kimi.com/code/console, owned by Quota as above; or, when no key is pasted, the Kimi CLI's sign-in in `credentials/kimi-code.json` under `KIMI_SHARE_DIR`, otherwise `<user profile>/.kimi`. The CLI owns that file: Quota reads `access_token` and `expires_at` on every read, never refreshes the token, and asks the person to sign in with the CLI again once it expires. |
| Response fields           | `usage` (`limit`, `used`, `remaining`, `resetTime`) is the weekly allowance; each `limits[]` entry is a shorter window whose `window.duration` and `window.timeUnit` give its length (300 minutes is the session window) and whose `detail` carries the counts; `usages.limit_month_total.used_ratio` is the monthly allowance. Counts arrive as numbers or strings.                           |
| Polling cadence           | 300 s                                                                                                                                                                                                                                                                                                                                                                                          |
| Multi-account support     | Yes: each pasted key is its own connection, and the CLI's sign-in is one more.                                                                                                                                                                                                                                                                                                                 |
| Live verification         | Not performed; no Kimi sign-in was available. Fixture tests cover the documented shapes.                                                                                                                                                                                                                                                                                                       |
| Undocumented-schema risk  | Medium. The shape follows omp and the Kimi CLI's own `/usage` command, which sends only the bearer token, as Quota does.                                                                                                                                                                                                                                                                       |

## Fixture

| Property                  | Value                                                                            |
| ------------------------- | -------------------------------------------------------------------------------- |
| Access method             | Local, in-process. No network call.                                              |
| Identity source           | Fixed identities declared by the fixture itself                                  |
| Credential path and owner | None                                                                             |
| Response fields           | A deterministic subset of the shapes above, chosen by the test                   |
| Polling cadence           | Test-controlled, with an injected clock                                          |
| Multi-account support     | Yes: a test can declare several accounts, including several of the same provider |
| Live verification         | Not applicable; the fixture is synthetic by definition                           |
| Undocumented-schema risk  | None                                                                             |

The fixture provider compiles only under the non-default `test-fixtures` Cargo feature and
never in a release build. `cargo xtask check-architecture` fails if `test-fixtures` enters
a default feature set, and `cargo xtask check-release` fails the same way, so a shipping
artifact cannot serve fixture data. Automated tests use sanitized recorded schemas and a
bounded local fake transport; they never require a live paid account.

## Decoder rules common to every provider

A decoder maps provider fields into domain types and nothing else. It does not decide
display text, and it does not invent a value. Specifically:

- an unreported optional window is absent; an expected window is unavailable, never a zero
  (see the provider-specific expectations in the decoder reference);
- a missing reset timestamp stays absent, so no reset date is guessed;
- a percentage above 100 % used keeps its original value, and only the drawn arc is
  clamped;
- malformed data or an HTML error page produces a typed unsupported or invalid state,
  never fabricated zeros;
- provider-specific merge and duplicate precedence rules live in the
  [decoder reference](../src-tauri/crates/quota-providers/README.md), rather than one
  universal multi-bucket rule;
- an allowance that covers the whole account has the scope resource `account`
  (`quota_domain::quota::scope::ACCOUNT_RESOURCE`). The overview draws only these as a
  card's period rings and lists every other scope, such as one model family's weekly
  limit, under its own name, so a narrower window is never shown as the account's.

## Not implemented: multi-profile discovery

Quota does not scan for accounts. It does not read a browser profile, decrypt a cookie,
walk a WSL distribution, or run a prompt-based probe to find a signed-in session. A person
names the provider and the profile they want, and the connector asks that one source.

This is the specification's own boundary, not a shortcut. Spec 7.10 excludes "automatic
cookie decryption, browser-profile scanning, challenge bypasses, and prompt-based CLI
probes" from version 1, and spec 7.9 requires WSL profiles to be selected explicitly
rather than found by a scan. Both rules point the same way.

Where an external client owns refresh-token rotation, Quota does not rotate or overwrite
that token either. It delegates through the owning interface, asks the person to reconnect
through that client, or takes its own authorization.

What is present instead is the declaration each adapter owes the rest of the application:
`ProviderCapabilities.cardinality` states whether a provider permits independent
simultaneous accounts, a single externally owned profile, or several workspaces under one
authorization. The domain and the renderer handle all three, so a person may monitor
several accounts from one provider. What is missing is the automatic finding of those
accounts, which the specification does not ask for.
