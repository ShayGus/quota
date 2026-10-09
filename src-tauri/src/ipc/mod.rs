//! The typed IPC layer.
//!
//! One Tauri Specta registry is used both to mount the handlers and to export
//! `src/generated/bindings.ts`. Nothing else emits or listens.

pub mod bindings;
pub mod commands;
pub mod commands_connection;
pub mod commands_groups;
pub mod commands_prefs;
pub mod commands_report;
pub mod commands_update;
pub mod commands_window;
pub mod events;
