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
    /// The weekly allowance for the Sonnet models, when the plan has one.
    #[serde(default, alias = "sevenDaySonnet")]
    pub(crate) seven_day_sonnet: Option<ClaudeWindow>,
    /// The weekly allowance for the OAuth apps a build can create.
    #[serde(
        default,
        alias = "sevenDayOauthApps",
        alias = "seven_day_claude_oauth_apps"
    )]
    pub(crate) seven_day_oauth_apps: Option<ClaudeWindow>,
    /// The weekly allowance for Claude Design.
    #[serde(
        default,
        alias = "sevenDayDesign",
        alias = "seven_day_claude_design",
        alias = "seven_day_omelette"
    )]
    pub(crate) seven_day_design: Option<ClaudeWindow>,
    /// The weekly allowance for Routines.
    #[serde(
        default,
        alias = "sevenDayRoutines",
        alias = "seven_day_claude_routines",
        alias = "seven_day_cowork"
    )]
    pub(crate) seven_day_routines: Option<ClaudeWindow>,
    /// A named-limits array, merged with the fixed windows above.
    #[serde(default, deserialize_with = "crate::decode::null_as_default")]
    pub(crate) limits: Vec<ClaudeLimit>,
    /// The paid extra-usage summary, when the plan has one.
    #[serde(default, alias = "extraUsage")]
    pub(crate) extra_usage: Option<ClaudeExtraUsage>,
}

impl ClaudeUsage {
    /// Every fixed field, in the order the reading reports them.
    pub(crate) fn fixed(&self) -> [Option<&ClaudeWindow>; 7] {
        [
            self.five_hour.as_ref(),
            self.seven_day.as_ref(),
            self.seven_day_opus.as_ref(),
            self.seven_day_sonnet.as_ref(),
            self.seven_day_oauth_apps.as_ref(),
            self.seven_day_design.as_ref(),
            self.seven_day_routines.as_ref(),
        ]
    }

    /// Whether the payload carried no window and no usable limit entry at all.
    pub(crate) fn is_empty(&self) -> bool {
        self.fixed().iter().all(Option::is_none)
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
    #[serde(default, alias = "usedPercent", alias = "utilization")]
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
    /// The reset instant for this limit, under either documented spelling.
    #[serde(default, alias = "resetsAt", alias = "reset_at")]
    pub(crate) resets_at: Option<Numberish>,
    /// The model this limit applies to, when it is model-specific.
    #[serde(default)]
    pub(crate) scope: Option<ClaudeLimitScope>,
}

impl ClaudeLimit {
    /// The reported reset instant for this limit.
    pub(crate) fn reset(&self) -> Option<&Numberish> {
        self.resets_at.as_ref()
    }
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
