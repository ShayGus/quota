//! Tauri desktop host for Quota.

#![forbid(unsafe_code)]
// Host failures are mapped onto `CommandError`, whose `operation`, `reason`, or
// `diagnostic_code` is the stable contract the renderer and the logs share. A
// wider error would replace a reviewable string with driver text.
#![expect(
    clippy::map_err_ignore,
    reason = "CommandError's operation and reason strings are the contract; Tauri text is not"
)]
#![doc = include_str!("../README.md")]
pub mod monitoring;

mod app_identity;
pub mod bootstrap;
pub mod bootstrap_helpers;
pub mod bug_report;
mod file_log;
pub mod ipc;
pub mod platform;
pub mod provider_catalog;
pub mod state;
pub mod updates;

pub use bootstrap::start;
pub use state::AppState;

use std::process::ExitCode;

/// Starts the desktop host and returns the process exit status.
///
/// A startup failure is reported on standard error and becomes a failing exit
/// code. The Tauri CLI builds this crate as a library and calls this from
/// `main.rs`, so the entry point stays free of logic on every platform.
#[must_use]
pub fn run() -> ExitCode {
    match bootstrap::start() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("quota failed to start: {error}");
            ExitCode::FAILURE
        }
    }
}
