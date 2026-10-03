# quota-core

Application services for Quota: the account registry, the snapshot builder, the refresh
supervisor, and alert evaluation.

This crate owns orchestration only. It may use Tokio. It does not depend on Tauri, on a
concrete provider adapter, on a storage plugin, or on any UI transport type. Provider,
persistence, and notification code implements the traits in `ports` and is injected from
outside.

## What this crate decides

- Which accounts exist, what each is bound to, and which generation a read belongs to. A
  late result from a superseded generation is rejected.
- What the canonical account order is, and at which revision it changed.
- When the next read for an account is due, under which strategy, and within which
  concurrency and backoff budget.
- When a downward threshold crossing deserves an alert, and when a recovery closes an
  episode.

## What this crate refuses to do

- It does not start a second async runtime. The desktop host spawns the supervisor once on
  Tauri's shared runtime and passes the handles in.
- It does not perform provider I/O. It calls a `ports::ProviderAdapter`.
- It does not persist. It calls a repository port.
- It does not guess a quota value. A provider that returns nothing produces
  `FetchOutcome::Partial` or a typed error, never a fabricated zero.
