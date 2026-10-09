//! Account, reading, and history port adapters over typed `SQLite` repositories.

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use quota_core::ports::{
    AccountRepository as AccountPort, HistoryRepository as HistoryPort, RepositoryError,
    StoredAccount,
};
use quota_domain::account::ConnectionSummary;
use quota_domain::ids::AccountId;
use quota_domain::quota::measurement::{Measurement, UnavailableReason};
use quota_domain::snapshot::AccountSnapshot;

use crate::error::TableContext;
use crate::sqlite::{
    AccountRepository, MeasurementRepository, NewAccount, NewConnection, SqliteRepositories,
};

fn map_error(error: &crate::PersistenceError) -> RepositoryError {
    RepositoryError::new("sqlite", error.to_string())
}

/// Implements the account and history ports over one migrated `SQLite` pool.
#[derive(Clone, Debug)]
pub struct SqliteAccountPortAdapter {
    repositories: SqliteRepositories,
}

impl SqliteAccountPortAdapter {
    /// Wraps the already-open repository set. It does not create a pool or run migrations.
    #[must_use]
    pub const fn new(repositories: SqliteRepositories) -> Self {
        Self { repositories }
    }

    async fn stored_accounts(&self) -> Result<Vec<StoredAccount>, RepositoryError> {
        let records = self
            .repositories
            .accounts()
            .list_all()
            .await
            .map_err(|e| map_error(&e))?;
        let mut accounts = Vec::with_capacity(records.len());
        for record in records {
            let connection = self
                .repositories
                .accounts()
                .connection(&record.connection_id)
                .await
                .map_err(|e| map_error(&e))?;
            let windows = self
                .repositories
                .measurements()
                .windows_for_account(&record.id)
                .await
                .map_err(|e| map_error(&e))?;
            let expected_but_missing_window_ids = windows
                .iter()
                .filter_map(|window| match window.measurement {
                    Measurement::Unavailable(UnavailableReason::NotReported) => {
                        Some(window.id.clone())
                    }
                    _ => None,
                })
                .collect();
            let balance = self
                .repositories
                .accounts()
                .balance_ledger(&record.id)
                .await
                .map_err(|e| map_error(&e))?;
            let group = self
                .repositories
                .accounts()
                .group_of(&record.id)
                .await
                .map_err(|e| map_error(&e))?;
            accounts.push(StoredAccount {
                account_id: record.id,
                connection: ConnectionSummary {
                    id: connection.id,
                    provider_id: connection.provider_id,
                    credential_ownership: connection.credential_ownership,
                    generation: connection.generation,
                    profile_label: connection.profile_label,
                    cardinality: connection.cardinality,
                    state: connection.state,
                    principal_id: connection.principal_id,
                    workspace_id: connection.workspace_id,
                    entitlement_id: connection.entitlement_id,
                },
                nickname: record.nickname,
                connection_ordinal: record.connection_ordinal,
                monitoring_enabled: record.monitoring_enabled,
                connection_state: record.connection_state,
                fetch_state: record.fetch_state,
                last_attempt_at: record.last_attempt_at,
                last_success_at: record.last_success_at,
                next_attempt_at: record.next_attempt_at,
                identity: record.verified_identity,
                windows,
                expected_but_missing_window_ids,
                balance,
                show_key_limit: record.show_key_limit,
                group,
            });
        }
        Ok(accounts)
    }

    /// Writes one account, its connection, its pool bindings and its readings
    /// in a single transaction, so a failure at any step leaves nothing of the
    /// account behind: a confirmed account is either saved whole or not at all.
    async fn store_account(&self, account: StoredAccount) -> Result<(), RepositoryError> {
        let mut transaction = self
            .repositories
            .pool()
            .begin()
            .await
            .table("accounts")
            .map_err(|e| map_error(&e))?;
        store_connection(&mut transaction, &account).await?;
        store_account_row(&mut transaction, &account).await?;
        store_windows(&mut transaction, &account).await?;
        store_balance(&mut transaction, &account).await?;
        transaction
            .commit()
            .await
            .table("accounts")
            .map_err(|e| map_error(&e))
    }
}

