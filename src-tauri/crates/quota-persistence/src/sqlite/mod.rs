//! Typed repositories over one `SQLite` pool.
//!
//! [`SqliteRepositories`] holds a handle to a pool that was already opened and
//! migrated. Cloning a [`sqlx::SqlitePool`] clones an `Arc` handle; it never
//! opens a second pool and never claims a second migration owner.

pub mod account_repository;
pub mod alert_repository;
pub mod backoff_repository;
pub mod connection_repository;
mod history_repository;
pub mod measurement_repository;
pub mod migrations;
mod outbox_repository;
pub mod pool;
pub mod preferences_repository;
mod reading_set;
mod rows;
mod window_writer;

use chrono::{DateTime, SecondsFormat, Utc};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::{PersistenceError, PersistenceResult};

pub use account_repository::{AccountRecord, AccountRepository, NewAccount};
pub use alert_repository::{AlertRepository, EpisodeKey};
pub use backoff_repository::{BackoffRecord, BackoffRepository};
pub use connection_repository::{ConnectionRecord, NewConnection};
pub use history_repository::HistoryEntry;
pub use measurement_repository::{MeasurementRepository, StoredMeasurement};
pub use migrations::run_migrations;
pub use pool::{SqlitePoolSettings, open_pool, verify_pool_settings};
pub use preferences_repository::OperationalPreferencesRepository;
/// The supervisor's own severity vocabulary, re-exported for storage callers.
pub use quota_core::ports::AlertLevel;
/// The typed repositories over one shared pool.
#[derive(Clone, Debug)]
pub struct SqliteRepositories {
    pool: sqlx::SqlitePool,
    accounts: AccountRepository,
    measurements: MeasurementRepository,
    backoff: BackoffRepository,
    alerts: AlertRepository,
    operational_preferences: OperationalPreferencesRepository,
}

impl SqliteRepositories {
    /// Wraps a pool that is already open and already migrated.
    #[must_use]
    pub fn new(pool: sqlx::SqlitePool) -> Self {
        Self {
            accounts: AccountRepository::new(pool.clone()),
            measurements: MeasurementRepository::new(pool.clone()),
            backoff: BackoffRepository::new(pool.clone()),
            alerts: AlertRepository::new(pool.clone()),
            operational_preferences: OperationalPreferencesRepository::new(pool.clone()),
            pool,
        }
    }

    /// The shared pool handle.
    #[must_use]
    pub fn pool(&self) -> &sqlx::SqlitePool {
        &self.pool
    }

    /// The connection, account, and pool-binding repository.
    #[must_use]
    pub fn accounts(&self) -> &AccountRepository {
        &self.accounts
    }

    /// The latest-reading and history repository.
    #[must_use]
    pub fn measurements(&self) -> &MeasurementRepository {
        &self.measurements
    }

    /// The scoped retry-deadline repository.
    #[must_use]
    pub fn backoff(&self) -> &BackoffRepository {
        &self.backoff
    }

    /// The alert-episode repository.
    #[must_use]
    pub fn alerts(&self) -> &AlertRepository {
        &self.alerts
    }

    /// The notification, operational privacy, and polling preference repository.
    #[must_use]
    pub fn operational_preferences(&self) -> &OperationalPreferencesRepository {
        &self.operational_preferences
    }
}

/// Text codecs for the closed vocabularies the schema stores as text.
///
/// The wire names come from the domain types' own Serde representation, so a
/// stored name cannot drift from the name the rest of the application uses.
pub(crate) mod codec {
    use super::{
        DateTime, DeserializeOwned, PersistenceError, PersistenceResult, SecondsFormat, Serialize,
        Utc,
    };

    /// Renders a closed-vocabulary value as its stored text.
    pub(crate) fn encode<T: Serialize>(
        value: &T,
        table: &'static str,
    ) -> PersistenceResult<String> {
        match serde_json::to_value(value) {
            Ok(serde_json::Value::String(text)) => Ok(text),
            Ok(_) => Err(PersistenceError::RowRejected {
                table,
                reason: "a closed-vocabulary column received a structured value",
            }),
            Err(_) => Err(PersistenceError::QueryFailed { table }),
        }
    }

    /// Reads a closed-vocabulary value back from its stored text.
    pub(crate) fn decode<T: DeserializeOwned>(
        text: &str,
        table: &'static str,
    ) -> PersistenceResult<T> {
        serde_json::from_value(serde_json::Value::String(text.to_owned())).map_err(|_| {
            PersistenceError::RowRejected {
                table,
                reason: "a stored name is outside this build's vocabulary",
            }
        })
    }

    /// Renders an instant as RFC 3339 UTC text.
    ///
    /// The format is fixed width to the millisecond, so stored instants sort
    /// correctly as text and a `WHERE at > ?` comparison is meaningful.
    pub(crate) fn instant(at: DateTime<Utc>) -> String {
        at.to_rfc3339_opts(SecondsFormat::Millis, true)
    }

    /// Reads an instant back from its stored text.
    pub(crate) fn parse_instant(
        text: &str,
        table: &'static str,
    ) -> PersistenceResult<DateTime<Utc>> {
        DateTime::parse_from_rfc3339(text)
            .map(|value| value.with_timezone(&Utc))
            .map_err(|_| PersistenceError::RowRejected {
                table,
                reason: "a stored timestamp is not RFC 3339 UTC text",
            })
    }

    /// Renders a JSON column value.
    pub(crate) fn json<T: Serialize>(value: &T, table: &'static str) -> PersistenceResult<String> {
        serde_json::to_string(value).map_err(|_| PersistenceError::QueryFailed { table })
    }
}
