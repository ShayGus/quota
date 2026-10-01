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

pub use bootstrap::run;
pub use state::AppState;
