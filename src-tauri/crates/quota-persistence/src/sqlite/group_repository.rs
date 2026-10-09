//! Account groups: which provider account an account belongs to.
//!
//! A group row holds a name and a provider; an account row names its group.
//! Every write here ends by deleting the groups no account names any more, so
//! a group lives exactly as long as it has members. These are `impl` blocks on
//! the same [`AccountRepository`] as [`crate::sqlite::account_repository`].

use quota_domain::group::AccountGroup;
use quota_domain::ids::{AccountGroupId, AccountId};
use quota_domain::provider::ProviderId;

use crate::error::{PersistenceError, PersistenceResult, TableContext};
use crate::sqlite::account_repository::AccountRepository;
use crate::sqlite::rows;

impl AccountRepository {
    /// The group one account belongs to, when it is grouped.
    ///
    /// # Errors
    /// Returns [`PersistenceError::RowRejected`] when a stored group id or
    /// name is not one this build accepts.
    pub async fn group_of(
        &self,
        account_id: &AccountId,
    ) -> PersistenceResult<Option<AccountGroup>> {
        let stored: Option<(String, String)> = sqlx::query_as(
            "SELECT g.id, g.name
               FROM accounts a
               JOIN account_groups g ON g.id = a.group_id
              WHERE a.id = ?",
        )
        .bind(account_id.as_str())
        .fetch_optional(&self.pool)
        .await
        .table("account_groups")?;
        stored
            .map(|(id, name)| {
                AccountGroupId::new(id)
                    .and_then(|id| AccountGroup::new(id, &name))
                    .map_err(|_| PersistenceError::RowRejected {
                        table: "account_groups",
                        reason: "the stored group could not be read",
                    })
            })
            .transpose()
    }

    /// Puts one account in `group`, creating or renaming the group row, or
    /// takes it out of every group, then deletes the groups left empty.
    pub(crate) async fn set_group_in(
        transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        account_id: &AccountId,
        provider_id: ProviderId,
        group: Option<&AccountGroup>,
    ) -> PersistenceResult<()> {
        if let Some(group) = group {
            sqlx::query(
                "INSERT INTO account_groups (id, provider_id, name) VALUES (?, ?, ?)
                 ON CONFLICT (id) DO UPDATE SET name = excluded.name",
            )
            .bind(group.id.as_str())
            .bind(provider_id.as_str())
            .bind(group.name.as_str())
            .execute(&mut **transaction)
            .await
            .table("account_groups")?;
        }
        let updated = sqlx::query("UPDATE accounts SET group_id = ? WHERE id = ?")
            .bind(group.map(|group| group.id.as_str()))
            .bind(account_id.as_str())
            .execute(&mut **transaction)
            .await
            .table("accounts")?
            .rows_affected();
        rows::require_one(updated, "accounts")?;
        Self::delete_empty_groups_on(&mut **transaction).await
    }

    /// Deletes every group no account names.
    pub(crate) async fn delete_empty_groups_on<'e>(
        executor: impl sqlx::SqliteExecutor<'e>,
    ) -> PersistenceResult<()> {
        sqlx::query(
            "DELETE FROM account_groups
              WHERE id NOT IN (SELECT group_id FROM accounts WHERE group_id IS NOT NULL)",
        )
        .execute(executor)
        .await
        .table("account_groups")?;
        Ok(())
    }
}
