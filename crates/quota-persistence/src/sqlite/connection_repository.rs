//! Connections: the authorization handle one or more accounts belong to.
//!
//! This is the connection half of [`AccountRepository`]. A connection is only
//! meaningful through the accounts it authorises, so both halves share one
//! repository value.

use quota_domain::account::{AccountCardinality, ConnectionState, CredentialOwnership};
use quota_domain::ids::{ConnectionId, EntitlementId, ProviderPrincipalId, WorkspaceId};
use quota_domain::provider::ProviderId;

use crate::error::{PersistenceError, PersistenceResult, TableContext};
use crate::sqlite::account_repository::AccountRepository;
use crate::sqlite::codec;
use crate::sqlite::rows;

/// A non-secret summary of one authorization connection.
///
/// The providers Quota targets report labels rather than opaque identifiers, so
/// the provider-identifier columns stay `NULL` until a provider supplies one.
/// No field here names a credential, a token, or a keyring entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionRecord {
    /// Opaque local connection identity.
    pub id: ConnectionId,
    /// The compiled provider adapter.
    pub provider_id: ProviderId,
    /// Who holds the credential.
    pub credential_ownership: CredentialOwnership,
    /// Incremented on every reconnect, so late results can be rejected.
    pub generation: u32,
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

impl AccountRepository {
    /// Inserts a connection, or replaces the descriptive fields of one that exists.
    ///
    /// The generation and the state of an existing connection are left alone;
    /// [`Self::bump_generation`] and [`Self::set_connection_state`] own those.
    ///
    /// # Errors
    /// Returns a typed persistence error when the write is refused.
    pub async fn upsert_connection(&self, connection: &NewConnection) -> PersistenceResult<()> {
        Self::upsert_connection_on(&self.pool, connection).await
    }

    /// [`Self::upsert_connection`] on a given executor, so a caller can make it part of a
    /// larger transaction.
    pub(crate) async fn upsert_connection_on<'e>(
        executor: impl sqlx::SqliteExecutor<'e>,
        connection: &NewConnection,
    ) -> PersistenceResult<()> {
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
        .execute(executor)
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
        rows::map_connection(&row)
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
        Self::set_connection_state_on(&self.pool, connection_id, state).await
    }

    /// [`Self::set_connection_state`] on a given executor, so a caller can make it part of a
    /// larger transaction.
    pub(crate) async fn set_connection_state_on<'e>(
        executor: impl sqlx::SqliteExecutor<'e>,
        connection_id: &ConnectionId,
        state: ConnectionState,
    ) -> PersistenceResult<()> {
        let encoded = codec::encode(&state, "connections")?;
        let updated = sqlx::query("UPDATE connections SET state = ? WHERE id = ?")
            .bind(encoded)
            .bind(connection_id.as_str())
            .execute(executor)
            .await
            .table("connections")?
            .rows_affected();
        rows::require_one(updated, "connections")
    }

    /// Records provider-verified opaque identities for this connection.
    ///
    /// Display labels are stored on the account row. These identifiers remain
    /// opaque and are only used to re-check the binding before accepting a read.
    ///
    /// # Errors
    /// Returns [`PersistenceError::RowRejected`] when the connection does not exist.
    pub async fn record_verified_binding(
        &self,
        connection_id: &ConnectionId,
        principal_id: Option<&ProviderPrincipalId>,
        workspace_id: Option<&WorkspaceId>,
        entitlement_id: Option<&EntitlementId>,
    ) -> PersistenceResult<()> {
        Self::record_verified_binding_on(
            &self.pool,
            connection_id,
            principal_id,
            workspace_id,
            entitlement_id,
        )
        .await
    }

    /// [`Self::record_verified_binding`] on a given executor, so a caller can make it part of a
    /// larger transaction.
    pub(crate) async fn record_verified_binding_on<'e>(
        executor: impl sqlx::SqliteExecutor<'e>,
        connection_id: &ConnectionId,
        principal_id: Option<&ProviderPrincipalId>,
        workspace_id: Option<&WorkspaceId>,
        entitlement_id: Option<&EntitlementId>,
    ) -> PersistenceResult<()> {
        let updated = sqlx::query(
            "UPDATE connections
                SET principal_id = ?, workspace_id = ?, entitlement_id = ?
              WHERE id = ?",
        )
        .bind(principal_id.map(quota_domain::ids::ProviderPrincipalId::as_str))
        .bind(workspace_id.map(quota_domain::ids::WorkspaceId::as_str))
        .bind(entitlement_id.map(quota_domain::ids::EntitlementId::as_str))
        .bind(connection_id.as_str())
        .execute(executor)
        .await
        .table("connections")?
        .rows_affected();
        rows::require_one(updated, "connections")
    }

    /// Advances the connection generation and returns the new value.
    ///
    /// A result produced under an earlier generation can then be rejected on
    /// read, so a reconnect cannot be overwritten by the attempt it replaced.
    ///
    /// # Errors
    /// Returns [`PersistenceError::RowRejected`] when no such connection exists.
    pub async fn bump_generation(&self, connection_id: &ConnectionId) -> PersistenceResult<u32> {
        let mut transaction = self.pool.begin().await.table("connections")?;

        let updated =
            sqlx::query("UPDATE connections SET generation = generation + 1 WHERE id = ?")
                .bind(connection_id.as_str())
                .execute(&mut *transaction)
                .await
                .table("connections")?
                .rows_affected();
        rows::require_one(updated, "connections")?;

        let generation: i64 = sqlx::query_scalar("SELECT generation FROM connections WHERE id = ?")
            .bind(connection_id.as_str())
            .fetch_one(&mut *transaction)
            .await
            .table("connections")?;

        transaction.commit().await.table("connections")?;
        u32::try_from(generation).map_err(|_| PersistenceError::RowRejected {
            table: "connections",
            reason: "a stored generation was negative",
        })
    }
}
