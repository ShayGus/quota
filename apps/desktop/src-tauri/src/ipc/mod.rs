//! The typed IPC layer.
//!
//! One Tauri Specta registry is used both to mount the handlers and to export
//! `apps/desktop/src/generated/bindings.ts`. Nothing else emits or listens.

pub mod bindings;
pub mod commands;
pub mod events;
