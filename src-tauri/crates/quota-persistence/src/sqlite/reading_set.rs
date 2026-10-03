//! Replacing an account's whole current set of readings.
//!
//! A provider may stop reporting a window. When the whole set arrives, the
//! windows it no longer names must leave the account's current readings, or a
//! restart would restore an allowance that no longer exists. The surplus is
//! swept here, beside the write that replaces it, so no caller can forget.

use quota_domain::ids::AccountId;
use quota_domain::quota::window::QuotaWindow;

use crate::error::{PersistenceResult, TableContext};
use crate::sqlite::MeasurementRepository;
use crate::sqlite::measurement_repository::persist_window;

/// Removes this account's current readings for windows the provider stopped naming.
///
/// The incoming windows are kept, and the shared window definitions are left
/// alone, because a pool can be reported by more than one account. History rows
/// are never touched: a reading that was once true stays in the history.
///
/// # Errors
/// Returns a typed persistence error when the delete fails.
pub(crate) async fn sweep_absent_windows(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    account_id: &AccountId,
    reported: &[QuotaWindow],
) -> PersistenceResult<()> {
    if reported.is_empty() {
        sqlx::query("DELETE FROM latest_measurements WHERE account_id = ?")
            .bind(account_id.as_str())
            .execute(&mut **transaction)
            .await
            .table("latest_measurements")?;
        return Ok(());
    }

    let placeholders = vec!["?"; reported.len()].join(", ");
    let statement = format!(
        "DELETE FROM latest_measurements WHERE account_id = ? AND window_id NOT IN ({placeholders})"
    );
    let mut query = sqlx::query(&statement).bind(account_id.as_str());
    for window in reported {
        query = query.bind(window.id.as_str());
    }
    query
        .execute(&mut **transaction)
        .await
        .table("latest_measurements")?;
    Ok(())
}

impl MeasurementRepository {
    /// Writes the account's whole current window set, replacing what came before.
    ///
    /// The incoming windows are written first and the surplus swept second, in
    /// one transaction, so a window the provider stopped reporting cannot return
    /// after a restart. Shared definitions and history rows are left alone.
    ///
    /// # Errors
    /// Returns a typed persistence error when the read fails.
    pub async fn replace_readings(
        &self,
        account_id: &AccountId,
        windows: &[QuotaWindow],
    ) -> PersistenceResult<usize> {
        let mut transaction = self.pool.begin().await.table("latest_measurements")?;
        let history_rows = Self::replace_readings_in(&mut transaction, account_id, windows).await?;
        transaction.commit().await.table("latest_measurements")?;
        Ok(history_rows)
    }

    /// [`Self::replace_readings`] inside a caller's transaction, which the
    /// caller commits.
    pub(crate) async fn replace_readings_in(
        transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        account_id: &AccountId,
        windows: &[QuotaWindow],
    ) -> PersistenceResult<usize> {
        let mut history_rows = 0;
        for window in windows {
            history_rows += usize::from(persist_window(transaction, account_id, window).await?);
        }
        sweep_absent_windows(transaction, account_id, windows).await?;
        Ok(history_rows)
    }
}
