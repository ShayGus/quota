//! Operational preferences survive `SQLite` round trips without default replacement.

#![expect(
    clippy::unwrap_used,
    reason = "the fixture is local and a failed setup must fail the test"
)]

mod support;

use quota_core::ports::{MonitoringRepository, OperationalPreferencesRepository};
use quota_domain::polling::{FixedIntervalPolicy, PollingStrategy, ProviderPollingPolicy};
use quota_domain::preferences::{
    NotificationPolicy, NotificationThresholds, OperationalPreferences,
    OperationalPrivacyPreferences, QuietHours,
};
use quota_domain::provider::ProviderId;
use quota_domain::snapshot::MonitoringState;
use quota_persistence::ports::{
    SqliteMonitoringPortAdapter, SqliteOperationalPreferencesPortAdapter,
};
use quota_persistence::sqlite::SqliteRepositories;
use support::TempDir;

#[tokio::test]
async fn saved_operational_policies_round_trip_and_keep_monitoring_state() {
    let directory = TempDir::new("operational-preferences");
    let repositories = SqliteRepositories::new(support::migrated(&directory).await);
    let monitoring = SqliteMonitoringPortAdapter::new(repositories.clone());
    let preferences = SqliteOperationalPreferencesPortAdapter::new(repositories);
    monitoring
        .save_monitoring_state(&MonitoringState::Paused)
        .await
        .unwrap();

    let saved = OperationalPreferences {
        revision: 19,
        notifications: NotificationPolicy {
            enabled: false,
            thresholds: NotificationThresholds {
                low_percent: 31.5,
                critical_percent: 7.25,
                hysteresis_percent: 2.5,
            },
            quiet_hours: QuietHours::DailyUtc {
                from_minute: 75,
                to_minute: 405,
            },
            recovery_enabled: false,
        },
        privacy: OperationalPrivacyPreferences {
            retain_history: false,
            export_identities: true,
        },
        polling: vec![ProviderPollingPolicy {
            provider_id: ProviderId::Codex,
            strategy: PollingStrategy::FixedInterval(FixedIntervalPolicy {
                visible_seconds: 420,
                background_seconds: 900,
                battery_saver_seconds: 1_800,
                minimum_seconds: 300,
            }),
            request_timeout_seconds: 11,
            helper_timeout_seconds: 14,
            backoff_minutes: vec![2, 7, 18],
            max_concurrent_remote_reads: 1,
            version: 4,
        }],
    };

    assert_eq!(preferences.save(&saved).await.unwrap(), saved);
    assert_eq!(preferences.load().await.unwrap(), saved);
    assert_eq!(
        monitoring.load_monitoring_state().await.unwrap(),
        MonitoringState::Paused
    );
}
