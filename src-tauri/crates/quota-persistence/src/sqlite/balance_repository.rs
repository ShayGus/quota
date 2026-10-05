//! Prepaid balances and the key-limit switch: per-account state that leaves
//! with its account.
//!
//! The ledger row carries a foreign key with `ON DELETE CASCADE`, so deleting
//! an account deletes its ledger, and every statement here addresses one
//! account by its primary key. These are `impl` blocks on the same
//! [`AccountRepository`] as [`crate::sqlite::account_repository`].

use quota_domain::balance::BalanceLedger;
use quota_domain::ids::AccountId;

use crate::error::{PersistenceError, PersistenceResult, TableContext};
use crate::sqlite::account_repository::AccountRepository;
use crate::sqlite::rows;

impl AccountRepository {
    /// Shows or hides one account's API key spend limit.
    ///
    /// # Errors
    /// Returns [`PersistenceError::RowRejected`] when no such account exists.
    pub async fn set_show_key_limit(
        &self,
        account_id: &AccountId,
        shown: bool,
    ) -> PersistenceResult<()> {
        Self::set_show_key_limit_on(&self.pool, account_id, shown).await
    }

    /// [`Self::set_show_key_limit`] on a given executor, so a caller can make it
    /// part of a larger transaction.
    pub(crate) async fn set_show_key_limit_on<'e>(
        executor: impl sqlx::SqliteExecutor<'e>,
        account_id: &AccountId,
        shown: bool,
    ) -> PersistenceResult<()> {
        let updated = sqlx::query("UPDATE accounts SET show_key_limit = ? WHERE id = ?")
            .bind(i64::from(shown))
            .bind(account_id.as_str())
            .execute(executor)
            .await
            .table("accounts")?
            .rows_affected();
        rows::require_one(updated, "accounts")
    }

    /// The prepaid-balance ledger of one account, when it has one.
    ///
    /// # Errors
    /// Returns [`PersistenceError::RowRejected`] when the stored ledger is not
    /// one this build can read.
    pub async fn balance_ledger(
        &self,
        account_id: &AccountId,
    ) -> PersistenceResult<Option<BalanceLedger>> {
        let stored: Option<String> =
            sqlx::query_scalar("SELECT ledger_json FROM account_balances WHERE account_id = ?")
                .bind(account_id.as_str())
                .fetch_optional(&self.pool)
                .await
                .table("account_balances")?;
        stored
            .map(|json| {
                serde_json::from_str(&json).map_err(|_| PersistenceError::RowRejected {
                    table: "account_balances",
                    reason: "the stored balance ledger could not be read",
                })
            })
            .transpose()
    }

    /// Writes one account's ledger, or removes it when the account has none.
    pub(crate) async fn store_balance_on<'e>(
        executor: impl sqlx::SqliteExecutor<'e>,
        account_id: &AccountId,
        ledger: Option<&BalanceLedger>,
    ) -> PersistenceResult<()> {
        let Some(ledger) = ledger else {
            sqlx::query("DELETE FROM account_balances WHERE account_id = ?")
                .bind(account_id.as_str())
                .execute(executor)
                .await
                .table("account_balances")?;
            return Ok(());
        };
        let json = serde_json::to_string(ledger).map_err(|_| PersistenceError::RowRejected {
            table: "account_balances",
            reason: "the balance ledger could not be encoded",
        })?;
        sqlx::query(
            "INSERT INTO account_balances (account_id, ledger_json) VALUES (?, ?)
             ON CONFLICT (account_id) DO UPDATE SET ledger_json = excluded.ledger_json",
        )
        .bind(account_id.as_str())
        .bind(json)
        .execute(executor)
        .await
        .table("account_balances")?;
        Ok(())
    }
}
