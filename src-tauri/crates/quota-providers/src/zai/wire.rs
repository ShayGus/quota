//! Wire shapes for Z.ai's coding-plan quota endpoint.
//!
//! `GET /api/monitor/usage/quota/limit` answers `{ success, code, msg, data }`.
//! `data.limits[]` lists one entry per meter and window, and `data.level`
//! names the plan. The endpoint is undocumented, so every field is optional
//! and unknown fields are ignored.

use serde::Deserialize;

use crate::decode::Numberish;

/// The response body.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct QuotaEnvelope {
    /// Whether the request was accepted.
    #[serde(default)]
    pub(crate) success: Option<bool>,
    /// The quota itself.
    #[serde(default)]
    pub(crate) data: Option<QuotaData>,
}

/// The plan and its meters.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct QuotaData {
    /// One entry per meter and window.
    #[serde(default, deserialize_with = "crate::decode::null_as_default")]
    pub(crate) limits: Vec<QuotaLimit>,
    /// The plan tier, such as `lite`, `pro` or `max`.
    #[serde(default)]
    pub(crate) level: Option<String>,
}

/// One meter over one window.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct QuotaLimit {
    /// `TOKENS_LIMIT`, `CREDIT_LIMIT`, or `TIME_LIMIT` (web tool requests).
    #[serde(default, rename = "type")]
    pub(crate) kind: Option<String>,
    /// The allowance for the window.
    #[serde(default)]
    pub(crate) usage: Option<Numberish>,
    /// What has been used of it.
    #[serde(default, rename = "currentValue")]
    pub(crate) current_value: Option<Numberish>,
    /// What is left of it.
    #[serde(default)]
    pub(crate) remaining: Option<Numberish>,
    /// Percent points used, rounded by the server.
    #[serde(default)]
    pub(crate) percentage: Option<Numberish>,
    /// When the window resets, in epoch milliseconds.
    #[serde(default, rename = "nextResetTime")]
    pub(crate) next_reset_time: Option<Numberish>,
    /// The window's unit: 3 hours, 4 days, 5 months, 6 weeks.
    #[serde(default)]
    pub(crate) unit: Option<Numberish>,
    /// How many units the window spans.
    #[serde(default)]
    pub(crate) number: Option<Numberish>,
}
