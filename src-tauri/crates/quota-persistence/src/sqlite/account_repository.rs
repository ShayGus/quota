//! Accounts: the monitored identities under one connection.
//!
//! Every statement here addresses one account by its primary key, so deleting
//! one account can never reach a sibling that shares a provider. The sibling
//! case is covered by an integration test against a real migrated file.
//!
//! Connection statements live in [`crate::sqlite::connection_repository`]; both
//! modules are `impl` blocks on the same [`AccountRepository`].

use chrono::{DateTime, Utc};
use quota_domain::account::{ConnectionState, FetchState, VerifiedIdentity};
use quota_domain::ids::{AccountId, ConnectionId, QuotaPoolId};
use quota_domain::provider::ProviderId;
use sqlx::SqlitePool;

use crate::error::{PersistenceError, PersistenceResult, TableContext};
use crate::sqlite::codec;
use crate::sqlite::rows;

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
    pub generation: u32,
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
    /// Whether the card shows the account's API key spend limit.
    pub show_key_limit: bool,
}

/// Reads and writes connections, accounts, and pool bindings.
#[derive(Clone, Debug)]
pub struct AccountRepository {
    pub(crate) pool: SqlitePool,
}

impl AccountRepository {
    /// Wraps an open pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Inserts an account, or updates the descriptive fields of one that exists.
    ///
    /// The account copies the connection's state inside the same transaction as
    /// the insert, so it can never record a state that a concurrent reconnect
    /// has already replaced.
    ///
    /// # Errors
    /// Returns [`PersistenceError::RowRejected`] when the named connection does
    /// not exist, and [`PersistenceError::IntegrityViolation`] on a duplicate
    /// identity or a duplicate ordinal within the connection.
    pub async fn upsert_account(&self, account: &NewAccount) -> PersistenceResult<()> {
        let mut transaction = self.pool.begin().await.table("accounts")?;
        Self::upsert_account_in(&mut transaction, account).await?;
        transaction.commit().await.table("accounts")?;
        Ok(())
    }

    /// [`Self::upsert_account`] inside a caller's transaction, which the caller commits.
    pub(crate) async fn upsert_account_in(
        transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        account: &NewAccount,
    ) -> PersistenceResult<()> {
        let fetch_state = codec::encode(&FetchState::Idle, "accounts")?;

        let connection_state: Option<String> =
            sqlx::query_scalar("SELECT state FROM connections WHERE id = ?")
                .bind(account.connection_id.as_str())
                .fetch_optional(&mut **transaction)
                .await
                .table("connections")?;
        let Some(connection_state) = connection_state else {
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
        .execute(&mut **transaction)
        .await
        .table("accounts")?;
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
        Self::set_monitoring_enabled_on(&self.pool, account_id, enabled).await
    }

    /// [`Self::set_monitoring_enabled`] on a given executor, so a caller can make it part of a
    /// larger transaction.
    pub(crate) async fn set_monitoring_enabled_on<'e>(
        executor: impl sqlx::SqliteExecutor<'e>,
        account_id: &AccountId,
        enabled: bool,
    ) -> PersistenceResult<()> {
        let updated = sqlx::query("UPDATE accounts SET monitoring_enabled = ? WHERE id = ?")
            .bind(i64::from(enabled))
            .bind(account_id.as_str())
            .execute(executor)
            .await
            .table("accounts")?
            .rows_affected();
        rows::require_one(updated, "accounts")
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
        Self::record_verified_identity_on(&self.pool, account_id, state, identity).await
    }

