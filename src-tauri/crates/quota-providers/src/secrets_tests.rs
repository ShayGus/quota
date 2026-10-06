//! The system credential store over an in-memory store: identifiers keep
//! builds apart, and a long secret is kept in parts.

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

/// A website session longer than Windows Credential Manager holds in one
/// entry is kept in parts and read back whole; a shorter value written
/// over it leaves no part behind, and deleting removes every part.
#[test]
fn a_long_secret_is_kept_in_parts_and_read_back_whole() {
    runtime().block_on(async {
        let system = memory();
        let store = SystemSecretStore::over(system.clone(), DEVELOPMENT);
        let connection = connection("c-long");
        let long = "abcdefghijklmnopqrstuvwxyz".repeat(120);

        store
            .write(&connection, &Secret::new(long.clone()))
            .await
            .expect("write");
        let read = store
            .read(&connection)
            .await
            .expect("read")
            .expect("an entry");
        assert_eq!(read.expose(), long);
        for part in 1..=3 {
            let piece = get(Some(&system), DEVELOPMENT, &part_name(&connection, part))
                .expect("read a part")
                .expect("the part exists");
            assert!(piece.chars().count() <= PART_LENGTH);
        }

        store
            .write(&connection, &Secret::new("short".to_owned()))
            .await
            .expect("write");
        assert_eq!(
            store
                .read(&connection)
                .await
                .expect("read")
                .expect("an entry")
                .expose(),
            "short"
        );
        assert!(
            get(Some(&system), DEVELOPMENT, &part_name(&connection, 1))
                .expect("read a part")
                .is_none(),
            "a shorter value left a part of the longer one behind"
        );

        store
            .write(&connection, &Secret::new(long))
            .await
            .expect("write");
        store.delete(&connection).await.expect("delete");
        assert!(store.read(&connection).await.expect("read").is_none());
        for part in 1..=3 {
            assert!(
                get(Some(&system), DEVELOPMENT, &part_name(&connection, part))
                    .expect("read a part")
                    .is_none()
            );
        }
    });
}

#[test]
fn parts_split_on_characters_and_name_their_count() {
    let value = "\u{e9}".repeat(PART_LENGTH + 1);
    let parts = split_parts(&value);
    assert_eq!(parts.len(), 2);
    assert_eq!(parts.concat(), value);
    assert_eq!(part_count(&format!("{PARTS_MARK}2")), Some(2));
    assert_eq!(part_count("sk-or-v1-abc"), None);
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
