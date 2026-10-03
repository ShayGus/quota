//! Rust-owned IPC contracts for the Quota desktop host.
//!
//! This crate holds transport definitions only: tagged identifiers, command
//! arguments and results, the typed error union, and event payloads. It has no
//! business services and no credential types. Domain types reach the renderer
//! directly through their own Serde and Specta derives, so there is exactly one
//! schema and no hand-maintained duplicate.

#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

pub mod commands;
pub mod errors;
pub mod events;
pub mod preferences;
pub mod refs;

pub use commands::{
    AccountSelection, BeginConnectionRequest, ConnectionAttemptAccepted, RefreshReason,
    RegisteredProvider, SetAccountEnabledRequest, SnapshotResponse, VerifiedCandidate,
    WindowModeChange,
};
pub use errors::CommandError;
pub use events::{
    ConnectionProgressChangedPayload, MonitoringStateChangedPayload,
    OverviewWindowStateChangedPayload, PersistenceStatusChangedPayload, PreferencesChangedPayload,
    SnapshotUpdatedPayload,
};
pub use preferences::{NotificationPolicy, NotificationThresholds, Preferences, PrivacyPolicy};
pub use refs::{AccountRef, AttemptRef, ConnectionRef};
