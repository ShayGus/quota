//! The schema, its constraints, the pool settings, and migration idempotency.
//!
//! Fixtures use `unwrap` for readability; this file is not `#[cfg(test)]`, so
//! the workspace's test-mode allowance does not reach it.

#![expect(
    clippy::unwrap_used,
    reason = "test fixtures assert the setup they build, so a broken fixture must fail loudly"
)]

mod support;

use std::time::Duration as StdDuration;

use quota_persistence::PersistenceError;
use quota_persistence::sqlite::{
    OperationalPreferencesRepository, SqlitePoolSettings, open_pool, run_migrations,
    verify_pool_settings,
};
use support::TempDir;

#[tokio::test]
async fn migrations_are_idempotent_on_reopen() {
    let directory = TempDir::new("migrate");
    let path = directory.database();

    let first = open_pool(&path, SqlitePoolSettings::default())
        .await
        .unwrap();
    run_migrations(&first).await.unwrap();
    let versions: Vec<u32> =
        sqlx::query_scalar("SELECT version FROM schema_migrations ORDER BY version")
            .fetch_all(&first)
            .await
            .unwrap();
    first.close().await;

    let second = open_pool(&path, SqlitePoolSettings::default())
        .await
        .unwrap();
    run_migrations(&second).await.unwrap();
    let again: Vec<u32> =
        sqlx::query_scalar("SELECT version FROM schema_migrations ORDER BY version")
            .fetch_all(&second)
            .await
            .unwrap();

    assert_eq!(versions, vec![1, 2]);
    assert_eq!(versions, again, "a reopen must not re-record a version");
    second.close().await;
}

