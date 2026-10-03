//! The versioned presentation-preferences document.
//!
//! The codec owns the storage key and the only `serde_json::Value` conversion in
//! this crate. Callers never name a key and never hand a raw value to a store;
//! they load a typed [`PresentationPreferences`] and save one.
//!
//! A stored document that this build cannot read is preserved for recovery
//! before an error is returned. It is never overwritten in place, and because
//! no preferences value is produced for it, corruption can never switch on a
//! preference the user did not choose.

use quota_domain::preferences::{PREFERENCES_SCHEMA_VERSION, PresentationPreferences};
use serde_json::Value;

use crate::error::{PersistenceError, PersistenceResult};

/// A key/value document store, addressed only by this codec's own keys.
pub trait PreferenceDocumentStore {
    /// Reads the raw value stored under `key`.
    ///
    /// # Errors
    /// Returns [`PersistenceError::StoreUnavailable`] when the store cannot be read.
    fn read(&self, key: &str) -> PersistenceResult<Option<Value>>;

    /// Writes `value` under `key`.
    ///
    /// # Errors
    /// Returns [`PersistenceError::StoreUnavailable`] when the store cannot be written.
    fn write(&self, key: &str, value: &Value) -> PersistenceResult<()>;
}

/// The versioned preference-document codec over one document store.
#[derive(Clone, Debug)]
pub struct PresentationPreferencesCodec<S> {
    store: S,
}

impl<S: PreferenceDocumentStore> PresentationPreferencesCodec<S> {
    /// The key the live preference document occupies.
    ///
    /// It is an implementation detail of this codec. Nothing outside this crate
    /// writes it.
    const LIVE_KEY: &'static str = "quota.preferences.presentation.v1";

    /// The key that holds a preserved document.
    ///
    /// It keeps the last document this build accepted, and it keeps a document
    /// this build rejected so a person or a later build can recover it.
    const RECOVERY_KEY: &'static str = "quota.preferences.presentation.recovery.v1";

    /// Wraps one document store.
    #[must_use]
    pub fn new(store: S) -> Self {
        Self { store }
    }

    /// Reads the stored document, or returns the defaults when none exists.
    ///
    /// Defaults apply only to a store that holds no document at all. A stored
    /// document that cannot be read produces its typed error instead.
    ///
    /// # Errors
    /// Returns [`PersistenceError::StoreDocumentCorrupt`] or
    /// [`PersistenceError::StoreDocumentUnsupported`] for a stored document
    /// this build cannot read, after preserving it, and
    /// [`PersistenceError::StoreUnavailable`] when the store itself fails.
    pub fn load(&self) -> PersistenceResult<PresentationPreferences> {
        Ok(self.validate()?.unwrap_or_default())
    }

    /// Reads the stored document without substituting defaults.
    ///
    /// # Returns
    /// `None` when the store holds no document.
    ///
    /// # Errors
    /// As [`Self::load`].
    pub fn validate(&self) -> PersistenceResult<Option<PresentationPreferences>> {
        let Some(stored) = self.store.read(Self::LIVE_KEY)? else {
            return Ok(None);
        };

        match decode(&stored) {
            Ok(preferences) => Ok(Some(preferences)),
            Err(error) => {
                self.preserve(&stored)?;
                Err(error)
            }
        }
    }

    /// Writes the document, advancing its revision first.
    ///
    /// The document already stored is copied to the recovery key before the new
    /// document replaces it, so a last-known-good document survives a save that
    /// a later build cannot read.
    ///
    /// # Returns
    /// The new revision.
    ///
    /// # Errors
    /// Returns [`PersistenceError::BackupFailed`] when the previous document
    /// could not be preserved, and [`PersistenceError::StoreUnavailable`] when
    /// the store itself fails.
    pub fn save(&self, preferences: &mut PresentationPreferences) -> PersistenceResult<u32> {
        if let Some(previous) = self.store.read(Self::LIVE_KEY)? {
            self.preserve(&previous)?;
        }

        let revision = preferences.advance();
        let document = serde_json::to_value(&*preferences)
            .map_err(|_| PersistenceError::StoreDocumentCorrupt)?;
        self.store.write(Self::LIVE_KEY, &document)?;
        Ok(revision)
    }

    /// Copies a document aside for recovery.
    fn preserve(&self, document: &Value) -> PersistenceResult<()> {
        self.store
            .write(Self::RECOVERY_KEY, document)
            .map_err(|_| PersistenceError::BackupFailed)
    }
}

/// Turns a stored value into typed preferences, or the typed reason it cannot.
fn decode(stored: &Value) -> PersistenceResult<PresentationPreferences> {
    let found_version = stored
        .get("schema_version")
        .and_then(Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or(PersistenceError::StoreDocumentCorrupt)?;

    if found_version != PREFERENCES_SCHEMA_VERSION {
        return Err(PersistenceError::StoreDocumentUnsupported {
            found_version,
            supported_version: PREFERENCES_SCHEMA_VERSION,
        });
    }

    serde_json::from_value(stored.clone()).map_err(|_| PersistenceError::StoreDocumentCorrupt)
}
