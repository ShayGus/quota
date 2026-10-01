//! Connections, accounts, quota pools, and their bindings.
//!
//! Every statement addresses one account by its primary key, so deleting one
//! account can never reach a sibling that shares a provider. The sibling case
//! is covered by an integration test against a real migrated file.

use chrono::{DateTime, Utc};
use quota_domain::account::{
    AccountCardinality, ConnectionState, CredentialOwnership, FetchState, VerifiedIdentity,
};
use quota_domain::ids::{AccountId, ConnectionId, EntitlementId, ProviderPrincipalId, WorkspaceId};
use quota_domain::provider::ProviderId;
use sqlx::{Row, SqlitePool};

use crate::error::{PersistenceError, PersistenceResult, TableContext};
use crate::sqlite::codec;

/// A non-secret summary of one authorization connection.
///
/// The providers Quota targets report labels rather than opaque identifiers, so
/// the verified identity lives on the account row and the provider-identifier
/// columns stay `NULL` until a provider supplies one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionRecord {
    /// Opaque local connection identity.
    pub id: ConnectionId,
    /// The compiled provider adapter.
    pub provider_id: ProviderId,
    /// Who holds the credential.
    pub credential_ownership: CredentialOwnership,
    /// Incremented on every reconnect, so late results can be rejected.
    pub generation: u64,
    /// Optional adapter profile label.
    pub profile_label: Option<String>,
    /// What the adapter supports.
    pub cardinality: AccountCardinality,
    /// Where the connection stands.
    pub state: ConnectionState,
    /// A provider-reported principal identifier, when the provider has one.
    pub principal_id: Option<ProviderPrincipalId>,
    /// A provider-reported workspace identifier, when the provider has one.
    pub workspace_id: Option<WorkspaceId>,
    /// A provider-reported entitlement identifier, when the provider has one.
    pub entitlement_id: Option<EntitlementId>,
}

/// The values a connection is created with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewConnection {
    /// Opaque local connection identity.
    pub id: ConnectionId,
    /// The compiled provider adapter.
    pub provider_id: ProviderId,
    /// Who holds the credential.
    pub credential_ownership: CredentialOwnership,
    /// Optional adapter profile label.
    pub profile_label: Option<String>,
    /// What the adapter supports.
    pub cardinality: AccountCardinality,
}

/// The values a new account is created with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewAccount {
    /// Immutable account identity.
    pub id: AccountId,
    /// The connection that authorises this account.
    pub connection_id: ConnectionId,
    /// The compiled provider adapter.
    pub provider_id: ProviderId,
    /// The user-chosen display name.
    pub nickname: String,
    /// Stable tie-break order within the connection.
    pub connection_ordinal: u32,
}

/// One monitored account as durable state holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountRecord {
    /// Immutable account identity.
    pub id: AccountId,
    /// The connection that authorises this account.
    pub connection_id: ConnectionId,
    /// The connection generation this row belongs to.
    pub generation: u64,
    /// The compiled provider adapter.
    pub provider_id: ProviderId,
    /// The user-chosen display name. Presentation only.
    pub nickname: String,
    /// Stable tie-break order within the connection.
    pub connection_ordinal: u32,
    /// Whether the supervisor schedules reads for this account.
    pub monitoring_enabled: bool,
    /// Where the connection stands.
    pub connection_state: ConnectionState,
    /// How the last read attempt went.
    pub fetch_state: FetchState,
    /// When the last read attempt ran.
    pub last_attempt_at: Option<DateTime<Utc>>,
    /// When a reading was last accepted.
    pub last_success_at: Option<DateTime<Utc>>,
    /// When the next read becomes eligible.
    pub next_attempt_at: Option<DateTime<Utc>>,
    /// The verified identity, when one was confirmed.
    pub verified_identity: Option<VerifiedIdentity>,
}

/// Reads and writes connections, accounts, and pool bindings.
#[derive(Clone, Debug)]
pub struct AccountRepository {
    pool: SqlitePool,
}

