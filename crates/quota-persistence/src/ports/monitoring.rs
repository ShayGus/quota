//! Adapter for the durable application-wide monitoring pause.

use async_trait::async_trait;
use quota_core::ports::{MonitoringRepository as MonitoringPort, RepositoryError};
use quota_domain::snapshot::MonitoringState;
use sqlx::Row;

use crate::sqlite::SqliteRepositories;

/// Implements the core monitoring-state port over the schema's singleton row.
#[derive(Clone, Debug)]
pub struct SqliteMonitoringPortAdapter {
    repositories: SqliteRepositories,
}

impl SqliteMonitoringPortAdapter {
    /// Wraps the already-open repository set.
    #[must_use]
    pub const fn new(repositories: SqliteRepositories) -> Self {
        Self { repositories }
    }
}

#[async_trait]
impl MonitoringPort for SqliteMonitoringPortAdapter {
    async fn load_monitoring_state(&self) -> Result<MonitoringState, RepositoryError> {
        let row = sqlx::query("SELECT monitoring_state FROM monitoring_preferences WHERE id = 1")
            .fetch_optional(self.repositories.pool())
            .await
            .map_err(|_| RepositoryError::new("sqlite", "monitoring state could not be read"))?;
        let Some(row) = row else {
            self.save_monitoring_state(&MonitoringState::Running)
                .await?;
            return Ok(MonitoringState::Running);
        };
        let value: String = row
            .try_get("monitoring_state")
            .map_err(|_| RepositoryError::new("sqlite", "monitoring state row is invalid"))?;
        match value.as_str() {
            "running" => Ok(MonitoringState::Running),
            "paused" => Ok(MonitoringState::Paused),
            _ => Err(RepositoryError::new(
                "sqlite",
                "monitoring state is outside the supported vocabulary",
            )),
        }
    }

    async fn save_monitoring_state(&self, state: &MonitoringState) -> Result<(), RepositoryError> {
        let value = match state {
            MonitoringState::Running => "running",
            MonitoringState::Paused => "paused",
            MonitoringState::RecoveryRequired(_) => {
                return Err(RepositoryError::new(
                    "sqlite",
                    "runtime recovery state is not a user monitoring preference",
                ));
            }
        };
        sqlx::query(
            "INSERT INTO monitoring_preferences (id, monitoring_state) VALUES (1, ?) \
             ON CONFLICT (id) DO UPDATE SET monitoring_state = excluded.monitoring_state",
        )
        .bind(value)
        .execute(self.repositories.pool())
        .await
        .map_err(|_| RepositoryError::new("sqlite", "monitoring state could not be saved"))?;
        Ok(())
    }
}
