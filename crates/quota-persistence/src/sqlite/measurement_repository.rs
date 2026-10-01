//! Latest readings.
//!
//! One accepted reading is written as one transaction: the window definition,
//! the current reading, and — when the reading actually moved — one history
//! row. An unchanged observation is coalesced rather than appended, so a
//! minute-by-minute poll does not grow the history table without bound.
//!
//! History reads and retention live in a sibling private module; both are
//! `impl` blocks on the same [`MeasurementRepository`].

use chrono::{DateTime, Utc};
use quota_domain::Percent;
use quota_domain::ids::{AccountId, QuotaWindowId};
use quota_domain::quota::issue::QuotaIssue;
use quota_domain::quota::measurement::Measurement;
use quota_domain::quota::window::QuotaWindow;
use sqlx::{Row, SqlitePool};

use crate::error::{PersistenceError, PersistenceResult, TableContext};
use crate::sqlite::codec;
use crate::sqlite::window_writer::upsert_window;

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

/// Writes current readings and reads optional history.
#[derive(Clone, Debug)]
pub struct MeasurementRepository {
    pub(crate) pool: SqlitePool,
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
        let remaining = window.measurement.remaining_percent().map(Percent::value);
        let received_at = codec::instant(window.received_at);

        let mut transaction = self.pool.begin().await.table("latest_measurements")?;

        upsert_window(&mut transaction, account_id, window).await?;

        let previous = read_previous(&mut transaction, account_id, window).await?;
        let unchanged = coalesces(previous, window);

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
        .bind(measurement_kind(&window.measurement))
        .bind(codec::json(&window.measurement, "latest_measurements")?)
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
        .bind(codec::json(&window.issues, "latest_measurements")?)
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
        let previous = sqlx::query_as(
            "SELECT measurement_json, observed_at FROM latest_measurements
              WHERE account_id = ? AND window_id = ?",
        )
        .bind(account_id.as_str())
        .bind(window.id.as_str())
        .fetch_optional(&self.pool)
        .await
        .table("latest_measurements")?;

        Ok(coalesces(previous, window))
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

        let observed_at: Option<String> =
            row.try_get("observed_at").table("latest_measurements")?;

        Ok(Some(StoredMeasurement {
            window_id: QuotaWindowId::new(
                row.try_get::<String, _>("window_id")
                    .table("latest_measurements")?,
            )
            .map_err(|_| PersistenceError::RowRejected {
                table: "latest_measurements",
                reason: "a stored window identity could not become a valid identifier",
            })?,
            measurement: serde_json::from_str(
                &row.try_get::<String, _>("measurement_json")
                    .table("latest_measurements")?,
            )
            .map_err(|_| PersistenceError::RowRejected {
                table: "latest_measurements",
                reason: "a stored reading is not a measurement this build can read",
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
            received_at: codec::parse_instant(
                &row.try_get::<String, _>("received_at")
                    .table("latest_measurements")?,
                "latest_measurements",
            )?,
            issues: serde_json::from_str(
                &row.try_get::<String, _>("issues_json")
                    .table("latest_measurements")?,
            )
            .map_err(|_| PersistenceError::RowRejected {
                table: "latest_measurements",
                reason: "stored validation findings are not readable by this build",
            })?,
        }))
    }
}

/// Reads the stored observation key for one account and window.
async fn read_previous(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    account_id: &AccountId,
    window: &QuotaWindow,
) -> PersistenceResult<Option<(String, Option<String>)>> {
    sqlx::query_as(
        "SELECT measurement_json, observed_at FROM latest_measurements
          WHERE account_id = ? AND window_id = ?",
    )
    .bind(account_id.as_str())
    .bind(window.id.as_str())
    .fetch_optional(&mut **transaction)
    .await
    .table("latest_measurements")
}

/// Whether the stored observation already matches the incoming reading.
///
/// Both the remaining percentage and the observation time must match. A
/// provider that re-reports the same value within the same period is a new
/// observation when its timestamp moved, and a repeated poll of the same
/// instant is not.
fn coalesces(previous: Option<(String, Option<String>)>, window: &QuotaWindow) -> bool {
    let remaining = window.measurement.remaining_percent().map(Percent::value);
    let observed_at = window.observed_at.map(codec::instant);

    previous.is_some_and(|(json, observed)| {
        let previous_remaining = serde_json::from_str::<Measurement>(&json)
            .ok()
            .and_then(|value| value.remaining_percent())
            .map(Percent::value);
        previous_remaining == remaining && observed == observed_at
    })
}

/// The stored discriminator of a reading.
///
/// It lets a recovery tool inspect the shape of a row whose JSON this build
/// cannot parse.
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
