//! Following the sign-ins other clients own.
//!
//! An account whose sign-in belongs to another client, such as Claude Code,
//! needs reconnecting whenever that client's sign-in lapses, and is fixed the
//! moment the client renews it. This watches each such sign-in file, so a
//! renewal made anywhere ends the reconnect demand within seconds rather than
//! at the next scheduled read. When the person allowed it, it also asks the
//! owning client to renew an expired sign-in itself; Quota never writes the
//! credential.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use chrono::{DateTime, Utc};
use quota_core::clock::Clock;
use quota_core::ports::{ProviderError, SignInRenewal};
use quota_domain::account::ConnectionState;
use quota_domain::ids::AccountId;
use quota_domain::provider::ProviderId;
use quota_domain::snapshot::MonitoringState;

use super::{MonitoringRuntime, RefreshReason};

/// How often the watched files are looked at. A look is one metadata read.
pub(super) const WATCH_INTERVAL: Duration = Duration::from_secs(10);

/// The shortest gap between two renewals of one provider's sign-in.
const RENEWAL_SPACING: chrono::Duration = chrono::Duration::minutes(15);

/// The gap after a renewal that did not move the sign-in, or could not run:
/// trying again soon would only repeat it.
const FRUITLESS_SPACING: chrono::Duration = chrono::Duration::hours(6);

/// What one watched file looked like: when it was last written, and its size.
type Stamp = Option<(SystemTime, u64)>;

/// What the watcher remembers between looks.
#[derive(Debug, Default)]
pub(super) struct SignInWatch {
    /// The last stamp of each provider's sign-in file.
    stamps: HashMap<ProviderId, Stamp>,
    /// When each provider's sign-in may next be renewed, shared with the
    /// renewal in flight so its outcome sets the next gap.
    renewals: Arc<tokio::sync::Mutex<HashMap<ProviderId, Renewal>>>,
}

/// One provider's renewal state.
#[derive(Clone, Copy, Debug)]
struct Renewal {
    /// No renewal starts before this.
    not_before: DateTime<Utc>,
    /// A renewal is running now.
    running: bool,
}

/// Looks at every watched sign-in once.
pub(super) async fn on_tick(runtime: &MonitoringRuntime, watch: &mut SignInWatch) {
    let state = &runtime.state;
    if *state.monitoring.read().await == MonitoringState::Paused {
        return;
    }
    let renew_claude = state.preferences.read().await.privacy.renew_claude_sign_in;
    for (provider_id, waiting) in waiting_accounts(runtime).await {
        let renew = provider_id == ProviderId::Claude && renew_claude;
        look_at(runtime, watch, provider_id, waiting, renew).await;
    }
}

/// Looks at one provider's sign-in file: a change re-reads the accounts that
/// wait for it, and an unchanged lapse may start a renewal.
async fn look_at(
    runtime: &MonitoringRuntime,
    watch: &mut SignInWatch,
    provider_id: ProviderId,
    waiting: Vec<AccountId>,
    renew: bool,
) {
    let Some(adapter) = runtime.state.providers.provider(provider_id).cloned() else {
        return;
    };
    let Some(file) = adapter.external_sign_in_file() else {
        return;
    };
    let changed = watch.restamp(provider_id, stamp_of(&file).await);
    if waiting.is_empty() {
        return;
    }
    if changed {
        reread(runtime, waiting).await;
    } else if renew {
        start_renewal(watch, provider_id, adapter, runtime.state.clock.clone()).await;
    }
}

/// Re-reads accounts whose sign-in was just renewed. A reconnect read
/// bypasses the backoff the lapsed sign-in earned.
async fn reread(runtime: &MonitoringRuntime, waiting: Vec<AccountId>) {
    if let Err(error) = runtime.refresh(waiting, RefreshReason::Reconnect).await {
        tracing::warn!(
            code = error.diagnostic_code(),
            "a renewed sign-in was not re-read"
        );
    }
}

/// Every provider with an account, and its monitored accounts that wait for a
/// reconnect. A provider whose accounts are all fine is kept, so its file is
/// still stamped and a later lapse is not mistaken for a change.
async fn waiting_accounts(runtime: &MonitoringRuntime) -> HashMap<ProviderId, Vec<AccountId>> {
    let mut providers: HashMap<ProviderId, Vec<AccountId>> = HashMap::new();
    for entry in runtime.state.registry.read().await.iter() {
        let waiting = providers.entry(entry.binding.provider_id).or_default();
        if entry.stored.monitoring_enabled
            && entry.stored.connection_state == ConnectionState::ReauthenticationRequired
        {
            waiting.push(entry.account_id().clone());
        }
    }
    providers
}

