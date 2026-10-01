//! Optional normalized history.
//!
//! History is prunable and optional, so its rows carry no foreign key: a
//! retention sweep of one account's rows must never cascade into another
//! account's rows. Every statement here is therefore scoped by `account_id`,
//! and a window that several accounts share does not make them siblings.

use chrono::{DateTime, Utc};
use quota_domain::ids::{AccountId, QuotaWindowId};
use sqlx::Row;

use crate::error::{PersistenceError, PersistenceResult, TableContext};
use crate::sqlite::codec;
use crate::sqlite::measurement_repository::MeasurementRepository;

/// One row of optional history.
#[derive(Clone, Debug, PartialEq)]
pub struct HistoryEntry {
    /// The window the reading belonged to.
    pub window_id: QuotaWindowId,
    /// The remaining percentage, when the reading had one.
    pub remaining_percent: Option<f64>,
    /// When the value was observed.
    pub observed_at: DateTime<Utc>,
}

impl MeasurementRepository {
    /// Lists the history rows of one account, oldest first.
    ///
    /// # Errors
    /// Returns a typed persistence error when the read fails or a stored row
    /// is outside this build's vocabulary.
    pub async fn history_for_account(
        &self,
        account_id: &AccountId,
    ) -> PersistenceResult<Vec<HistoryEntry>> {
        let rows = sqlx::query(
            "SELECT window_id, remaining_percent, observed_at
               FROM measurement_history
              WHERE account_id = ?
              ORDER BY observed_at, id",
        )
        .bind(account_id.as_str())
        .fetch_all(&self.pool)
        .await
        .table("measurement_history")?;

        rows.iter().map(map_entry).collect()
    }

    /// Deletes the optional history rows of one account, and no other account's.
    ///
    /// # Returns
    /// The number of history rows removed.
    ///
    /// # Errors
    /// Returns a typed persistence error when the delete fails.
    pub async fn clear_history_for_account(
        &self,
        account_id: &AccountId,
    ) -> PersistenceResult<u64> {
        let deleted = sqlx::query("DELETE FROM measurement_history WHERE account_id = ?")
            .bind(account_id.as_str())
            .execute(&self.pool)
            .await
            .table("measurement_history")?
            .rows_affected();
        Ok(deleted)
    }
}

/// Maps one stored history row.
fn map_entry(row: &sqlx::sqlite::SqliteRow) -> PersistenceResult<HistoryEntry> {
    Ok(HistoryEntry {
        window_id: QuotaWindowId::new(
            row.try_get::<String, _>("window_id")
                .table("measurement_history")?,
        )
        .map_err(|_| PersistenceError::RowRejected {
            table: "measurement_history",
            reason: "a stored window identity could not become a valid identifier",
        })?,
        remaining_percent: row
            .try_get("remaining_percent")
            .table("measurement_history")?,
        observed_at: codec::parse_instant(
            &row.try_get::<String, _>("observed_at")
                .table("measurement_history")?,
            "measurement_history",
        )?,
    })
}
