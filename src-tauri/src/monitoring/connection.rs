//! Cancellable connection discovery and verification for candidate review.

use std::sync::Arc;

use quota_contracts::CommandError;
use quota_contracts::commands::BeginConnectionRequest;
use quota_contracts::events::{ConnectionProgress, ConnectionProgressChangedPayload};
use quota_core::clock::Clock;
use quota_core::ports::{ProviderAdapter, ProviderError, Secret};
use quota_domain::ids::ConnectionAttemptId;
use tauri_specta::Event;
use tokio::sync::watch;

use super::confirm;
use super::{MonitoringRuntime, REMOTE_TIMEOUT, RuntimeState};

/// Emits one attempt's progress under a revision that only moves forward.
///
/// The renderer accepts only a strictly newer revision for an attempt, so the
/// number comes from one counter rather than being spelled out at each call
/// site. A terminal result then never reuses a revision an earlier progress
/// value already used, which is what let a real failure be dropped as stale.
#[derive(Debug)]
pub(super) struct AttemptReporter {
    revision: tokio::sync::Mutex<Option<u32>>,
}

impl AttemptReporter {
    /// A reporter whose first revision is one, matching the first event.
    pub(super) fn new() -> Self {
        Self {
            revision: tokio::sync::Mutex::new(Some(1)),
        }
    }

    /// Emits one progress value under a fresh revision.
    pub(super) async fn emit(
        &self,
        state: &RuntimeState,
        attempt_id: &ConnectionAttemptId,
        progress: ConnectionProgress,
    ) {
        let Some(revision) = self.next_revision(&progress).await else {
            return;
        };
        let progress = match progress {
            ConnectionProgress::Failed { error } => ConnectionProgress::Failed {
                error: report_attempt_failure(error, &crate::file_log::location(&state.app)),
            },
            other => other,
        };
        let app_instance_id = state.snapshots.lock().await.app_instance_id().clone();
        let event =
            crate::ipc::events::ConnectionProgressChanged(ConnectionProgressChangedPayload {
                app_instance_id,
                attempt_id: attempt_id.clone(),
                attempt_revision: revision,
                progress,
            });
        if event.emit_to(&state.app, "settings").is_err() {
            tracing::warn!("connection progress was not delivered to the settings window");
        }
    }
    async fn next_revision(&self, progress: &ConnectionProgress) -> Option<u32> {
        let mut next_revision = self.revision.lock().await;
        let revision = (*next_revision)?;
        *next_revision = match progress {
            ConnectionProgress::Started
            | ConnectionProgress::AwaitingUser { .. }
            | ConnectionProgress::AwaitingConfirmation { .. } => Some(revision + 1),
            _ => None,
        };
        Some(revision)
    }
}

pub(super) async fn run_connection_attempt(
    runtime: MonitoringRuntime,
    adapter: Arc<dyn ProviderAdapter>,
    request: BeginConnectionRequest,
    attempt_id: ConnectionAttemptId,
    mut cancelled: watch::Receiver<bool>,
    reporter: Arc<AttemptReporter>,
) -> Result<(), CommandError> {
    let Some(request) = super::device::sign_in(
        &runtime,
        &adapter,
        request,
        &attempt_id,
        &mut cancelled,
        &reporter,
    )
    .await?
    else {
        return Ok(());
    };
    let secret = super::credentials::supplied(&adapter, &request)?;
    let Some(candidates) = discover_connection_candidates(
        &runtime,
        &adapter,
        &request,
        &attempt_id,
        &mut cancelled,
        &reporter,
    )
    .await?
    else {
        return Ok(());
    };
    if candidates.is_empty() {
        return Err(CommandError::UnsupportedMethod {
            requested: request
                .profile_label
                .unwrap_or_else(|| "local credential".into()),
        });
    }

    // One attempt offers one candidate. An adapter that reports several
    // accounts is connected one attempt at a time, because the person confirms
    // one identity per attempt and the wizard shows one.
    let candidate = candidates.into_iter().next();
    if *cancelled.borrow() {
        return Ok(());
    }
    let Some(candidate) = candidate else {
        return Ok(());
    };
    let ids = confirm::candidate_binding(&adapter, &candidate);
    let Some((read, timestamps)) = read_candidate_quota(
        &runtime,
        &adapter,
        &ids.binding,
        secret.as_ref(),
        &attempt_id,
        &mut cancelled,
    )
    .await?
    else {
        return Ok(());
    };
    let pending = confirm::PendingConnection::new(
        candidate,
        request,
        ids,
        read,
        timestamps,
        Arc::clone(&reporter),
    );
    let verified = confirm::hold_candidate(&runtime, &attempt_id, pending, &cancelled).await?;
    reporter
        .emit(
            &runtime.state,
            &attempt_id,
            ConnectionProgress::AwaitingConfirmation {
                candidate: verified,
            },
        )
        .await;
    Ok(())
}

/// Discovers locally visible accounts, filtered to the requested profile, or
/// the account a supplied credential signs in.
///
/// Returns `None` when cancellation wins the discovery race.
async fn discover_connection_candidates(
    runtime: &MonitoringRuntime,
    adapter: &Arc<dyn ProviderAdapter>,
    request: &BeginConnectionRequest,
    attempt_id: &ConnectionAttemptId,
    cancelled: &mut watch::Receiver<bool>,
    reporter: &AttemptReporter,
) -> Result<Option<Vec<quota_core::ports::DiscoveredAccount>>, CommandError> {
    if *cancelled.borrow() {
        return Ok(None);
    }
    reporter
        .emit(&runtime.state, attempt_id, ConnectionProgress::Started)
        .await;
    let secret = super::credentials::supplied(adapter, request)?;
    let discovered = tokio::select! {
        _ = cancelled.changed() => return Ok(None),
        result = tokio::time::timeout(REMOTE_TIMEOUT, match secret {
            Some(ref secret) => adapter.discover_with(secret),
            None => adapter.discover_accounts(),
        }) => {
            result
                .map_err(|_| CommandError::Internal {
                    code: "connection_discovery_timeout".into(),
                })?
                .map_err(attempt_error)?
        }
    };
    let candidates: Vec<_> = discovered
        .into_iter()
        .filter(|candidate| {
            request
                .profile_label
                .as_ref()
                .is_none_or(|profile| candidate.profile_label.as_ref() == Some(profile))
        })
        .collect();
    Ok(Some(candidates))
}

