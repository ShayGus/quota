//! Updates: a published release is offered to the person, and installed if they say yes.
//!
//! The repository-root `README.md`'s Updates section owns the schedule and pop-up
//! behaviour; `docs/RELEASING.md` owns the release feed and signing procedure.
//!
//! The work happens here, in Rust, on a task of its own: the windows may be
//! hidden or closed while the tray application lives, so no window takes part in
//! the check or the install. The pop-up is a small window of Quota's own that the
//! host opens when something is found; it only draws what the host says and reports
//! which button was pressed, and it is given no updater permission.
//!
//! Layout: `policy` decides whether this build may check at all, `schedule`
//! says when a check is due, `flow` is one check cycle over the `UpdateHost`
//! port, and `host` is that port's real implementation.

mod flow;
mod host;
mod policy;
mod prompt;
mod schedule;
#[cfg(feature = "sample-data")]
mod test_endpoint;

use tauri::{AppHandle, Manager, Wry};

use self::flow::UpdateFlow;
use self::host::TauriHost;
use self::policy::{BuildFacts, UpdateDecision, may_check_for_updates};
pub use self::prompt::PromptSlot;
use self::schedule::SystemClock;

#[cfg(feature = "sample-data")]
pub use self::test_endpoint::retarget_for_tests;

/// Starts the update checks if this build may run them.
///
/// Returns at once. The checks run on their own task, so a slow or failing
/// server never delays the application.
pub(crate) fn start(app: &AppHandle<Wry>) {
    // The pop-up's commands read this; it is empty unless a check finds something.
    app.manage(PromptSlot::default());
    let facts = BuildFacts::current(&app.config().identifier);
    match may_check_for_updates(&facts) {
        UpdateDecision::Skip(reason) => {
            tracing::info!(
                reason = reason.reason(),
                "update checks are off in this build"
            );
        }
        UpdateDecision::Check => {
            let flow = UpdateFlow::new(TauriHost::new(app.clone()), SystemClock::new());
            tauri::async_runtime::spawn(flow.run());
        }
    }
}
