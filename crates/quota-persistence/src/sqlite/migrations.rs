//! Ordered, embedded, transactional migrations.
//!
//! A hand-rolled ordered list is used instead of `sqlx::migrate!`. This crate
//! declares every version and embeds each SQL file in the binary. File names do
//! not define versions, so the list keeps the recorded version and applied SQL
//! visibly aligned. A shipped build cannot migrate against another file.

use sqlx::{Executor, SqlitePool, Transaction};

use crate::error::{PersistenceError, PersistenceResult, TableContext};
use crate::sqlite::codec;

/// The version the initial migration records.
pub const INITIAL_VERSION: u32 = 1;

/// The version that adds the durable operational settings columns.
pub const OPERATIONAL_PREFERENCES_VERSION: u32 = 2;

/// Every migration, in application order.
///
/// Each entry is `(version, statements)`. A statement list is executed as one
/// transaction; either the whole migration is recorded, or nothing of it.
pub const MIGRATIONS: &[(u32, &str)] = &[
    (
        INITIAL_VERSION,
        include_str!("../../migrations/0001_initial.sql"),
    ),
    (
        OPERATIONAL_PREFERENCES_VERSION,
        include_str!("../../migrations/0002_operational_preferences.sql"),
    ),
];

/// Applies every migration that `schema_migrations` does not already record.
///
/// The run is idempotent: a database that already records a version is left
/// untouched, so a restart applies nothing. Because the whole run happens
/// inside one transaction per migration, an interrupted migration leaves the
/// database at the previous version and the next start retries it in full.
///
/// # Errors
/// Returns [`PersistenceError::MigrationFailed`] when a migration cannot be
/// applied, and [`PersistenceError::QueryFailed`] when the version table cannot
/// be read.
pub async fn run_migrations(pool: &SqlitePool) -> PersistenceResult<()> {
    ensure_version_table(pool).await?;
    let applied = applied_versions(pool).await?;

    for (version, statements) in MIGRATIONS {
        if applied.contains(version) {
            continue;
        }
        apply(pool, *version, statements).await?;
    }
    Ok(())
}

/// Creates the version table when this database is new.
async fn ensure_version_table(pool: &SqlitePool) -> PersistenceResult<()> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
             version    INTEGER PRIMARY KEY,
             applied_at TEXT NOT NULL
         )",
    )
    .execute(pool)
    .await
    .table("schema_migrations")?;
    Ok(())
}

/// Reads the versions this database already records.
async fn applied_versions(pool: &SqlitePool) -> PersistenceResult<Vec<u32>> {
    sqlx::query_scalar::<_, u32>("SELECT version FROM schema_migrations")
        .fetch_all(pool)
        .await
        .map_err(|_| PersistenceError::QueryFailed {
            table: "schema_migrations",
        })
}

/// Applies one migration and records its version, in one transaction.
async fn apply(pool: &SqlitePool, version: u32, statements: &str) -> PersistenceResult<()> {
    let mut transaction = pool
        .begin()
        .await
        .map_err(|_| PersistenceError::MigrationFailed { version })?;

    run_statements(&mut transaction, statements)
        .await
        .map_err(|_| PersistenceError::MigrationFailed { version })?;

    sqlx::query("INSERT INTO schema_migrations (version, applied_at) VALUES (?, ?)")
        .bind(version)
        .bind(codec::instant(chrono::Utc::now()))
        .execute(&mut *transaction)
        .await
        .map_err(|_| PersistenceError::MigrationFailed { version })?;

    transaction
        .commit()
        .await
        .map_err(|_| PersistenceError::MigrationFailed { version })
}

/// Executes a migration file statement by statement.
///
/// [`sqlx::raw_sql`] executes a multi-statement script through the `SQLite`
/// multi-statement path, which does not surface a failing statement's own text.
/// Executing each statement separately keeps the failure attributable.
async fn run_statements(
    transaction: &mut Transaction<'_, sqlx::Sqlite>,
    statements: &str,
) -> Result<(), sqlx::Error> {
    for statement in statements.split(';') {
        let statement = statement.trim();
        if statement.is_empty() {
            continue;
        }
        transaction.execute(statement).await?;
    }
    Ok(())
}