type Transaction<'t> = sqlx::Transaction<'t, sqlx::Sqlite>;

async fn store_connection(
    transaction: &mut Transaction<'_>,
    account: &StoredAccount,
) -> Result<(), RepositoryError> {
    let connection = &account.connection;
    let new_connection = NewConnection {
        id: connection.id.clone(),
        provider_id: connection.provider_id,
        credential_ownership: connection.credential_ownership,
        profile_label: connection.profile_label.clone(),
        cardinality: connection.cardinality,
    };
    AccountRepository::upsert_connection_on(&mut **transaction, &new_connection)
        .await
        .map_err(|e| map_error(&e))?;
    AccountRepository::set_connection_state_on(
        &mut **transaction,
        &connection.id,
        connection.state,
    )
    .await
    .map_err(|e| map_error(&e))?;
    AccountRepository::record_verified_binding_on(
        &mut **transaction,
        &connection.id,
        connection.principal_id.as_ref(),
        connection.workspace_id.as_ref(),
        connection.entitlement_id.as_ref(),
    )
    .await
    .map_err(|e| map_error(&e))
}

async fn store_account_row(
    transaction: &mut Transaction<'_>,
    account: &StoredAccount,
) -> Result<(), RepositoryError> {
    let new_account = NewAccount {
        id: account.account_id.clone(),
        connection_id: account.connection.id.clone(),
        provider_id: account.connection.provider_id,
        nickname: account.nickname.clone(),
        connection_ordinal: account.connection_ordinal,
    };
    AccountRepository::upsert_account_in(transaction, &new_account)
        .await
        .map_err(|e| map_error(&e))?;
    AccountRepository::set_monitoring_enabled_on(
        &mut **transaction,
        &account.account_id,
        account.monitoring_enabled,
    )
    .await
    .map_err(|e| map_error(&e))?;
    if let Some(identity) = account.identity.as_ref() {
        AccountRepository::record_verified_identity_on(
            &mut **transaction,
            &account.account_id,
            account.connection_state,
            identity,
        )
        .await
        .map_err(|e| map_error(&e))?;
    }
    AccountRepository::record_attempt_on(
        &mut **transaction,
        &account.account_id,
        account.fetch_state,
        account.last_attempt_at,
        account.last_success_at,
        account.next_attempt_at,
    )
    .await
    .map_err(|e| map_error(&e))
}

/// The account's prepaid-balance ledger, its key-limit switch and its group.
async fn store_balance(
    transaction: &mut Transaction<'_>,
    account: &StoredAccount,
) -> Result<(), RepositoryError> {
    AccountRepository::set_show_key_limit_on(
        &mut **transaction,
        &account.account_id,
        account.show_key_limit,
    )
    .await
    .map_err(|e| map_error(&e))?;
    AccountRepository::store_balance_on(
        &mut **transaction,
        &account.account_id,
        account.balance.as_ref(),
    )
    .await
    .map_err(|e| map_error(&e))?;
    AccountRepository::set_group_in(
        transaction,
        &account.account_id,
        account.connection.provider_id,
        account.group.as_ref(),
    )
    .await
    .map_err(|e| map_error(&e))
}

async fn store_windows(
    transaction: &mut Transaction<'_>,
    account: &StoredAccount,
) -> Result<(), RepositoryError> {
    for window in &account.windows {
        AccountRepository::bind_pool_in(
            transaction,
            &account.account_id,
            &window.pool_id,
            account.connection.provider_id,
            false,
        )
        .await
        .map_err(|e| map_error(&e))?;
    }
    MeasurementRepository::replace_readings_in(transaction, &account.account_id, &account.windows)
        .await
        .map(|_| ())
        .map_err(|e| map_error(&e))
}

