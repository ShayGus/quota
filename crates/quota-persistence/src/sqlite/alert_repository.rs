//! Alert episodes and their notification outbox.
//!
//! An episode is keyed by account, window, definition version, and level, so
//! two independent accounts never share an episode merely because they name the
//! same provider. The key is also the outbox deduplication key, so a second
//! crossing of the same threshold cannot enqueue a second notification.
//!
//! Outbox statements live in a sibling private module; both are `impl` blocks
//! on the same [`AlertRepository`].

use chrono::{DateTime, Utc};
use quota_domain::ids::{AccountId, QuotaWindowId};
use sqlx::{Row, SqlitePool};

use crate::error::{PersistenceError, PersistenceResult, TableContext};
use crate::sqlite::codec;
use crate::sqlite::rows;

/// The severity an episode was opened at.
///
/// The thresholds behind these levels are a supervisor policy, and the
/// supervisor owns the vocabulary. Storage keeps the supervisor's own type so a
/// stored level can never drift from the level the supervisor produced.
pub use quota_core::ports::AlertLevel;

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
    pub(crate) pool: SqlitePool,
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
        let parts = EpisodeParts::of(key);
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
        .bind(&parts.account_id)
        .bind(&parts.window_id)
        .bind(parts.version)
        .bind(&parts.level)
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
        let parts = EpisodeParts::of(key);
        let open: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM alert_episodes
              WHERE account_id = ? AND window_id = ? AND definition_version = ? AND level = ?
                AND closed_at IS NULL",
        )
        .bind(&parts.account_id)
        .bind(&parts.window_id)
        .bind(parts.version)
        .bind(&parts.level)
        .fetch_optional(&self.pool)
        .await
        .table("alert_episodes")?;
        Ok(open.is_some())
    }

    /// Reports whether a notification is already enqueued for this open episode.
    ///
    /// A supervisor consults this before enqueuing, so one level cannot notify
    /// twice while the same episode stays open.
    ///
    /// # Errors
    /// Returns a typed persistence error when the read fails.
    pub async fn is_armed(&self, key: &EpisodeKey) -> PersistenceResult<bool> {
        let parts = EpisodeParts::of(key);
        let armed: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM alert_episodes
              WHERE account_id = ? AND window_id = ? AND definition_version = ? AND level = ?
                AND armed_at IS NOT NULL AND closed_at IS NULL",
        )
        .bind(&parts.account_id)
        .bind(&parts.window_id)
        .bind(parts.version)
        .bind(&parts.level)
        .fetch_optional(&self.pool)
        .await
        .table("alert_episodes")?;
        Ok(armed.is_some())
    }

    /// Records that a notification for this episode has been enqueued.
    ///
    /// # Errors
    /// Returns [`PersistenceError::RowRejected`] when no open episode exists.
    pub async fn mark_armed(
        &self,
        key: &EpisodeKey,
        armed_at: DateTime<Utc>,
    ) -> PersistenceResult<()> {
        let parts = EpisodeParts::of(key);
        let updated = sqlx::query(
            "UPDATE alert_episodes
                SET armed_at = ?
              WHERE account_id = ? AND window_id = ? AND definition_version = ? AND level = ?
                AND closed_at IS NULL",
        )
        .bind(codec::instant(armed_at))
        .bind(&parts.account_id)
        .bind(&parts.window_id)
        .bind(parts.version)
        .bind(&parts.level)
        .execute(&self.pool)
        .await
        .table("alert_episodes")?
        .rows_affected();
        rows::require_one(updated, "alert_episodes")
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
        let parts = EpisodeParts::of(key);
        let updated = sqlx::query(
            "UPDATE alert_episodes
                SET closed_at = ?
              WHERE account_id = ? AND window_id = ? AND definition_version = ? AND level = ?
                AND closed_at IS NULL",
        )
        .bind(codec::instant(closed_at))
        .bind(&parts.account_id)
        .bind(&parts.window_id)
        .bind(parts.version)
        .bind(&parts.level)
        .execute(&self.pool)
        .await
        .table("alert_episodes")?
        .rows_affected();

        if updated == 1 {
            return Ok(true);
        }
        // No live row was closed. The key must still exist, otherwise the caller
        // is closing an episode that was never opened.
        self.episode(key).await?;
        Ok(false)
    }

    /// Reads one stored episode.
    ///
    /// # Errors
    /// Returns [`PersistenceError::RowRejected`] when no episode exists for the key.
    pub async fn episode(&self, key: &EpisodeKey) -> PersistenceResult<AlertEpisode> {
        let parts = EpisodeParts::of(key);
        let row = sqlx::query(
            "SELECT opened_at, armed_at, closed_at FROM alert_episodes
              WHERE account_id = ? AND window_id = ? AND definition_version = ? AND level = ?",
        )
        .bind(&parts.account_id)
        .bind(&parts.window_id)
        .bind(parts.version)
        .bind(&parts.level)
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
}

/// One episode key, split into the columns the schema stores.
pub(crate) struct EpisodeParts {
    /// The account column.
    pub(crate) account_id: String,
    /// The window column.
    pub(crate) window_id: String,
    /// The definition-version column.
    pub(crate) version: i64,
    /// The level column.
    pub(crate) level: String,
}

impl EpisodeParts {
    /// Splits an episode key into its stored columns.
    pub(crate) fn of(key: &EpisodeKey) -> Self {
        Self {
            account_id: key.account_id.as_str().to_owned(),
            window_id: key.window_id.as_str().to_owned(),
            version: i64::from(key.definition_version),
            level: key.level.as_str().to_owned(),
        }
    }
}
/// Renders the outbox deduplication key for an episode.
///
/// The fields are serialized as a JSON array rather than joined with a
/// separator, because an identifier may itself contain any separator.
pub(crate) fn outbox_key(key: &EpisodeKey) -> PersistenceResult<String> {
    codec::json(
        &serde_json::json!([
            key.account_id.as_str(),
            key.window_id.as_str(),
            key.definition_version,
            key.level.as_str()
        ]),
        "notification_outbox",
    )
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
