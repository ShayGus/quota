//! The handover of the sign-in address to the platform's browser opener.
//!
//! A person who signs in is told that a browser will open for them. What the
//! host can honestly prove is that the exact address left through the opener
//! the host actually uses, and that a platform with no opener to give back
//! says so instead of staying silent.
//!
//! Neither proof needs a browser. On Linux the launchers are throwaway
//! `xdg-open` (and its siblings) in a sandbox `PATH` recording the command
//! line they were given; the probe runs in a child process because `PATH` is
//! process-wide and this crate denies writing to the environment. On Windows
//! the handler is a throwaway per-user `https` registration that records the
//! same. Neither touches the machine's real default browser, and the Windows
//! registration is removed again on every path out of the test.
#![cfg(any(target_os = "linux", windows))]
#![expect(
    clippy::tests_outside_test_module,
    reason = "an integration test binary holds nothing but tests and compiles without cfg(test)"
)]
#![allow(
    clippy::expect_used,
    reason = "clippy.toml already allows expect inside tests, but the lint only              recognises a #[test] function, and these helpers are called by them"
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use quota_desktop_lib::bootstrap_helpers::open_external;
use tauri::test::{MockRuntime, mock_builder, mock_context, noop_assets};

#[cfg(target_os = "linux")]
use quota_contracts::CommandError;

/// The address a first device sign-in hands to the platform: the provider's
/// own verification address plus the query separators that a lossy handover
/// is known to drop.
const SIGN_IN_URL: &str =
    "https://auth.meta.com/oauth/device/?code=ABCD-EFGH&client=1031625952748946";

/// A host with the opener registered, and no window, no profile, and no
/// account behind it. The handover does not touch any of those.
fn opener_host() -> tauri::App<MockRuntime> {
    mock_builder()
        .plugin(tauri_plugin_opener::init())
        .build(mock_context(noop_assets()))
        .expect("a host with the opener can be built")
}

/// A directory nothing else is using, for the launcher and its record.
fn scratch(tag: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let dir = std::env::temp_dir().join(format!(
        "quota-browser-launch-{tag}-{}-{stamp}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("the sandbox directory can be created");
    dir
}

/// Waits for something written after a detached child starts, rather than
/// racing it.
fn wait_for_record(path: &Path) -> String {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Ok(text) = fs::read_to_string(path)
            && text.lines().last() == Some("complete")
        {
            return text;
        }
        assert!(
            Instant::now() < deadline,
            "nothing was recorded at {} in time",
            path.display()
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn a_partial_record_is_not_consumed_before_completion() {
    let sandbox = scratch("partial");
    let record = sandbox.join("record.txt");
    fs::write(&record, "argc=1\n").expect("the first field can be written");
    let target = record.clone();
    let finished = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        fs::write(&target, "argc=1\narg1=address\ncomplete\n")
            .expect("the record can be completed");
    });
    let recorded = wait_for_record(&record);
    finished.join().expect("the recorder finished");
    assert_eq!(recorded, "argc=1\narg1=address\ncomplete\n");
    fs::remove_dir_all(sandbox).expect("the sandbox can be removed");
}

/// The name this test answers to when it is re-run as its own probe.
#[cfg(target_os = "linux")]
const PROBE: &str = "QUOTA_BROWSER_LAUNCH_PROBE";

/// Launchers the platform may reach for, in the order the opener tries them.
#[cfg(target_os = "linux")]
const LAUNCHERS: &[&str] = &["xdg-open", "gio", "gnome-open", "kde-open"];

/// A launcher that records how it was invoked and exits at once.
///
/// The record path is baked in rather than passed through the environment,
/// because the point of the test is what the launcher received and not what
/// it inherited. `printf %s` keeps the address as data, so an address that
/// contains `%` cannot be reinterpreted by the shell that runs this.
#[cfg(target_os = "linux")]
const STUB_LAUNCHER: &str = r#"#!/bin/sh
{
  printf 'program=%s\n' "$0"
  printf 'argc=%s\n' "$#"
  i=0
  for a in "$@"; do
    i=$((i + 1))
    printf 'arg%s=%s\n' "$i" "$a"
  done
  printf 'complete\n'
} >> "@@"
exit 0
"#;

#[cfg(target_os = "linux")]
fn write_stub_launchers(dir: &Path, record: &Path) {
    use std::os::unix::fs::PermissionsExt as _;

    let script = STUB_LAUNCHER.replace("@@", &record.display().to_string());
    for name in LAUNCHERS {
        let path = dir.join(name);
        fs::write(&path, script.as_bytes()).expect("a stub launcher can be written");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))
            .expect("a stub launcher can be made executable");
    }
}

