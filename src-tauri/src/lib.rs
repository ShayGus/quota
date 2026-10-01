//! Tauri desktop host for Quota.

#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]
pub mod monitoring;

pub mod bootstrap;
pub mod bootstrap_helpers;
pub mod ipc;
pub mod platform;
pub mod state;

pub use bootstrap::start;

/// Starts the desktop host, reporting a startup failure and exiting.
///
/// The Tauri CLI builds this crate as a library and calls this from `main.rs`,
/// so the entry point stays free of logic on every platform.
pub fn run() {
    if let Err(error) = bootstrap::start() {
        eprintln!("quota failed to start: {error}");
        std::process::exit(1);
    }
}
pub use state::AppState;
