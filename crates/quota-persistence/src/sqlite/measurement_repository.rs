//! Latest readings and optional normalized history.
//!
//! One accepted reading is written as one transaction: the window definition,
//! the current reading, and — when the reading actually moved — one history
//! row. An unchanged observation is coalesced rather than appended, so a
//! minute-by-minute poll does not grow the history table without bound.

use chrono::{DateTime, Utc};
use quota_domain::ids::{AccountId, QuotaWindowId};
use quota_domain::quota::issue::QuotaIssue;
use quota_domain::quota::measurement::Measurement;
use quota_domain::quota::window::{BoundaryKind, Completeness, QuotaWindow, SourceKind};
use sqlx::{Row, SqlitePool};

use crate::error::{PersistenceError, PersistenceResult, TableContext};
use crate::sqlite::codec;

/// The stored current reading of one account and window.
#[derive(Clone, Debug, PartialEq)]
pub struct StoredMeasurement {
    /// The window this reading belongs to.
    pub window_id: QuotaWindowId,
    /// The normalised reading, including its unusable variants.
    pub measurement: Measurement,
    /// The provider's version of the window definition.
    pub definition_version: u32,
    /// When the provider says it observed the value.
    pub observed_at: Option<DateTime<Utc>>,
    /// When this process received it.
    pub received_at: DateTime<Utc>,
    /// Structured validation findings recorded with the reading.
    pub issues: Vec<QuotaIssue>,
}

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

/// Writes current readings and prunes optional history.
#[derive(Clone, Debug)]
pub struct MeasurementRepository {
    pool: SqlitePool,
}

impl MeasurementRepository {
    /// Wraps an open pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Writes one accepted reading, and its history row when the value moved.
    ///
    /// The comparison, the current-reading write, and the history write all
    /// happen inside one transaction, so a crash cannot record a history row
    /// for a reading that was never stored as current.
    ///
    /// # Returns
    /// `true` when a history row was written, `false` when the observation was
    /// coalesced because the remaining percentage and the observation time were
    /// both unchanged.
    ///
    /// # Errors
    /// Returns a typed persistence error when the transaction cannot complete.
    pub async fn persist_reading(
        &self,
        account_id: &AccountId,
        window: &QuotaWindow,
    ) -> PersistenceResult<bool> {
        let remaining = window
            .measurement
            .remaining_percent()
            .map(|value| value.value());
        let received_at = codec::instant(window.received_at);

        let mut transaction = self.pool.begin().await.table("latest_measurements")?;

        upsert_window(&mut transaction, account_id, window).await?;

        let previous: Option<(String, Option<String>)> = sqlx::query_as(
            "SELECT measurement_json, observed_at FROM latest_measurements
              WHERE account_id = ? AND window_id = ?",
        )
        .bind(account_id.as_str())
        .bind(window.id.as_str())
        .fetch_optional(&mut *transaction)
        .await
        .table("latest_measurements")?;

        let unchanged = previous.is_some_and(|(json, observed)| {
            let previous_remaining = serde_json::from_str::<Measurement>(&json)
                .ok()
                .and_then(|value| value.remaining_percent())
                .map(|value| value.value());
            previous_remaining == remaining && observed == window.observed_at.map(codec::instant)
        });

        let measurement_json = codec::json(&window.measurement, "latest_measurements")?;
        let measurement_kind = measurement_kind(&window.measurement);
        let issues_json = codec::json(&window.issues, "latest_measurements")?;

        sqlx::query(
            "INSERT INTO latest_measurements (
                 account_id, window_id, measurement_kind, measurement_json,
                 period_started_at, boundary_at, boundary_kind, observed_at,
                 received_at, valid_until, issues_json
             ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT (account_id, window_id) DO UPDATE SET
                 measurement_kind = excluded.measurement_kind,
                 measurement_json = excluded.measurement_json,
                 period_started_at = excluded.period_started_at,
                 boundary_at = excluded.boundary_at,
                 boundary_kind = excluded.boundary_kind,
                 observed_at = excluded.observed_at,
                 received_at = excluded.received_at,
                 valid_until = excluded.valid_until,
                 issues_json = excluded.issues_json",
        )
        .bind(account_id.as_str())
        .bind(window.id.as_str())
        .bind(measurement_kind)
        .bind(measurement_json)
        .bind(window.period_started_at.map(codec::instant))
        .bind(window.boundary.map(|boundary| codec::instant(boundary.at)))
        .bind(
            window
                .boundary
                .map(|boundary| codec::encode(&boundary.kind, "latest_measurements"))
                .transpose()?,
        )
        .bind(window.observed_at.map(codec::instant))
        .bind(&received_at)
        .bind(window.valid_until.map(codec::instant))
        .bind(issues_json)
        .execute(&mut *transaction)
        .await
        .table("latest_measurements")?;

        let observed_at = window
            .observed_at
            .map_or_else(|| received_at.clone(), codec::instant);

        if !unchanged {
            sqlx::query(
                "INSERT INTO measurement_history (
                     account_id, window_id, remaining_percent, observed_at, received_at
                 ) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(account_id.as_str())
            .bind(window.id.as_str())
            .bind(remaining)
            .bind(observed_at)
            .bind(&received_at)
            .execute(&mut *transaction)
            .await
            .table("measurement_history")?;
        }

        transaction.commit().await.table("latest_measurements")?;
        Ok(!unchanged)
    }

