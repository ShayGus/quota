//! Wire shapes for Cursor's usage summary.
//!
//! `GET https://cursor.com/api/usage-summary` answers the billing cycle, the
//! membership, and `individualUsage`: the plan's share by model kind, an
//! enterprise total, and on-demand spend. Amounts are in US cents. The
//! endpoint is undocumented, so every field is optional and unknown fields are
//! ignored.

use serde::Deserialize;

use crate::decode::Numberish;

/// The response body.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UsageSummary {
    /// When the billing cycle ends.
    #[serde(default)]
    pub(crate) billing_cycle_end: Option<Numberish>,
    /// The plan, such as `pro`.
    #[serde(default)]
    pub(crate) membership_type: Option<String>,
    /// The person's own usage.
    #[serde(default)]
    pub(crate) individual_usage: Option<IndividualUsage>,
}

/// The person's own usage.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct IndividualUsage {
    /// The plan's included usage.
    #[serde(default)]
    pub(crate) plan: Option<Bucket>,
    /// An enterprise seat's total.
    #[serde(default)]
    pub(crate) overall: Option<Bucket>,
    /// Spend beyond the plan.
    #[serde(default)]
    pub(crate) on_demand: Option<Bucket>,
}

/// One allowance, in US cents, or as percentages.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Bucket {
    /// `false` when the allowance does not apply.
    #[serde(default)]
    pub(crate) enabled: Option<bool>,
    /// Cents used.
    #[serde(default)]
    pub(crate) used: Option<Numberish>,
    /// The allowance in cents, or `null` for none.
    #[serde(default)]
    pub(crate) limit: Option<Numberish>,
    /// Cents left.
    #[serde(default)]
    pub(crate) remaining: Option<Numberish>,
    /// Percent used by Cursor's own models.
    #[serde(default)]
    pub(crate) auto_percent_used: Option<Numberish>,
    /// Percent used by other models.
    #[serde(default)]
    pub(crate) api_percent_used: Option<Numberish>,
    /// Percent used in all.
    #[serde(default)]
    pub(crate) total_percent_used: Option<Numberish>,
}

/// The signed-in account.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct Me {
    /// Its address.
    #[serde(default)]
    pub(crate) email: Option<String>,
}
