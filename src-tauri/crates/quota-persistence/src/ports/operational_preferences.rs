//! Adapter from the operational preference port to its `SQLite` owner.

use async_trait::async_trait;
use quota_core::ports::{OperationalPreferencesRepository as PreferencePort, RepositoryError};
use quota_domain::preferences::OperationalPreferences;

use crate::sqlite::SqliteRepositories;

/// Implements operational preferences over the shared `SQLite` repository set.
#[derive(Clone, Debug)]
pub struct SqliteOperationalPreferencesPortAdapter {
    repositories: SqliteRepositories,
}

impl SqliteOperationalPreferencesPortAdapter {
    /// Wraps the already-open repository set.
    #[must_use]
    pub const fn new(repositories: SqliteRepositories) -> Self {
        Self { repositories }
    }
}

#[async_trait]
impl PreferencePort for SqliteOperationalPreferencesPortAdapter {
    async fn load(&self) -> Result<OperationalPreferences, RepositoryError> {
        self.repositories
            .operational_preferences()
            .load()
            .await
            .map_err(|error| RepositoryError::new("sqlite", error.to_string()))
    }

    async fn save(
        &self,
        preferences: &OperationalPreferences,
    ) -> Result<OperationalPreferences, RepositoryError> {
        self.repositories
            .operational_preferences()
            .save(preferences)
            .await
            .map_err(|error| RepositoryError::new("sqlite", error.to_string()))
    }
}
