//! Adapters from the application's port traits to the typed repositories.
//!
//! `quota-core` owns the interfaces the application depends on. This module
//! binds them to the repositories in this crate, so the desktop host can inject
//! a concrete durable owner without the core importing SQL or a plugin.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use quota_core::ports::{BackoffRepository as BackoffPort, BackoffState, RepositoryError};
use quota_domain::polling::LimitScope;

use crate::sqlite::SqliteRepositories;
use crate::sqlite::backoff_repository::BackoffRecord;

/// Maps a persistence failure onto the port error, naming the durable owner.
fn map_error(error: &crate::PersistenceError) -> RepositoryError {
    RepositoryError::new("sqlite", error.to_string())
}

/// Implements the application's scoped backoff port over the typed repository.
#[derive(Clone, Debug)]
pub struct SqliteBackoffRepository {
    inner: SqliteRepositories,
}

impl SqliteBackoffRepository {
    /// Wraps a repository set.
    #[must_use]
    pub const fn new(inner: SqliteRepositories) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl BackoffPort for SqliteBackoffRepository {
    async fn persist_backoff(
        &self,
        scope: &LimitScope,
        state: BackoffState,
    ) -> Result<(), RepositoryError> {
        let record = BackoffRecord {
            scope: scope.clone(),
            attempts: state.attempts,
            next_eligible_at: state.next_eligible_at,
            provider_retry_after: state.provider_retry_after,
        };
        self.inner
            .backoff()
            .persist(&record)
            .await
            .map_err(|error| map_error(&error))
    }

    async fn load_backoff(
        &self,
        scope: &LimitScope,
    ) -> Result<Option<BackoffState>, RepositoryError> {
        self.inner
            .backoff()
            .read(scope)
            .await
            .map(|record| {
                record.map(|value| BackoffState {
                    attempts: value.attempts,
                    next_eligible_at: value.next_eligible_at,
                    provider_retry_after: value.provider_retry_after,
                })
            })
            .map_err(|error| map_error(&error))
    }

    async fn clear_backoff(&self, scope: &LimitScope) -> Result<(), RepositoryError> {
        self.inner
            .backoff()
            .clear(scope)
            .await
            .map(|_deleted| ())
            .map_err(|error| map_error(&error))
    }
}

/// Whether one scope is still waiting out a deadline at `now`.
///
/// This is the same predicate the scheduler uses, exposed so a caller can ask
/// without loading the whole record.
#[derive(Clone, Debug)]
pub struct SqliteRateLimitProbe {
    inner: SqliteRepositories,
}

impl SqliteRateLimitProbe {
    /// Wraps a repository set.
    #[must_use]
    pub const fn new(inner: SqliteRepositories) -> Self {
        Self { inner }
    }

    /// Whether the scope's deadline is still in the future.
    ///
    /// # Errors
    /// Returns a typed persistence error when the deadline cannot be read.
    pub async fn is_waiting(
        &self,
        scope: &LimitScope,
        now: DateTime<Utc>,
    ) -> Result<bool, RepositoryError> {
        self.inner
            .backoff()
            .is_rate_limited_now(scope, now)
            .await
            .map_err(|error| map_error(&error))
    }
}
