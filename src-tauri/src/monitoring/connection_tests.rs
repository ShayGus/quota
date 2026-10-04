use quota_contracts::commands::VerifiedCandidate;
use quota_domain::account::{ConnectionState, VerifiedIdentity};
use quota_domain::provider::ProviderId;
use quota_domain::quota::window::SourceKind;

use super::*;

#[test]
fn terminal_failures_report_and_log_a_sanitized_reason() {
    let path =
        std::env::temp_dir().join(format!("quota-attempt-failures-{}.log", std::process::id()));
    let file = std::fs::File::create(&path).expect("a log can be created");
    let log = path.to_string_lossy().into_owned();
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .without_time()
        .with_writer(move || file.try_clone().expect("the log can be shared"))
        .finish();
    let mut details = Vec::new();
    tracing::subscriber::with_default(subscriber, || {
        for code in [
            "browser_sign_in_timeout",
            "browser_sign_in_declined",
            "browser_sign_in_expired",
            "connection_discovery_timeout",
            "connection_read_timeout",
        ] {
            let reported =
                report_attempt_failure(CommandError::Internal { code: code.into() }, &log);
            let serialized = serde_json::to_string(&reported).expect("a wire error");
            let received: CommandError = serde_json::from_str(&serialized).expect("a wire error");
            assert!(matches!(&received, CommandError::ProviderRefused { .. }));
            if let CommandError::ProviderRefused { reason } = received {
                let detail = reason
                    .strip_suffix(&format!(" The log is at {log}"))
                    .expect("the failure must name its log")
                    .to_owned();
                assert!(!detail.is_empty());
                assert_ne!(detail, "Quota hit an internal problem.");
                details.push(detail);
            }
        }
        let reported = report_attempt_failure(
            attempt_error(&ProviderError::InvalidData {
                detail: "https://auth.example.test/device?code=PRIVATE-CODE".into(),
            }),
            &log,
        );
        assert!(matches!(&reported, CommandError::ProviderRefused { .. }));
        if let CommandError::ProviderRefused { reason } = reported {
            assert!(reason.ends_with(&format!("The log is at {log}")));
            assert!(!reason.contains("PRIVATE-CODE"));
        }
    });
    let logged = std::fs::read_to_string(&path).expect("the failures were logged");
    for detail in details {
        assert_eq!(logged.matches(&detail).count(), 1);
    }
    assert!(logged.contains("The provider's answer was incomplete or inconsistent"));
    assert!(!logged.contains("https://"));
    assert!(!logged.contains("PRIVATE-CODE"));
    std::fs::remove_file(path).expect("the log can be removed");
}

#[test]
fn reporting_preserves_typed_recovery_and_cancellation() {
    for error in [
        CommandError::Cancelled,
        CommandError::ReconnectRequired,
        CommandError::PersistenceUnavailable {
            owner: "sqlite".into(),
        },
    ] {
        assert_eq!(report_attempt_failure(error.clone(), "quota.log"), error);
    }
}

/// The progress an attempt reports once it has verified one identity.
fn awaiting_confirmation() -> ConnectionProgress {
    ConnectionProgress::AwaitingConfirmation {
        candidate: VerifiedCandidate {
            provider_id: ProviderId::Fixture,
            nickname: "Personal".into(),
            identity: VerifiedIdentity {
                principal_label: "demo@example.com".into(),
                workspace_label: None,
                plan_label: None,
                source: SourceKind::LocalCapture,
            },
            windows: Vec::new(),
        },
    }
}

#[tokio::test]
async fn acknowledged_cancellation_prevents_all_later_progress() {
    let reporter = AttemptReporter::new();
    assert_eq!(
        reporter.next_revision(&ConnectionProgress::Cancelled).await,
        Some(1)
    );
    for progress in [
        ConnectionProgress::Started,
        ConnectionProgress::AwaitingUser { sign_in: None },
        awaiting_confirmation(),
        ConnectionProgress::Verified {
            state: ConnectionState::Connected,
        },
        ConnectionProgress::Failed {
            error: CommandError::ReconnectRequired,
        },
        ConnectionProgress::Cancelled,
    ] {
        assert_eq!(reporter.next_revision(&progress).await, None);
    }
}

#[tokio::test]
async fn normal_progress_advances_until_the_terminal_result() {
    for terminal in [
        ConnectionProgress::Verified {
            state: ConnectionState::Connected,
        },
        ConnectionProgress::Failed {
            error: CommandError::ReconnectRequired,
        },
    ] {
        let reporter = AttemptReporter::new();
        assert_eq!(
            reporter.next_revision(&ConnectionProgress::Started).await,
            Some(1)
        );
        assert_eq!(
            reporter
                .next_revision(&ConnectionProgress::AwaitingUser { sign_in: None })
                .await,
            Some(2)
        );
        assert_eq!(
            reporter.next_revision(&awaiting_confirmation()).await,
            Some(3)
        );
        assert_eq!(reporter.next_revision(&terminal).await, Some(4));
        assert_eq!(
            reporter.next_revision(&ConnectionProgress::Started).await,
            None
        );
    }
}
