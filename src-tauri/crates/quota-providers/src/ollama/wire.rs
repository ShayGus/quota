//! Wire shapes for ollama.com's account endpoints.
//!
//! `GET /api/balance` answers the plan's included usage: on a plan with usage
//! limits, the five-hour session window and the weekly window, each as the
//! percentage left with its reset time; on a plan with a monthly allowance,
//! the allowance and what is left of it in dollars. It also answers the
//! credit bought beyond the plan, which Quota does not read yet. `POST
//! /api/me` answers the account; called directly, ollama.com capitalises its
//! keys (`Plan`, `Email`). Both are the endpoints behind Ollama's own settings
//! page and are undocumented, so every field is optional and unknown fields
//! are ignored.

use serde::Deserialize;

use crate::decode::Numberish;

/// The body of `GET /api/balance`.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct BalanceBody {
    /// What the plan includes.
    #[serde(default)]
    pub(crate) included: Option<Included>,
}

/// What the plan includes: limit windows, a monthly allowance, or both.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct Included {
    /// The five-hour session window.
    #[serde(default)]
    pub(crate) session: Option<Limit>,
    /// The weekly window.
    #[serde(default)]
    pub(crate) weekly: Option<Limit>,
    /// The monthly window, written the same way.
    #[serde(default)]
    pub(crate) monthly: Option<Limit>,
    /// The monthly allowance, in dollars.
    #[serde(default)]
    pub(crate) allowance_usd: Option<Numberish>,
    /// What is left of the monthly allowance, in dollars.
    #[serde(default)]
    pub(crate) balance_usd: Option<Numberish>,
    /// The allowance's current period.
    #[serde(default)]
    pub(crate) period: Option<Period>,
}

/// One limit window.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct Limit {
    /// The percentage of the window left, from 0 to 100.
    #[serde(default)]
    pub(crate) remaining_percent: Option<Numberish>,
    /// When the window resets.
    #[serde(default)]
    pub(crate) resets_at: Option<Numberish>,
}

/// The allowance's current period.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct Period {
    /// When the period ends and the allowance renews.
    #[serde(default)]
    pub(crate) until: Option<Numberish>,
}

/// The body of `POST /api/me`.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct Account {
    /// The plan, such as `pro`.
    #[serde(default, rename = "Plan", alias = "plan")]
    pub(crate) plan: Option<String>,
    /// The account's address.
    #[serde(default, rename = "Email", alias = "email")]
    pub(crate) email: Option<String>,
}