/// Reads one candidate binding with a timeout.
///
/// Returns `None` when cancellation wins the permit or read race.
async fn read_candidate_quota(
    runtime: &MonitoringRuntime,
    adapter: &Arc<dyn ProviderAdapter>,
    binding: &quota_core::ports::ConnectionBinding,
    secret: Option<&Secret>,
    attempt_id: &ConnectionAttemptId,
    cancelled: &mut watch::Receiver<bool>,
) -> Result<Option<(quota_core::ports::QuotaRead, confirm::ReadTimestamps)>, CommandError> {
    let permit = tokio::select! {
        _ = cancelled.changed() => return Ok(None),
        permit = runtime.state.permits.clone().acquire_owned() => {
            permit.map_err(|_| CommandError::Cancelled)?
        }
    };
    let dispatched_at = runtime.state.clock.now();
    let response = tokio::select! {
        _ = cancelled.changed() => {
            drop(permit);
            return Ok(None);
        }
        result = tokio::time::timeout(
            REMOTE_TIMEOUT,
            {
                let context = quota_core::ports::ReadContext {
                    attempt_id: attempt_id.clone(),
                    deadline: Some(dispatched_at + chrono::Duration::seconds(10)),
                };
                match secret {
                    Some(secret) => adapter.read_with(binding, context, secret),
                    None => adapter.read_quota(binding, context),
                }
            },
        ) => result
            .map_err(|_| CommandError::Internal {
                code: "connection_read_timeout".into(),
            })?
            .map_err(attempt_error)?,
    };
    let completed_at = runtime.state.clock.now();
    drop(permit);
    let read = response
        .read()
        .cloned()
        .ok_or_else(|| CommandError::Internal {
            code: "connection_read_unavailable".into(),
        })?;
    Ok(Some((
        read,
        confirm::ReadTimestamps {
            dispatched_at,
            completed_at,
        },
    )))
}

/// The failure a first attempt reports, before any account exists.
///
/// Nothing has been added, so nothing can be reconnected: reporting the
/// provider's refusal as [`CommandError::ReconnectRequired`] would send the
/// person back to the sign-in they just attempted, and as a window-permission
/// error the words would have nothing to do with what happened.
pub(super) fn attempt_error(error: ProviderError) -> CommandError {
    let detail = match error {
        ProviderError::Cancelled => return CommandError::Cancelled,
        ProviderError::Authentication => {
            "The provider did not accept this sign-in, so nothing was added"
        }
        ProviderError::Authorization => "The provider declined this account, so nothing was added",
        ProviderError::InvalidData { .. } => {
            "The provider's answer was incomplete or inconsistent, so nothing was added"
        }
        ProviderError::UnsupportedSchema { .. } => {
            "The provider answered in a format this version of Quota cannot read yet, \
             so nothing was added"
        }
        ProviderError::Transient { .. } => {
            "The provider reported a temporary failure, so nothing was added. Try again"
        }
        ProviderError::RateLimited { .. } => {
            "The provider is asking for less traffic, so nothing was added. Try again shortly"
        }
    };
    CommandError::ProviderRefused {
        reason: detail.into(),
    }
}

fn report_attempt_failure(error: CommandError, log: &str) -> CommandError {
    let detail = match error {
        CommandError::Internal { code } => match code.as_str() {
            "browser_sign_in_timeout" => "The provider did not answer the sign-in in time. Try again.",
            "browser_sign_in_declined" => "The sign-in was declined on the provider's page. Nothing was added.",
            "browser_sign_in_expired" => "The sign-in code expired before it was entered. Start again for a new code.",
            "connection_discovery_timeout" => "The provider did not finish discovering the account in time. Nothing was added. Try again.",
            "connection_read_timeout" => "The provider did not finish verifying the quota in time. Nothing was added. Try again.",
            "unsupported_schema" => "The provider answered in a format this version of Quota cannot read yet.",
            "invalid_data" => "The provider's answer was incomplete or inconsistent, so no reading was taken.",
            "credential_store_refused" => "The operating system key store refused to keep the credential.",
            _ => "Quota hit an internal problem.",
        }.to_owned(),
        CommandError::ProviderRefused { reason } => format!("{reason}."),
        other => return other,
    };
    tracing::warn!(reason = detail, "connection attempt failed");
    CommandError::ProviderRefused {
        reason: format!("{detail} The log is at {log}"),
    }
}

pub(super) fn provider_command_error(error: ProviderError) -> CommandError {
    match error {
        ProviderError::Authentication => CommandError::ReconnectRequired,
        ProviderError::Authorization => CommandError::PermissionDenied {
            window_label: "provider".into(),
        },
        other => CommandError::Internal {
            code: other.diagnostic_code().to_owned(),
        },
    }
}

#[cfg(test)]
mod tests {
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
                let received: CommandError =
                    serde_json::from_str(&serialized).expect("a wire error");
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
                attempt_error(ProviderError::InvalidData {
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
}
