//! Wire shapes for `MiniMax`'s token-plan endpoint.
//!
//! `GET /v1/token_plan/remains` answers HTTP 200 even for a refused key, so
//! `base_resp.status_code` is the real verdict: `0` is success. The buckets sit
//! in `model_remains[]`, at the root or under `data`. The endpoint is
//! undocumented, so every field is optional and unknown fields are ignored.

use serde::Deserialize;

use crate::decode::Numberish;

/// The response body.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct RemainsEnvelope {
    /// The verdict.
    #[serde(default)]
    pub(crate) base_resp: Option<BaseResponse>,
    /// The buckets, when they sit at the root.
    #[serde(default, deserialize_with = "crate::decode::null_as_default")]
    pub(crate) model_remains: Vec<Bucket>,
    /// The buckets and plan, when they sit under `data`.
    #[serde(default)]
    pub(crate) data: Option<RemainsData>,
    /// The plan's name, when it sits at the root.
    #[serde(default, alias = "plan_name", alias = "combo_title")]
    pub(crate) current_subscribe_title: Option<String>,
}

/// The buckets and plan, under `data`.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct RemainsData {
    /// One bucket per plan quota.
    #[serde(default, deserialize_with = "crate::decode::null_as_default")]
    pub(crate) model_remains: Vec<Bucket>,
    /// The plan's name.
    #[serde(default, alias = "plan_name", alias = "combo_title")]
    pub(crate) current_subscribe_title: Option<String>,
}

impl RemainsEnvelope {
    /// The buckets, wherever they sit.
    pub(crate) fn buckets(&self) -> &[Bucket] {
        match &self.data {
            Some(data) if self.model_remains.is_empty() => &data.model_remains,
            _ => &self.model_remains,
        }
    }

    /// The plan's name, wherever it sits.
    pub(crate) fn plan(&self) -> Option<&str> {
        self.current_subscribe_title
            .as_deref()
            .or_else(|| self.data.as_ref()?.current_subscribe_title.as_deref())
            .map(str::trim)
            .filter(|plan| !plan.is_empty())
    }
}

/// The verdict on the request.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct BaseResponse {
    /// `0` for success.
    #[serde(default)]
    pub(crate) status_code: Option<Numberish>,
}

/// One plan quota: a rolling interval window and a weekly window.
#[derive(Debug, Default, Deserialize)]
pub(crate) struct Bucket {
    /// The quota's name: `general` for the plan-wide one.
    #[serde(default)]
    pub(crate) model_name: Option<String>,
    /// The interval window's start, in epoch seconds or milliseconds.
    #[serde(default)]
    pub(crate) start_time: Option<Numberish>,
    /// The interval window's end.
    #[serde(default)]
    pub(crate) end_time: Option<Numberish>,
    /// Percent of the interval window remaining.
    #[serde(default)]
    pub(crate) current_interval_remaining_percent: Option<Numberish>,
    /// The interval window's request allowance.
    #[serde(default)]
    pub(crate) current_interval_total_count: Option<Numberish>,
    /// `1` normal, `2` exhausted, `3` unlimited.
    #[serde(default)]
    pub(crate) current_interval_status: Option<Numberish>,
    /// The weekly window's end.
    #[serde(default)]
    pub(crate) weekly_end_time: Option<Numberish>,
    /// Percent of the weekly window remaining.
    #[serde(default)]
    pub(crate) current_weekly_remaining_percent: Option<Numberish>,
    /// The weekly window's request allowance.
    #[serde(default)]
    pub(crate) current_weekly_total_count: Option<Numberish>,
    /// `1` normal, `2` exhausted, `3` unlimited.
    #[serde(default)]
    pub(crate) current_weekly_status: Option<Numberish>,
}
