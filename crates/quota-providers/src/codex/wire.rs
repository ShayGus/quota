//! Wire shapes for the Codex usage endpoints.
//!
//! These endpoints are undocumented, so every field here is optional and every
//! documented spelling is accepted as an alias. Unknown fields are ignored on
//! purpose: an added field must never change how the reading is interpreted, and
//! must never fail the parse. See `crates/quota-providers/README.md` for the
//! schema risk this carries.
//!
//! A body may wrap its limits under `rate_limit`, flatten them onto the root,
//! or do both. Both are read, because a body that carries a review allowance at
//! the root is still reporting one.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::decode::Numberish;

/// The two windows one Codex block reports, whichever way it reported them.
pub(crate) type WindowPair<'a> = (Option<&'a CodexWindow>, Option<&'a CodexWindow>);

/// The response body, whichever shape the endpoint chose.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct CodexEnvelope {
    /// The named rate-limit block, when the body wraps one.
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
    /// Whatever the body reports at its own root.
    #[serde(default, flatten)]
    pub(crate) root: CodexLimitSet,
}

impl CodexEnvelope {
    /// Every rate-limit block this body carried, the named one first.
    ///
    /// The named container wins when both describe the same pair, but a window
    /// reported only at the root is never dropped.
    pub(crate) fn limit_sets(&self) -> Vec<&CodexLimitSet> {
        let root = if self.root.is_empty() {
            None
        } else {
            Some(&self.root)
        };
        [self.rate_limit.as_ref(), root]
            .into_iter()
            .flatten()
            .collect()
    }
}

/// One group of Codex rate-limit windows.
///
/// The same type is used for the whole block, for the nested review block, and
/// for the block inside one named additional limit: the provider spells all
/// three the same way, either as a pair or as a bare window on the block.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct CodexLimitSet {
    /// The first window of the pair.
    #[serde(default, alias = "primary")]
    pub(crate) primary_window: Option<CodexWindow>,
    /// The second window of the pair, when the source reports one.
    #[serde(default, alias = "secondary")]
    pub(crate) secondary_window: Option<CodexWindow>,
    /// The code-review allowance, when the source reports one.
    #[serde(default, alias = "codeReviewRateLimit")]
    pub(crate) code_review_rate_limit: Option<Box<CodexLimitSet>>,
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
    /// A window carried directly on the block instead of inside a pair.
    #[serde(default, flatten)]
    pub(crate) inline: CodexWindow,
}

impl CodexLimitSet {
    /// Whether the block names no window and no bucket at all.
    pub(crate) fn is_empty(&self) -> bool {
        self.pair().0.is_none()
            && self.secondary_window.is_none()
            && self.code_review_rate_limit.is_none()
            && self.additional_rate_limits.is_empty()
            && self.rate_limits_by_limit_id.is_empty()
    }

    /// The pair this block reports, however the source spelled it.
    ///
    /// A window carried inline rather than under `primary_window` is still a
    /// first window, not an absent one.
    pub(crate) fn pair(&self) -> (Option<&CodexWindow>, Option<&CodexWindow>) {
        let first = self
            .primary_window
            .as_ref()
            .or_else(|| (!self.inline.is_empty()).then_some(&self.inline));
        (first, self.secondary_window.as_ref())
    }

    /// Every additional bucket, as `(identifier, display name, window pair)`.
    ///
    /// A named entry may describe one window or a pair, and both keep the
    /// bucket identity the provider gave the entry.
    pub(crate) fn buckets(&self) -> Vec<(String, String, WindowPair<'_>)> {
        let mut buckets = Vec::new();
        for limit in &self.additional_rate_limits {
            let pair = limit.rate_limit.as_ref().map(CodexLimitSet::pair);
            if pair.is_none_or(|(first, second)| first.is_none() && second.is_none()) {
                continue;
            }
            let pair = pair.unwrap_or_default();
            buckets.push((
                crate::decode::identifier(limit.identifier()),
                limit.label().to_owned(),
                pair,
            ));
        }
        for (key, window) in &self.rate_limits_by_limit_id {
            buckets.push((
                crate::decode::identifier(key),
                key.clone(),
                (Some(window), None),
            ));
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
    #[serde(default, alias = "limitName", alias = "limit_name")]
    pub(crate) name: Option<String>,
    /// The label the provider shows for the bucket.
    #[serde(default, alias = "displayName")]
    pub(crate) display_name: Option<String>,
    /// The windows nested under `rate_limit`, however they are shaped.
    #[serde(default, alias = "limit")]
    pub(crate) rate_limit: Option<CodexLimitSet>,
}

impl CodexAdditionalLimit {
    /// The bucket identifier this entry keeps.
    pub(crate) fn identifier(&self) -> &str {
        self.id
            .as_deref()
            .or(self.name.as_deref())
            .or(self.display_name.as_deref())
            .unwrap_or("additional")
    }

    /// The label shown for this entry.
    pub(crate) fn label(&self) -> &str {
        self.display_name
            .as_deref()
            .or(self.name.as_deref())
            .or(self.id.as_deref())
            .unwrap_or("Additional limit")
    }
}

/// One Codex window, with every documented spelling of every field.
#[derive(Debug, Clone, Default, Deserialize)]
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
    /// Whether this window carries no reported field at all.
    pub(crate) fn is_empty(&self) -> bool {
        self.used_percent.is_none()
            && self.reset_at.is_none()
            && self.reset_after_seconds.is_none()
            && self.limit_window_seconds.is_none()
            && self.window_duration_mins.is_none()
    }

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
