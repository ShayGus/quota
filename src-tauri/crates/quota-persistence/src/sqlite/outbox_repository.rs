//! The durable notification outbox.
//!
//! A durable outbox deduplicates retries; it does not make an operating-system
//! notification exactly-once across every crash boundary. What it does promise
//! is that one episode key can hold at most one queued notification, which is
//! what stops a threshold crossing from producing a storm.

use chrono::{DateTime, Utc};

use crate::error::{PersistenceResult, TableContext};
use crate::sqlite::alert_repository::{AlertRepository, EpisodeKey, EpisodeParts, outbox_key};
use crate::sqlite::codec;

impl AlertRepository {
    /// Enqueues one notification for an episode, at most once per episode key.
    ///
    /// # Returns
    /// `true` when this call inserted the entry, `false` when an entry for the
    /// same episode key already existed.
    ///
    /// # Errors
    /// Returns a typed persistence error when the write is refused.
    pub async fn enqueue_notification(
        &self,
        key: &EpisodeKey,
        created_at: DateTime<Utc>,
    ) -> PersistenceResult<bool> {
        let parts = EpisodeParts::of(key);
        let episode_key = outbox_key(key)?;

        let inserted = sqlx::query(
            "INSERT INTO notification_outbox (
                 account_id, window_id, level, episode_key, created_at, delivered_at
             ) VALUES (?, ?, ?, ?, ?, NULL)
             ON CONFLICT (episode_key) DO NOTHING",
        )
        .bind(&parts.account_id)
        .bind(&parts.window_id)
        .bind(&parts.level)
        .bind(&episode_key)
        .bind(codec::instant(created_at))
        .execute(&self.pool)
        .await
        .table("notification_outbox")?
        .rows_affected();

        // `SQLite` reports zero changed rows when the conflict clause suppressed
        // the insert, so this row count is the deduplication answer itself.
        Ok(inserted == 1)
    }

    /// Lists the episode keys that have an undelivered notification.
    ///
    /// # Errors
    /// Returns a typed persistence error when the read fails.
    pub async fn undelivered_keys(&self) -> PersistenceResult<Vec<String>> {
        sqlx::query_scalar("SELECT episode_key FROM notification_outbox WHERE delivered_at IS NULL")
            .fetch_all(&self.pool)
            .await
            .table("notification_outbox")
    }

    /// Reports whether one episode key already has a queued notification.
    ///
    /// # Errors
    /// Returns a typed persistence error when the read fails.
    pub async fn is_queued(&self, key: &EpisodeKey) -> PersistenceResult<bool> {
        let episode_key = outbox_key(key)?;
        let queued: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM notification_outbox WHERE episode_key = ?")
                .bind(&episode_key)
                .fetch_optional(&self.pool)
                .await
                .table("notification_outbox")?;
        Ok(queued.is_some())
    }
}
