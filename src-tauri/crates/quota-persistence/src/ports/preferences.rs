//! Adapter from the core preference port to the typed presentation document.

use async_trait::async_trait;
use quota_core::ports::{PreferenceRepository as PreferencePort, RepositoryError};
use quota_domain::preferences::PresentationPreferences;

use crate::PersistenceError;
use crate::store::document::{PreferenceDocumentStore, PresentationPreferencesCodec};

/// Implements the core preference port over the versioned Store document codec.
#[derive(Clone, Debug)]
pub struct PresentationPreferencesPort<S> {
    codec: PresentationPreferencesCodec<S>,
}

impl<S> PresentationPreferencesPort<S>
where
    S: PreferenceDocumentStore,
{
    /// Wraps the typed document codec.
    #[must_use]
    pub fn new(codec: PresentationPreferencesCodec<S>) -> Self {
        Self { codec }
    }
}

#[async_trait]
impl<S> PreferencePort for PresentationPreferencesPort<S>
where
    S: PreferenceDocumentStore + Send + Sync,
{
    async fn load(&self) -> Result<PresentationPreferences, RepositoryError> {
        self.codec.load().map_err(|error| map_error(&error))
    }

    async fn save(
        &self,
        preferences: &PresentationPreferences,
    ) -> Result<PresentationPreferences, RepositoryError> {
        let mut saved = preferences.clone();
        self.codec
            .save(&mut saved)
            .map_err(|error| map_error(&error))?;
        Ok(saved)
    }
}

fn map_error(error: &PersistenceError) -> RepositoryError {
    RepositoryError::new("store", error.to_string())
}
