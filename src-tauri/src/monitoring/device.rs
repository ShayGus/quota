//! A browser sign-in, for a connection attempt that asked for one.
//!
//! The adapter asks its provider for a code; the host opens the provider's
//! sign-in page and shows the code in the settings window, then polls until
//! the person approves, declines, lets the code expire, or cancels. The
//! granted token then continues exactly like a pasted credential: verified,
//! held in memory, and stored only when the person adds the account.
//!
//! Two things are reported rather than swallowed: the code reaches the screen
//! before the browser is asked for, so a slow platform still leaves something
//! to read, and a platform that never takes the page reports the address,
//! the code, and the log's path while approval polling continues.

use std::sync::Arc;
use std::time::Duration;

use quota_contracts::CommandError;
use quota_contracts::commands::{BeginConnectionRequest, PastedCredential};
use quota_contracts::events::{BrowserSignIn, ConnectionProgress};
use quota_core::ports::{DeviceAuthorization, DevicePoll, ProviderAdapter};
use quota_domain::ids::ConnectionAttemptId;
use tokio::sync::watch;

use super::connection::{AttemptReporter, attempt_error};
use super::{MonitoringRuntime, REMOTE_TIMEOUT};

/// Extra seconds between polls each time the provider asks Quota to slow down.
const SLOW_DOWN_SECONDS: u64 = 5;

/// How long the platform gets to take the page before the attempt says so.
///
/// A launcher that hangs is the failure a person cannot see: the code would
/// sit on screen with no browser beside it and no message. Bounding it turns
/// that into the same loud report as an outright refusal.
const LAUNCH_TIMEOUT: Duration = Duration::from_secs(10);

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
            .map_err(|error| attempt_error(&error))?,
    };
    let page = authorization
        .verification_uri_complete
        .as_deref()
        .unwrap_or(&authorization.verification_uri);
    // The code goes on screen before the browser is asked for. A platform
    // that is slow, or that refuses, then still leaves the person reading
    // something, and the report below lands on a screen already showing the
    // sign-in rather than on one stuck at "Verifying…".
    let mut sign_in = BrowserSignIn {
        user_code: authorization.user_code.clone(),
        verification_uri: authorization.verification_uri.clone(),
        launch_error: None,
    };
    reporter
        .emit(
            &runtime.state,
            attempt_id,
            ConnectionProgress::AwaitingUser {
                sign_in: Some(sign_in.clone()),
            },
        )
        .await;
    if let Err(error) = open_page(runtime, page, &authorization.user_code).await {
        sign_in.launch_error = Some(error);
        reporter
            .emit(
                &runtime.state,
                attempt_id,
                ConnectionProgress::AwaitingUser {
                    sign_in: Some(sign_in),
                },
            )
            .await;
    }
    let Some(granted) = wait(adapter, &authorization, cancelled).await? else {
        return Ok(None);
    };
    request.credential = Some(PastedCredential::new(granted));
    request.browser_sign_in = false;
    Ok(Some(request))
}

/// Hands the sign-in page to the browser, off the async runtime and bounded.
///
/// The launch runs on a blocking thread because a platform launcher may wait
/// on the shell, and holding that here would stall every other attempt.
///
/// # Errors
/// Returns [`CommandError::NativeOperationFailed`] carrying the address, the
/// code, and the log's path whenever the platform does not take the page.
async fn open_page(
    runtime: &MonitoringRuntime,
    url: &str,
    user_code: &str,
) -> Result<(), CommandError> {
    let log = crate::file_log::location(&runtime.state.app);
    let app = runtime.state.app.clone();
    let target = url.to_owned();
    let launched = tokio::time::timeout(
        LAUNCH_TIMEOUT,
        tokio::task::spawn_blocking(move || crate::bootstrap_helpers::open_external(&app, &target)),
    )
    .await;
    match launched {
        Ok(Ok(Ok(()))) => Ok(()),
        Ok(Ok(Err(_))) => Err(refused(
            url,
            user_code,
            "the system browser refused the page",
            &log,
        )),
        Ok(Err(_)) => {
            let detail = "the browser launch stopped unexpectedly";
            crate::file_log::browser_failure(url, detail);
            Err(refused(url, user_code, detail, &log))
        }
        Err(_) => {
            let detail = format!(
                "the browser did not answer within {} seconds",
                LAUNCH_TIMEOUT.as_secs()
            );
            crate::file_log::browser_failure(url, &detail);
            Err(refused(url, user_code, &detail, &log))
        }
    }
}

/// The words a refused launch is reported in: what to open, what to type, and
/// where to look afterwards.
fn refused(
    url: &str,
    user_code: &str,
    detail: &str,
    log: &(impl std::fmt::Display + ?Sized),
) -> CommandError {
    CommandError::NativeOperationFailed {
        operation: "browser_launch".into(),
        reason: format!("{detail}. Open {url} and enter the code {user_code}. The log is at {log}"),
    }
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
            ) => polled
                .map_err(|_| timed_out())?
                .map_err(|error| attempt_error(&error))?,
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
