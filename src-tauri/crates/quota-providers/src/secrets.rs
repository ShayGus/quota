//! The operating system's credential store, behind Quota's secret port.
//!
//! A credential Quota owns itself, such as an API key the person pasted, is
//! kept here and nowhere else: Windows Credential Manager, the Secret Service
//! on Linux, or the login Keychain on macOS. Each entry is named for the
//! connection it signs in, under Quota's application identifier.
//!
//! The port is async, and this adapter owns its threading: every blocking store
//! call runs on a worker thread, and the store is released off the async
//! runtime, so an async caller neither blocks a worker nor nests a runtime.

use std::fmt::{self, Debug, Formatter};
use std::sync::Arc;

#[cfg(feature = "test-fixtures")]
use std::sync::MutexGuard;

use async_trait::async_trait;
use keyring_core::{CredentialStore, Error};
use quota_core::ports::{Secret, SecretStore, SecretStoreError};
use quota_domain::ids::ConnectionId;

/// This system's credential store, or, when it has none, a store that
/// refuses every operation, so the providers that need one say so instead of
/// stopping the application.
///
/// `service` is the application identifier the build runs with. A development
/// build files its entries under its own identifier, so it never reads or
/// removes the ones the production build owns.
///
/// The store is opened on a worker thread, so this is safe to call from inside
/// a running Tokio runtime.
#[must_use]
pub async fn system(service: &str) -> Arc<dyn SecretStore> {
    let service = service.to_owned();
    match tokio::task::spawn_blocking(move || SystemSecretStore::open(&service)).await {
        Ok(Ok(store)) => Arc::new(store),
        Ok(Err(_)) | Err(_) => unavailable(),
    }
}

/// A store that refuses every operation as unavailable.
#[must_use]
pub fn unavailable() -> Arc<dyn SecretStore> {
    Arc::new(Unavailable)
}

/// No credential store.
#[derive(Debug)]
struct Unavailable;

#[async_trait]
impl SecretStore for Unavailable {
    async fn read(&self, _connection: &ConnectionId) -> Result<Option<Secret>, SecretStoreError> {
        Err(SecretStoreError::Unavailable)
    }

    async fn write(
        &self,
        _connection: &ConnectionId,
        _secret: &Secret,
    ) -> Result<(), SecretStoreError> {
        Err(SecretStoreError::Unavailable)
    }

    async fn delete(&self, _connection: &ConnectionId) -> Result<(), SecretStoreError> {
        Err(SecretStoreError::Unavailable)
    }
}

/// The credential store this system provides.
pub struct SystemSecretStore {
    /// The application identifier every entry is filed under.
    service: String,
    store: Option<Arc<CredentialStore>>,
}

impl SystemSecretStore {
    /// Opens this system's credential store.
    ///
    /// This synchronous constructor must run outside an async runtime: the
    /// Linux store drives its own runtime while opening. Async callers use
    /// [`system`] instead.
    ///
    /// # Errors
    /// Returns [`SecretStoreError::Unavailable`] when the system has none, for
    /// example a Linux session without a Secret Service.
    pub fn open(service: &str) -> Result<Self, SecretStoreError> {
        Ok(Self::over(
            crate::platform::system().open_credential_store(service)?,
            service,
        ))
    }

    /// Wraps one credential store, so tests can use an in-memory one.
    #[must_use]
    pub fn over(store: Arc<CredentialStore>, service: &str) -> Self {
        Self {
            store: Some(store),
            service: service.to_owned(),
        }
    }
}

impl Drop for SystemSecretStore {
    fn drop(&mut self) {
        // A credential store's own teardown can drive a runtime of its own, so
        // release this owner's reference on a thread without a Tokio context,
        // including startup-error cleanup before managed state retains the
        // registry. The scope waits for that release; the regression test is
        // `startup_failure_drops_both_registries_outside_the_runtime`.
        if tokio::runtime::Handle::try_current().is_ok() {
            std::thread::scope(|scope| {
                scope.spawn(|| drop(self.store.take()));
            });
        }
    }
}

