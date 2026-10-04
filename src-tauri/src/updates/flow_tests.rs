use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::super::schedule::{CHECK_INTERVAL, fake::FakeClock};
use super::*;

const MINUTE: Duration = Duration::from_secs(60);
const ONE_SECOND_SHORT_OF_THE_INTERVAL: Duration = Duration::from_secs(30 * 60 - 1);

/// What the scripted host was asked, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Call {
    Check,
    Ask { version: String, current: String },
    Install(String),
    Relaunch,
    TellFailed,
}

/// What a scripted check returns: (version, current), nothing, or a failure.
type ScriptedCheck = Result<Option<(&'static str, &'static str)>, &'static str>;

#[derive(Default)]
struct Script {
    checks: VecDeque<ScriptedCheck>,
    answers: VecDeque<bool>,
    install: VecDeque<Result<(), &'static str>>,
    calls: Vec<Call>,
}

/// A host that replays a script and records every call.
#[derive(Clone, Default)]
struct ScriptedHost(Arc<Mutex<Script>>);

impl ScriptedHost {
    fn check_finds(&self, version: &'static str, current: &'static str) -> &Self {
        self.0
            .lock()
            .unwrap()
            .checks
            .push_back(Ok(Some((version, current))));
        self
    }

    fn check_finds_nothing(&self) -> &Self {
        self.0.lock().unwrap().checks.push_back(Ok(None));
        self
    }

    fn check_fails(&self, reason: &'static str) -> &Self {
        self.0.lock().unwrap().checks.push_back(Err(reason));
        self
    }

    fn user_answers(&self, ok: bool) -> &Self {
        self.0.lock().unwrap().answers.push_back(ok);
        self
    }

    fn install_result(&self, result: Result<(), &'static str>) -> &Self {
        self.0.lock().unwrap().install.push_back(result);
        self
    }

    fn calls(&self) -> Vec<Call> {
        self.0.lock().unwrap().calls.clone()
    }

    fn checks_made(&self) -> usize {
        self.calls()
            .iter()
            .filter(|call| **call == Call::Check)
            .count()
    }
}

impl UpdateHost for ScriptedHost {
    type Pending = String;

    async fn check(&self) -> Result<Option<Found<String>>, String> {
        let mut script = self.0.lock().unwrap();
        script.calls.push(Call::Check);
        match script.checks.pop_front().expect("an unscripted check") {
            Ok(Some((version, current))) => Ok(Some(Found {
                version: version.parse().unwrap(),
                current: current.parse().unwrap(),
                pending: version.to_owned(),
            })),
            Ok(None) => Ok(None),
            Err(reason) => Err(reason.to_owned()),
        }
    }

    async fn ask(&self, version: &Version, current: &Version) -> bool {
        let mut script = self.0.lock().unwrap();
        script.calls.push(Call::Ask {
            version: version.to_string(),
            current: current.to_string(),
        });
        script.answers.pop_front().expect("an unscripted answer")
    }

    async fn install(&self, pending: String) -> Result<(), String> {
        let mut script = self.0.lock().unwrap();
        script.calls.push(Call::Install(pending));
        script
            .install
            .pop_front()
            .expect("an unscripted install")
            .map_err(str::to_owned)
    }

    fn relaunch(&self) {
        self.0.lock().unwrap().calls.push(Call::Relaunch);
    }

    async fn tell_install_failed(&self) {
        self.0.lock().unwrap().calls.push(Call::TellFailed);
    }
}

/// A flow over a scripted host and a clock the test holds a second handle on.
fn flow() -> (
    UpdateFlow<ScriptedHost, Arc<FakeClock>>,
    ScriptedHost,
    Arc<FakeClock>,
) {
    let host = ScriptedHost::default();
    let clock = Arc::new(FakeClock::new());
    (UpdateFlow::new(host.clone(), clock.clone()), host, clock)
}

impl Clock for Arc<FakeClock> {
    fn now(&self) -> super::super::schedule::Moment {
        FakeClock::now(self)
    }
}

