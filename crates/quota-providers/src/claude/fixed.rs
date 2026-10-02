//! The fixed Claude usage fields, and the windows they describe.
//!
//! Claude reports its account-wide allowances, its model-specific weekly
//! allowances, and its product allowances in fields of the same shape. They are
//! listed here once so the mapping does not have to know the field order, and
//! so a newly documented product allowance is one line rather than a new branch.

use chrono::{DateTime, Utc};
use quota_domain::ids::QuotaPoolId;
use quota_domain::provider::ProviderId;
use quota_domain::quota::window::{MetricRole, QuotaCategory, WindowSemantics};

use crate::claude::wire::{ClaudeUsage, ClaudeWindow};
use crate::decode::WindowDraft;

/// The duration of the short rolling window, in seconds.
pub(super) const SESSION_SECONDS: i64 = 18_000;

/// The duration of the weekly window, in seconds.
pub(super) const WEEKLY_SECONDS: i64 = 604_800;

/// One window the payload may report in a fixed field of its own.
pub(super) struct FixedWindow<'a> {
    /// The identity the window keeps across reads.
    pub(super) bucket: &'static str,
    /// The metered resource the allowance covers.
    pub(super) resource: &'static str,
    /// The label shown for that resource.
    pub(super) label: &'static str,
    /// The period the field names.
    pub(super) category: QuotaCategory,
    /// Whether an account on this plan is expected to report the window.
    ///
    /// Only the two account-wide allowances are expected. A model-specific or
    /// product allowance the payload does not mention is simply absent, not a
    /// window this plan failed to report.
    pub(super) expected: bool,
    /// The reported window, when the payload carried one.
    pub(super) wire: Option<&'a ClaudeWindow>,
}

/// The fixed fields the payload may carry, in a stable order.
pub(super) fn fixed_windows(usage: &ClaudeUsage) -> Vec<FixedWindow<'_>> {
    let [
        five_hour,
        weekly,
        opus,
        sonnet,
        oauth_apps,
        design,
        routines,
    ] = usage.fixed();
    vec![
        account("five-hour", QuotaCategory::Session, true, five_hour),
        account("weekly", QuotaCategory::Weekly, true, weekly),
        product("weekly-opus", "opus", "Claude Opus", opus),
        product("weekly-sonnet", "sonnet", "Claude Sonnet", sonnet),
        product(
            "weekly-oauth-apps",
            "oauth-apps",
            "Claude OAuth apps",
            oauth_apps,
        ),
        product("weekly-design", "design", "Claude Design", design),
        product("weekly-routines", "routines", "Claude Routines", routines),
    ]
}

/// An account-wide allowance.
fn account<'a>(
    bucket: &'static str,
    category: QuotaCategory,
    expected: bool,
    wire: Option<&'a ClaudeWindow>,
) -> FixedWindow<'a> {
    FixedWindow {
        bucket,
        resource: quota_domain::quota::scope::ACCOUNT_RESOURCE,
        label: "Claude account",
        category,
        expected,
        wire,
    }
}

/// A model-specific or product allowance.
fn product<'a>(
    bucket: &'static str,
    resource: &'static str,
    label: &'static str,
    wire: Option<&'a ClaudeWindow>,
) -> FixedWindow<'a> {
    FixedWindow {
        bucket,
        resource,
        label,
        category: QuotaCategory::Weekly,
        expected: false,
        wire,
    }
}

impl FixedWindow<'_> {
    /// The draft for this window, whether the payload reported it or not.
    pub(super) fn draft<'a>(
        &self,
        pool: &'a QuotaPoolId,
        received_at: DateTime<Utc>,
    ) -> WindowDraft<'a> {
        WindowDraft {
            provider: ProviderId::Claude,
            pool_id: pool,
            category: self.category,
            resource: self.resource,
            resource_label: self.label,
            bucket_id: Some(self.bucket),
            metric_role: MetricRole::IncludedAllowance,
            semantics: WindowSemantics::RollingPeriod,
            duration_seconds: match self.category {
                QuotaCategory::Session => Some(SESSION_SECONDS),
                _ => Some(WEEKLY_SECONDS),
            },
            received_at,
        }
    }
}
