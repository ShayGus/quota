//! Typed `SQLite` storage for notification, privacy, and polling preferences.

use quota_domain::preferences::OperationalPreferences;
use sqlx::{Row, SqlitePool};

use crate::error::{PersistenceError, PersistenceResult, TableContext};

/// The singleton row that owns monitoring and operational preferences.
#[derive(Clone, Debug)]
pub struct OperationalPreferencesRepository {
    pool: SqlitePool,
}

impl OperationalPreferencesRepository {
    /// Wraps the already-open shared pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Loads saved policy values, or the documented defaults when no row exists.
    pub async fn load(&self) -> PersistenceResult<OperationalPreferences> {
        let row = sqlx::query(
            "SELECT preferences_revision, notification_policy_json, \
             operational_privacy_json, polling_policies_json \
             FROM monitoring_preferences WHERE id = 1",
        )
        .fetch_optional(&self.pool)
        .await
        .table("monitoring_preferences")?;
        let Some(row) = row else {
            return Ok(OperationalPreferences::default());
        };
        let revision: i64 = row
            .try_get("preferences_revision")
            .map_err(|_| invalid_row("revision"))?;
        let notifications_json: String = row
            .try_get("notification_policy_json")
            .map_err(|_| invalid_row("notification policy"))?;
        let privacy_json: String = row
            .try_get("operational_privacy_json")
            .map_err(|_| invalid_row("privacy policy"))?;
        let polling_json: String = row
            .try_get("polling_policies_json")
            .map_err(|_| invalid_row("polling policies"))?;
        Ok(OperationalPreferences {
            revision: u64::try_from(revision).map_err(|_| invalid_row("revision"))?,
            notifications: serde_json::from_str(&notifications_json)
                .map_err(|_| invalid_row("notification policy"))?,
            privacy: serde_json::from_str(&privacy_json)
                .map_err(|_| invalid_row("privacy policy"))?,
            polling: serde_json::from_str(&polling_json)
                .map_err(|_| invalid_row("polling policies"))?,
        })
    }

    /// Saves all operational fields with one atomic `SQLite` statement.
    pub async fn save(
        &self,
        preferences: &OperationalPreferences,
    ) -> PersistenceResult<OperationalPreferences> {
        let revision = i64::try_from(preferences.revision).map_err(|_| invalid_row("revision"))?;
        let notifications = serde_json::to_string(&preferences.notifications).map_err(|_| {
            PersistenceError::QueryFailed {
                table: "monitoring_preferences",
            }
        })?;
        let privacy = serde_json::to_string(&preferences.privacy).map_err(|_| {
            PersistenceError::QueryFailed {
                table: "monitoring_preferences",
            }
        })?;
        let polling = serde_json::to_string(&preferences.polling).map_err(|_| {
            PersistenceError::QueryFailed {
                table: "monitoring_preferences",
            }
        })?;
        sqlx::query(
            "INSERT INTO monitoring_preferences \
             (id, monitoring_state, preferences_revision, notification_policy_json, \
              operational_privacy_json, polling_policies_json) \
             VALUES (1, 'running', ?, ?, ?, ?) \
             ON CONFLICT (id) DO UPDATE SET \
              preferences_revision = excluded.preferences_revision, \
              notification_policy_json = excluded.notification_policy_json, \
              operational_privacy_json = excluded.operational_privacy_json, \
              polling_policies_json = excluded.polling_policies_json",
        )
        .bind(revision)
        .bind(notifications)
        .bind(privacy)
        .bind(polling)
        .execute(&self.pool)
        .await
        .table("monitoring_preferences")?;
        Ok(preferences.clone())
    }
}

fn invalid_row(reason: &'static str) -> PersistenceError {
    PersistenceError::RowRejected {
        table: "monitoring_preferences",
        reason,
    }
}
