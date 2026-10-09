# quota-persistence

Typed durable state for Quota: SQLite repositories for monitoring state, and a versioned
document codec for presentation preferences.

## What this crate owns

[The architecture ownership table](../../../docs/architecture.md#persisted-state-ownership)
owns the Store, SQLite, native geometry, and credential boundaries. This crate implements
the Store document codec and SQLite repositories; it stores no token, cookie,
authorization header, or credential locator.

## One pool, one migration owner

The pool is opened once, by the caller:

```rust,ignore
let settings = SqlitePoolSettings::default();
let pool = quota_persistence::sqlite::open_pool(path, settings).await?;
quota_persistence::sqlite::run_migrations(&pool).await?;
quota_persistence::sqlite::verify_pool_settings(&pool, &settings).await?;
let repositories = SqliteRepositories::new(pool);
```

`SqliteRepositories::new` takes an already-open pool. Cloning a `sqlx::SqlitePool` clones
an `Arc` handle, so the repositories share the pool the desktop host resolved from
`tauri-plugin-sql`; this crate never creates a second pool and never claims a second
migration owner for the same file.

### `verify_pool_settings` is not decoration

SQLite reports `foreign_keys`, `journal_mode`, `synchronous`, and `busy_timeout` **per
connection**. A one-off `PRAGMA` statement configures the single connection that executed
it and proves nothing about the rest of the pool. `verify_pool_settings` acquires the
pool's whole connection budget, holds every connection at once, and reads all four
settings from each one. Any connection that disagrees, or any connection the pool cannot
open, is reported as `PersistenceError::PoolUnavailable`.

The four values are not configurable, so no call site can silently weaken durability: WAL
journal, `synchronous = FULL`, `foreign_keys = ON`, and a busy timeout bounded to
`250 ms ..= 30 s`.

## Migrations

The ordered list is hand-written (`sqlite::MIGRATIONS`), not `sqlx::migrate!`. The
reasons, in order of weight:

1. `sqlx::migrate!` derives version numbers from file names, while this crate explicitly
   declares each version beside its embedded SQL in `sqlite::MIGRATIONS`. That list is the
   owner of migration order and recorded versions.
2. The statements are embedded with `include_str!`, so a shipped binary cannot migrate
   against a file other than the one that was tested.

Each migration runs as one transaction. A failure rolls the migration back whole and
leaves the previous version recorded, so the next start retries it in full. A version
already recorded in `schema_migrations` is never re-applied, so a restart is a no-op.

## Queries are not compile-checked against a live database

`sqlx::query!` and `sqlx::query_as!` require either a live `DATABASE_URL` or a prepared
`.sqlx` cache produced by `cargo sqlx prepare`. Neither is available in this build
environment.

The repositories therefore use `sqlx::query` / `sqlx::query_scalar` with explicit `bind`
calls and fallible typed row mapping (`Row::try_get`, with the addressed table attached to
every failure). Table and column names are literals written in this crate; none can
originate from user input. A stored row outside this build's vocabulary becomes
`PersistenceError::RowRejected` naming the table and a fixed reason, never a substituted
value.

## The preference document

`PresentationPreferencesCodec` owns its storage keys as private constants. There is no
`set(key: String, value: Value)` API; the only `serde_json::Value` conversion in this
crate lives inside the codec.

- `load` returns the defaults only when the store holds **no** document.
- A document this build cannot read is **preserved** under the recovery key before an
  error is returned, so it is never silently overwritten.
- A document whose `schema_version` is outside this build reports
  `StoreDocumentUnsupported { found_version, supported_version }`.
- Corruption never produces a preferences value, so a corrupt document cannot set
  `always_on_top`, `launch_behavior`, or any other preference.
- `save` calls `PresentationPreferences::advance` before writing, so the revision is
  monotonic, and it preserves the previous document first.

`save` returns `PersistenceError::BackupFailed` when that preservation write fails, so a
failed save cannot destroy the last-known-good document.

## Features

| Feature         | Effect                                                                                             |
| --------------- | -------------------------------------------------------------------------------------------------- |
| _(default)_     | SQLite repositories and the document codec.                                                        |
| `tauri-plugins` | Adds `store::plugin`, the `tauri_plugin_store`-backed store implementing the same load/save shape. |

`store::plugin` does not exist when `tauri-plugins` is off, so a release build that does
not intend to link the plugin registry cannot reach it by accident.

## Retention

`measurement_history` carries **no** foreign key on purpose. It is optional, prunable
history, so deleting one account's rows or clearing one account's history can never
cascade into another account's rows. Every history statement is scoped by `account_id`.
`measurement_history(account_id, observed_at)` and
`measurement_history(window_id, observed_at)` index the retention queries, and pruning
never touches `accounts`, `alert_episodes`, `notification_outbox`, or `refresh_backoff`.

`account_balances` is different: it is not history but the state a prepaid balance is
measured from (the last top-up, the top-ups seen, and a week of hourly spending samples),
so it carries a foreign key and leaves with its account, and clearing reading history
leaves it alone. It is written in the same transaction as the rest of the account, as is
the account's `show_key_limit` switch.

`account_groups` holds the provider accounts the person puts keys together under: an id,
the provider and a name, never a key. An account names at most one group through
`accounts.group_id`, saved in the same transaction as the account. Every account write and
every account delete ends by deleting the groups no account names, so a group lives
exactly as long as it has members; deleting a group by any other path leaves its accounts
ungrouped (`ON DELETE SET NULL`), never deleted.

## Tests

The SQLite integration tests under `tests/` run against real migrated on-disk files in
temporary directories. They cover migration idempotency on reopen, foreign-key
enforcement, per-connection settings verification, account-delete isolation between two
accounts of one provider, one-transaction reading writes, history coalescing, scoped
backoff and `is_rate_limited_now`, episode and outbox deduplication, and the corrupt/newer
store-document round trips.
