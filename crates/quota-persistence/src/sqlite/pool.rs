//! Pool construction and proof that every pooled connection carries the
//! required `SQLite` settings.
//!
//! A one-off `PRAGMA` statement configures the single connection that executed
//! it. `SQLite` reports foreign-key enforcement, journal mode, synchronous
//! setting, and busy timeout per connection, so a pooled database is only
//! configured once every connection in the pool agrees. [`verify_pool_settings`]
//! is what proves that; it is not a decoration.

use std::path::Path;
use std::time::Duration;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};

use crate::error::{PersistenceError, PersistenceResult};

/// The `PRAGMA journal_mode` value this crate requires.
const REQUIRED_JOURNAL_MODE: &str = "wal";
/// The `PRAGMA synchronous` value this crate requires. `SQLite` reports `2` for `FULL`.
const REQUIRED_SYNCHRONOUS: i64 = 2;

/// The bounded settings applied to every pooled connection.
///
/// Journal mode, synchronous setting, and foreign-key enforcement are fixed by
/// this crate and are not configurable, so no call site can silently weaken
/// durability. Only the two values below have a permitted range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SqlitePoolSettings {
    max_connections: u32,
    busy_timeout: Duration,
}

impl SqlitePoolSettings {
    /// The fewest pooled connections this crate accepts.
    pub const MIN_MAX_CONNECTIONS: u32 = 1;
    /// The most pooled connections this crate accepts.
    pub const MAX_MAX_CONNECTIONS: u32 = 16;
    /// The default number of pooled connections.
    pub const DEFAULT_MAX_CONNECTIONS: u32 = 4;
    /// The shortest accepted busy timeout.
    pub const MIN_BUSY_TIMEOUT: Duration = Duration::from_millis(250);
    /// The longest accepted busy timeout.
    pub const MAX_BUSY_TIMEOUT: Duration = Duration::from_secs(30);
    /// The default busy timeout. A write that meets a lock waits this long.
    pub const DEFAULT_BUSY_TIMEOUT: Duration = Duration::from_secs(5);

    /// Builds a settings value inside the permitted bounds.
    ///
    /// # Errors
    /// Returns [`PersistenceError::PoolUnavailable`] when either value is
    /// outside its range.
    pub fn new(max_connections: u32, busy_timeout: Duration) -> PersistenceResult<Self> {
        if !(Self::MIN_MAX_CONNECTIONS..=Self::MAX_MAX_CONNECTIONS).contains(&max_connections) {
            return Err(PersistenceError::PoolUnavailable);
        }
        if busy_timeout < Self::MIN_BUSY_TIMEOUT || busy_timeout > Self::MAX_BUSY_TIMEOUT {
            return Err(PersistenceError::PoolUnavailable);
        }
        Ok(Self {
            max_connections,
            busy_timeout,
        })
    }

    /// The number of connections the pool will open.
    #[must_use]
    pub const fn max_connections(self) -> u32 {
        self.max_connections
    }

    /// How long a connection waits for a lock before failing.
    #[must_use]
    pub const fn busy_timeout(self) -> Duration {
        self.busy_timeout
    }

    /// The journal mode this crate requires, as `PRAGMA journal_mode` reports it.
    #[must_use]
    pub const fn required_journal_mode() -> &'static str {
        REQUIRED_JOURNAL_MODE
    }

    /// The synchronous setting this crate requires, as `PRAGMA synchronous` reports it.
    #[must_use]
    pub const fn required_synchronous() -> i64 {
        REQUIRED_SYNCHRONOUS
    }
}

impl Default for SqlitePoolSettings {
    fn default() -> Self {
        Self {
            max_connections: Self::DEFAULT_MAX_CONNECTIONS,
            busy_timeout: Self::DEFAULT_BUSY_TIMEOUT,
        }
    }
}

/// Opens one pool over the database file at `path`, with the required settings.
///
/// The file is created when missing. This function does not migrate; pair it
/// with [`crate::sqlite::run_migrations`].
///
/// # Errors
/// Returns [`PersistenceError::PoolUnavailable`] when the pool cannot be opened.
pub async fn open_pool(
    path: &Path,
    settings: SqlitePoolSettings,
) -> PersistenceResult<sqlx::SqlitePool> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Full)
        .busy_timeout(settings.busy_timeout);

    SqlitePoolOptions::new()
        .max_connections(settings.max_connections)
        .acquire_timeout(settings.busy_timeout)
        .connect_with(options)
        .await
        .map_err(|_| PersistenceError::PoolUnavailable)
}

/// Checks the four required settings on every connection the pool can hand out.
///
/// The pool's whole connection budget is acquired at once and held while each
/// connection is read, so a connection that opens the connection lazily and
/// silently keeps `SQLite`'s defaults cannot pass unnoticed. A single one-off
/// `PRAGMA` on one connection would prove nothing about the rest.
///
/// # Errors
/// Returns [`PersistenceError::PoolUnavailable`] when a connection cannot be
/// acquired or disagrees with `settings`. The observed values are reported on
/// the `quota_persistence::pool` tracing target.
pub async fn verify_pool_settings(
    pool: &sqlx::SqlitePool,
    settings: &SqlitePoolSettings,
) -> PersistenceResult<()> {
    let mut held = Vec::with_capacity(settings.max_connections() as usize);
    for _ in 0..settings.max_connections() {
        let connection = pool
            .acquire()
            .await
            .map_err(|_| PersistenceError::PoolUnavailable)?;
        held.push(connection);
    }

    for (index, connection) in held.iter_mut().enumerate() {
        verify_one(connection, settings, index).await?;
    }
    Ok(())
}

/// Reads the four settings from one connection and compares them.
async fn verify_one(
    connection: &mut sqlx::SqliteConnection,
    settings: &SqlitePoolSettings,
    index: usize,
) -> PersistenceResult<()> {
    let journal_mode: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(&mut *connection)
        .await
        .map_err(|_| PersistenceError::PoolUnavailable)?;
    let synchronous: i64 = sqlx::query_scalar("PRAGMA synchronous")
        .fetch_one(&mut *connection)
        .await
        .map_err(|_| PersistenceError::PoolUnavailable)?;
    let foreign_keys: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(&mut *connection)
        .await
        .map_err(|_| PersistenceError::PoolUnavailable)?;
    let busy_timeout: i64 = sqlx::query_scalar("PRAGMA busy_timeout")
        .fetch_one(&mut *connection)
        .await
        .map_err(|_| PersistenceError::PoolUnavailable)?;

    let expected_busy_timeout = i64::try_from(settings.busy_timeout().as_millis())
        .map_err(|_| PersistenceError::PoolUnavailable)?;
    let agrees = journal_mode == REQUIRED_JOURNAL_MODE
        && synchronous == REQUIRED_SYNCHRONOUS
        && foreign_keys == 1
        && busy_timeout == expected_busy_timeout;

    if agrees {
        return Ok(());
    }

    tracing::warn!(
        target: "quota_persistence::pool",
        connection = index,
        journal_mode = %journal_mode,
        synchronous,
        foreign_keys,
        busy_timeout,
        required_journal_mode = REQUIRED_JOURNAL_MODE,
        required_synchronous = REQUIRED_SYNCHRONOUS,
        required_busy_timeout = expected_busy_timeout,
        "a pooled connection does not carry the required SQLite settings"
    );
    Err(PersistenceError::PoolUnavailable)
}
