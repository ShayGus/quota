//! The preference document over `tauri_plugin_store`.
//!
//! This module exists only under the non-default `tauri-plugins` feature. It
//! implements the same load/save shape as the in-crate codec, over the store
//! file the plugin manages, so the desktop host can hand its store in without
//! the rest of the application depending on a Tauri type.
//!
//! Nothing secret belongs here. The plugin's change events can carry stored
//! values to the renderer, and this store holds presentation preferences only.

use quota_domain::preferences::PresentationPreferences;
use serde_json::Value;
use tauri::Runtime;
use tauri_plugin_store::Store;

use crate::error::{PersistenceError, PersistenceResult};
use crate::store::document::{PreferenceDocumentStore, PresentationPreferencesCodec};

/// A document store backed by one `tauri_plugin_store` file.
pub struct PluginDocumentStore<R: Runtime> {
    store: std::sync::Arc<Store<R>>,
}

impl<R: Runtime> PluginDocumentStore<R> {
    /// Wraps a store the plugin has already opened.
    #[must_use]
    pub fn new(store: std::sync::Arc<Store<R>>) -> Self {
        Self { store }
    }
}

impl<R: Runtime> PreferenceDocumentStore for PluginDocumentStore<R> {
    fn read(&self, key: &str) -> PersistenceResult<Option<Value>> {
        Ok(self.store.get(key))
    }

    fn write(&self, key: &str, value: &Value) -> PersistenceResult<()> {
        let previous = self.store.get(key);
        self.store.set(key.to_owned(), value.clone());
        if self.store.save().is_err() {
            if let Some(previous) = previous {
                self.store.set(key.to_owned(), previous);
            } else {
                self.store.delete(key);
            }
            return Err(PersistenceError::StoreUnavailable);
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "plugin_tests.rs"]
mod tests;

/// The typed preference repository over the plugin's store.
pub struct StorePreferencesRepository<R: Runtime> {
    codec: PresentationPreferencesCodec<PluginDocumentStore<R>>,
}

impl<R: Runtime> StorePreferencesRepository<R> {
    /// Wraps a store the plugin has already opened.
    #[must_use]
    pub fn new(store: std::sync::Arc<Store<R>>) -> Self {
        Self {
            codec: PresentationPreferencesCodec::new(PluginDocumentStore::new(store)),
        }
    }

    /// Reads the stored document, or returns the defaults when none exists.
    ///
    /// # Errors
    /// As [`PresentationPreferencesCodec::load`].
    pub fn load(&self) -> PersistenceResult<PresentationPreferences> {
        self.codec.load()
    }

    /// Reads the stored document without substituting defaults.
    ///
    /// # Errors
    /// As [`PresentationPreferencesCodec::validate`].
    pub fn validate(&self) -> PersistenceResult<Option<PresentationPreferences>> {
        self.codec.validate()
    }

    /// Writes the document and returns its new revision.
    ///
    /// # Errors
    /// As [`PresentationPreferencesCodec::save`].
    pub fn save(&self, preferences: &mut PresentationPreferences) -> PersistenceResult<u32> {
        self.codec.save(preferences)
    }
}