#[tokio::test]
async fn an_available_update_shows_one_pop_up_naming_both_versions() {
    let (mut flow, host, _clock) = flow();
    host.check_finds("0.2.0", "0.1.0").user_answers(false);
    flow.pass().await;
    assert_eq!(
        host.calls(),
        [
            Call::Check,
            Call::Ask {
                version: "0.2.0".to_owned(),
                current: "0.1.0".to_owned(),
            },
        ]
    );
}

#[tokio::test]
async fn ok_installs_and_then_relaunches() {
    let (mut flow, host, _clock) = flow();
    host.check_finds("0.2.0", "0.1.0")
        .user_answers(true)
        .install_result(Ok(()));
    let outcome = flow.pass().await;
    assert_eq!(outcome, Outcome::Installed("0.2.0".parse().unwrap()));
    let calls = host.calls();
    assert_eq!(calls[0], Call::Check);
    assert!(matches!(calls[1], Call::Ask { .. }));
    assert_eq!(calls[2], Call::Install("0.2.0".to_owned()));
    assert_eq!(calls[3], Call::Relaunch);
    assert_eq!(calls.len(), 4);
}

#[tokio::test]
async fn cancel_does_nothing() {
    let (mut flow, host, _clock) = flow();
    host.check_finds("0.2.0", "0.1.0").user_answers(false);
    let outcome = flow.pass().await;
    assert_eq!(outcome, Outcome::Declined("0.2.0".parse().unwrap()));
    let calls = host.calls();
    assert_eq!(calls.len(), 2);
    assert!(!calls.contains(&Call::Relaunch));
    assert!(!calls.iter().any(|call| matches!(call, Call::Install(_))));
}

#[tokio::test]
async fn a_failed_check_shows_nothing() {
    let (mut flow, host, _clock) = flow();
    host.check_fails("could not reach the server");
    let outcome = flow.pass().await;
    assert_eq!(outcome, Outcome::CheckFailed);
    assert_eq!(host.calls(), [Call::Check]);
}

#[tokio::test]
async fn no_update_shows_nothing() {
    let (mut flow, host, _clock) = flow();
    host.check_finds_nothing();
    assert_eq!(flow.pass().await, Outcome::UpToDate);
    assert_eq!(host.calls(), [Call::Check]);
}

#[tokio::test]
async fn a_failed_install_shows_the_error_pop_up_and_keeps_running() {
    let (mut flow, host, _clock) = flow();
    host.check_finds("0.2.0", "0.1.0")
        .user_answers(true)
        .install_result(Err("the signature does not match"));
    let outcome = flow.pass().await;
    assert_eq!(outcome, Outcome::InstallFailed("0.2.0".parse().unwrap()));
    let calls = host.calls();
    assert_eq!(calls.last(), Some(&Call::TellFailed));
    assert!(!calls.contains(&Call::Relaunch));
}

#[tokio::test]
async fn one_check_at_start_and_none_before_the_interval_has_passed() {
    let (mut flow, host, clock) = flow();
    host.check_finds_nothing().check_finds_nothing();
    assert_eq!(flow.pass().await, Outcome::UpToDate);
    clock.advance(ONE_SECOND_SHORT_OF_THE_INTERVAL);
    assert_eq!(flow.pass().await, Outcome::NotDue);
    assert_eq!(host.checks_made(), 1);
}

#[tokio::test]
async fn the_second_check_runs_when_the_interval_is_up() {
    let (mut flow, host, clock) = flow();
    host.check_finds_nothing().check_finds_nothing();
    flow.pass().await;
    clock.advance(CHECK_INTERVAL);
    assert_eq!(flow.pass().await, Outcome::UpToDate);
    assert_eq!(host.checks_made(), 2);
}

#[tokio::test]
async fn a_long_sleep_triggers_one_check_not_many() {
    let (mut flow, host, clock) = flow();
    host.check_finds_nothing().check_finds_nothing();
    flow.pass().await;
    // An hour and a half asleep: the monotonic clock saw none of it.
    clock.sleep(3 * CHECK_INTERVAL);
    assert_eq!(flow.pass().await, Outcome::UpToDate);
    assert_eq!(flow.pass().await, Outcome::NotDue);
    assert_eq!(flow.pass().await, Outcome::NotDue);
    assert_eq!(host.checks_made(), 2);
}

