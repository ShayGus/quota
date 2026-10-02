//! The versioned preference document, through the codec.
//!
//! Fixtures use `unwrap` for readability; this file is not `#[cfg(test)]`, so
//! the workspace's test-mode allowance does not reach it.

#![expect(
    clippy::tests_outside_test_module,
    reason = "an integration test binary holds nothing but tests and compiles without cfg(test)"
)]
#![expect(
    clippy::unwrap_used,
    reason = "test fixtures assert the setup they build, so a broken fixture must fail loudly"
)]

mod support;

use quota_domain::preferences::{
    IndicatorStyle, LaunchBehavior, OverviewMode, PREFERENCES_SCHEMA_VERSION,
    PresentationPreferences, PrivacyAliasMode, Theme,
};
use quota_persistence::{PersistenceError, PresentationPreferencesCodec};
use support::{FailingStore, LIVE_KEY, MemoryStore, RECOVERY_KEY};

#[test]
fn the_store_round_trips_a_preferences_document() {
    let store = MemoryStore::default();
    let codec = PresentationPreferencesCodec::new(store.clone());

    assert_eq!(codec.validate().unwrap(), None);
    let defaults = codec.load().unwrap();
    assert_eq!(defaults, PresentationPreferences::default());
    assert!(
        !defaults.always_on_top,
        "an absent document must not opt the user in"
    );

    let mut updated = defaults.clone();
    updated.theme = Theme::Dark;
    updated.indicator_style = IndicatorStyle::Bar;
    updated.overview_mode = OverviewMode::Tray;
    updated.always_on_top = true;
    updated.launch_behavior = LaunchBehavior::RestoreLastMode;
    updated.privacy_alias_mode = PrivacyAliasMode::StableAliases;
    updated.reduce_motion = true;
    assert_eq!(codec.save(&mut updated).unwrap(), 1);
    assert_eq!(updated.revision, 1);
    assert_eq!(codec.load().unwrap(), updated);

    // A second save advances the revision again, and the caller's value carries
    // the same revision that was stored.
    assert_eq!(codec.save(&mut updated).unwrap(), 2);
    assert_eq!(updated.revision, 2);
    assert_eq!(codec.load().unwrap().revision, 2);

    let keys = store.keys();
    assert_eq!(keys.len(), 2, "the codec owns exactly its own two keys");
    assert!(
        keys.iter()
            .all(|key| key.starts_with("quota.preferences.presentation."))
    );
}

#[test]
fn a_save_preserves_the_document_it_replaces() {
    let store = MemoryStore::default();
    let codec = PresentationPreferencesCodec::new(store.clone());

    let mut first = PresentationPreferences::default();
    codec.save(&mut first).unwrap();
    let before = store.read_external(LIVE_KEY).unwrap();

    let mut second = PresentationPreferences {
        theme: Theme::Dark,
        ..PresentationPreferences::default()
    };
    codec.save(&mut second).unwrap();

    assert_eq!(
        store.read_external(RECOVERY_KEY),
        Some(before),
        "the replaced document must be preserved verbatim"
    );
    assert_eq!(codec.load().unwrap().theme, Theme::Dark);
}

#[test]
fn a_corrupt_document_is_preserved_and_never_opted_in() {
    let store = MemoryStore::default();
    let codec = PresentationPreferencesCodec::new(store.clone());

    // A document that carries a topmost preference but is not a preferences
    // document this build can read.
    let corrupt = serde_json::json!({ "theme": "dark", "always_on_top": true });
    store.write_external(LIVE_KEY, corrupt.clone());

    assert_eq!(
        codec.load().unwrap_err(),
        PersistenceError::StoreDocumentCorrupt
    );
    assert_eq!(
        codec.validate().unwrap_err(),
        PersistenceError::StoreDocumentCorrupt
    );

    // The rejected document is kept, not replaced by defaults.
    assert_eq!(store.read_external(LIVE_KEY), Some(corrupt.clone()));
    assert_eq!(
        store.read_external(RECOVERY_KEY),
        Some(corrupt),
        "the corrupt document must be preserved for recovery"
    );
}

