//! Wire shapes for Kimi for Coding's usage endpoint.
//!
//! `GET /coding/v1/usages` answers a weekly `usage` summary, `limits[]` for
//! the shorter windows, each with its own `window` and `detail`, and
//! `usages` with monthly ratios. Numbers arrive as JSON numbers or strings.
//! The endpoint is undocumented, so every field is optional and unknown fields
//! are ignored.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::decode::Numberish;

/// The response body.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct UsageEnvelope {
    /// The account, with its membership level.
    #[serde(default)]
    pub(crate) user: Option<User>,
    /// The weekly allowance.
    #[serde(default)]
    pub(crate) usage: Option<Detail>,
    /// The shorter windows.
    #[serde(default, deserialize_with = "crate::decode::null_as_default")]
    pub(crate) limits: Vec<Limit>,
    /// Monthly ratios, keyed by name: `limit_month_total` is the account's.
    #[serde(default, deserialize_with = "crate::decode::null_as_default")]
    pub(crate) usages: BTreeMap<String, Ratio>,
}

/// The account.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct User {
    /// The plan.
    #[serde(default)]
    pub(crate) membership: Option<Membership>,
}

/// The plan.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct Membership {
    /// The plan's level, such as `LEVEL_INTERMEDIATE`.
    #[serde(default)]
    pub(crate) level: Option<String>,
}

/// One allowance's counts and reset.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct Detail {
    /// The allowance.
    #[serde(default)]
    pub(crate) limit: Option<Numberish>,
    /// What has been used.
    #[serde(default)]
    pub(crate) used: Option<Numberish>,
    /// What is left.
    #[serde(default)]
    pub(crate) remaining: Option<Numberish>,
    /// When it resets: a date string or epoch seconds.
    #[serde(
        default,
        rename = "resetTime",
        alias = "reset_time",
        alias = "resetAt",
        alias = "reset_at"
    )]
    pub(crate) reset_time: Option<Numberish>,
}

/// One shorter window.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct Limit {
    /// The window's length.
    #[serde(default)]
    pub(crate) window: Option<Span>,
    /// The window's counts.
    #[serde(default)]
    pub(crate) detail: Option<Detail>,
}

/// A window's length: `duration` in `timeUnit`, such as 300 minutes.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct Span {
    /// How many units.
    #[serde(default)]
    pub(crate) duration: Option<Numberish>,
    /// The unit, such as `TIME_UNIT_MINUTE`.
    #[serde(default, rename = "timeUnit", alias = "time_unit")]
    pub(crate) time_unit: Option<String>,
}

/// A monthly ratio.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct Ratio {
    /// The fraction used, from 0 to 1.
    #[serde(default)]
    pub(crate) used_ratio: Option<Numberish>,
    /// When it resets.
    #[serde(
        default,
        rename = "resetTime",
        alias = "reset_time",
        alias = "resetAt",
        alias = "reset_at"
    )]
    pub(crate) reset_time: Option<Numberish>,
}
