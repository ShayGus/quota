//! Native adapters.
//!
//! Each adapter is a thin wrapper over one Tauri capability. Business rules
//! stay in `quota-core`; these types exist so a window operation can be tested
//! through a seam and so the renderer never holds a Tauri handle.

pub mod tray;
pub mod window;
mod window_transition;

pub use window::{OverviewWindowController, OverviewWindowState};