/// When a file was last written and how big it is, or `None` without one.
async fn stamp_of(file: &std::path::Path) -> Stamp {
    let metadata = tokio::fs::metadata(file).await.ok()?;
    Some((metadata.modified().ok()?, metadata.len()))
}

impl SignInWatch {
    /// Records a provider's new stamp, and whether it differs from the last.
    /// The first look is the baseline, not a change.
    fn restamp(&mut self, provider_id: ProviderId, stamp: Stamp) -> bool {
        self.stamps
            .insert(provider_id, stamp)
            .is_some_and(|previous| previous != stamp)
    }
}

/// Starts one renewal in the background, unless one ran too recently.
async fn start_renewal(
    watch: &SignInWatch,
    provider_id: ProviderId,
    adapter: Arc<dyn quota_core::ports::ProviderAdapter>,
    clock: Arc<quota_core::SystemClock>,
) {
    let now = clock.now();
    {
        let mut renewals = watch.renewals.lock().await;
        if !renewal_due(renewals.get(&provider_id), now) {
            return;
        }
        renewals.insert(
            provider_id,
            Renewal {
                not_before: now + RENEWAL_SPACING,
                running: true,
            },
        );
    }
    let renewals = watch.renewals.clone();
    tauri::async_runtime::spawn(async move {
        let outcome = adapter.renew_external_sign_in().await;
        match &outcome {
            Ok(result) => tracing::info!(
                provider = provider_id.as_str(),
                outcome = ?result,
                "asked the owning client to renew its sign-in"
            ),
            Err(error) => tracing::warn!(
                provider = provider_id.as_str(),
                code = error.diagnostic_code(),
                "the sign-in renewal did not run"
            ),
        }
        renewals.lock().await.insert(
            provider_id,
            Renewal {
                not_before: clock.now() + spacing_after(&outcome),
                running: false,
            },
        );
    });
}

/// Whether a renewal may start now: none is running, and the gap the last one
/// earned has passed.
fn renewal_due(last: Option<&Renewal>, now: DateTime<Utc>) -> bool {
    last.is_none_or(|renewal| !renewal.running && renewal.not_before <= now)
}

/// The gap a renewal's outcome earns before the next one.
fn spacing_after(outcome: &Result<SignInRenewal, ProviderError>) -> chrono::Duration {
    match outcome {
        Ok(SignInRenewal::Renewed | SignInRenewal::NotNeeded) | Err(_) => RENEWAL_SPACING,
        Ok(SignInRenewal::Unchanged | SignInRenewal::Unavailable) => FRUITLESS_SPACING,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(minutes: i64) -> DateTime<Utc> {
        DateTime::UNIX_EPOCH + chrono::Duration::minutes(minutes)
    }

    #[test]
    fn a_sign_in_file_changes_only_after_a_first_look() {
        let mut watch = SignInWatch::default();
        let written = Some((SystemTime::UNIX_EPOCH, 10));
        assert!(!watch.restamp(ProviderId::Claude, written));
        assert!(!watch.restamp(ProviderId::Claude, written));
        let rewritten = Some((SystemTime::UNIX_EPOCH + Duration::from_secs(1), 10));
        assert!(watch.restamp(ProviderId::Claude, rewritten));
        assert!(watch.restamp(ProviderId::Claude, None));
    }

    #[test]
    fn a_first_renewal_is_due_at_once() {
        assert!(renewal_due(None, at(0)));
    }

    #[test]
    fn a_running_renewal_is_never_doubled() {
        let running = Renewal {
            not_before: at(0),
            running: true,
        };
        assert!(!renewal_due(Some(&running), at(60)));
    }

    #[test]
    fn a_renewal_waits_out_its_gap() {
        let last = Renewal {
            not_before: at(15),
            running: false,
        };
        assert!(!renewal_due(Some(&last), at(14)));
        assert!(renewal_due(Some(&last), at(15)));
    }

    #[test]
    fn a_fruitless_renewal_waits_much_longer_than_a_working_one() {
        assert_eq!(spacing_after(&Ok(SignInRenewal::Renewed)), RENEWAL_SPACING);
        assert_eq!(
            spacing_after(&Err(ProviderError::Authentication)),
            RENEWAL_SPACING
        );
        assert_eq!(
            spacing_after(&Ok(SignInRenewal::Unchanged)),
            FRUITLESS_SPACING
        );
        assert_eq!(
            spacing_after(&Ok(SignInRenewal::Unavailable)),
            FRUITLESS_SPACING
        );
        assert!(FRUITLESS_SPACING > RENEWAL_SPACING);
    }
}
