use quota_domain::preferences::OverviewMode;
use tauri::test::{mock_builder, mock_context, noop_assets};
use tauri_plugin_store::StoreExt;

use super::*;

#[test]
fn failed_document_writes_restore_cache_before_a_later_plugin_save() {
    let app = mock_builder()
        .plugin(tauri_plugin_store::Builder::default().build())
        .build(mock_context(noop_assets()))
        .unwrap();
    let directory =
        tempfile::tempdir_in(concat!(env!("CARGO_MANIFEST_DIR"), "/../../target")).unwrap();
    let path = directory.path().join("preferences.json");
    let store = app
        .store_builder(&path)
        .disable_auto_save()
        .build()
        .unwrap();
    let documents = PluginDocumentStore::new(store.clone());

    for key in [
        "quota.preferences.presentation.v1",
        "quota.preferences.presentation.recovery.v1",
    ] {
        for previous_mode in [Some(OverviewMode::Tray), Some(OverviewMode::Floating), None] {
            store.clear();
            let previous = previous_mode.map(|overview_mode| {
                serde_json::to_value(PresentationPreferences {
                    overview_mode,
                    revision: 4,
                    ..PresentationPreferences::default()
                })
                .unwrap()
            });
            if let Some(previous) = &previous {
                documents.write(key, previous).unwrap();
            } else {
                store.save().unwrap();
            }
            let requested = serde_json::to_value(PresentationPreferences {
                overview_mode: if previous_mode == Some(OverviewMode::Floating) {
                    OverviewMode::Tray
                } else {
                    OverviewMode::Floating
                },
                revision: 5,
                always_on_top: true,
                ..PresentationPreferences::default()
            })
            .unwrap();

            std::fs::remove_file(&path).unwrap();
            std::fs::create_dir(&path).unwrap();
            assert_eq!(
                documents.write(key, &requested),
                Err(PersistenceError::StoreUnavailable)
            );
            assert_eq!(documents.read(key).unwrap(), previous);

            std::fs::remove_dir(&path).unwrap();
            store.save().unwrap();
            store.clear();
            store.reload_ignore_defaults().unwrap();
            assert_eq!(documents.read(key).unwrap(), previous);
            documents.write(key, &requested).unwrap();
            store.clear();
            store.reload_ignore_defaults().unwrap();
            assert_eq!(documents.read(key).unwrap(), Some(requested));
        }
    }
}
