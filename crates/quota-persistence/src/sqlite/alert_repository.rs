//! Alert episodes and their notification outbox.
//!
//! An episode is keyed by account, window, definition version, and level, so
//! two independent accounts never share an episode merely because they name the
//! same provider. The key is also the outbox deduplication key, so a second
//! crossing of the same threshold cannot enqueue a second notification.

use chrono::{DateTime, Utc};
use quota_domain::ids::{AccountId, QuotaWindowId};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use crate::error::{PersistenceError, PersistenceResult, TableContext};
use crate::sqlite::codec;

/// The severity an episode was opened at.
///
/// The thresholds behind these levels are a supervisor policy, not a storage
/// concern; storage records which level an episode belongs to so a level can
/// re-arm independently of the others.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertLevel {
    /// The allowance is running low.
    Low,
    /// The allowance is nearly exhausted.
    Critical,
    /// The allowance is exhausted.
    Exhausted,
}

/// The identity of one alert episode.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct EpisodeKey {
    /// The account the allowance belongs to.
    pub account_id: AccountId,
    /// The window the allowance belongs to.
    pub window_id: QuotaWindowId,
    /// The definition version the episode was opened against.
    pub definition_version: u32,
    /// The severity level.
    pub level: AlertLevel,
}

impl EpisodeKey {
    /// Builds an episode identity.
    #[must_use]
    pub fn new(
        account_id: AccountId,
        window_id: QuotaWindowId,
        definition_version: u32,
        level: AlertLevel,
    ) -> Self {
        Self {
            account_id,
            window_id,
            definition_version,
            level,
        }
    }
}

/// One stored alert episode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlertEpisode {
    /// The episode identity.
    pub key: EpisodeKey,
    /// When the episode was opened.
    pub opened_at: DateTime<Utc>,
    /// When a notification for this episode was enqueued.
    pub armed_at: Option<DateTime<Utc>>,
    /// When a verified recovery closed the episode.
    pub closed_at: Option<DateTime<Utc>>,
}

/// Opens, arms, and closes alert episodes.
#[derive(Clone, Debug)]
pub struct AlertRepository {
    pool: SqlitePool,
}

impl AlertRepository {
    /// Wraps an open pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Opens an episode unless one is already live for this key.
    ///
    /// A closed episode for the same key is re-opened in place, so a
    /// re-crossing after a verified recovery does not need a new row and the
    /// outbox deduplication key keeps its meaning across periods.
    ///
    /// # Returns
    /// `true` when this call opened the episode, `false` when a live episode
    /// already exists and nothing was changed.
    ///
    /// # Errors
    /// Returns a typed persistence error when the write is refused.
    pub async fn open_episode(
        &self,
        key: &EpisodeKey,
        opened_at: DateTime<Utc>,
    ) -> PersistenceResult<bool> {
        let (account_id, window_id, version, level) = key_parts(key)?;
        let changed = sqlx::query(
            "INSERT INTO alert_episodes (
                 account_id, window_id, definition_version, level, opened_at, armed_at, closed_at
             ) VALUES (?, ?, ?, ?, ?, NULL, NULL)
             ON CONFLICT (account_id, window_id, definition_version, level)
             DO UPDATE SET
                 opened_at = excluded.opened_at,
                 armed_at = NULL,
                 closed_at = NULL
             WHERE alert_episodes.closed_at IS NOT NULL",
        )
        .bind(&account_id)
        .bind(&window_id)
        .bind(version)
        .bind(&level)
        .bind(codec::instant(opened_at))
        .execute(&self.pool)
        .await
        .table("alert_episodes")?
        .rows_affected();

