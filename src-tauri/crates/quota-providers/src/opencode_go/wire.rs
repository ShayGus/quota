//! Wire shapes for the `OpenCode` Zen Go usage endpoint.
//!
//! The endpoint is undocumented, so every field is optional and every documented
//! spelling is accepted as an alias. Unknown fields are ignored on purpose: an
//! added field must never change how the reading is interpreted, and must never
//! fail the parse. See `crates/quota-providers/README.md` for the schema risk
//! this carries.

use serde::Deserialize;

use crate::decode::Numberish;

/// The response body, whichever shape the endpoint chose.
///
/// The usage payload sits under `usage`, or is the root object itself. Both are
/// accepted, and the named container wins.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct OpenCodeGoEnvelope {
    /// The usage block, when the body wraps it.
    #[serde(default)]
    pub(crate) usage: Option<OpenCodeGoUsage>,
    /// The root object acting as the usage block itself.
    #[serde(default, flatten)]
    pub(crate) root: OpenCodeGoUsage,
}

impl OpenCodeGoEnvelope {
    /// The usage block, preferring the named container.
    pub(crate) fn usage(&self) -> &OpenCodeGoUsage {
        self.usage.as_ref().unwrap_or(&self.root)
    }
}

/// The three windows this provider reports.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct OpenCodeGoUsage {
    /// The rolling allowance.
    #[serde(
        default,
        rename = "rollingUsage",
        alias = "rolling",
        alias = "rolling_usage"
    )]
    pub(crate) rolling: Option<OpenCodeGoBucket>,
    /// The weekly allowance.
    #[serde(
        default,
        rename = "weeklyUsage",
        alias = "weekly",
        alias = "weekly_usage"
    )]
    pub(crate) weekly: Option<OpenCodeGoBucket>,
    /// The monthly allowance. This is the only connector in scope that has one.
    #[serde(
        default,
        rename = "monthlyUsage",
        alias = "monthly",
        alias = "monthly_usage"
    )]
    pub(crate) monthly: Option<OpenCodeGoBucket>,
}

impl OpenCodeGoUsage {
    /// Whether the block named no window at all.
    pub(crate) fn is_empty(&self) -> bool {
        self.rolling.is_none() && self.weekly.is_none() && self.monthly.is_none()
    }
}

/// One window, with every documented spelling of every field.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct OpenCodeGoBucket {
    /// Percent points used.
    #[serde(
        default,
        alias = "percentUsed",
        alias = "usedPercent",
        alias = "usagePercent"
    )]
    pub(crate) percent: Option<Numberish>,
    /// Percent points still available, when that is the reported polarity.
    #[serde(default, rename = "percentRemaining", alias = "remainingPercent")]
    pub(crate) percent_remaining: Option<Numberish>,
    /// The reset instant, as a date string or epoch seconds.
    #[serde(
        default,
        rename = "resetsAt",
        alias = "resetAt",
        alias = "reset_at",
        alias = "nextResetTime"
    )]
    pub(crate) resets_at: Option<Numberish>,
    /// The reset delay in seconds.
    #[serde(default, alias = "resetInSec")]
    pub(crate) reset_in_sec: Option<Numberish>,
}