#[tokio::test]
async fn operational_preferences_migration_preserves_monitoring_and_adds_defaults() {
    let directory = TempDir::new("preferences-upgrade");
    let pool = open_pool(&directory.database(), SqlitePoolSettings::default())
        .await
        .unwrap();
    sqlx::query(
        "CREATE TABLE schema_migrations (version INTEGER PRIMARY KEY, applied_at TEXT NOT NULL)",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::raw_sql(include_str!("../migrations/0001_initial.sql"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO schema_migrations (version, applied_at) VALUES (1, ?)")
        .bind("2026-01-01T00:00:00Z")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO monitoring_preferences (id, monitoring_state) VALUES (1, 'paused')")
        .execute(&pool)
        .await
        .unwrap();

    run_migrations(&pool).await.unwrap();

    let state: String =
        sqlx::query_scalar("SELECT monitoring_state FROM monitoring_preferences WHERE id = 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    let preferences = OperationalPreferencesRepository::new(pool.clone())
        .load()
        .await
        .unwrap();
    assert_eq!(state, "paused");
    assert_eq!(
        preferences,
        quota_domain::preferences::OperationalPreferences::default()
    );
    pool.close().await;
}

#[tokio::test]
async fn every_declared_table_exists_after_migration() {
    let directory = TempDir::new("tables");
    let pool = support::migrated(&directory).await;

    let names: Vec<String> =
        sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .fetch_all(&pool)
            .await
            .unwrap();

    for required in [
        "schema_migrations",
        "connections",
        "accounts",
        "quota_pools",
        "account_pool_bindings",
        "quota_windows",
        "latest_measurements",
        "measurement_history",
        "alert_episodes",
        "notification_outbox",
        "refresh_backoff",
        "monitoring_preferences",
    ] {
        assert!(
            names.iter().any(|name| name == required),
            "{required} is missing"
        );
    }

    pool.close().await;
}

#[tokio::test]
async fn no_table_or_column_names_a_credential() {
    let directory = TempDir::new("no-secrets");
    let pool = support::migrated(&directory).await;

    // Comments are excluded: the schema's own header explains that it holds no
    // credential, and that sentence must not be mistaken for a column name.
    let ddl: String = sqlx::query_scalar(
        "SELECT group_concat(COALESCE(sql, ''), ' ') FROM sqlite_master WHERE sql IS NOT NULL",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    let lowered = ddl.to_lowercase();
    for forbidden in [
        "token",
        "cookie",
        "authorization",
        "secret",
        "password",
        "keyring",
    ] {
        assert!(
            !lowered.contains(forbidden),
            "`{forbidden}` must not appear in any table or column name"
        );
    }

    pool.close().await;
}

#[tokio::test]
async fn foreign_keys_are_enforced_and_reject_a_bad_insert() {
    let directory = TempDir::new("fk");
    let pool = support::migrated(&directory).await;

    let dangling_binding = sqlx::query(
        "INSERT INTO account_pool_bindings (account_id, pool_id)
         VALUES ('missing-account', 'missing-pool')",
    )
    .execute(&pool)
    .await;
    assert!(
        dangling_binding.is_err(),
        "a binding to a missing account must be refused"
    );

    let missing_pool = sqlx::query(
        "INSERT INTO quota_windows (
             id, pool_id, scope_resource, scope_label, category, semantics,
             metric_role, enforcement, source_kind, completeness, definition_version
         ) VALUES ('win-x', 'absent-pool', 'r', 'R', 'weekly', 'unknown',
                   'included_allowance', 'unknown', 'documented_api', 'complete', 1)",
    )
    .execute(&pool)
    .await;
    assert!(missing_pool.is_err(), "a window must reference a real pool");

    let orphan_history = sqlx::query(
        "INSERT INTO measurement_history (account_id, window_id, observed_at, received_at)
         VALUES ('absent', 'absent', '2026-03-01T00:00:00.000Z', '2026-03-01T00:00:00.000Z')",
    )
    .execute(&pool)
    .await;
    assert!(
        orphan_history.is_ok(),
        "prunable history carries no foreign key, so it can never cascade"
    );

    let second_monitoring_row = sqlx::query(
        "INSERT INTO monitoring_preferences (id, monitoring_state) VALUES (2, 'running')",
    )
    .execute(&pool)
    .await;
    assert!(
        second_monitoring_row.is_err(),
        "the monitoring singleton must reject a second row"
    );

    pool.close().await;
}

#[tokio::test]
async fn deleting_a_connection_cascades_to_its_accounts() {
    let directory = TempDir::new("cascade");
    let pool = support::migrated(&directory).await;
    let repositories = quota_persistence::SqliteRepositories::new(pool.clone());

    repositories
        .accounts()
        .upsert_connection(&support::connection("conn-1"))
        .await
        .unwrap();
    repositories
        .accounts()
        .upsert_account(&support::account("acct-1", "conn-1", 0, "First"))
        .await
        .unwrap();

    sqlx::query("DELETE FROM connections WHERE id = 'conn-1'")
        .execute(&pool)
        .await
        .unwrap();
    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM accounts")
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(
        remaining, 0,
        "ON DELETE CASCADE must remove the account rows"
    );
    pool.close().await;
}

#[tokio::test]
async fn pool_settings_are_verified_on_every_connection() {
    let directory = TempDir::new("settings");
    let settings = SqlitePoolSettings::default();
    let pool = open_pool(&directory.database(), settings).await.unwrap();
    run_migrations(&pool).await.unwrap();
    verify_pool_settings(&pool, &settings).await.unwrap();

    let journal_mode: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(&pool)
        .await
        .unwrap();
    let synchronous: i64 = sqlx::query_scalar("PRAGMA synchronous")
        .fetch_one(&pool)
        .await
        .unwrap();
    let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(&pool)
        .await
        .unwrap();
    let busy_timeout: i64 = sqlx::query_scalar("PRAGMA busy_timeout")
        .fetch_one(&pool)
        .await
        .unwrap();

    assert_eq!(journal_mode, SqlitePoolSettings::required_journal_mode());
    assert_eq!(synchronous, SqlitePoolSettings::required_synchronous());
    assert_eq!(foreign_keys, 1);
    assert_eq!(
        busy_timeout,
        i64::try_from(settings.busy_timeout().as_millis()).unwrap()
    );

    // Every connection the pool can hand out must be checked, not one. The
    // default pool budget is greater than one, so the loop below is not vacuous.
    assert!(settings.max_connections() > 1);

    // A pool whose live setting disagrees with the expected settings is refused.
    let disagreeing = SqlitePoolSettings::new(4, StdDuration::from_secs(9)).unwrap();
    assert_eq!(
        verify_pool_settings(&pool, &disagreeing).await.unwrap_err(),
        PersistenceError::PoolUnavailable
    );

    pool.close().await;
}

#[tokio::test]
async fn pool_settings_reject_values_outside_their_bounds() {
    assert_eq!(
        SqlitePoolSettings::new(0, SqlitePoolSettings::DEFAULT_BUSY_TIMEOUT).unwrap_err(),
        PersistenceError::PoolUnavailable
    );
    assert_eq!(
        SqlitePoolSettings::new(
            SqlitePoolSettings::MAX_MAX_CONNECTIONS + 1,
            SqlitePoolSettings::DEFAULT_BUSY_TIMEOUT
        )
        .unwrap_err(),
        PersistenceError::PoolUnavailable
    );
    assert_eq!(
        SqlitePoolSettings::new(
            SqlitePoolSettings::DEFAULT_MAX_CONNECTIONS,
            StdDuration::from_millis(1)
        )
        .unwrap_err(),
        PersistenceError::PoolUnavailable
    );
    assert_eq!(
        SqlitePoolSettings::new(
            SqlitePoolSettings::DEFAULT_MAX_CONNECTIONS,
            StdDuration::from_secs(31)
        )
        .unwrap_err(),
        PersistenceError::PoolUnavailable
    );
    assert!(SqlitePoolSettings::new(2, StdDuration::from_secs(2)).is_ok());
}
