use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use quota_core::ports::RepositoryError;
use quota_domain::preferences::{
    IndicatorStyle, LaunchBehavior, OperationalPreferences, OverviewMode, PresentationPreferences,
    PrivacyAliasMode, Theme,
};
use quota_persistence::ports::PresentationPreferencesPort;
use quota_persistence::store::document::PreferenceDocumentStore;
use quota_persistence::{PersistenceError, PersistenceResult, PresentationPreferencesCodec};
use serde_json::Value;

use super::*;

#[derive(Clone, Default)]
struct DocumentStore {
    values: Arc<Mutex<HashMap<String, Value>>>,
    fail_writes: Arc<AtomicBool>,
}

impl PreferenceDocumentStore for DocumentStore {
    fn read(&self, key: &str) -> PersistenceResult<Option<Value>> {
        let values = self.values.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(values.get(key).cloned())
    }

    fn write(&self, key: &str, value: &Value) -> PersistenceResult<()> {
        if self.fail_writes.load(Ordering::Relaxed) {
            return Err(PersistenceError::StoreUnavailable);
        }
        let mut values = self.values.lock().unwrap_or_else(PoisonError::into_inner);
        values.insert(key.into(), value.clone());
        Ok(())
    }
}

struct OperationalStore {
    value: Mutex<OperationalPreferences>,
    reads: AtomicUsize,
    writes: AtomicUsize,
    fail_load: bool,
    fail_save: bool,
}

type RepositoryFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, RepositoryError>> + Send + 'a>>;

impl OperationalPreferencesRepository for OperationalStore {
    fn load<'life0, 'async_trait>(
        &'life0 self,
    ) -> RepositoryFuture<'async_trait, OperationalPreferences>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            self.reads.fetch_add(1, Ordering::Relaxed);
            if self.fail_load {
                Err(RepositoryError::new("sqlite", "unavailable"))
            } else {
                Ok(self.value.lock().unwrap().clone())
            }
        })
    }

    fn save<'life0, 'life1, 'async_trait>(
        &'life0 self,
        preferences: &'life1 OperationalPreferences,
    ) -> RepositoryFuture<'async_trait, OperationalPreferences>
    where
        'life0: 'async_trait,
        'life1: 'async_trait,
        Self: 'async_trait,
    {
        Box::pin(async move {
            self.writes.fetch_add(1, Ordering::Relaxed);
            if self.fail_save {
                Err(RepositoryError::new("sqlite", "unavailable"))
            } else {
                *self.value.lock().unwrap() = preferences.clone();
                Ok(preferences.clone())
            }
        })
    }
}

fn setup(
    mode: OverviewMode,
) -> (
    DocumentStore,
    PresentationPreferencesPort<DocumentStore>,
    OperationalStore,
    Preferences,
) {
    let document = DocumentStore::default();
    let codec = PresentationPreferencesCodec::new(document.clone());
    let mut presentation = PresentationPreferences {
        overview_mode: mode,
        revision: 4,
        ..PresentationPreferences::default()
    };
    codec.save(&mut presentation).unwrap();
    let operational = OperationalPreferences {
        revision: 8,
        ..OperationalPreferences::default()
    };
    let confirmed = crate::bootstrap_helpers::from_persisted(&presentation, &operational);
    (
        document,
        PresentationPreferencesPort::new(codec),
        OperationalStore {
            value: Mutex::new(operational),
            reads: AtomicUsize::new(0),
            writes: AtomicUsize::new(0),
            fail_load: false,
            fail_save: false,
        },
        confirmed,
    )
}

#[tokio::test]
async fn mode_changes_use_only_the_presentation_owner_even_when_sqlite_cannot_save() {
    for mode in [OverviewMode::Floating, OverviewMode::Tray] {
        let previous = if mode == OverviewMode::Floating {
            OverviewMode::Tray
        } else {
            OverviewMode::Floating
        };
        for fail_load in [false, true] {
            let (document, presentation, mut operational, confirmed) = setup(previous);
            operational.fail_load = fail_load;
            operational.fail_save = true;
            let mut requested = confirmed.clone();
            requested.overview_mode = mode;
            let saved =
                persist_preference_owners(&presentation, &operational, &requested, &confirmed)
                    .await
                    .unwrap();
            assert_eq!(saved.overview_mode, mode);
            assert!(saved.revision > confirmed.revision);
            assert_eq!(operational.reads.load(Ordering::Relaxed), 0);
            assert_eq!(operational.writes.load(Ordering::Relaxed), 0);
            let restarted = crate::bootstrap_helpers::from_persisted(
                &PresentationPreferencesCodec::new(document).load().unwrap(),
                &operational.value.lock().unwrap(),
            );
            assert_eq!(restarted, saved);
        }
    }
}

#[tokio::test]
async fn failed_presentation_saves_preserve_the_mode_restored_at_restart() {
    for previous in [OverviewMode::Tray, OverviewMode::Floating] {
        let (document, presentation, operational, confirmed) = setup(previous);
        let before = presentation.load().await.unwrap();
        document.fail_writes.store(true, Ordering::Relaxed);
        let mut requested = confirmed.clone();
        requested.overview_mode = if previous == OverviewMode::Tray {
            OverviewMode::Floating
        } else {
            OverviewMode::Tray
        };
        persist_preference_owners(&presentation, &operational, &requested, &confirmed)
            .await
            .unwrap_err();
        assert_eq!(
            PresentationPreferencesCodec::new(document).load().unwrap(),
            before
        );
        assert_eq!(operational.reads.load(Ordering::Relaxed), 0);
        assert_eq!(operational.writes.load(Ordering::Relaxed), 0);
    }
}

#[tokio::test]
async fn every_presentation_setting_preserves_operational_values_and_revisions() {
    let (document, presentation, mut operational, confirmed) = setup(OverviewMode::Tray);
    operational.fail_save = true;
    let mut requested = confirmed.clone();
    requested.theme = Theme::Dark;
    requested.indicator_style = IndicatorStyle::Bar;
    requested.always_on_top = true;
    requested.launch_behavior = LaunchBehavior::RestoreLastMode;
    requested.privacy.alias_mode = PrivacyAliasMode::StableAliases;
    requested.reduce_motion = true;
    let saved = persist_preference_owners(&presentation, &operational, &requested, &confirmed)
        .await
        .unwrap();
    requested.revision = saved.revision;
    assert_eq!(saved, requested);
    let restarted = crate::bootstrap_helpers::from_persisted(
        &PresentationPreferencesCodec::new(document).load().unwrap(),
        &operational.value.lock().unwrap(),
    );
    assert_eq!(restarted, saved);
    assert_eq!(operational.reads.load(Ordering::Relaxed), 0);
    assert_eq!(operational.writes.load(Ordering::Relaxed), 0);
}

#[tokio::test]
async fn operational_changes_still_save_and_confirm_their_owner() {
    let (_, presentation, operational, confirmed) = setup(OverviewMode::Floating);
    let mut requested = confirmed.clone();
    requested.notifications.thresholds.alerts.exhausted = false;
    requested.privacy.retain_history = false;
    let saved = persist_preference_owners(&presentation, &operational, &requested, &confirmed)
        .await
        .unwrap();
    requested.revision = saved.revision;
    assert_eq!(saved, requested);
    assert_eq!(operational.reads.load(Ordering::Relaxed), 1);
    assert_eq!(operational.writes.load(Ordering::Relaxed), 1);
    assert_eq!(
        *operational.value.lock().unwrap(),
        crate::bootstrap_helpers::to_operational(&saved, saved.revision),
    );
}