/// Runs this test again as a child whose `PATH` is only the sandbox.
///
/// `PATH` cannot be written from here: the workspace denies unsafe code and
/// `std::env::set_var` is unsafe on this edition. Handing the child its own
/// `PATH` is the same isolation without it.
#[cfg(target_os = "linux")]
fn run_as_probe(test_name: &str, sandbox: &Path) -> std::process::ExitStatus {
    std::process::Command::new(std::env::current_exe().expect("the test is running from a file"))
        .args(["--exact", test_name, "--nocapture", "--test-threads=1"])
        .env(PROBE, sandbox)
        .env("PATH", sandbox)
        .status()
        .expect("the probe can be started")
}

/// What the probe did with the address, written where the parent can read it.
#[cfg(target_os = "linux")]
fn record_outcome(dir: &Path, outcome: Result<(), CommandError>) {
    let text = match outcome {
        Ok(()) => "ok\n".to_owned(),
        Err(CommandError::NativeOperationFailed { operation, reason }) => {
            format!("refused\n{operation}\n{reason}\n")
        }
        Err(other) => format!("other\n{other:?}\n"),
    };
    fs::write(dir.join("outcome.txt"), text).expect("the probe can report what it did");
}

#[cfg(target_os = "linux")]
#[test]
fn the_sign_in_address_reaches_the_platform_launcher_intact() {
    if let Some(dir) = std::env::var_os(PROBE) {
        let dir = PathBuf::from(dir);
        let host = opener_host();
        record_outcome(&dir, open_external(host.handle(), SIGN_IN_URL));
        return;
    }

    let sandbox = scratch("arrives");
    let record = sandbox.join("record.txt");
    write_stub_launchers(&sandbox, &record);

    let probe = run_as_probe(
        "the_sign_in_address_reaches_the_platform_launcher_intact",
        &sandbox,
    );
    assert!(
        probe.success(),
        "the probe must run; its output is above this failure"
    );

    let outcome =
        fs::read_to_string(sandbox.join("outcome.txt")).expect("the probe reported what it did");
    assert_eq!(outcome, "ok\n", "the platform must accept the address");

    let recorded = wait_for_record(&record);
    assert!(
        recorded.contains("argc=1\n"),
        "the launcher must be handed one argument, the address itself; got:\n{recorded}"
    );
    assert!(
        recorded.contains(&format!("arg1={SIGN_IN_URL}\n")),
        "the address must reach the launcher exactly as the provider issued it; \
         got:\n{recorded}"
    );
}