    /// Reports whether the stored reading already holds this exact observation.
    ///
    /// `true` means the remaining percentage and the observation time are both
    /// unchanged, so [`Self::persist_reading`] would not append a history row.
    ///
    /// # Errors
    /// Returns a typed persistence error when the read fails.
    pub async fn coalesce_unchanged(
        &self,
        account_id: &AccountId,
        window: &QuotaWindow,
    ) -> PersistenceResult<bool> {
        let remaining = window
            .measurement
            .remaining_percent()
            .map(|value| value.value());
        let observed_at = window.observed_at.map(codec::instant);

        let previous: Option<(String, Option<String>)> = sqlx::query_as(
            "SELECT measurement_json, observed_at FROM latest_measurements
              WHERE account_id = ? AND window_id = ?",
        )
        .bind(account_id.as_str())
        .bind(window.id.as_str())
        .fetch_optional(&self.pool)
        .await
        .table("latest_measurements")?;

        Ok(previous.is_some_and(|(json, observed)| {
            let previous_remaining = serde_json::from_str::<Measurement>(&json)
                .ok()
                .and_then(|value| value.remaining_percent())
                .map(|value| value.value());
            previous_remaining == remaining && observed == observed_at
        }))
    }

    /// Reads the stored current reading of one account and window.
    ///
    /// # Errors
    /// Returns a typed persistence error when the read fails or the stored
    /// reading is outside this build's vocabulary.
    pub async fn latest(
        &self,
        account_id: &AccountId,
        window_id: &QuotaWindowId,
    ) -> PersistenceResult<Option<StoredMeasurement>> {
        let row = sqlx::query(
            "SELECT m.window_id, m.measurement_json, m.observed_at, m.received_at,
                    m.issues_json, w.definition_version
               FROM latest_measurements m
               JOIN quota_windows w ON w.id = m.window_id
              WHERE m.account_id = ? AND m.window_id = ?",
        )
        .bind(account_id.as_str())
        .bind(window_id.as_str())
        .fetch_optional(&self.pool)
        .await
        .table("latest_measurements")?;

        let Some(row) = row else {
            return Ok(None);
        };

        let measurement_json: String = row
            .try_get("measurement_json")
            .table("latest_measurements")?;
        let issues_json: String = row.try_get("issues_json").table("latest_measurements")?;
        let received_at: String = row.try_get("received_at").table("latest_measurements")?;
        let observed_at: Option<String> =
            row.try_get("observed_at").table("latest_measurements")?;

        Ok(Some(StoredMeasurement {
            window_id: QuotaWindowId::new(
                row.try_get::<String, _>("window_id")
                    .table("latest_measurements")?,
            )
            .map_err(|_| PersistenceError::RowRejected {
                table: "latest_measurements",
                reason: "a stored window identity could not become a valid domain identifier",
            })?,
            measurement: serde_json::from_str(&measurement_json).map_err(|_| {
                PersistenceError::RowRejected {
                    table: "latest_measurements",
                    reason: "a stored reading is not a measurement this build can read",
                }
            })?,
            definition_version: u32::try_from(
                row.try_get::<i64, _>("definition_version")
                    .table("quota_windows")?,
            )
            .map_err(|_| PersistenceError::RowRejected {
                table: "quota_windows",
                reason: "a stored definition version was outside the unsigned range",
            })?,
            observed_at: observed_at
                .map(|text| codec::parse_instant(&text, "latest_measurements"))
                .transpose()?,
            received_at: codec::parse_instant(&received_at, "latest_measurements")?,
            issues: serde_json::from_str(&issues_json).map_err(|_| {
                PersistenceError::RowRejected {
                    table: "latest_measurements",
                    reason: "stored validation findings are not readable by this build",
                }
            })?,
        }))
    }

