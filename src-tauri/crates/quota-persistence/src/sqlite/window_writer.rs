//! The window definition a reading references.
//!
//! A window is written before the reading that uses it, inside the same
//! transaction, so a reading can never reference a window definition that was
//! never stored.

use quota_domain::ids::AccountId;
use quota_domain::quota::window::QuotaWindow;

use crate::error::{PersistenceResult, TableContext};
use crate::sqlite::codec;

/// Writes the window definition the measurements reference.
///
/// The pool row the window references is created from the owning account's
/// provider when it does not exist yet. The pool's provider is therefore never
/// invented: it is read from the account, and an unknown account creates no
/// pool row, so the window insert fails its foreign key and the transaction
/// rolls the whole reading back.
pub(crate) async fn upsert_window(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    account_id: &AccountId,
    window: &QuotaWindow,
) -> PersistenceResult<()> {
    sqlx::query(
        "INSERT OR IGNORE INTO quota_pools (id, provider_id, shared)
         SELECT ?, provider_id, 0 FROM accounts WHERE id = ?",
    )
    .bind(window.pool_id.as_str())
    .bind(account_id.as_str())
    .execute(&mut **transaction)
    .await
    .table("quota_pools")?;

    sqlx::query(
        "INSERT INTO quota_windows (
             id, pool_id, provider_bucket_id, scope_resource, scope_label, category,
             semantics, duration_seconds, metric_role, enforcement, source_kind,
             completeness, definition_version
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         ON CONFLICT (id) DO UPDATE SET
             provider_bucket_id = excluded.provider_bucket_id,
             scope_resource = excluded.scope_resource,
             scope_label = excluded.scope_label,
             category = excluded.category,
             semantics = excluded.semantics,
             duration_seconds = excluded.duration_seconds,
             metric_role = excluded.metric_role,
             enforcement = excluded.enforcement,
             source_kind = excluded.source_kind,
             completeness = excluded.completeness,
             definition_version = excluded.definition_version",
    )
    .bind(window.id.as_str())
    .bind(window.pool_id.as_str())
    .bind(window.provider_bucket_id.as_deref())
    .bind(window.scope.resource().as_str())
    .bind(window.scope.label())
    .bind(codec::encode(&window.category, "quota_windows")?)
    .bind(codec::encode(&window.semantics, "quota_windows")?)
    .bind(
        window
            .duration
            .map(|duration| duration.num_seconds())
            .filter(|seconds| *seconds >= 0),
    )
    .bind(codec::encode(&window.metric_role, "quota_windows")?)
    .bind(codec::encode(&window.enforcement, "quota_windows")?)
    .bind(codec::encode(&window.source, "quota_windows")?)
    .bind(codec::encode(&window.completeness, "quota_windows")?)
    .bind(i64::from(window.definition_version.0))
    .execute(&mut **transaction)
    .await
    .table("quota_windows")?;
    Ok(())
}