impl AccountRepository {
    /// Wraps an open pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Inserts a connection, or replaces the descriptive fields of one that exists.
    ///
    /// The generation and the state of an existing connection are left alone;
    /// [`Self::bump_generation`] and [`Self::set_connection_state`] own those.
    ///
    /// # Errors
    /// Returns a typed persistence error when the write is refused.
    pub async fn upsert_connection(&self, connection: &NewConnection) -> PersistenceResult<()> {
        let ownership = codec::encode(&connection.credential_ownership, "connections")?;
        let cardinality = codec::encode(&connection.cardinality, "connections")?;
        let state = codec::encode(&ConnectionState::NeverConnected, "connections")?;

        sqlx::query(
            "INSERT INTO connections (
                 id, provider_id, credential_ownership, generation, profile_label,
                 cardinality, state, principal_id, workspace_id, entitlement_id
             ) VALUES (?, ?, ?, 0, ?, ?, ?, NULL, NULL, NULL)
             ON CONFLICT (id) DO UPDATE SET
                 provider_id = excluded.provider_id,
                 credential_ownership = excluded.credential_ownership,
                 profile_label = excluded.profile_label,
                 cardinality = excluded.cardinality",
        )
        .bind(connection.id.as_str())
        .bind(connection.provider_id.as_str())
        .bind(ownership)
        .bind(connection.profile_label.as_deref())
        .bind(cardinality)
        .bind(state)
        .execute(&self.pool)
        .await
        .table("connections")?;
        Ok(())
    }

    /// Reads one connection.
    ///
    /// # Errors
    /// Returns [`PersistenceError::RowRejected`] when no such connection exists.
    pub async fn connection(
        &self,
        connection_id: &ConnectionId,
    ) -> PersistenceResult<ConnectionRecord> {
        let row = sqlx::query(
            "SELECT id, provider_id, credential_ownership, generation, profile_label,
                    cardinality, state, principal_id, workspace_id, entitlement_id
               FROM connections WHERE id = ?",
        )
        .bind(connection_id.as_str())
        .fetch_optional(&self.pool)
        .await
        .table("connections")?
        .ok_or(PersistenceError::RowRejected {
            table: "connections",
            reason: "no connection matched the requested identity",
        })?;
        map_connection(&row)
    }

    /// Records where a connection stands.
    ///
    /// # Errors
    /// Returns [`PersistenceError::RowRejected`] when no such connection exists.
    pub async fn set_connection_state(
        &self,
        connection_id: &ConnectionId,
        state: ConnectionState,
    ) -> PersistenceResult<()> {
        let encoded = codec::encode(&state, "connections")?;
        let updated = sqlx::query("UPDATE connections SET state = ? WHERE id = ?")
            .bind(encoded)
            .bind(connection_id.as_str())
            .execute(&self.pool)
            .await
            .table("connections")?
            .rows_affected();
        require_one(updated, "connections")
    }

    /// Advances the connection generation and returns the new value.
    ///
    /// A result produced under an earlier generation can then be rejected on
    /// read, so a reconnect cannot be overwritten by an attempt it replaced.
    ///
    /// # Errors
    /// Returns [`PersistenceError::RowRejected`] when no such connection exists.
    pub async fn bump_generation(&self, connection_id: &ConnectionId) -> PersistenceResult<u64> {
        let mut transaction = self.pool.begin().await.table("connections")?;

        let updated =
            sqlx::query("UPDATE connections SET generation = generation + 1 WHERE id = ?")
                .bind(connection_id.as_str())
                .execute(&mut *transaction)
                .await
                .table("connections")?
                .rows_affected();
        require_one(updated, "connections")?;

        let generation: i64 = sqlx::query_scalar("SELECT generation FROM connections WHERE id = ?")
            .bind(connection_id.as_str())
            .fetch_one(&mut *transaction)
            .await
            .table("connections")?;

        transaction.commit().await.table("connections")?;
        u64::try_from(generation).map_err(|_| PersistenceError::RowRejected {
            table: "connections",
            reason: "a stored generation was negative",
        })
    }

    /// Inserts an account, or updates the descriptive fields of one that exists.
    ///
    /// The account copies the connection's generation and state inside the same
    /// transaction as the insert, so it can never record a state that a
    /// concurrent reconnect has already replaced.
    ///
    /// # Errors
    /// Returns [`PersistenceError::RowRejected`] when the named connection does
    /// not exist, and [`PersistenceError::IntegrityViolation`] on a duplicate
    /// identity or a duplicate ordinal within the connection.
    pub async fn upsert_account(&self, account: &NewAccount) -> PersistenceResult<()> {
        let fetch_state = codec::encode(&FetchState::Idle, "accounts")?;

        let mut transaction = self.pool.begin().await.table("accounts")?;

        let connection: Option<(i64, String)> =
            sqlx::query_as("SELECT generation, state FROM connections WHERE id = ?")
                .bind(account.connection_id.as_str())
                .fetch_optional(&mut *transaction)
                .await
                .table("connections")?;
        let Some((_generation, connection_state)) = connection else {
            return Err(PersistenceError::RowRejected {
                table: "connections",
                reason: "the account names a connection that does not exist",
            });
        };

        sqlx::query(
            "INSERT INTO accounts (
                 id, connection_id, provider_id, nickname, connection_ordinal,
                 monitoring_enabled, connection_state, fetch_state, last_attempt_at,
                 last_success_at, next_attempt_at, verified_principal_label,
                 verified_workspace_label, verified_plan_label, identity_source
             ) VALUES (?, ?, ?, ?, ?, 1, ?, ?, NULL, NULL, NULL, NULL, NULL, NULL, NULL)
             ON CONFLICT (id) DO UPDATE SET
                 nickname = excluded.nickname,
                 connection_ordinal = excluded.connection_ordinal",
        )
        .bind(account.id.as_str())
        .bind(account.connection_id.as_str())
        .bind(account.provider_id.as_str())
        .bind(account.nickname.as_str())
        .bind(i64::from(account.connection_ordinal))
        .bind(connection_state)
        .bind(fetch_state)
        .execute(&mut *transaction)
        .await
        .table("accounts")?;

        transaction.commit().await.table("accounts")?;
        Ok(())
    }

    /// Enables or disables scheduling for one account.
    ///
    /// # Errors
    /// Returns [`PersistenceError::RowRejected`] when no such account exists.
    pub async fn set_monitoring_enabled(
        &self,
        account_id: &AccountId,
        enabled: bool,
    ) -> PersistenceResult<()> {
        let updated = sqlx::query("UPDATE accounts SET monitoring_enabled = ? WHERE id = ?")
            .bind(i64::from(enabled))
            .bind(account_id.as_str())
            .execute(&self.pool)
            .await
            .table("accounts")?
            .rows_affected();
        require_one(updated, "accounts")
    }

    /// Records the verified identity and connection state of one account.
    ///
    /// # Errors
    /// Returns [`PersistenceError::RowRejected`] when no such account exists.
    pub async fn record_verified_identity(
        &self,
        account_id: &AccountId,
        state: ConnectionState,
        identity: &VerifiedIdentity,
    ) -> PersistenceResult<()> {
        let encoded_state = codec::encode(&state, "accounts")?;
        let encoded_source = codec::encode(&identity.source, "accounts")?;

        let updated = sqlx::query(
            "UPDATE accounts
                SET connection_state = ?,
                    verified_principal_label = ?,
                    verified_workspace_label = ?,
                    verified_plan_label = ?,
                    identity_source = ?
              WHERE id = ?",
        )
        .bind(encoded_state)
        .bind(identity.principal_label.as_str())
        .bind(identity.workspace_label.as_deref())
        .bind(identity.plan_label.as_deref())
        .bind(encoded_source)
        .bind(account_id.as_str())
        .execute(&self.pool)
        .await
        .table("accounts")?
        .rows_affected();
        require_one(updated, "accounts")
    }

    /// Records the outcome of one read attempt.
    ///
    /// # Errors
    /// Returns [`PersistenceError::RowRejected`] when no such account exists.
    pub async fn record_attempt(
        &self,
        account_id: &AccountId,
        state: FetchState,
        attempted_at: DateTime<Utc>,
        next_attempt_at: Option<DateTime<Utc>>,
    ) -> PersistenceResult<()> {
        let encoded = codec::encode(&state, "accounts")?;
        let succeeded = i64::from(state == FetchState::Idle);
        let at = codec::instant(attempted_at);

        let updated = sqlx::query(
            "UPDATE accounts
                SET fetch_state = ?,
                    last_attempt_at = ?,
                    last_success_at = CASE WHEN ? = 1 THEN ? ELSE last_success_at END,
                    next_attempt_at = ?
              WHERE id = ?",
        )
        .bind(encoded)
        .bind(&at)
        .bind(succeeded)
        .bind(&at)
        .bind(next_attempt_at.map(codec::instant))
        .bind(account_id.as_str())
        .execute(&self.pool)
        .await
        .table("accounts")?
        .rows_affected();
        require_one(updated, "accounts")
    }

    /// Lists the accounts of one connection, in stable ordinal order.
    ///
    /// # Errors
    /// Returns a typed persistence error when the read fails or a stored row is
    /// outside this build's vocabulary.
    pub async fn list_for_connection(
        &self,
        connection_id: &ConnectionId,
    ) -> PersistenceResult<Vec<AccountRecord>> {
        let rows = sqlx::query(
            "SELECT a.id, a.connection_id, c.generation, a.provider_id, a.nickname,
                    a.connection_ordinal, a.monitoring_enabled, a.connection_state,
                    a.fetch_state, a.last_attempt_at, a.last_success_at,
                    a.next_attempt_at, a.verified_principal_label,
                    a.verified_workspace_label, a.verified_plan_label, a.identity_source
               FROM accounts a
               JOIN connections c ON c.id = a.connection_id
              WHERE a.connection_id = ?
              ORDER BY a.connection_ordinal",
        )
        .bind(connection_id.as_str())
        .fetch_all(&self.pool)
        .await
        .table("accounts")?;

        rows.iter().map(map_account).collect()
    }

    /// Deletes one account by its primary key, and only its own rows.
    ///
    /// Its latest measurements and its pool bindings follow through
    /// `ON DELETE CASCADE`. Its optional history rows carry no foreign key, so
    /// the same transaction removes them by the same identity. A sibling
    /// account of the same provider is a different primary key and is never
    /// addressed by these statements.
    ///
    /// # Errors
    /// Returns a typed persistence error when the delete fails.
    pub async fn delete_account(&self, account_id: &AccountId) -> PersistenceResult<bool> {
        let mut transaction = self.pool.begin().await.table("accounts")?;

        sqlx::query("DELETE FROM measurement_history WHERE account_id = ?")
            .bind(account_id.as_str())
            .execute(&mut *transaction)
            .await
            .table("measurement_history")?;

        let deleted = sqlx::query("DELETE FROM accounts WHERE id = ?")
            .bind(account_id.as_str())
            .execute(&mut *transaction)
            .await
            .table("accounts")?
            .rows_affected();

        transaction.commit().await.table("accounts")?;
        Ok(deleted == 1)
    }

    /// Records that an account shares a quota pool.
    ///
    /// # Errors
    /// Returns a typed persistence error when the write is refused, including
    /// when the account does not exist.
    pub async fn bind_pool(
        &self,
        account_id: &AccountId,
        pool_id: &quota_domain::ids::QuotaPoolId,
        provider_id: ProviderId,
        shared: bool,
    ) -> PersistenceResult<()> {
        let mut transaction = self.pool.begin().await.table("quota_pools")?;

        sqlx::query("INSERT OR IGNORE INTO quota_pools (id, provider_id, shared) VALUES (?, ?, ?)")
            .bind(pool_id.as_str())
            .bind(provider_id.as_str())
            .bind(i64::from(shared))
            .execute(&mut *transaction)
            .await
            .table("quota_pools")?;

        sqlx::query(
            "INSERT OR IGNORE INTO account_pool_bindings (account_id, pool_id) VALUES (?, ?)",
        )
        .bind(account_id.as_str())
        .bind(pool_id.as_str())
        .execute(&mut *transaction)
        .await
        .table("account_pool_bindings")?;

        transaction.commit().await.table("account_pool_bindings")?;
        Ok(())
    }
}

