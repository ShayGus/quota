//! Wire shapes for the Claude OAuth usage and profile routes.
//!
//! Both routes are undocumented, so every field is optional and every
//! documented spelling is accepted as an alias. Unknown fields are ignored on
//! purpose: an added field must never change how the reading is interpreted, and
//! must never fail the parse. See `crates/quota-providers/README.md` for the
//! schema risk this carries.

use serde::Deserialize;

use crate::decode::Numberish;

/// The usage response.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct ClaudeUsage {
    /// The short rolling allowance.
    #[serde(default, alias = "fiveHour")]
    pub(crate) five_hour: Option<ClaudeWindow>,
    /// The weekly allowance.
    #[serde(default, alias = "sevenDay")]
    pub(crate) seven_day: Option<ClaudeWindow>,
    /// The weekly allowance for the Opus models, when the plan has one.
    #[serde(default, alias = "sevenDayOpus")]
    pub(crate) seven_day_opus: Option<ClaudeWindow>,
    /// A named-limits array. A usable array replaces the three fixed windows.
    #[serde(default)]
    pub(crate) limits: Vec<ClaudeLimit>,
    /// The paid extra-usage summary, when the plan has one.
    #[serde(default, alias = "extraUsage")]
    pub(crate) extra_usage: Option<ClaudeExtraUsage>,
}

impl ClaudeUsage {
    /// Whether the payload carried no window and no usable limit entry at all.
    pub(crate) fn is_empty(&self) -> bool {
        self.five_hour.is_none()
            && self.seven_day.is_none()
            && self.seven_day_opus.is_none()
            && !self
                .limits
                .iter()
                .any(|entry| entry.percent.is_some() || entry.group.is_some())
            && self.extra_usage.is_none()
    }
}

/// One fixed Claude window.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct ClaudeWindow {
    /// Percent points used, as a number or a numeric string.
    #[serde(default, alias = "usedPercent", alias = "used_percent")]
    pub(crate) utilization: Option<Numberish>,
    /// The reset instant, as epoch seconds or a date string.
    #[serde(default)]
    pub(crate) resets_at: Option<Numberish>,
    /// The reset instant under its shorter spelling.
    #[serde(default)]
    pub(crate) reset_at: Option<Numberish>,
}

impl ClaudeWindow {
    /// The reported reset instant, from either spelling.
    pub(crate) fn reset(&self) -> Option<&Numberish> {
        self.resets_at.as_ref().or(self.reset_at.as_ref())
    }
}

/// One entry of the named-limits array.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct ClaudeLimit {
    /// Percent points used.
    #[serde(default, alias = "usedPercent", alias = "utilization")]
    pub(crate) percent: Option<Numberish>,
    /// The group this limit belongs to, such as a weekly allowance.
    #[serde(default, alias = "type")]
    pub(crate) kind: Option<String>,
    /// The named group shown for this limit.
    #[serde(default)]
    pub(crate) group: Option<String>,
    /// The reset instant for this limit.
    #[serde(default)]
    pub(crate) resets_at: Option<Numberish>,
    /// The model this limit applies to, when it is model-specific.
    #[serde(default)]
    pub(crate) scope: Option<ClaudeLimitScope>,
}

/// The model scope of one named limit.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct ClaudeLimitScope {
    /// The provider's own model identifier.
    #[serde(default)]
    pub(crate) model: Option<ClaudeModel>,
}

/// A model named inside a limit scope.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct ClaudeModel {
    /// The model identifier.
    #[serde(default)]
    pub(crate) id: Option<String>,
    /// The label shown for the model.
    #[serde(default, alias = "displayName")]
    pub(crate) display_name: Option<String>,
}

/// The paid extra-usage summary.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct ClaudeExtraUsage {
    /// Whether the account has extra usage enabled.
    #[serde(default, alias = "isEnabled")]
    pub(crate) is_enabled: Option<bool>,
    /// Percent points of the cap used.
    #[serde(default)]
    pub(crate) utilization: Option<Numberish>,
    /// The monthly cap, in minor units of the reported currency.
    #[serde(default, alias = "monthlyLimit")]
    pub(crate) monthly_limit: Option<Numberish>,
    /// The amount consumed, in minor units of the reported currency.
    #[serde(default, alias = "usedCredits")]
    pub(crate) used_credits: Option<Numberish>,
    /// The number of decimal places the amounts use. Defaults to two.
    #[serde(default, alias = "decimalPlaces")]
    pub(crate) decimal_places: Option<Numberish>,
    /// The currency code the amounts are in.
    #[serde(default)]
    pub(crate) currency: Option<String>,
}

/// The identity response.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct ClaudeProfile {
    /// The signed-in account.
    #[serde(default)]
    pub(crate) account: Option<ClaudeAccount>,
    /// The organization the account belongs to, when the route reports one.
    #[serde(default)]
    pub(crate) organization: Option<ClaudeOrganization>,
}

/// The signed-in account inside the profile response.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct ClaudeAccount {
    /// The account identity. Without it the reading is unverified.
    #[serde(default)]
    pub(crate) uuid: Option<String>,
    /// The address the account signed in with.
    #[serde(default)]
    pub(crate) email: Option<String>,
}

/// The organization inside the profile response.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct ClaudeOrganization {
    /// The organization's label.
    #[serde(default)]
    pub(crate) name: Option<String>,
}
