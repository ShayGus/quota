//! Wire shapes for the Codex usage endpoints.
//!
//! These endpoints are undocumented, so every field here is optional and every
//! documented spelling is accepted as an alias. Unknown fields are ignored on
//! purpose: an added field must never change how the reading is interpreted, and
//! must never fail the parse. See `crates/quota-providers/README.md` for the
//! schema risk this carries.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::decode::Numberish;

/// The response body, whichever shape the endpoint chose.
///
/// The rate-limit payload sits under `rate_limit`/`rateLimits`/`rate_limits`, or
/// is the root object itself. Both are accepted, and the named container wins.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct CodexEnvelope {
    /// The rate-limit block, when the body wraps it.
    #[serde(default, alias = "rateLimits", alias = "rate_limits")]
    pub(crate) rate_limit: Option<CodexLimitSet>,
    /// The prepaid credit summary, when the body carries one.
    #[serde(default)]
    pub(crate) credits: Option<CodexCredits>,
    /// The plan label, when the body carries one.
    #[serde(default, alias = "planType")]
    pub(crate) plan_type: Option<String>,
    /// The signed-in address, when the body repeats it.
    #[serde(default)]
    pub(crate) email: Option<String>,
    /// The account identity, when the body repeats it.
    #[serde(default, alias = "accountId")]
    pub(crate) account_id: Option<String>,
    /// The root object acting as the rate-limit block itself.
    #[serde(default, flatten)]
    pub(crate) root: CodexLimitSet,
}

/// One group of Codex rate-limit windows.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct CodexLimitSet {
    /// The first window of the pair, or the whole single-window view.
    #[serde(default, alias = "primary")]
    pub(crate) primary_window: Option<CodexWindow>,
    /// The second window of the pair, when the source reports one.
    #[serde(default, alias = "secondary")]
    pub(crate) secondary_window: Option<CodexWindow>,
    /// The code-review allowance, when the source reports one.
    #[serde(default, alias = "codeReviewRateLimit")]
    pub(crate) code_review_rate_limit: Option<CodexWindow>,
    /// Additional named buckets, such as a model-specific allowance.
    #[serde(default, alias = "additionalRateLimits")]
    pub(crate) additional_rate_limits: Vec<CodexAdditionalLimit>,
    /// The multi-bucket view, keyed by the provider's own limit identifier.
    #[serde(
        default,
        rename = "rateLimitsByLimitId",
        alias = "rateLimitsByLimitID",
        alias = "rate_limits_by_limit_id"
    )]
    pub(crate) rate_limits_by_limit_id: BTreeMap<String, CodexWindow>,
    /// The plan label, when the block carries one.
    #[serde(default, alias = "planType")]
    pub(crate) plan_type: Option<String>,
}

impl CodexLimitSet {
    /// Whether the block names no window and no bucket at all.
    pub(crate) fn is_empty(&self) -> bool {
        self.primary_window.is_none()
            && self.secondary_window.is_none()
            && self.code_review_rate_limit.is_none()
            && self.additional_rate_limits.is_empty()
            && self.rate_limits_by_limit_id.is_empty()
    }

    /// Every named bucket, as `(bucket identifier, display name, window)`.
    pub(crate) fn buckets(&self) -> Vec<(String, String, &CodexWindow)> {
        let mut buckets: Vec<(String, String, &CodexWindow)> = Vec::new();
        for limit in &self.additional_rate_limits {
            let id = limit
                .id
                .as_deref()
                .or(limit.name.as_deref())
                .or(limit.display_name.as_deref())
                .unwrap_or("additional");
            let label = limit
                .display_name
                .as_deref()
                .or(limit.name.as_deref())
                .or(limit.id.as_deref())
                .unwrap_or("Additional limit");
            if let Some(window) = limit.window() {
                buckets.push((id.to_owned(), label.to_owned(), window));
            }
        }
        for (key, window) in &self.rate_limits_by_limit_id {
            buckets.push((key.clone(), key.clone(), window));
        }
        buckets
    }
}

/// One additional named limit inside the Codex payload.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct CodexAdditionalLimit {
    /// The provider's own identifier for the bucket.
    #[serde(default)]
    pub(crate) id: Option<String>,
    /// A short name for the bucket.
    #[serde(default)]
    pub(crate) name: Option<String>,
    /// The label the provider shows for the bucket.
    #[serde(default, alias = "displayName")]
    pub(crate) display_name: Option<String>,
    /// The window wrapped under `rate_limit`, when the entry nests it.
    #[serde(default, alias = "limit")]
    pub(crate) rate_limit: Option<CodexWindow>,
    /// The window carried directly on the entry.
    #[serde(default)]
    pub(crate) window: Option<CodexWindow>,
}

impl CodexAdditionalLimit {
    /// The window this entry describes, however it is nested.
    pub(crate) fn window(&self) -> Option<&CodexWindow> {
        self.rate_limit.as_ref().or(self.window.as_ref())
    }
}

/// One Codex window, with every documented spelling of every field.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct CodexWindow {
    /// Percent points used, as a number or a numeric string.
    #[serde(default, alias = "usedPercent", alias = "used_percentage")]
    pub(crate) used_percent: Option<Numberish>,
    /// The reset instant, as epoch seconds or a date string.
    #[serde(default, alias = "resetsAt", alias = "resetAt")]
    pub(crate) reset_at: Option<Numberish>,
    /// The reset delay in seconds.
    #[serde(default, alias = "resetAfterSeconds", alias = "resets_in_seconds")]
    pub(crate) reset_after_seconds: Option<Numberish>,
    /// The window duration in seconds.
    #[serde(default, alias = "limitWindowSeconds")]
    pub(crate) limit_window_seconds: Option<Numberish>,
    /// The window duration in minutes.
    #[serde(
        default,
        alias = "windowDurationMins",
        alias = "window_duration_minutes"
    )]
    pub(crate) window_duration_mins: Option<Numberish>,
}

impl CodexWindow {
    /// The window duration in seconds, from either documented spelling.
    pub(crate) fn duration_seconds(&self) -> Option<i64> {
        if let Some(seconds) = self.limit_window_seconds.as_ref() {
            return crate::decode::scaled_seconds(seconds, 1);
        }
        let minutes = self.window_duration_mins.as_ref()?;
        crate::decode::scaled_seconds(minutes, 60)
    }
}

/// The prepaid credit summary some payloads carry.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct CodexCredits {
    /// The remaining credit balance, as a number or a numeric string.
    #[serde(default, alias = "creditBalance")]
    pub(crate) balance: Option<Numberish>,
    /// Whether the credit balance has no ceiling.
    #[serde(default, alias = "isUnlimited", alias = "has_unlimited_credits")]
    pub(crate) unlimited: Option<bool>,
}