#[tokio::test]
async fn the_interval_runs_from_the_end_of_the_cycle() {
    // The pop-up stays open for half an hour; the next check is half an hour after it closes.
    let (mut flow, host, clock) = flow();
    host.check_finds("0.2.0", "0.1.0").user_answers(false);
    host.check_finds_nothing();
    flow.pass().await;
    clock.advance(5 * MINUTE);
    assert_eq!(flow.pass().await, Outcome::NotDue);
    clock.advance(25 * MINUTE);
    assert_eq!(flow.pass().await, Outcome::UpToDate);
}

#[tokio::test]
async fn a_declined_version_is_not_asked_again_until_a_restart() {
    let (mut flow, host, clock) = flow();
    host.check_finds("0.2.0", "0.1.0").user_answers(false);
    host.check_finds("0.2.0", "0.1.0");
    host.check_finds("0.2.0", "0.1.0");
    flow.pass().await;
    clock.advance(CHECK_INTERVAL);
    assert_eq!(
        flow.pass().await,
        Outcome::AlreadyAnswered("0.2.0".parse().unwrap())
    );
    clock.advance(CHECK_INTERVAL);
    assert_eq!(
        flow.pass().await,
        Outcome::AlreadyAnswered("0.2.0".parse().unwrap())
    );
    let asks = host
        .calls()
        .iter()
        .filter(|call| matches!(call, Call::Ask { .. }))
        .count();
    assert_eq!(asks, 1);

    // A restart is a new flow with an empty memory: it asks again.
    let restarted = ScriptedHost::default();
    restarted.check_finds("0.2.0", "0.1.0").user_answers(false);
    let mut flow = UpdateFlow::new(restarted.clone(), Arc::new(FakeClock::new()));
    flow.pass().await;
    assert_eq!(restarted.calls().len(), 2);
}

#[tokio::test]
async fn a_newer_version_asks_again_after_a_decline() {
    let (mut flow, host, clock) = flow();
    host.check_finds("0.2.0", "0.1.0").user_answers(false);
    host.check_finds("0.3.0", "0.1.0").user_answers(false);
    flow.pass().await;
    clock.advance(CHECK_INTERVAL);
    assert_eq!(
        flow.pass().await,
        Outcome::Declined("0.3.0".parse().unwrap())
    );
    let asks: Vec<_> = host
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            Call::Ask { version, current } => Some((version, current)),
            _ => None,
        })
        .collect();
    assert_eq!(
        asks,
        [
            ("0.2.0".to_owned(), "0.1.0".to_owned()),
            ("0.3.0".to_owned(), "0.1.0".to_owned()),
        ]
    );
}

#[tokio::test]
async fn an_older_release_than_the_declined_one_is_not_offered() {
    let (mut flow, host, clock) = flow();
    host.check_finds("0.3.0", "0.1.0").user_answers(false);
    host.check_finds("0.2.0", "0.1.0");
    flow.pass().await;
    clock.advance(CHECK_INTERVAL);
    assert_eq!(
        flow.pass().await,
        Outcome::AlreadyAnswered("0.2.0".parse().unwrap())
    );
}

#[tokio::test]
async fn a_failed_install_is_not_offered_again_until_a_restart() {
    let (mut flow, host, clock) = flow();
    host.check_finds("0.2.0", "0.1.0")
        .user_answers(true)
        .install_result(Err("disk full"));
    host.check_finds("0.2.0", "0.1.0");
    flow.pass().await;
    clock.advance(CHECK_INTERVAL);
    assert_eq!(
        flow.pass().await,
        Outcome::AlreadyAnswered("0.2.0".parse().unwrap())
    );
}

#[tokio::test]
async fn a_failed_check_is_tried_again_at_the_next_interval() {
    let (mut flow, host, clock) = flow();
    host.check_fails("offline");
    host.check_finds("0.2.0", "0.1.0").user_answers(false);
    assert_eq!(flow.pass().await, Outcome::CheckFailed);
    assert_eq!(flow.pass().await, Outcome::NotDue);
    clock.advance(CHECK_INTERVAL);
    assert_eq!(
        flow.pass().await,
        Outcome::Declined("0.2.0".parse().unwrap())
    );
}
