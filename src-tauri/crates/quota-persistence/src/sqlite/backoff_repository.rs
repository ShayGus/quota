//! Scoped retry deadlines.
//!
//! A rate limit belongs to a scope, not to whichever account happened to
//! receive the response. [`LimitScope`] is that scope, and it is the key this
//! table stores, so a provider-wide or shared-credential limit survives a
//! restart and cannot be bypassed by rotating to another account.

use chrono::{DateTime, Utc};
use quota_domain::ids::{AccountId, ConnectionId, QuotaPoolId};
use quota_domain::polling::LimitScope;
use quota_domain::provider::ProviderId;
use sqlx::{Row, SqlitePool};

use crate::error::{PersistenceError, PersistenceResult, TableContext};
use crate::sqlite::codec;

/// The stored scope name of an account limit.
const SCOPE_ACCOUNT: &str = "account";
/// The stored scope name of a connection limit.
const SCOPE_CONNECTION: &str = "connection";
/// The stored scope name of a provider-wide limit.
const SCOPE_PROVIDER: &str = "provider";
/// The stored scope name of a shared quota-pool limit.
const SCOPE_QUOTA_POOL: &str = "quota_pool";
/// The stored scope name of an origin-address limit, which has no identity.
const SCOPE_SOURCE_ADDRESS: &str = "source_address";

/// One stored retry deadline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackoffRecord {
    /// The domain the limit applies to.
    pub scope: LimitScope,
    /// How many consecutive failures this scope has recorded.
    pub attempts: u32,
    /// The instant before which no read is attempted for this scope.
    pub next_eligible_at: DateTime<Utc>,
    /// The provider's own retry hint, when it sent one.
    pub provider_retry_after: Option<DateTime<Utc>>,
}

/// Reads and writes scoped retry deadlines.
#[derive(Clone, Debug)]
pub struct BackoffRepository {
    pool: SqlitePool,
}

impl BackoffRepository {
    /// Wraps an open pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Writes one retry deadline, replacing any deadline already stored for the
    /// scope.
    ///
    /// # Errors
    /// Returns a typed persistence error when the write is refused.
    pub async fn persist(&self, record: &BackoffRecord) -> PersistenceResult<()> {
        let (kind, id) = scope_parts(&record.scope);
        sqlx::query(
            "INSERT INTO refresh_backoff (
                 scope_kind, scope_id, attempts, next_eligible_at, provider_retry_after
             ) VALUES (?, ?, ?, ?, ?)
             ON CONFLICT (scope_kind, scope_id) DO UPDATE SET
                 attempts = excluded.attempts,
                 next_eligible_at = excluded.next_eligible_at,
                 provider_retry_after = excluded.provider_retry_after",
        )
        .bind(kind)
        .bind(id)
        .bind(i64::from(record.attempts))
        .bind(codec::instant(record.next_eligible_at))
        .bind(record.provider_retry_after.map(codec::instant))
        .execute(&self.pool)
        .await
        .table("refresh_backoff")?;
        Ok(())
    }

    /// Records one more failure for a scope and stores the resulting deadline.
    ///
    /// # Returns
    /// The stored record, including the new attempt count.
    ///
    /// # Errors
    /// Returns a typed persistence error when the write is refused.
    pub async fn record_failure(
        &self,
        scope: &LimitScope,
        next_eligible_at: DateTime<Utc>,
        provider_retry_after: Option<DateTime<Utc>>,
    ) -> PersistenceResult<BackoffRecord> {
        let (kind, id) = scope_parts(scope);
        sqlx::query(
            "INSERT INTO refresh_backoff (
                 scope_kind, scope_id, attempts, next_eligible_at, provider_retry_after
             ) VALUES (?, ?, 1, ?, ?)
             ON CONFLICT (scope_kind, scope_id) DO UPDATE SET
                 attempts = attempts + 1,
                 next_eligible_at = excluded.next_eligible_at,
                 provider_retry_after = excluded.provider_retry_after",
        )
        .bind(&kind)
        .bind(&id)
        .bind(codec::instant(next_eligible_at))
        .bind(provider_retry_after.map(codec::instant))
        .execute(&self.pool)
        .await
        .table("refresh_backoff")?;

        self.read(scope)
            .await?
            .ok_or(PersistenceError::RowRejected {
                table: "refresh_backoff",
                reason: "the deadline just written was not readable",
            })
    }

    /// Reads the stored deadline for one scope.
    ///
    /// # Errors
    /// Returns a typed persistence error when the read fails.
    pub async fn read(&self, scope: &LimitScope) -> PersistenceResult<Option<BackoffRecord>> {
        let (kind, id) = scope_parts(scope);
        let row = sqlx::query(
            "SELECT scope_kind, scope_id, attempts, next_eligible_at, provider_retry_after
               FROM refresh_backoff
              WHERE scope_kind = ? AND scope_id = ?",
        )
        .bind(kind)
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .table("refresh_backoff")?;

        row.as_ref().map(map_record).transpose()
    }

    /// Reports whether this scope is still inside its stored deadline.
    ///
    /// The comparison is made against the supplied instant rather than the wall
    /// clock, so the caller's clock stays the single source of truth.
    ///
    /// # Errors
    /// Returns a typed persistence error when the read fails.
    pub async fn is_rate_limited_now(
        &self,
        scope: &LimitScope,
        now: DateTime<Utc>,
    ) -> PersistenceResult<bool> {
        let (kind, id) = scope_parts(scope);
        let stored: Option<String> = sqlx::query_scalar(
            "SELECT next_eligible_at FROM refresh_backoff
              WHERE scope_kind = ? AND scope_id = ?",
        )
        .bind(kind)
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .table("refresh_backoff")?;

        match stored {
            Some(text) => Ok(codec::parse_instant(&text, "refresh_backoff")? > now),
            None => Ok(false),
        }
    }

    /// Removes the stored deadline for one scope.
    ///
    /// # Errors
    /// Returns a typed persistence error when the delete fails.
    pub async fn clear(&self, scope: &LimitScope) -> PersistenceResult<bool> {
        let (kind, id) = scope_parts(scope);
        let deleted =
            sqlx::query("DELETE FROM refresh_backoff WHERE scope_kind = ? AND scope_id = ?")
                .bind(kind)
                .bind(id)
                .execute(&self.pool)
                .await
                .table("refresh_backoff")?
                .rows_affected();
        Ok(deleted == 1)
    }
}

