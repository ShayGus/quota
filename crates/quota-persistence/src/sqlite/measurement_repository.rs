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
use quota_domain::ids::{AccountId, DefinitionVersion, QuotaPoolId, QuotaWindowId, ResourceId};
use quota_domain::quota::issue::QuotaIssue;
use quota_domain::quota::measurement::Measurement;
use quota_domain::quota::scope::QuotaScope;
use quota_domain::quota::window::{
    Boundary, BoundaryKind, Completeness, Enforcement, MetricRole, QuotaCategory, QuotaWindow,
    SourceKind, WindowSemantics,
};
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
        self.persist_readings(account_id, std::slice::from_ref(window))
            .await
            .map(|history_rows| history_rows != 0)
    }

    /// Writes every window from one accepted provider response atomically.
    ///
    /// The current measurements and any changed history rows commit together.
    /// A failure leaves the previous complete account snapshot intact.
    ///
    /// # Returns
    /// The number of history rows written.
    ///
    /// # Errors
    /// Returns a typed persistence error when any window cannot be stored.
    pub async fn persist_readings(
        &self,
        account_id: &AccountId,
        windows: &[QuotaWindow],
    ) -> PersistenceResult<usize> {
        let mut transaction = self.pool.begin().await.table("latest_measurements")?;
        let mut history_rows = 0;
        for window in windows {
            history_rows +=
                usize::from(persist_window(&mut transaction, account_id, window).await?);
        }
        transaction.commit().await.table("latest_measurements")?;
        Ok(history_rows)
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

    /// Reads every current quota window for one account.
    ///
    /// Windows without a current measurement are not returned. A provider that
    /// expected a window but did not report it persists an explicit
    /// `Unavailable(NotReported)` measurement, so it remains visible without
    /// inventing a value or timestamp here.
    ///
    /// # Errors
    /// Returns a typed error when any persisted value is malformed or outside
    /// this build's domain vocabulary.
    pub async fn windows_for_account(
        &self,
        account_id: &AccountId,
    ) -> PersistenceResult<Vec<QuotaWindow>> {
        let rows = sqlx::query(
            "SELECT w.id, w.pool_id, w.provider_bucket_id, w.scope_resource,
                    w.scope_label, w.category, w.semantics, w.duration_seconds,
                    w.metric_role, w.enforcement, w.source_kind, w.completeness,
                    w.definition_version, m.measurement_json, m.period_started_at,
                    m.boundary_at, m.boundary_kind, m.observed_at, m.received_at,
                    m.valid_until, m.issues_json
               FROM latest_measurements m
               JOIN quota_windows w ON w.id = m.window_id
              WHERE m.account_id = ?
              ORDER BY w.category, w.id",
        )
        .bind(account_id.as_str())
        .fetch_all(&self.pool)
        .await
        .table("latest_measurements")?;

        rows.iter().map(map_window).collect()
    }
}

/// A window's stable definition, separated from its changing measurement.
struct WindowDefinition {
    id: QuotaWindowId,
    pool_id: quota_domain::ids::QuotaPoolId,
    provider_bucket_id: Option<String>,
    scope: QuotaScope,
    category: QuotaCategory,
    semantics: WindowSemantics,
    duration: Option<chrono::Duration>,
    metric_role: MetricRole,
    enforcement: Enforcement,
    source: SourceKind,
    completeness: Completeness,
    definition_version: DefinitionVersion,
}

/// A window's latest changing measurement fields.
struct WindowReading {
    measurement: Measurement,
    period_started_at: Option<DateTime<Utc>>,
    boundary: Option<Boundary>,
    observed_at: Option<DateTime<Utc>>,
    received_at: DateTime<Utc>,
    valid_until: Option<DateTime<Utc>>,
    issues: Vec<QuotaIssue>,
}

/// Maps a joined definition and latest-measurement row into the domain window.
fn map_window(row: &sqlx::sqlite::SqliteRow) -> PersistenceResult<QuotaWindow> {
    let definition = read_definition(row)?;
    let reading = read_window_reading(row)?;
    Ok(QuotaWindow {
        id: definition.id,
        provider_bucket_id: definition.provider_bucket_id,
        pool_id: definition.pool_id,
        scope: definition.scope,
        category: definition.category,
        semantics: definition.semantics,
        duration: definition.duration,
        metric_role: definition.metric_role,
        enforcement: definition.enforcement,
        measurement: reading.measurement,
        period_started_at: reading.period_started_at,
        boundary: reading.boundary,
        observed_at: reading.observed_at,
        received_at: reading.received_at,
        valid_until: reading.valid_until,
        source: definition.source,
        completeness: definition.completeness,
        definition_version: definition.definition_version,
        issues: reading.issues,
    })
}