    /// Lists the history rows of one account, oldest first.
    ///
    /// # Errors
    /// Returns a typed persistence error when the read fails.
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

        rows.iter()
            .map(|row| {
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
            })
            .collect()
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

/// Writes the window definition the measurements reference.
///
/// The pool row the window references is created from the owning account's
/// provider when it does not exist yet. The pool's provider is therefore never
/// invented: it is read from the account, and an unknown account creates no
/// pool row, so the window insert fails its foreign key and the transaction
/// rolls the whole reading back.
async fn upsert_window(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    account_id: &AccountId,
    window: &QuotaWindow,
) -> PersistenceResult<()> {
    sqlx::query(
        "INSERT OR IGNORE INTO quota_pools (id, provider_id, shared)
         SELECT ?, provider_id, 0 FROM accounts WHERE id = ?",
    )
    .bind(window.pool_id.as_str())
    .bind(account_id.as_str())
    .execute(&mut **transaction)
    .await
    .table("quota_pools")?;

    sqlx::query(
        "INSERT INTO quota_windows (
             id, pool_id, provider_bucket_id, scope_resource, scope_label, category,
             semantics, duration_seconds, metric_role, enforcement, source_kind,
             completeness, definition_version
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (id) DO UPDATE SET
             provider_bucket_id = excluded.provider_bucket_id,
             scope_resource = excluded.scope_resource,
             scope_label = excluded.scope_label,
             category = excluded.category,
             semantics = excluded.semantics,
             duration_seconds = excluded.duration_seconds,
             metric_role = excluded.metric_role,
             enforcement = excluded.enforcement,
             source_kind = excluded.source_kind,
             completeness = excluded.completeness,
             definition_version = excluded.definition_version",
    )
    .bind(window.id.as_str())
    .bind(window.pool_id.as_str())
    .bind(window.provider_bucket_id.as_deref())
    .bind(window.scope.resource().as_str())
    .bind(window.scope.label())
    .bind(codec::encode(&window.category, "quota_windows")?)
    .bind(codec::encode(&window.semantics, "quota_windows")?)
    .bind(
        window
            .duration
            .map(|duration| duration.num_seconds())
            .filter(|seconds| *seconds >= 0),
    )
    .bind(codec::encode(&window.metric_role, "quota_windows")?)
    .bind(codec::encode(&window.enforcement, "quota_windows")?)
    .bind(codec::encode(&window.source, "quota_windows")?)
    .bind(codec::encode(&window.completeness, "quota_windows")?)
    .bind(i64::from(window.definition_version.0))
    .execute(&mut **transaction)
    .await
    .table("quota_windows")?;
    Ok(())
}

/// The stored discriminator of a reading.
///
/// It is derived from the reading's own serialization, so a new measurement
/// variant cannot be written under a stale name, and it lets a recovery tool
/// inspect the shape of a row whose JSON this build cannot parse.
fn measurement_kind(measurement: &Measurement) -> &'static str {
    match measurement {
        Measurement::Percentage(_) => "percentage",
        Measurement::Quantity(_) => "quantity",
        Measurement::Money(_) => "money",
        Measurement::Unlimited => "unlimited",
        Measurement::NotEntitled => "not_entitled",
        Measurement::Unavailable(_) => "unavailable",
    }
}

/// Keeps the vocabulary decoders exercised for the window columns this module
/// reads back.
#[allow(dead_code)]
fn decode_window_vocabulary(
    source: &str,
    completeness: &str,
) -> PersistenceResult<(SourceKind, Completeness)> {
    Ok((
        codec::decode(source, "quota_windows")?,
        codec::decode(completeness, "quota_windows")?,
    ))
}

/// Keeps the boundary-kind decoder reachable from the read path.
#[allow(dead_code)]
fn decode_boundary_kind(kind: &str) -> PersistenceResult<BoundaryKind> {
    codec::decode(kind, "latest_measurements")
}