/// Maps a "no rows were changed" outcome onto a typed rejection.
fn require_one(changed: u64, table: &'static str) -> PersistenceResult<()> {
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
fn map_account(row: &sqlx::sqlite::SqliteRow) -> PersistenceResult<AccountRecord> {
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
        id: AccountId::new(row.try_get::<String, _>("id").table("accounts")?)
            .map_err(|_| reject_identity("accounts"))?,
        connection_id: ConnectionId::new(
            row.try_get::<String, _>("connection_id")
                .table("accounts")?,
        )
        .map_err(|_| reject_identity("accounts"))?,
        generation: read_u64(row, "generation", "accounts")?,
        provider_id: ProviderId::parse(&row.try_get::<String, _>("provider_id").table("accounts")?)
            .map_err(|_| PersistenceError::RowRejected {
                table: "accounts",
                reason: "the stored provider name is outside this build's vocabulary",
            })?,
        nickname: row.try_get("nickname").table("accounts")?,
        connection_ordinal: u32::try_from(
            row.try_get::<i64, _>("connection_ordinal")
                .table("accounts")?,
        )
        .map_err(|_| PersistenceError::RowRejected {
            table: "accounts",
            reason: "a stored ordinal was outside the unsigned range",
        })?,
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
        last_attempt_at: read_optional_instant(row, "last_attempt_at")?,
        last_success_at: read_optional_instant(row, "last_success_at")?,
        next_attempt_at: read_optional_instant(row, "next_attempt_at")?,
        verified_identity,
    })
}

