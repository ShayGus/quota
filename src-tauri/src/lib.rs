//! Tauri desktop host for Quota.
//!
//! This crate is an outer composition layer. It maps domain results onto the
//! typed IPC contracts, mounts the Tauri Specta registry, and owns the native
//! window, tray, notification, and secret adapters. It holds no quota rules of
//! its own.

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