impl Debug for SystemSecretStore {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("SystemSecretStore").finish_non_exhaustive()
    }
}

/// The longest value one entry holds, in characters.
///
/// Windows Credential Manager refuses a secret over 2,560 bytes, and a
/// website session such as `TypeSafe`'s console cookies is longer. Each
/// character is stored in two bytes there, so a part stays at 1,200, well
/// inside the limit. A longer value is kept in numbered parts, and the
/// connection's own entry names how many there are.
const PART_LENGTH: usize = 1_200;

/// How a connection's entry says its value is kept in parts. No credential
/// Quota keeps starts with this.
const PARTS_MARK: &str = "quota-parts:v1:";

/// The value cut into parts no longer than `PART_LENGTH` characters.
fn split_parts(value: &str) -> Vec<String> {
    let characters: Vec<char> = value.chars().collect();
    characters
        .chunks(PART_LENGTH)
        .map(|part| part.iter().collect())
        .collect()
}

/// How many parts an entry's value names, when it names any.
fn part_count(value: &str) -> Option<usize> {
    value.strip_prefix(PARTS_MARK)?.parse().ok()
}

/// The entry name of one part of a connection's value.
fn part_name(connection: &ConnectionId, part: usize) -> String {
    format!("{}#part-{part}", connection.as_str())
}

/// An entry by name, under the application identifier.
fn named(
    store: Option<&Arc<CredentialStore>>,
    service: &str,
    name: &str,
) -> Result<keyring_core::Entry, SecretStoreError> {
    store
        .ok_or(SecretStoreError::Unavailable)?
        .build(service, name, None)
        .map_err(|error| classify(&error))
}

/// One entry's value, `None` when there is no entry.
fn get(
    store: Option<&Arc<CredentialStore>>,
    service: &str,
    name: &str,
) -> Result<Option<String>, SecretStoreError> {
    match named(store, service, name)?.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(Error::NoEntry) => Ok(None),
        Err(error) => Err(classify(&error)),
    }
}

/// Removes one entry; a missing one is already removed.
fn remove(
    store: Option<&Arc<CredentialStore>>,
    service: &str,
    name: &str,
) -> Result<(), SecretStoreError> {
    match named(store, service, name)?.delete_credential() {
        Ok(()) | Err(Error::NoEntry) => Ok(()),
        Err(error) => Err(classify(&error)),
    }
}

/// Removes the parts numbered `from` to `to`.
fn remove_parts(
    store: Option<&Arc<CredentialStore>>,
    service: &str,
    connection: &ConnectionId,
    from: usize,
    to: usize,
) -> Result<(), SecretStoreError> {
    for part in from..=to {
        remove(store, service, &part_name(connection, part))?;
    }
    Ok(())
}

#[async_trait]
impl SecretStore for SystemSecretStore {
    async fn read(&self, connection: &ConnectionId) -> Result<Option<Secret>, SecretStoreError> {
        let store = self.store.clone();
        let service = self.service.clone();
        let connection = connection.clone();
        on_blocking_worker(move || {
            let store = store.as_ref();
            let Some(value) = get(store, &service, connection.as_str())? else {
                return Ok(None);
            };
            let Some(count) = part_count(&value) else {
                return Ok(Some(Secret::new(value)));
            };
            let mut joined = String::new();
            for part in 1..=count {
                // A missing part is a value that cannot be put back together.
                let piece = get(store, &service, &part_name(&connection, part))?
                    .ok_or(SecretStoreError::Refused)?;
                joined.push_str(&piece);
            }
            Ok(Some(Secret::new(joined)))
        })
        .await
    }

