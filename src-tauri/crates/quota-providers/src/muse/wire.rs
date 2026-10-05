//! Wire shapes for Muse Code's subscription endpoint.
//!
//! `POST https://api.meta.ai/muse-code/key` answers the subscription, its usage
//! and the account. The same answer carries a model API key; this shape has no
//! field for it, so the key is never read into Quota, kept, or logged. The
//! endpoint is undocumented, so every field is optional and unknown fields are
//! ignored.

use serde::Deserialize;

use crate::decode::Numberish;

/// The response body, without the key it also carries.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct SubscriptionAnswer {
    /// Whether the subscription is active.
    #[serde(default)]
    pub(crate) is_subs_active: Option<bool>,
    /// The plan's name.
    #[serde(default)]
    pub(crate) subs_tier_name: Option<String>,
    /// The account's address.
    #[serde(default)]
    pub(crate) user_email: Option<String>,
    /// The account's identifier.
    #[serde(default)]
    pub(crate) user_id: Option<String>,
    /// The usage windows. Missing while the rolling window is idle.
    #[serde(default)]
    pub(crate) subs_usage: Option<SubscriptionUsage>,
}

impl SubscriptionAnswer {
    /// The account's stable identity: Meta's user id, otherwise the account's
    /// address, as oh-my-pi does. Meta leaves the id out for some accounts.
    pub(crate) fn account_id(&self) -> Option<String> {
        let present = |value: &Option<String>| {
            value
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        };
        present(&self.user_id)
            .or_else(|| present(&self.user_email).map(|address| address.to_lowercase()))
    }
}

/// The two windows.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct SubscriptionUsage {
    /// The rolling window, five hours on today's plans.
    #[serde(default)]
    pub(crate) window: Option<UsageWindow>,
    /// The weekly window.
    #[serde(default)]
    pub(crate) weekly: Option<UsageWindow>,
}

/// One window.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct UsageWindow {
    /// Percent used.
    #[serde(default)]
    pub(crate) used_percent: Option<Numberish>,
    /// When it resets: a date string or epoch seconds.
    #[serde(default)]
    pub(crate) resets_at: Option<Numberish>,
    /// Its length, in minutes.
    #[serde(default)]
    pub(crate) window_duration_mins: Option<Numberish>,
}
