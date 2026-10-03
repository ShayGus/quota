//! The operating system's credential store, behind Quota's secret port.
//!
//! A credential Quota owns itself, such as an API key the person pasted, is
//! kept here and nowhere else: Windows Credential Manager, the Secret Service
//! on Linux, or the login Keychain on macOS. Each entry is named for the
//! connection it signs in, under Quota's application identifier.

use std::fmt::{self, Debug, Formatter};
use std::sync::Arc;

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
/// On Linux, call this outside an async runtime; async callers should use
/// [`system_off_the_runtime`].
#[must_use]
pub fn system(service: &str) -> Arc<dyn SecretStore> {
    match SystemSecretStore::open(service) {
        Ok(store) => Arc::new(store),
        Err(_) => unavailable(),
    }
}

/// This system's credential store, opened on a blocking worker.
///
/// The Linux Secret Service store connects through a blocking API that drives
/// a Tokio runtime of its own, so opening it on an async worker panics with
/// "Cannot start a runtime from within a runtime" and takes the whole backend
/// down with it. This runs the same open off the caller's runtime. If opening
/// fails, for example because there is no store or session bus, it returns a
/// store that reports every operation unavailable, as does a worker that
/// cannot be joined. Credential reads, writes, and deletes remain synchronous;
/// async callers must also run those operations on blocking workers.
///
/// `service` is the application identifier the build runs with, exactly as in
/// [`system`].
#[must_use]
pub async fn system_off_the_runtime(service: &str) -> Arc<dyn SecretStore> {
    let service = service.to_owned();
    match tokio::task::spawn_blocking(move || system(&service)).await {
        Ok(store) => store,
        Err(_) => unavailable(),
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

impl SecretStore for Unavailable {
    fn read(&self, _connection: &ConnectionId) -> Result<Option<Secret>, SecretStoreError> {
        Err(SecretStoreError::Unavailable)
    }

    fn write(&self, _connection: &ConnectionId, _secret: &Secret) -> Result<(), SecretStoreError> {
        Err(SecretStoreError::Unavailable)
    }

    fn delete(&self, _connection: &ConnectionId) -> Result<(), SecretStoreError> {
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
        Ok(Self::over(platform_store()?, service))
    }

    /// Wraps one credential store, so tests can use an in-memory one.
    #[must_use]
    pub fn over(store: Arc<CredentialStore>, service: &str) -> Self {
        Self {
            store: Some(store),
            service: service.to_owned(),
        }
    }

    fn entry(&self, connection: &ConnectionId) -> Result<keyring_core::Entry, SecretStoreError> {
        self.store
            .as_ref()
            .ok_or(SecretStoreError::Unavailable)?
            .build(&self.service, connection.as_str(), None)
            .map_err(|error| classify(&error))
    }
}

#[cfg(target_os = "linux")]
impl Drop for SystemSecretStore {
    fn drop(&mut self) {
        // Secret Service destruction can drive its own runtime. Release this
        // owner's store reference on a thread without a Tokio context, including
        // startup-error cleanup before managed state retains the registry. The
        // scope waits for that release; the regression test is
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

impl SecretStore for SystemSecretStore {
    fn read(&self, connection: &ConnectionId) -> Result<Option<Secret>, SecretStoreError> {
        match self.entry(connection)?.get_password() {
            Ok(value) => Ok(Some(Secret::new(value))),
            Err(Error::NoEntry) => Ok(None),
            Err(error) => Err(classify(&error)),
        }
    }

    fn write(&self, connection: &ConnectionId, secret: &Secret) -> Result<(), SecretStoreError> {
        self.entry(connection)?
            .set_password(secret.expose())
            .map_err(|error| classify(&error))
    }

    fn delete(&self, connection: &ConnectionId) -> Result<(), SecretStoreError> {
        match self.entry(connection)?.delete_credential() {
            Ok(()) | Err(Error::NoEntry) => Ok(()),
            Err(error) => Err(classify(&error)),
        }
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

#[cfg(windows)]
fn platform_store() -> Result<Arc<CredentialStore>, SecretStoreError> {
    let store: Arc<CredentialStore> =
        windows_native_keyring_store::Store::new().map_err(|_| SecretStoreError::Unavailable)?;
    Ok(store)
}

#[cfg(target_os = "linux")]
fn platform_store() -> Result<Arc<CredentialStore>, SecretStoreError> {
    let store: Arc<CredentialStore> = zbus_secret_service_keyring_store::Store::new()
        .map_err(|_| SecretStoreError::Unavailable)?;
    Ok(store)
}

#[cfg(target_os = "macos")]
fn platform_store() -> Result<Arc<CredentialStore>, SecretStoreError> {
    let store: Arc<CredentialStore> = apple_native_keyring_store::keychain::Store::new()
        .map_err(|_| SecretStoreError::Unavailable)?;
    Ok(store)
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
fn platform_store() -> Result<Arc<CredentialStore>, SecretStoreError> {
    Err(SecretStoreError::Unavailable)
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

    fn store() -> SystemSecretStore {
        SystemSecretStore::over(memory(), PRODUCTION)
    }

    fn connection(id: &str) -> ConnectionId {
        ConnectionId::new(id).expect("a connection id")
    }

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

    #[cfg(all(target_os = "linux", feature = "test-fixtures"))]
    #[test]
    fn startup_failure_drops_both_registries_outside_the_runtime() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("a runtime");

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
                    .expect("write");
                assert_eq!(
                    secrets
                        .read(&connection)
                        .expect("read")
                        .expect("a secret")
                        .expose(),
                    "test-key"
                );
                secrets.delete(&connection).expect("delete");
                assert!(secrets.read(&connection).expect("read").is_none());
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

    #[test]
    fn a_connection_without_a_secret_reads_as_none() {
        assert!(store().read(&connection("c1")).expect("read").is_none());
    }

    #[test]
    fn each_connection_keeps_its_own_secret_until_it_is_deleted() {
        let store = store();
        store
            .write(&connection("c1"), &Secret::new("first".to_owned()))
            .expect("write");
        store
            .write(&connection("c2"), &Secret::new("second".to_owned()))
            .expect("write");
        let read = store
            .read(&connection("c1"))
            .expect("read")
            .expect("a secret");
        assert_eq!(read.expose(), "first");

        store.delete(&connection("c1")).expect("delete");
        assert!(store.read(&connection("c1")).expect("read").is_none());
        assert!(store.read(&connection("c2")).expect("read").is_some());
        // Removing what is already gone is not a failure.
        store.delete(&connection("c1")).expect("a second delete");
    }

    #[test]
    fn a_development_build_never_reaches_a_production_entry() {
        let system = memory();
        let development = SystemSecretStore::over(system.clone(), DEVELOPMENT);
        let production = SystemSecretStore::over(system, PRODUCTION);
        let connection = connection("c1");

        development
            .write(&connection, &Secret::new("development".to_owned()))
            .expect("write");

        assert!(
            production.read(&connection).expect("read").is_none(),
            "the production identifier read a development entry"
        );
        production.delete(&connection).expect("delete");
        assert!(
            development.read(&connection).expect("read").is_some(),
            "the production identifier removed a development entry"
        );
    }

    /// Writes, reads and removes one throwaway entry in this system's real
    /// store. It touches the person's credential store, so it runs only when
    /// asked for: `cargo test -p quota-providers -- --ignored system_store`.
    #[test]
    #[ignore = "uses the real system credential store"]
    fn the_system_store_keeps_and_removes_an_entry() {
        let store =
            SystemSecretStore::open(PRODUCTION).expect("this system has a credential store");
        let connection = ConnectionId::generate();
        store
            .write(&connection, &Secret::new("round-trip".to_owned()))
            .expect("write");
        let read = store.read(&connection).expect("read").expect("a secret");
        assert_eq!(read.expose(), "round-trip");
        store.delete(&connection).expect("delete");
        assert!(store.read(&connection).expect("read").is_none());
    }
}

/// This system's real Linux store, opened the way `initialize_backend` opens
/// it: from inside the running Tokio runtime the desktop host spawns.
#[cfg(test)]
mod linux_system_store {
    #[cfg(target_os = "linux")]
    use super::*;

    /// Opening the real store from inside a running Tokio runtime never panics.
    ///
    /// The Linux Secret Service store drives a Tokio runtime of its own, so
    /// this used to panic with "Cannot start a runtime from within a runtime",
    /// before any window was shown. A machine with an unlocked keyring reads a
    /// generated connection as no entry. A session with no Secret Service
    /// behind it, with a session bus or without one, reports the store as
    /// unavailable. Both are stores the application starts on; anything else is
    /// a store this did not expect.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_system_store_opens_inside_a_running_tokio_runtime() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("a runtime");

        runtime.block_on(async {
            // The development identifier, so this never reaches a production entry.
            let store = system_off_the_runtime("app.quota.monitor.dev").await;
            let read = tokio::task::spawn_blocking(move || store.read(&ConnectionId::generate()))
                .await
                .expect("the credential read worker completes");
            assert!(
                matches!(&read, Ok(None) | Err(SecretStoreError::Unavailable)),
                "the store neither worked nor reported itself unavailable: {read:?}"
            );
        });
    }
}