        Ok(changed == 1)
    }

    /// Reports whether one episode is still open for this key.
    ///
    /// # Errors
    /// Returns a typed persistence error when the read fails.
    pub async fn is_open(&self, key: &EpisodeKey) -> PersistenceResult<bool> {
        let (account_id, window_id, version, level) = key_parts(key)?;
        let open: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM alert_episodes
              WHERE account_id = ? AND window_id = ? AND definition_version = ? AND level = ?
                AND closed_at IS NULL",
        )
        .bind(&account_id)
        .bind(&window_id)
        .bind(version)
        .bind(&level)
        .fetch_optional(&self.pool)
        .await
        .table("alert_episodes")?;
        Ok(open.is_some())
    }

    /// Reports whether a notification is already enqueued for this open episode.
    ///
    /// A supervisor consults this before enqueuing, so a level cannot notify
    /// twice while the same episode is open.
    ///
    /// # Errors
    /// Returns a typed persistence error when the read fails.
    pub async fn is_armed(&self, key: &EpisodeKey) -> PersistenceResult<bool> {
        let (account_id, window_id, version, level) = key_parts(key)?;
        let armed: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM alert_episodes
              WHERE account_id = ? AND window_id = ? AND definition_version = ? AND level = ?
                AND armed_at IS NOT NULL AND closed_at IS NULL",
        )
        .bind(&account_id)
        .bind(&window_id)
        .bind(version)
        .bind(&level)
        .fetch_optional(&self.pool)
        .await
        .table("alert_episodes")?;
        Ok(armed.is_some())
    }

    /// Records that a notification for this episode has been enqueued.
    ///
    /// # Errors
    /// Returns [`PersistenceError::RowRejected`] when no live episode exists.
    pub async fn mark_armed(
        &self,
        key: &EpisodeKey,
        armed_at: DateTime<Utc>,
    ) -> PersistenceResult<()> {
        let (account_id, window_id, version, level) = key_parts(key)?;
        let updated = sqlx::query(
            "UPDATE alert_episodes
                SET armed_at = ?
              WHERE account_id = ? AND window_id = ? AND definition_version = ? AND level = ?
                AND closed_at IS NULL",
        )
        .bind(codec::instant(armed_at))
        .bind(&account_id)
        .bind(&window_id)
        .bind(version)
        .bind(&level)
        .execute(&self.pool)
        .await
        .table("alert_episodes")?
        .rows_affected();
        require_one(updated)
    }

    /// Closes an episode after a verified recovery.
    ///
    /// # Returns
    /// `true` when this call closed an open episode, `false` when it was
    /// already closed.
    ///
    /// # Errors
    /// Returns [`PersistenceError::RowRejected`] when no episode exists for the key.
    pub async fn close_episode(
        &self,
        key: &EpisodeKey,
        closed_at: DateTime<Utc>,
    ) -> PersistenceResult<bool> {
        let (account_id, window_id, version, level) = key_parts(key)?;
        let updated = sqlx::query(
            "UPDATE alert_episodes
                SET closed_at = ?
              WHERE account_id = ? AND window_id = ? AND definition_version = ? AND level = ?
                AND closed_at IS NULL",
        )
        .bind(codec::instant(closed_at))
        .bind(&account_id)
        .bind(&window_id)
        .bind(version)
        .bind(&level)
        .execute(&self.pool)
        .await
        .table("alert_episodes")?
        .rows_affected();

        if updated == 1 {
            return Ok(true);
        }
        // No live row was closed. The key must still exist, or the caller is
        // closing an episode that was never opened.
        self.episode(key).await?;
        Ok(false)
    }

    /// Reads one stored episode.
    ///
    /// # Errors
    /// Returns [`PersistenceError::RowRejected`] when no episode exists for the key.
    pub async fn episode(&self, key: &EpisodeKey) -> PersistenceResult<AlertEpisode> {
        let (account_id, window_id, version, level) = key_parts(key)?;
        let row = sqlx::query(
            "SELECT opened_at, armed_at, closed_at FROM alert_episodes
              WHERE account_id = ? AND window_id = ? AND definition_version = ? AND level = ?",
        )
        .bind(&account_id)
        .bind(&window_id)
        .bind(version)
        .bind(&level)
        .fetch_optional(&self.pool)
        .await
        .table("alert_episodes")?
        .ok_or(PersistenceError::RowRejected {
            table: "alert_episodes",
            reason: "no episode matched the requested key",
        })?;

        Ok(AlertEpisode {
            key: key.clone(),
            opened_at: codec::parse_instant(
                &row.try_get::<String, _>("opened_at")
                    .table("alert_episodes")?,
                "alert_episodes",
            )?,
            armed_at: read_instant(&row, "armed_at")?,
            closed_at: read_instant(&row, "closed_at")?,
        })
    }

    /// Enqueues one notification for an episode, at most once per episode key.
    ///
    /// # Returns
    /// `true` when this call inserted the entry, `false` when an entry for the
    /// same episode key already exists.
    ///
    /// # Errors
    /// Returns a typed persistence error when the write is refused and could
    /// not be attributed to the deduplication key.
    pub async fn enqueue_notification(
        &self,
        key: &EpisodeKey,
        created_at: DateTime<Utc>,
    ) -> PersistenceResult<bool> {
        let (account_id, window_id, _version, level) = key_parts(key)?;
        let episode_key = episode_key_text(key)?;

        let inserted = sqlx::query(
            "INSERT INTO notification_outbox (
                 account_id, window_id, level, episode_key, created_at, delivered_at
             ) VALUES (?, ?, ?, ?, ?, NULL)
             ON CONFLICT (episode_key) DO NOTHING",
        )
        .bind(&account_id)
        .bind(&window_id)
        .bind(&level)
        .bind(&episode_key)
        .bind(codec::instant(created_at))
        .execute(&self.pool)
        .await
        .table("notification_outbox")?
        .rows_affected();

        // SQLite reports zero changed rows when the conflict clause suppressed
        // the insert, so this is the deduplication answer itself.
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
}

/// Splits an episode key into its stored columns.
fn key_parts(key: &EpisodeKey) -> PersistenceResult<(String, String, i64, String)> {
    Ok((
        key.account_id.as_str().to_owned(),
        key.window_id.as_str().to_owned(),
        i64::from(key.definition_version),
        codec::encode(&key.level, "alert_episodes")?,
    ))
}

/// Renders the outbox deduplication key for an episode.
///
/// The fields are serialized as a JSON array rather than joined with a
/// separator, because an identifier may itself contain any separator character.
fn episode_key_text(key: &EpisodeKey) -> PersistenceResult<String> {
    codec::json(
        &serde_json::json!([
            key.account_id.as_str(),
            key.window_id.as_str(),
            key.definition_version,
            key.level
        ]),
        "notification_outbox",
    )
}

/// Maps a "no rows were changed" outcome onto a typed rejection.
fn require_one(changed: u64) -> PersistenceResult<()> {
    if changed == 1 {
        Ok(())
    } else {
        Err(PersistenceError::RowRejected {
            table: "alert_episodes",
            reason: "no open episode matched the requested key",
        })
    }
}

/// Reads an optional RFC 3339 column.
fn read_instant(
    row: &sqlx::sqlite::SqliteRow,
    column: &'static str,
) -> PersistenceResult<Option<DateTime<Utc>>> {
    match row
        .try_get::<Option<String>, _>(column)
        .table("alert_episodes")?
    {
        Some(text) => codec::parse_instant(&text, "alert_episodes").map(Some),
        None => Ok(None),
    }
}
