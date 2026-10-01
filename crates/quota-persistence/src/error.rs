//! Typed failures for durable state.
//!
//! Every variant carries only structured context, and no variant can carry a
//! row value: the `SQLite` schema has no credential column, and a rejected row is
//! described by its table and a fixed reason instead of by its content.

use sqlx::error::ErrorKind;
use thiserror::Error;

/// Why durable state could not be read or written.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PersistenceError {
    /// No pooled connection could be opened, or the requested pool settings are
    /// outside their permitted bounds, or a live connection disagrees with them.
    #[error("the SQLite pool is unavailable or does not carry the required settings")]
    PoolUnavailable,
    /// An ordered migration could not be applied, so the database is unchanged.
    #[error("migration {version} failed to apply")]
    MigrationFailed {
        /// The version of the migration that failed.
        version: u32,
    },
    /// A statement failed for a reason other than an integrity rule.
    #[error("a query against `{table}` failed")]
    QueryFailed {
        /// The table the statement addressed.
        table: &'static str,
    },
    /// The database refused a write because a constraint was violated.
    ///
    /// `SQLite` does not report a constraint name through `SQLx`, so this field
    /// carries the table the refused write addressed.
    #[error("`{constraint}` rejected the write: an integrity constraint was violated")]
    IntegrityViolation {
        /// The table the refused write addressed.
        constraint: &'static str,
    },
    /// A stored row was present but could not become a domain value.
    #[error("`{table}` returned a row this build cannot use: {reason}")]
    RowRejected {
        /// The table the row came from.
        table: &'static str,
        /// A fixed description of the rejected shape.
        reason: &'static str,
    },
    /// The preference store backend could not be reached.
    #[error("the preference store is unavailable")]
    StoreUnavailable,
    /// The stored preference document is not a document this build can read.
    #[error("the stored preference document could not be interpreted")]
    StoreDocumentCorrupt,
    /// The stored preference document declares a schema version outside this build.
    #[error(
        "the stored preference document declares schema version {found_version}, \
         and this build supports only {supported_version}"
    )]
    StoreDocumentUnsupported {
        /// The version the stored document declares.
        found_version: u32,
        /// The version this build writes and reads.
        supported_version: u32,
    },
    /// A document could not be copied into its recovery slot.
    #[error("the preference document could not be preserved for recovery")]
    BackupFailed,
}

/// The result of a durable-state operation.
pub type PersistenceResult<T> = Result<T, PersistenceError>;

/// Classifies a raw `SQLx` failure into the variant that describes it.
pub(crate) fn query_error(table: &'static str, error: sqlx::Error) -> PersistenceError {
    match error {
        sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed => PersistenceError::PoolUnavailable,
        sqlx::Error::Database(database) => match database.kind() {
            ErrorKind::UniqueViolation
            | ErrorKind::ForeignKeyViolation
            | ErrorKind::NotNullViolation
            | ErrorKind::CheckViolation => {
                PersistenceError::IntegrityViolation { constraint: table }
            }
            _ => PersistenceError::QueryFailed { table },
        },
        _ => PersistenceError::QueryFailed { table },
    }
}

/// Attaches the addressed table to a raw `SQLx` failure.
pub(crate) trait TableContext<T> {
    /// Classifies this failure against the table the statement addressed.
    fn table(self, table: &'static str) -> PersistenceResult<T>;
}

impl<T> TableContext<T> for Result<T, sqlx::Error> {
    fn table(self, table: &'static str) -> PersistenceResult<T> {
        self.map_err(|error| query_error(table, error))
    }
}
