//! A browser sign-in, for a connection attempt that asked for one.
//!
//! The adapter asks its provider for a code; the host opens the provider's
//! sign-in page and shows the code in the settings window, then polls until
//! the person approves, declines, lets the code expire, or cancels. The
//! granted token then continues exactly like a pasted credential: verified,
//! held in memory, and stored only when the person adds the account.

use std::sync::Arc;
use std::time::Duration;

use quota_contracts::CommandError;
use quota_contracts::commands::{BeginConnectionRequest, PastedCredential};
use quota_contracts::events::{BrowserSignIn, ConnectionProgress};
use quota_core::ports::{DeviceAuthorization, DevicePoll, ProviderAdapter};
use quota_domain::ids::ConnectionAttemptId;
use tokio::sync::watch;

use super::connection::{AttemptReporter, provider_command_error};
use super::{MonitoringRuntime, REMOTE_TIMEOUT};

/// Extra seconds between polls each time the provider asks Quota to slow down.
const SLOW_DOWN_SECONDS: u64 = 5;

/// Runs the browser sign-in an attempt asked for, and answers the request
/// carrying the granted credential. A request that did not ask for one comes
/// back unchanged; `None` means the attempt was cancelled.
pub(super) async fn sign_in(
    runtime: &MonitoringRuntime,
    adapter: &Arc<dyn ProviderAdapter>,
    mut request: BeginConnectionRequest,
    attempt_id: &ConnectionAttemptId,
    cancelled: &mut watch::Receiver<bool>,
    reporter: &AttemptReporter,
) -> Result<Option<BeginConnectionRequest>, CommandError> {
    if !request.browser_sign_in {
        return Ok(Some(request));
    }
    reporter
        .emit(&runtime.state, attempt_id, ConnectionProgress::Started)
        .await;
    let authorization = tokio::select! {
        _ = cancelled.changed() => return Ok(None),
        started = tokio::time::timeout(REMOTE_TIMEOUT, adapter.begin_device_sign_in()) => started
            .map_err(|_| timed_out())?
            .map_err(provider_command_error)?,
    };
    let page = authorization
        .verification_uri_complete
        .as_deref()
        .unwrap_or(&authorization.verification_uri);
    if crate::bootstrap_helpers::open_external(&runtime.state.app, page).is_err() {
        // The page and code stay on screen, so the person can open it.
        tracing::warn!("the browser sign-in page could not be opened");
    }
    reporter
        .emit(
            &runtime.state,
            attempt_id,
            ConnectionProgress::AwaitingUser {
                sign_in: Some(BrowserSignIn {
                    user_code: authorization.user_code.clone(),
                    verification_uri: authorization.verification_uri.clone(),
                }),
            },
        )
        .await;
    let Some(granted) = wait(adapter, &authorization, cancelled).await? else {
        return Ok(None);
    };
    request.credential = Some(PastedCredential::new(granted));
    request.browser_sign_in = false;
    Ok(Some(request))
}

/// Polls until the person finishes, at the pace the provider asks for.
async fn wait(
    adapter: &Arc<dyn ProviderAdapter>,
    authorization: &DeviceAuthorization,
    cancelled: &mut watch::Receiver<bool>,
) -> Result<Option<String>, CommandError> {
    let mut interval = authorization.interval_seconds;
    loop {
        tokio::select! {
            _ = cancelled.changed() => return Ok(None),
            () = tokio::time::sleep(Duration::from_secs(interval)) => {}
        }
        let polled = tokio::select! {
            _ = cancelled.changed() => return Ok(None),
            polled = tokio::time::timeout(
                REMOTE_TIMEOUT,
                adapter.poll_device_sign_in(authorization),
            ) => polled.map_err(|_| timed_out())?.map_err(provider_command_error)?,
        };
        match polled {
            DevicePoll::Pending => {}
            DevicePoll::SlowDown => interval += SLOW_DOWN_SECONDS,
            DevicePoll::Granted(secret) => return Ok(Some(secret.expose().to_owned())),
            DevicePoll::Denied => return Err(internal("browser_sign_in_declined")),
            DevicePoll::Expired => return Err(internal("browser_sign_in_expired")),
        }
    }
}

fn timed_out() -> CommandError {
    internal("browser_sign_in_timeout")
}

fn internal(code: &str) -> CommandError {
    CommandError::Internal { code: code.into() }
}