/// Maps one connection row.
fn map_connection(row: &sqlx::sqlite::SqliteRow) -> PersistenceResult<ConnectionRecord> {
    Ok(ConnectionRecord {
        id: ConnectionId::new(row.try_get::<String, _>("id").table("connections")?)
            .map_err(|_| reject_identity("connections"))?,
        provider_id: ProviderId::parse(
            &row.try_get::<String, _>("provider_id")
                .table("connections")?,
        )
        .map_err(|_| PersistenceError::RowRejected {
            table: "connections",
            reason: "the stored provider name is outside this build's vocabulary",
        })?,
        credential_ownership: codec::decode(
            &row.try_get::<String, _>("credential_ownership")
                .table("connections")?,
            "connections",
        )?,
        generation: read_u64(row, "generation", "connections")?,
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
        principal_id: read_optional_identity(row, "principal_id")?,
        workspace_id: read_optional_identity(row, "workspace_id")?,
        entitlement_id: read_optional_identity(row, "entitlement_id")?,
    })
}

/// A fixed rejection for a stored identity that cannot become a domain value.
fn reject_identity(table: &'static str) -> PersistenceError {
    PersistenceError::RowRejected {
        table,
        reason: "a stored identity could not become a valid domain identifier",
    }
}

/// Reads a non-negative integer column as `u64`.
fn read_u64(
    row: &sqlx::sqlite::SqliteRow,
    column: &'static str,
    table: &'static str,
) -> PersistenceResult<u64> {
    let value: i64 = row.try_get(column).table(table)?;
    u64::try_from(value).map_err(|_| PersistenceError::RowRejected {
        table,
        reason: "a stored counter was negative",
    })
}

/// Reads an optional RFC 3339 column.
fn read_optional_instant(
    row: &sqlx::sqlite::SqliteRow,
    column: &'static str,
) -> PersistenceResult<Option<DateTime<Utc>>> {
    match row.try_get::<Option<String>, _>(column).table("accounts")? {
        Some(text) => codec::parse_instant(&text, "accounts").map(Some),
        None => Ok(None),
    }
}

/// Reads an optional stored identity column.
fn read_optional_identity<T>(
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
                reason: "a stored identity could not become a valid domain identifier",
            }),
        None => Ok(None),
    }
}
