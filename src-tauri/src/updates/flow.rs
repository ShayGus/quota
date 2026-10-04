//! What one update check does, and when the next one runs.
//!
//! Everything here is plain logic over the [`UpdateHost`] port. The real host
//! (`host.rs`) talks to the updater plugin and the pop-up window; the tests below talk to a
//! scripted one, so every branch of the behaviour is exercised without a
//! network, a window, or a restart.
//!
//! One [`UpdateFlow`] owns the whole cycle and runs it one step at a time: it
//! checks, asks, installs, and only then looks at the clock again. Two checks, or
//! two pop-ups, cannot overlap because nothing else can start either.

use std::future::Future;

use semver::Version;

use super::schedule::{Clock, Schedule, WAKE_POLL};

/// An update the host found.
#[derive(Debug)]
pub(crate) struct Found<Pending> {
    /// The version on offer.
    pub version: Version,
    /// The version running now.
    pub current: Version,
    /// What the host needs to install it.
    pub pending: Pending,
}

/// What the flow needs from the outside world.
pub(crate) trait UpdateHost: Send + Sync {
    /// The host's handle on a found update.
    type Pending: Send;

    /// Looks for a published update. `Ok(None)` means there is none.
    fn check(&self) -> impl Future<Output = Result<Option<Found<Self::Pending>>, String>> + Send;

    /// Shows the pop-up that names both versions and waits. `true` only for the
    /// OK button; Cancel and closing the pop-up are both `false`. On `true` the
    /// pop-up stays up, showing the install, until the host closes it.
    fn ask(&self, version: &Version, current: &Version) -> impl Future<Output = bool> + Send;

    /// Downloads, verifies, and installs the update.
    fn install(&self, pending: Self::Pending) -> impl Future<Output = Result<(), String>> + Send;

    /// Starts the new version in place of this one.
    fn relaunch(&self);

    /// Tells the person, in the same pop-up, that the update failed, and waits
    /// for it to be closed.
    fn tell_install_failed(&self) -> impl Future<Output = ()> + Send;
}

/// What one pass over the schedule did. The flow's own record of it, for the log
/// and for the tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Outcome {
    /// Not due yet.
    NotDue,
    /// The check itself failed.
    CheckFailed,
    /// Nothing newer is published.
    UpToDate,
    /// This version, or a newer one, was already declined or failed to install.
    AlreadyAnswered(Version),
    /// The person pressed Cancel or closed the pop-up.
    Declined(Version),
    /// The update installed and the application is restarting.
    Installed(Version),
    /// The person accepted and the install failed.
    InstallFailed(Version),
}

/// The check, the pop-up, the install, and the memory between them.
pub(crate) struct UpdateFlow<Host, Time> {
    host: Host,
    clock: Time,
    schedule: Schedule,
    /// The newest version already put to the person without it being installed:
    /// declined, or accepted and failed. Kept in memory only, so a restart asks
    /// again, and a newer version is always asked about.
    answered: Option<Version>,
}

impl<Host: UpdateHost, Time: Clock> UpdateFlow<Host, Time> {
    /// A flow that has not checked yet, so its first pass checks.
    pub(crate) fn new(host: Host, clock: Time) -> Self {
        Self {
            host,
            clock,
            schedule: Schedule::default(),
            answered: None,
        }
    }

    /// Runs for as long as the application does: a pass now, then one every
    /// [`WAKE_POLL`].
    pub(crate) async fn run(mut self) {
        loop {
            let outcome = self.pass().await;
            tracing::debug!(?outcome, "update schedule pass");
            tokio::time::sleep(WAKE_POLL).await;
        }
    }

    /// One pass: does a whole check cycle if one is due, and nothing otherwise.
    pub(crate) async fn pass(&mut self) -> Outcome {
        if !self.schedule.is_due(self.clock.now()) {
            return Outcome::NotDue;
        }
        let outcome = self.cycle().await;
        // The next interval starts when the cycle ends, however long the
        // pop-up was open.
        self.schedule.finished(self.clock.now());
        outcome
    }

    /// Checks once and, when there is something to offer, offers it.
    async fn cycle(&mut self) -> Outcome {
        let found = match self.host.check().await {
            Ok(Some(found)) => found,
            Ok(None) => return Outcome::UpToDate,
            Err(reason) => {
                tracing::warn!(%reason, "the update check failed");
                return Outcome::CheckFailed;
            }
        };
        if self
            .answered
            .as_ref()
            .is_some_and(|old| found.version <= *old)
        {
            return Outcome::AlreadyAnswered(found.version);
        }
        // Whatever happens next, this version has been put to the person.
        self.answered = Some(found.version.clone());
        self.offer(found).await
    }

    /// Asks about `found`, and installs it if the answer is OK.
    async fn offer(&self, found: Found<Host::Pending>) -> Outcome {
        let Found {
            version,
            current,
            pending,
        } = found;
        if !self.host.ask(&version, &current).await {
            return Outcome::Declined(version);
        }
        if self.install(pending).await {
            Outcome::Installed(version)
        } else {
            Outcome::InstallFailed(version)
        }
    }

    /// Installs and relaunches. On failure, says so and keeps running.
    async fn install(&self, pending: Host::Pending) -> bool {
        match self.host.install(pending).await {
            Ok(()) => {
                self.host.relaunch();
                true
            }
            Err(reason) => {
                tracing::warn!(%reason, "the update could not be installed");
                self.host.tell_install_failed().await;
                false
            }
        }
    }
}

#[cfg(test)]
#[path = "flow_tests.rs"]
mod tests;
