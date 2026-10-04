//! The behaviour every credential store keeps, run once for both adapters.
//!
//! Integration tests are compiled without `cfg(test)`, so the crate-wide
//! test-context allowance in `clippy.toml` does not reach this file.
#![expect(
    clippy::tests_outside_test_module,
    reason = "an integration test binary holds nothing but tests and compiles without cfg(test)"
)]
#![expect(
    clippy::expect_used,
    reason = "a contract assertion must fail loudly and read as prose"
)]

use quota_core::ports::{Secret, SecretStore};
use quota_domain::ids::ConnectionId;
use quota_providers::secrets::MemorySecretStore;

/// The identifier `src-tauri/tauri.conf.json` gives a production build and the
/// one its development overlay gives a development build.
const PRODUCTION: &str = "app.quota.monitor";
const DEVELOPMENT: &str = "app.quota.monitor.dev";

/// Every behavioural guarantee a store keyed by connection keeps.
async fn assert_secret_store_contract(store: &dyn SecretStore) {
    assert_read_write_delete(store).await;
    assert_connections_are_isolated(store).await;
}

/// A read-miss is `None`, and a write, a read, and a delete round-trip.
///
/// Removing an entry that is already gone is a success, not a failure.
async fn assert_read_write_delete(store: &dyn SecretStore) {
    let connection = ConnectionId::generate();
    assert!(
        store.read(&connection).await.expect("read").is_none(),
        "a connection with no secret reads as none"
    );

    store
        .write(&connection, &Secret::new("first".to_owned()))
        .await
        .expect("write");
    assert_eq!(
        store
            .read(&connection)
            .await
            .expect("read")
            .expect("a secret")
            .expose(),
        "first"
    );

    store.delete(&connection).await.expect("delete");
    assert!(store.read(&connection).await.expect("read").is_none());
    store.delete(&connection).await.expect("a second delete");
}

/// One connection never reads, or loses, another connection's secret.
async fn assert_connections_are_isolated(store: &dyn SecretStore) {
    let first = ConnectionId::generate();
    let second = ConnectionId::generate();
    store
        .write(&first, &Secret::new("first".to_owned()))
        .await
        .expect("write");
    store
        .write(&second, &Secret::new("second".to_owned()))
        .await
        .expect("write");
    assert_eq!(
        store
            .read(&first)
            .await
            .expect("read")
            .expect("a secret")
            .expose(),
        "first",
        "one connection keeps its own secret"
    );
    store.delete(&first).await.expect("delete");
    assert!(store.read(&first).await.expect("read").is_none());
    assert!(store.read(&second).await.expect("read").is_some());
    store.delete(&second).await.expect("cleanup");
}

/// One application identifier never reaches another identifier's entry.
///
/// Only a store that carries an identifier can keep this guarantee, so the
/// suite asks for it on the two adapters that do.
async fn assert_identifier_isolation(development: &dyn SecretStore, production: &dyn SecretStore) {
    let connection = ConnectionId::generate();
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
    development.delete(&connection).await.expect("cleanup");
}

/// The in-memory adapter keeps every guarantee, with no system store.
#[tokio::test]
async fn the_memory_store_keeps_the_contract() {
    assert_secret_store_contract(&MemorySecretStore::new()).await;
}

/// The host adapter keeps every guarantee, with the identifiers the
/// application builds with. It touches the person's real credential store, so
/// it runs only when asked for:
/// `cargo test -p quota-providers -- --ignored system_store`.
///
/// A host with no usable store, such as a WSL session with no Secret Service,
/// reports itself unavailable and is reported as such here; it is a store the
/// application starts on, not a contract violation.
#[tokio::test]
#[ignore = "uses the real system credential store"]
async fn the_system_store_keeps_the_contract() {
    let development = quota_providers::secrets::system(DEVELOPMENT).await;
    let probe = ConnectionId::generate();
    if let Err(error) = development.read(&probe).await {
        assert!(
            matches!(error, quota_core::ports::SecretStoreError::Unavailable),
            "the host store neither worked nor reported itself unavailable: {error:?}"
        );
        eprintln!("this host has no usable credential store; the contract cannot run here");
        return;
    }
    assert_secret_store_contract(development.as_ref()).await;
    let production = quota_providers::secrets::system(PRODUCTION).await;
    assert_identifier_isolation(development.as_ref(), production.as_ref()).await;
}