#[cfg(target_os = "linux")]
#[test]
fn a_platform_without_a_launcher_reports_the_address_it_could_not_place() {
    if let Some(dir) = std::env::var_os(PROBE) {
        let dir = PathBuf::from(dir);
        let host = opener_host();
        record_outcome(&dir, open_external(host.handle(), SIGN_IN_URL));
        return;
    }

    // Left with no launchers in it: a host on a platform that will not take
    // the address must say so rather than stay quiet.
    let sandbox = scratch("refused");

    let probe = run_as_probe(
        "a_platform_without_a_launcher_reports_the_address_it_could_not_place",
        &sandbox,
    );
    assert!(
        probe.success(),
        "the probe must run; its output is above this failure"
    );

    let outcome =
        fs::read_to_string(sandbox.join("outcome.txt")).expect("the probe reported what it did");
    let mut lines = outcome.lines();
    assert_eq!(lines.next(), Some("refused"), "the refusal is structured");
    assert_eq!(lines.next(), Some("open_external"));
    let reason = lines.next().expect("a refusal carries its reason");
    assert!(
        reason.contains(SIGN_IN_URL),
        "the reason must name the address that could not be opened; got: {reason}"
    );
    assert!(
        reason.contains("The log is at"),
        "the reason must name the log; got: {reason}"
    );
}

/// The per-user `https` registration this test installs, removed on the way
/// out of every path that installs it.
#[cfg(windows)]
struct InstalledHandler;

#[cfg(windows)]
impl Drop for InstalledHandler {
    fn drop(&mut self) {
        let _ = reg(&["delete", r"HKCU\Software\Classes\https", "/f"]);
    }
}

#[cfg(windows)]
fn reg(args: &[&str]) -> std::process::ExitStatus {
    std::process::Command::new("reg.exe")
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .expect("reg.exe can be started")
}

#[cfg(windows)]
fn handler_already_installed() -> bool {
    reg(&["query", r"HKCU\Software\Classes\https"]).success()
}

/// Points the per-user `https` handler at a PowerShell script that records the
/// address it was given.
///
/// The script is reached with `-File`, so the address arrives as one literal
/// argument and no intermediate shell can reinterpret an `&` or a `%` in it.
#[cfg(windows)]
fn install_handler(script: &Path, record: &Path) {
    let source = format!(
        "Add-Content -LiteralPath '{record}' -Value ('argc=' + $args.Count)\n\
         Add-Content -LiteralPath '{record}' -Value ('arg1=' + $args[0])\n\
         Add-Content -LiteralPath '{record}' -Value 'complete'\n",
        record = record.display()
    );
    fs::write(script, source).expect("the recorder can be written");

    let command = format!(
        "powershell.exe -NoProfile -ExecutionPolicy Bypass -File \"{}\" \"%1\"",
        script.display()
    );
    let key = r"HKCU\Software\Classes\https";
    let command_key = r"HKCU\Software\Classes\https\shell\open\command";

    for args in [
        vec![
            "add",
            key,
            "/ve",
            "/t",
            "REG_SZ",
            "/d",
            "URL:Hyperlink",
            "/f",
        ],
        vec![
            "add",
            key,
            "/v",
            "URL Protocol",
            "/t",
            "REG_SZ",
            "/d",
            "",
            "/f",
        ],
        vec![
            "add",
            command_key,
            "/ve",
            "/t",
            "REG_SZ",
            "/d",
            command.as_str(),
            "/f",
        ],
    ] {
        assert!(
            reg(&args).success(),
            "the throwaway handler must be registered: {args:?}"
        );
    }
}

#[cfg(windows)]
#[test]
fn the_sign_in_address_reaches_the_windows_browser_handler_intact() {
    if handler_already_installed() {
        println!("skipped: this machine already has a per-user https handler");
        return;
    }

    let sandbox = scratch("windows");
    let record = sandbox.join("record.txt");
    let script = sandbox.join("record.ps1");
    let _installed = InstalledHandler;
    install_handler(&script, &record);

    let host = opener_host();
    let outcome = open_external(host.handle(), SIGN_IN_URL);
    assert!(
        outcome.is_ok(),
        "the platform must accept the address: {outcome:?}"
    );

    let recorded = wait_for_record(&record);
    assert!(
        recorded.contains(&format!("arg1={SIGN_IN_URL}")),
        "the address must reach the platform's handler exactly as the provider \
         issued it; got:\n{recorded}"
    );
}