/// Splits a scope into its stored kind and identity.
///
/// A source-address limit carries no identity in [`LimitScope`], so its
/// identity column stores empty text and the kind alone identifies the row.
fn scope_parts(scope: &LimitScope) -> (String, String) {
    let (kind, id) = match scope {
        LimitScope::Account(id) => (SCOPE_ACCOUNT, id.as_str().to_owned()),
        LimitScope::Connection(id) => (SCOPE_CONNECTION, id.as_str().to_owned()),
        LimitScope::Provider(id) => (SCOPE_PROVIDER, id.as_str().to_owned()),
        LimitScope::QuotaPool(id) => (SCOPE_QUOTA_POOL, id.as_str().to_owned()),
        LimitScope::SourceAddress => (SCOPE_SOURCE_ADDRESS, String::new()),
    };
    (kind.to_owned(), id)
}

/// Rebuilds a scope from its stored kind and identity.
fn scope_from(kind: &str, id: &str) -> PersistenceResult<LimitScope> {
    let rejected = || PersistenceError::RowRejected {
        table: "refresh_backoff",
        reason: "a stored scope could not become a valid domain value",
    };
    match kind {
        SCOPE_ACCOUNT => AccountId::new(id)
            .map(LimitScope::Account)
            .map_err(|_| rejected()),
        SCOPE_CONNECTION => ConnectionId::new(id)
            .map(LimitScope::Connection)
            .map_err(|_| rejected()),
        SCOPE_PROVIDER => ProviderId::parse(id)
            .map(LimitScope::Provider)
            .map_err(|_| rejected()),
        SCOPE_QUOTA_POOL => QuotaPoolId::new(id)
            .map(LimitScope::QuotaPool)
            .map_err(|_| rejected()),
        SCOPE_SOURCE_ADDRESS => Ok(LimitScope::SourceAddress),
        _ => Err(rejected()),
    }
}

/// Maps one stored deadline row.
fn map_record(row: &sqlx::sqlite::SqliteRow) -> PersistenceResult<BackoffRecord> {
    let kind: String = row.try_get("scope_kind").table("refresh_backoff")?;
    let id: String = row.try_get("scope_id").table("refresh_backoff")?;
    let attempts: i64 = row.try_get("attempts").table("refresh_backoff")?;
    let next_eligible_at: String = row.try_get("next_eligible_at").table("refresh_backoff")?;
    let provider_retry_after: Option<String> = row
        .try_get("provider_retry_after")
        .table("refresh_backoff")?;

    Ok(BackoffRecord {
        scope: scope_from(&kind, &id)?,
        attempts: u32::try_from(attempts).map_err(|_| PersistenceError::RowRejected {
            table: "refresh_backoff",
            reason: "a stored attempt count was negative",
        })?,
        next_eligible_at: codec::parse_instant(&next_eligible_at, "refresh_backoff")?,
        provider_retry_after: provider_retry_after
            .map(|text| codec::parse_instant(&text, "refresh_backoff"))
            .transpose()?,
    })
}