    async fn write(
        &self,
        connection: &ConnectionId,
        secret: &Secret,
    ) -> Result<(), SecretStoreError> {
        let store = self.store.clone();
        let service = self.service.clone();
        let connection = connection.clone();
        let value = secret.expose().to_owned();
        on_blocking_worker(move || {
            let store = store.as_ref();
            let before = get(store, &service, connection.as_str())?
                .as_deref()
                .and_then(part_count)
                .unwrap_or(0);
            let parts = split_parts(&value);
            let written = if parts.len() <= 1 {
                named(store, &service, connection.as_str())?
                    .set_password(&value)
                    .map_err(|error| classify(&error))?;
                0
            } else {
                // The parts first, then the entry that names them, so a
                // reader never finds an entry naming parts not yet written.
                for (index, part) in parts.iter().enumerate() {
                    named(store, &service, &part_name(&connection, index + 1))?
                        .set_password(part)
                        .map_err(|error| classify(&error))?;
                }
                named(store, &service, connection.as_str())?
                    .set_password(&format!("{PARTS_MARK}{}", parts.len()))
                    .map_err(|error| classify(&error))?;
                parts.len()
            };
            // Parts a longer earlier value left behind.
            remove_parts(store, &service, &connection, written + 1, before)
        })
        .await
    }

    async fn delete(&self, connection: &ConnectionId) -> Result<(), SecretStoreError> {
        let store = self.store.clone();
        let service = self.service.clone();
        let connection = connection.clone();
        on_blocking_worker(move || {
            let store = store.as_ref();
            let count = get(store, &service, connection.as_str())?
                .as_deref()
                .and_then(part_count)
                .unwrap_or(0);
            remove_parts(store, &service, &connection, 1, count)?;
            remove(store, &service, connection.as_str())
        })
        .await
    }
}

/// The secret of each connection, keyed by its identifier.
#[cfg(feature = "test-fixtures")]
type Entries = std::collections::HashMap<String, String>;

/// An in-memory credential store, so a test needs no system store.
///
/// Compiled only with the non-default `test-fixtures` feature, which
/// `cargo xtask check-release` keeps out of every release build.
#[cfg(feature = "test-fixtures")]
#[derive(Debug, Default)]
pub struct MemorySecretStore {
    /// The secret of each connection, keyed by its identifier.
    entries: std::sync::Mutex<Entries>,
}

#[cfg(feature = "test-fixtures")]
impl MemorySecretStore {
    /// A store with no entries.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The entries, locked for one operation.
    fn entries(&self) -> Result<MutexGuard<'_, Entries>, SecretStoreError> {
        self.entries
            .lock()
            .map_err(|_| SecretStoreError::Unavailable)
    }
}

#[cfg(feature = "test-fixtures")]
#[async_trait]
impl SecretStore for MemorySecretStore {
    async fn read(&self, connection: &ConnectionId) -> Result<Option<Secret>, SecretStoreError> {
        Ok(self
            .entries()?
            .get(connection.as_str())
            .cloned()
            .map(Secret::new))
    }

    async fn write(
        &self,
        connection: &ConnectionId,
        secret: &Secret,
    ) -> Result<(), SecretStoreError> {
        self.entries()?
            .insert(connection.as_str().to_owned(), secret.expose().to_owned());
        Ok(())
    }

    async fn delete(&self, connection: &ConnectionId) -> Result<(), SecretStoreError> {
        self.entries()?.remove(connection.as_str());
        Ok(())
    }
}

/// Runs one blocking store call on a worker thread, so the caller keeps its
/// runtime.
///
/// A worker that cannot be joined, because it was cancelled or it panicked, is
/// reported as an unavailable store, never as a panic on the caller.
async fn on_blocking_worker<T, F>(work: F) -> Result<T, SecretStoreError>
where
    F: FnOnce() -> Result<T, SecretStoreError> + Send + 'static,
    T: Send + 'static,
{
    match tokio::task::spawn_blocking(work).await {
        Ok(result) => result,
        Err(_) => Err(SecretStoreError::Unavailable),
    }
}

/// Maps a store failure onto the port's closed set, dropping its text, which
/// can name the entry.
fn classify(error: &Error) -> SecretStoreError {
    match error {
        Error::NoStorageAccess(_) | Error::NoDefaultStore => SecretStoreError::Unavailable,
        _ => SecretStoreError::Refused,
    }
}

#[cfg(test)]
#[path = "secrets_tests.rs"]
mod tests;
