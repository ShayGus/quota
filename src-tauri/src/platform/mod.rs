//! Native adapters.
//!
//! Each adapter is a thin wrapper over one Tauri capability. Business rules
//! stay in `quota-core`; these types exist so a window operation can be tested
//! through a seam and so the renderer never holds a Tauri handle.

pub mod app_view;
pub mod autostart;
pub(crate) mod browser;
pub mod popover_height;
pub mod settings_window;
pub mod tray;
pub mod tray_anchor;
pub mod widget;
pub mod widget_screens;
pub mod window;
pub mod window_events;
mod window_transition;

pub use window::{OverviewWindowController, OverviewWindowState};