    /// [`Self::record_verified_identity`] on a given executor, so a caller can make it part of a
    /// larger transaction.
    pub(crate) async fn record_verified_identity_on<'e>(
        executor: impl sqlx::SqliteExecutor<'e>,
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
        .execute(executor)
        .await
        .table("accounts")?
        .rows_affected();
        rows::require_one(updated, "accounts")
    }

    /// Records the outcome of one read attempt.
    ///
    /// # Errors
    /// Returns [`PersistenceError::RowRejected`] when no such account exists.
    pub async fn record_attempt(
        &self,
        account_id: &AccountId,
        state: FetchState,
        attempted_at: Option<DateTime<Utc>>,
        succeeded_at: Option<DateTime<Utc>>,
        next_attempt_at: Option<DateTime<Utc>>,
    ) -> PersistenceResult<()> {
        Self::record_attempt_on(
            &self.pool,
            account_id,
            state,
            attempted_at,
            succeeded_at,
            next_attempt_at,
        )
        .await
    }

    /// [`Self::record_attempt`] on a given executor, so a caller can make it part of a
    /// larger transaction.
    pub(crate) async fn record_attempt_on<'e>(
        executor: impl sqlx::SqliteExecutor<'e>,
        account_id: &AccountId,
        state: FetchState,
        attempted_at: Option<DateTime<Utc>>,
        succeeded_at: Option<DateTime<Utc>>,
        next_attempt_at: Option<DateTime<Utc>>,
    ) -> PersistenceResult<()> {
        let encoded = codec::encode(&state, "accounts")?;
        let updated = sqlx::query(
            "UPDATE accounts
                SET fetch_state = ?,
                    last_attempt_at = ?,
                    last_success_at = ?,
                    next_attempt_at = ?
              WHERE id = ?",
        )
        .bind(encoded)
        .bind(attempted_at.map(codec::instant))
        .bind(succeeded_at.map(codec::instant))
        .bind(next_attempt_at.map(codec::instant))
        .bind(account_id.as_str())
        .execute(executor)
        .await
        .table("accounts")?
        .rows_affected();
        rows::require_one(updated, "accounts")
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
                    a.verified_workspace_label, a.verified_plan_label, a.identity_source,
                    a.show_key_limit
               FROM accounts a
               JOIN connections c ON c.id = a.connection_id
              WHERE a.connection_id = ?
              ORDER BY a.connection_ordinal",
        )
        .bind(connection_id.as_str())
        .fetch_all(&self.pool)
        .await
        .table("accounts")?;

        rows.iter().map(rows::map_account).collect()
    }

    /// Lists every account in stable connection order.
    ///
    /// # Errors
    /// Returns a typed persistence error when the read fails or a stored row is
    /// outside this build's vocabulary.
    pub async fn list_all(&self) -> PersistenceResult<Vec<AccountRecord>> {
        let rows = sqlx::query(
            "SELECT a.id, a.connection_id, c.generation, a.provider_id, a.nickname,
                    a.connection_ordinal, a.monitoring_enabled, a.connection_state,
                    a.fetch_state, a.last_attempt_at, a.last_success_at,
                    a.next_attempt_at, a.verified_principal_label,
                    a.verified_workspace_label, a.verified_plan_label, a.identity_source,
                    a.show_key_limit
               FROM accounts a
               JOIN connections c ON c.id = a.connection_id
              ORDER BY a.connection_ordinal, a.id",
        )
        .fetch_all(&self.pool)
        .await
        .table("accounts")?;

        rows.iter().map(rows::map_account).collect()
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
        pool_id: &QuotaPoolId,
        provider_id: ProviderId,
        shared: bool,
    ) -> PersistenceResult<()> {
        let mut transaction = self.pool.begin().await.table("quota_pools")?;
        Self::bind_pool_in(&mut transaction, account_id, pool_id, provider_id, shared).await?;
        transaction.commit().await.table("account_pool_bindings")?;
        Ok(())
    }

    /// [`Self::bind_pool`] inside a caller's transaction, which the caller commits.
    pub(crate) async fn bind_pool_in(
        transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        account_id: &AccountId,
        pool_id: &QuotaPoolId,
        provider_id: ProviderId,
        shared: bool,
    ) -> PersistenceResult<()> {
        sqlx::query("INSERT OR IGNORE INTO quota_pools (id, provider_id, shared) VALUES (?, ?, ?)")
            .bind(pool_id.as_str())
            .bind(provider_id.as_str())
            .bind(i64::from(shared))
            .execute(&mut **transaction)
            .await
            .table("quota_pools")?;

        sqlx::query(
            "INSERT OR IGNORE INTO account_pool_bindings (account_id, pool_id) VALUES (?, ?)",
        )
        .bind(account_id.as_str())
        .bind(pool_id.as_str())
        .execute(&mut **transaction)
        .await
        .table("account_pool_bindings")?;
        Ok(())
    }
}
