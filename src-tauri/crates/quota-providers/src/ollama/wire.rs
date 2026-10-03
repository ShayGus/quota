//! Wire shapes for ollama.com's account endpoints.
//!
//! `GET /api/usage` answers the plan's limits as fractions used, with no reset
//! times, and recent spend beyond the plan. `POST /api/me` answers the account;
//! called directly, ollama.com capitalises its keys (`Plan`, `Email`). Both are
//! the endpoints behind Ollama's own settings page and are undocumented, so
//! every field is optional and unknown fields are ignored.

use serde::Deserialize;

use crate::decode::Numberish;

/// The body of `GET /api/usage`.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct UsageBody {
    /// The plan's windows.
    #[serde(default)]
    pub(crate) limits: Option<Limits>,
}

/// The plan's windows.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct Limits {
    /// The five-hour session window.
    #[serde(default)]
    pub(crate) session: Option<Limit>,
    /// The weekly window.
    #[serde(default)]
    pub(crate) weekly: Option<Limit>,
    /// The monthly window.
    #[serde(default)]
    pub(crate) monthly: Option<Limit>,
}

/// One window.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct Limit {
    /// The fraction of the window used, from 0 to 1.
    #[serde(default)]
    pub(crate) usage: Option<Numberish>,
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