fn read_definition(row: &sqlx::sqlite::SqliteRow) -> PersistenceResult<WindowDefinition> {
    let id = QuotaWindowId::new(row.try_get::<String, _>("id").table("quota_windows")?).map_err(
        |_| PersistenceError::RowRejected {
            table: "quota_windows",
            reason: "a window identity is invalid",
        },
    )?;
    let pool_id = QuotaPoolId::new(row.try_get::<String, _>("pool_id").table("quota_windows")?)
        .map_err(|_| PersistenceError::RowRejected {
            table: "quota_windows",
            reason: "a quota pool identity is invalid",
        })?;
    let resource = ResourceId::new(
        row.try_get::<String, _>("scope_resource")
            .table("quota_windows")?,
    )
    .map_err(|_| PersistenceError::RowRejected {
        table: "quota_windows",
        reason: "a resource identity is invalid",
    })?;
    let label: String = row.try_get("scope_label").table("quota_windows")?;
    let scope = QuotaScope::new(resource, label).map_err(|_| PersistenceError::RowRejected {
        table: "quota_windows",
        reason: "a scope label is invalid",
    })?;
    let duration_seconds: Option<i64> = row.try_get("duration_seconds").table("quota_windows")?;
    let duration = duration_seconds
        .map(|seconds| {
            if seconds < 0 {
                Err(PersistenceError::RowRejected {
                    table: "quota_windows",
                    reason: "a stored duration was negative",
                })
            } else {
                Ok(chrono::Duration::seconds(seconds))
            }
        })
        .transpose()?;
    let definition_version = u32::try_from(
        row.try_get::<i64, _>("definition_version")
            .table("quota_windows")?,
    )
    .map(DefinitionVersion)
    .map_err(|_| PersistenceError::RowRejected {
        table: "quota_windows",
        reason: "a definition version is outside the supported range",
    })?;
    Ok(WindowDefinition {
        id,
        pool_id,
        provider_bucket_id: row.try_get("provider_bucket_id").table("quota_windows")?,
        scope,
        category: decode_column(row, "category", "quota_windows")?,
        semantics: decode_column(row, "semantics", "quota_windows")?,
        duration,
        metric_role: decode_column(row, "metric_role", "quota_windows")?,
        enforcement: decode_column(row, "enforcement", "quota_windows")?,
        source: decode_column(row, "source_kind", "quota_windows")?,
        completeness: decode_column(row, "completeness", "quota_windows")?,
        definition_version,
    })
}

fn read_window_reading(row: &sqlx::sqlite::SqliteRow) -> PersistenceResult<WindowReading> {
    let measurement = read_json_column(row, "measurement_json", "latest_measurements")?;
    let boundary_at: Option<String> = row.try_get("boundary_at").table("latest_measurements")?;
    let boundary_kind: Option<String> =
        row.try_get("boundary_kind").table("latest_measurements")?;
    let boundary = match (boundary_at, boundary_kind) {
        (Some(at), Some(kind)) => {
            let kind: BoundaryKind = serde_json::from_value(serde_json::Value::String(kind))
                .map_err(|_| PersistenceError::RowRejected {
                    table: "latest_measurements",
                    reason: "a boundary kind is outside this build's vocabulary",
                })?;
            Some(Boundary {
                at: codec::parse_instant(&at, "latest_measurements")?,
                kind,
            })
        }
        (None, None) => None,
        _ => {
            return Err(PersistenceError::RowRejected {
                table: "latest_measurements",
                reason: "a boundary is missing its time or kind",
            });
        }
    };
    Ok(WindowReading {
        measurement,
        period_started_at: optional_instant(row, "period_started_at")?,
        boundary,
        observed_at: optional_instant(row, "observed_at")?,
        received_at: codec::parse_instant(
            &row.try_get::<String, _>("received_at")
                .table("latest_measurements")?,
            "latest_measurements",
        )?,
        valid_until: optional_instant(row, "valid_until")?,
        issues: read_json_column(row, "issues_json", "latest_measurements")?,
    })
}

fn decode_column<T: serde::de::DeserializeOwned>(
    row: &sqlx::sqlite::SqliteRow,
    column: &'static str,
    table: &'static str,
) -> PersistenceResult<T> {
    codec::decode(&row.try_get::<String, _>(column).table(table)?, table)
}

fn read_json_column<T: serde::de::DeserializeOwned>(
    row: &sqlx::sqlite::SqliteRow,
    column: &'static str,
    table: &'static str,
) -> PersistenceResult<T> {
    serde_json::from_str(&row.try_get::<String, _>(column).table(table)?).map_err(|_| {
        PersistenceError::RowRejected {
            table,
            reason: "stored JSON is not readable by this build",
        }
    })
}

fn optional_instant(
    row: &sqlx::sqlite::SqliteRow,
    column: &'static str,
) -> PersistenceResult<Option<DateTime<Utc>>> {
    row.try_get::<Option<String>, _>(column)
        .table("latest_measurements")?
        .map(|text| codec::parse_instant(&text, "latest_measurements"))
        .transpose()
}

/// Writes one window inside the caller's transaction.
async fn persist_window(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    account_id: &AccountId,
    window: &QuotaWindow,
) -> PersistenceResult<bool> {
    let remaining = window.measurement.remaining_percent().map(Percent::value);
    let received_at = codec::instant(window.received_at);
    upsert_window(transaction, account_id, window).await?;
    let unchanged = coalesces(
        read_previous(transaction, account_id, window).await?,
        window,
    );
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
    .execute(&mut **transaction)
    .await
    .table("latest_measurements")?;

    if !unchanged {
        let observed_at = window
            .observed_at
            .map_or_else(|| received_at.clone(), codec::instant);
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
        .execute(&mut **transaction)
        .await
        .table("measurement_history")?;
    }
    Ok(!unchanged)
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