#[test]
fn a_document_saved_with_the_retired_density_field_still_loads() {
    let store = MemoryStore::default();
    let codec = PresentationPreferencesCodec::new(store.clone());

    store.write_external(
        LIVE_KEY,
        serde_json::json!({
            "schema_version": PREFERENCES_SCHEMA_VERSION,
            "revision": 4,
            "theme": "dark",
            "density": "comfortable",
            "indicator_style": "bar",
            "overview_mode": "floating",
            "always_on_top": true,
            "launch_behavior": "restore_last_mode",
            "privacy_alias_mode": "off",
            "reduce_motion": false
        }),
    );

    let loaded = codec.load().unwrap();
    assert_eq!(loaded.theme, Theme::Dark);
    assert_eq!(loaded.indicator_style, IndicatorStyle::Bar);
    assert_eq!(loaded.overview_mode, OverviewMode::Floating);
    assert!(loaded.always_on_top);
    assert_eq!(loaded.launch_behavior, LaunchBehavior::RestoreLastMode);
}

#[test]
fn a_document_with_a_misshaped_field_never_produces_a_topmost_preference() {
    let store = MemoryStore::default();
    let codec = PresentationPreferencesCodec::new(store.clone());

    store.write_external(
        LIVE_KEY,
        serde_json::json!({
            "schema_version": PREFERENCES_SCHEMA_VERSION,
            "revision": 4,
            "theme": "dark",
            "indicator_style": "ring",
            "overview_mode": "floating",
            "always_on_top": true,
            "launch_behavior": 7,
            "privacy_alias_mode": "off",
            "reduce_motion": false
        }),
    );

    assert_eq!(
        codec.load().unwrap_err(),
        PersistenceError::StoreDocumentCorrupt
    );
}

#[test]
fn a_document_from_a_newer_build_is_refused_and_preserved() {
    let store = MemoryStore::default();
    let codec = PresentationPreferencesCodec::new(store.clone());

    let mut newer = support::stored_defaults();
    newer["schema_version"] = serde_json::json!(PREFERENCES_SCHEMA_VERSION + 1);
    store.write_external(LIVE_KEY, newer.clone());

    assert_eq!(
        codec.load().unwrap_err(),
        PersistenceError::StoreDocumentUnsupported {
            found_version: PREFERENCES_SCHEMA_VERSION + 1,
            supported_version: PREFERENCES_SCHEMA_VERSION
        }
    );
    assert_eq!(
        store.read_external(LIVE_KEY),
        Some(newer.clone()),
        "a newer document is never silently overwritten"
    );
    assert_eq!(store.read_external(RECOVERY_KEY), Some(newer));
}

#[test]
fn a_document_with_no_version_is_corrupt_rather_than_unsupported() {
    let store = MemoryStore::default();
    let codec = PresentationPreferencesCodec::new(store.clone());

    store.write_external(LIVE_KEY, serde_json::json!({ "always_on_top": true }));
    assert_eq!(
        codec.load().unwrap_err(),
        PersistenceError::StoreDocumentCorrupt
    );
}

#[test]
fn an_unavailable_store_is_reported_as_such() {
    let codec = PresentationPreferencesCodec::new(FailingStore);
    assert_eq!(
        codec.load().unwrap_err(),
        PersistenceError::StoreUnavailable
    );
    assert_eq!(
        codec.validate().unwrap_err(),
        PersistenceError::StoreUnavailable
    );

    let mut updated = PresentationPreferences::default();
    assert_eq!(
        codec.save(&mut updated).unwrap_err(),
        PersistenceError::StoreUnavailable
    );
}

#[test]
fn a_failed_backup_stops_the_save_and_keeps_the_previous_document() {
    let store = MemoryStore::default();
    let previous = support::stored_defaults();
    store.write_external(LIVE_KEY, previous.clone());

    // The save preserves the existing document before writing the new one.
    // Refusing the very first write therefore refuses the save, and the
    // previous document is untouched.
    store.fail_writes_after(0);
    let mut updated = PresentationPreferences::default();
    assert_eq!(
        store.codec().save(&mut updated).unwrap_err(),
        PersistenceError::BackupFailed
    );
    assert_eq!(store.read_external(LIVE_KEY), Some(previous));
}

#[test]
fn a_save_from_a_corrupt_document_does_not_destroy_the_corrupt_bytes() {
    let store = MemoryStore::default();
    let codec = PresentationPreferencesCodec::new(store.clone());

    let corrupt = serde_json::json!({ "theme": "dark", "always_on_top": true });
    store.write_external(LIVE_KEY, corrupt.clone());

    let mut updated = PresentationPreferences::default();
    codec.save(&mut updated).unwrap();

    assert_eq!(
        store.read_external(RECOVERY_KEY),
        Some(corrupt),
        "the unreadable document must still be recoverable after a later save"
    );
    assert_eq!(codec.load().unwrap(), updated);
}
