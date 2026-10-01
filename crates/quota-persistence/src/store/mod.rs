//! Store-backed presentation preferences.

pub mod document;
#[cfg(feature = "tauri-plugins")]
pub mod plugin;

pub use document::{PreferenceDocumentStore, PresentationPreferencesCodec};
