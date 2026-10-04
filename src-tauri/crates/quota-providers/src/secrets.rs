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

#[async_trait]
impl SecretStore for SystemSecretStore {
    async fn read(&self, connection: &ConnectionId) -> Result<Option<Secret>, SecretStoreError> {
        let store = self.store.clone();
        let service = self.service.clone();
        let connection = connection.clone();
        on_blocking_worker(move || {
            match entry(store.as_ref(), &service, &connection)?.get_password() {
                Ok(value) => Ok(Some(Secret::new(value))),
                Err(Error::NoEntry) => Ok(None),
                Err(error) => Err(classify(&error)),
            }
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
            entry(store.as_ref(), &service, &connection)?
                .set_password(&value)
                .map_err(|error| classify(&error))
        })
        .await
    }

    async fn delete(&self, connection: &ConnectionId) -> Result<(), SecretStoreError> {
        let store = self.store.clone();
        let service = self.service.clone();
        let connection = connection.clone();
        on_blocking_worker(move || {
            match entry(store.as_ref(), &service, &connection)?.delete_credential() {
                Ok(()) | Err(Error::NoEntry) => Ok(()),
                Err(error) => Err(classify(&error)),
            }
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

/// Builds the store entry for one connection.
fn entry(
    store: Option<&Arc<CredentialStore>>,
    service: &str,
    connection: &ConnectionId,
) -> Result<keyring_core::Entry, SecretStoreError> {
    store
        .ok_or(SecretStoreError::Unavailable)?
        .build(service, connection.as_str(), None)
        .map_err(|error| classify(&error))
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
mod tests {
    use super::*;

    /// The identifier `src-tauri/tauri.conf.json` gives a production build and
    /// the one its development overlay gives a development build.
    const PRODUCTION: &str = "app.quota.monitor";
    const DEVELOPMENT: &str = "app.quota.monitor.dev";

    fn memory() -> Arc<CredentialStore> {
        keyring_core::mock::Store::new().expect("the in-memory store opens")
    }

    fn connection(id: &str) -> ConnectionId {
        ConnectionId::new(id).expect("a connection id")
    }

    fn runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("a runtime")
    }

    /// The store destruction path runs outside the async runtime.
    #[cfg(target_os = "linux")]
    #[derive(Debug)]
    struct DropReportingStore {
        inner: Arc<CredentialStore>,
        dropped: std::sync::mpsc::Sender<bool>,
    }

    #[cfg(target_os = "linux")]
    impl keyring_core::api::CredentialStoreApi for DropReportingStore {
        fn vendor(&self) -> String {
            self.inner.vendor()
        }

        fn id(&self) -> String {
            self.inner.id()
        }

        fn build(
            &self,
            service: &str,
            user: &str,
            modifiers: Option<&std::collections::HashMap<&str, &str>>,
        ) -> keyring_core::Result<keyring_core::Entry> {
            self.inner.build(service, user, modifiers)
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    #[cfg(target_os = "linux")]
    impl Drop for DropReportingStore {
        fn drop(&mut self) {
            self.dropped
                .send(tokio::runtime::Handle::try_current().is_ok())
                .expect("the drop observer is still listening");
        }
    }

    /// A startup failure drops both registries, and the store, off the runtime.
    #[cfg(all(target_os = "linux", feature = "test-fixtures"))]
    #[test]
    fn startup_failure_drops_both_registries_outside_the_runtime() {
        let runtime = runtime();

        for with_fixture in [false, true] {
            let (dropped, observer) = std::sync::mpsc::channel();
            let store = SystemSecretStore::over(
                Arc::new(DropReportingStore {
                    inner: memory(),
                    dropped,
                }),
                DEVELOPMENT,
            );
            let result = runtime.block_on(async move {
                let secrets: Arc<dyn SecretStore> = Arc::new(store);
                let providers = if with_fixture {
                    crate::ProviderRegistry::with_fixture(secrets)
                } else {
                    crate::ProviderRegistry::production(secrets)
                }
                .expect("the registry builds");
                let connection = connection("startup-cleanup");
                let secrets = providers.secrets();
                secrets
                    .write(&connection, &Secret::new("test-key".to_owned()))
                    .await
                    .expect("write");
                assert_eq!(
                    secrets
                        .read(&connection)
                        .await
                        .expect("read")
                        .expect("a secret")
                        .expose(),
                    "test-key"
                );
                secrets.delete(&connection).await.expect("delete");
                assert!(secrets.read(&connection).await.expect("read").is_none());
                std::future::ready(Err::<(), &str>("sample_persist:test")).await?;
                Ok::<(), &str>(())
            });

            assert_eq!(result, Err("sample_persist:test"));
            assert!(
                !observer
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .expect("the credential store was destroyed"),
                "credential store destruction entered the async runtime"
            );
        }
    }

    /// Two identifiers over one store: neither reaches the other's entry.
    #[test]
    fn a_development_build_never_reaches_a_production_entry() {
        runtime().block_on(async {
            let system = memory();
            let development = SystemSecretStore::over(system.clone(), DEVELOPMENT);
            let production = SystemSecretStore::over(system, PRODUCTION);
            let connection = connection("c1");

            development
                .write(&connection, &Secret::new("development".to_owned()))
                .await
                .expect("write");

            assert!(
                production.read(&connection).await.expect("read").is_none(),
                "the production identifier read a development entry"
            );
            production.delete(&connection).await.expect("delete");
            assert!(
                development.read(&connection).await.expect("read").is_some(),
                "the production identifier removed a development entry"
            );
        });
    }

    /// Opening the real store from inside a running Tokio runtime never panics.
    ///
    /// The Linux Secret Service store drives a runtime of its own, so opening it
    /// on an async worker used to panic with "Cannot start a runtime from within
    /// a runtime", before any window was shown. A machine with an unlocked
    /// keyring reads a generated connection as no entry. A session with no
    /// Secret Service behind it, with a session bus or without one, reports the
    /// store as unavailable. Both are stores the application starts on; anything
    /// else is a store this did not expect.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_system_store_opens_inside_a_running_tokio_runtime() {
        runtime().block_on(async {
            // The development identifier, so this never reaches a production
            // entry. Neither call leaves the runtime.
            let store = system(DEVELOPMENT).await;
            let read = store.read(&ConnectionId::generate()).await;
            assert!(
                matches!(&read, Ok(None) | Err(SecretStoreError::Unavailable)),
                "the store neither worked nor reported itself unavailable: {read:?}"
            );
        });
    }
}
