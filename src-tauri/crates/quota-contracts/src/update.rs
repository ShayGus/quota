//! The update pop-up: what it shows, what a person can answer, and the event
//! that carries a change of what it shows.
//!
//! The host owns the update flow and the pop-up's state. The pop-up window only
//! draws the current [`UpdatePrompt`] and sends back an [`UpdateResponse`].

use serde::{Deserialize, Serialize};
use specta::Type;

/// What the update pop-up is showing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case", tag = "kind", content = "context")]
pub enum UpdatePrompt {
    /// A newer version is published and the person is asked about it.
    Offer {
        /// The version on offer, such as `0.2.0`.
        version: String,
        /// The version running now.
        current: String,
    },
    /// The person said yes; the update is downloading and installing.
    Installing {
        /// The version being installed.
        version: String,
    },
    /// The update was accepted and could not be installed.
    Failed,
}

/// What the person pressed in the update pop-up.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum UpdateResponse {
    /// OK: install the offered update now.
    Install,
    /// Cancel, or closing the pop-up, on an offer.
    Decline,
    /// Close, on the message that the install failed.
    Dismiss,
}

/// The pop-up changed what it shows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct UpdatePromptChangedPayload {
    /// What the pop-up shows now.
    pub prompt: UpdatePrompt,
}