#[async_trait]
impl AccountPort for SqliteAccountPortAdapter {
    async fn load_accounts(&self) -> Result<Vec<StoredAccount>, RepositoryError> {
        self.stored_accounts().await
    }

    async fn upsert_account(&self, account: StoredAccount) -> Result<(), RepositoryError> {
        self.store_account(account).await
    }

    async fn set_enabled(
        &self,
        account_id: &AccountId,
        enabled: bool,
    ) -> Result<(), RepositoryError> {
        self.repositories
            .accounts()
            .set_monitoring_enabled(account_id, enabled)
            .await
            .map_err(|e| map_error(&e))
    }

    async fn bump_generation(
        &self,
        connection_id: &quota_domain::ids::ConnectionId,
    ) -> Result<u32, RepositoryError> {
        self.repositories
            .accounts()
            .bump_generation(connection_id)
            .await
            .map_err(|e| map_error(&e))
    }

    async fn remove_account(&self, account_id: &AccountId) -> Result<(), RepositoryError> {
        self.repositories
            .accounts()
            .delete_account(account_id)
            .await
            .map(|_| ())
            .map_err(|e| map_error(&e))
    }

    async fn persist_reading(
        &self,
        account_id: &AccountId,
        windows: &[quota_domain::quota::window::QuotaWindow],
        _observed_at: Option<DateTime<Utc>>,
    ) -> Result<(), RepositoryError> {
        self.repositories
            .measurements()
            .replace_readings(account_id, windows)
            .await
            .map(|_| ())
            .map_err(|e| map_error(&e))
    }

    async fn snapshot_of(
        &self,
        account_id: &AccountId,
    ) -> Result<Option<AccountSnapshot>, RepositoryError> {
        let accounts = self.stored_accounts().await?;
        let Some(account) = accounts
            .into_iter()
            .find(|item| &item.account_id == account_id)
        else {
            return Ok(None);
        };
        let now = Utc::now();
        let input = quota_domain::ranking::RankingInput {
            account_id: &account.account_id,
            connection_ordinal: account.connection_ordinal,
            windows: &account.windows,
            monitoring_enabled: account.monitoring_enabled,
            monitoring_paused: false,
            connection_state: account.connection_state,
            now,
        };
        let order = quota_domain::ranking::rank_account(&input);
        Ok(Some(AccountSnapshot {
            account_id: account.account_id,
            connection_id: account.connection.id,
            connection_generation: account.connection.generation,
            provider_id: account.connection.provider_id,
            nickname: account.nickname,
            identity: account.identity,
            connection_ordinal: account.connection_ordinal,
            monitoring_enabled: account.monitoring_enabled,
            connection_state: account.connection_state,
            fetch_state: account.fetch_state,
            last_attempt_at: account.last_attempt_at,
            last_success_at: account.last_success_at,
            next_attempt_at: account.next_attempt_at,
            windows: account.windows,
            expected_but_missing_window_ids: account.expected_but_missing_window_ids,
            order,
            balance: account.balance.as_ref().map(|ledger| ledger.summary(now)),
            show_key_limit: account.show_key_limit,
            group: account.group,
        }))
    }
}

/// Implements account-scoped history deletion over `SQLite`.
#[derive(Clone, Debug)]
pub struct SqliteHistoryPortAdapter {
    repositories: SqliteRepositories,
}

impl SqliteHistoryPortAdapter {
    /// Wraps the already-open repository set.
    #[must_use]
    pub const fn new(repositories: SqliteRepositories) -> Self {
        Self { repositories }
    }
}

#[async_trait]
impl HistoryPort for SqliteHistoryPortAdapter {
    async fn clear_history(&self, account_id: &AccountId) -> Result<(), RepositoryError> {
        self.repositories
            .measurements()
            .clear_history_for_account(account_id)
            .await
            .map(|_| ())
            .map_err(|e| map_error(&e))
    }
}
