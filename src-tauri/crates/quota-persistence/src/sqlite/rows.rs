//! Typed row mapping for the connection and account tables.
//!
//! Mapping is fallible and never substitutes a value: a stored row outside this
//! build's vocabulary becomes a [`PersistenceError::RowRejected`] instead of a
//! guess.

use chrono::{DateTime, Utc};
use quota_domain::account::VerifiedIdentity;
use quota_domain::provider::ProviderId;
use sqlx::Row;

use crate::error::{PersistenceError, PersistenceResult, TableContext};
use crate::sqlite::account_repository::AccountRecord;
use crate::sqlite::codec;
use crate::sqlite::connection_repository::ConnectionRecord;

/// Maps a "no rows were changed" outcome onto a typed rejection.
pub(crate) fn require_one(changed: u64, table: &'static str) -> PersistenceResult<()> {
    if changed == 1 {
        Ok(())
    } else {
        Err(PersistenceError::RowRejected {
            table,
            reason: "no row matched the requested identity",
        })
    }
}

/// Maps one joined account row.
pub(crate) fn map_account(row: &sqlx::sqlite::SqliteRow) -> PersistenceResult<AccountRecord> {
    let identity_source: Option<String> = row.try_get("identity_source").table("accounts")?;
    let principal: Option<String> = row.try_get("verified_principal_label").table("accounts")?;

    let verified_identity = match (identity_source, principal) {
        (Some(source), Some(principal_label)) => Some(VerifiedIdentity {
            principal_label,
            workspace_label: row.try_get("verified_workspace_label").table("accounts")?,
            plan_label: row.try_get("verified_plan_label").table("accounts")?,
            source: codec::decode(&source, "accounts")?,
        }),
        _ => None,
    };

    Ok(AccountRecord {
        id: required_identity(row, "id", "accounts")?,
        connection_id: required_identity(row, "connection_id", "accounts")?,
        generation: counter(row, "generation", "accounts")?,
        provider_id: provider(row, "accounts")?,
        nickname: row.try_get("nickname").table("accounts")?,
        connection_ordinal: u32::try_from(
            row.try_get::<i64, _>("connection_ordinal")
                .table("accounts")?,
        )
        .map_err(|_| negative("accounts", "a stored ordinal was negative"))?,
        monitoring_enabled: row
            .try_get::<bool, _>("monitoring_enabled")
            .table("accounts")?,
        connection_state: codec::decode(
            &row.try_get::<String, _>("connection_state")
                .table("accounts")?,
            "accounts",
        )?,
        fetch_state: codec::decode(
            &row.try_get::<String, _>("fetch_state").table("accounts")?,
            "accounts",
        )?,
        last_attempt_at: instant(row, "last_attempt_at", "accounts")?,
        last_success_at: instant(row, "last_success_at", "accounts")?,
        next_attempt_at: instant(row, "next_attempt_at", "accounts")?,
        verified_identity,
        show_key_limit: row.try_get::<bool, _>("show_key_limit").table("accounts")?,
    })
}

/// Maps one connection row.
pub(crate) fn map_connection(row: &sqlx::sqlite::SqliteRow) -> PersistenceResult<ConnectionRecord> {
    Ok(ConnectionRecord {
        id: required_identity(row, "id", "connections")?,
        provider_id: provider(row, "connections")?,
        credential_ownership: codec::decode(
            &row.try_get::<String, _>("credential_ownership")
                .table("connections")?,
            "connections",
        )?,
        generation: counter(row, "generation", "connections")?,
        profile_label: row.try_get("profile_label").table("connections")?,
        cardinality: codec::decode(
            &row.try_get::<String, _>("cardinality")
                .table("connections")?,
            "connections",
        )?,
        state: codec::decode(
            &row.try_get::<String, _>("state").table("connections")?,
            "connections",
        )?,
        principal_id: optional_identity(row, "principal_id")?,
        workspace_id: optional_identity(row, "workspace_id")?,
        entitlement_id: optional_identity(row, "entitlement_id")?,
    })
}

/// Reads a stored provider name.
fn provider(row: &sqlx::sqlite::SqliteRow, table: &'static str) -> PersistenceResult<ProviderId> {
    let name: String = row.try_get("provider_id").table(table)?;
    ProviderId::parse(&name).map_err(|_| PersistenceError::RowRejected {
        table,
        reason: "the stored provider name is outside this build's vocabulary",
    })
}

/// Reads a stored identifier that must be present.
fn required_identity<T>(
    row: &sqlx::sqlite::SqliteRow,
    column: &'static str,
    table: &'static str,
) -> PersistenceResult<T>
where
    T: TryFrom<String>,
{
    let text: String = row.try_get(column).table(table)?;
    T::try_from(text).map_err(|_| PersistenceError::RowRejected {
        table,
        reason: "a stored identity was not a valid domain identifier",
    })
}

/// Reads an optional stored identifier.
fn optional_identity<T>(
    row: &sqlx::sqlite::SqliteRow,
    column: &'static str,
) -> PersistenceResult<Option<T>>
where
    T: TryFrom<String>,
{
    match row
        .try_get::<Option<String>, _>(column)
        .table("connections")?
    {
        Some(text) => T::try_from(text)
            .map(Some)
            .map_err(|_| PersistenceError::RowRejected {
                table: "connections",
                reason: "a stored identity was not a valid domain identifier",
            }),
        None => Ok(None),
    }
}

/// Reads a non-negative counter column as `u32`.
fn counter(
    row: &sqlx::sqlite::SqliteRow,
    column: &'static str,
    table: &'static str,
) -> PersistenceResult<u32> {
    let value: i64 = row.try_get(column).table(table)?;
    u32::try_from(value).map_err(|_| negative(table, "a stored counter was negative"))
}

/// Reads an optional RFC 3339 column.
fn instant(
    row: &sqlx::sqlite::SqliteRow,
    column: &'static str,
    table: &'static str,
) -> PersistenceResult<Option<DateTime<Utc>>> {
    match row.try_get::<Option<String>, _>(column).table(table)? {
        Some(text) => codec::parse_instant(&text, table).map(Some),
        None => Ok(None),
    }
}

/// A rejection for a stored value outside its permitted range.
fn negative(table: &'static str, reason: &'static str) -> PersistenceError {
    PersistenceError::RowRejected { table, reason }
}
