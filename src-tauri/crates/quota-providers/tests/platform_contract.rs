//! The path contract every platform implementation keeps, on every host.
//!
//! All four implementations compile on every host, so a Linux or Windows CI
//! runner runs the Windows, Linux, macOS, and unknown-system cases alike. The
//! suite builds each implementation directly, never through the selected one.
//!
//! Integration tests are compiled without `cfg(test)`, so the crate-wide
//! test-context allowance in `clippy.toml` does not reach this file.
#![expect(
    clippy::tests_outside_test_module,
    reason = "an integration test binary holds nothing but tests and compiles without cfg(test)"
)]
#![expect(
    clippy::expect_used,
    reason = "a contract assertion must fail loudly and read as prose"
)]

use std::collections::HashMap;
use std::path::PathBuf;

use quota_core::ports::SecretStoreError;
use quota_providers::credentials::{
    claude_credentials_file, codex_auth_file, opencode_auth_file, profile_directory,
};
use quota_providers::platform::{Linux, Lookup, Macos, Platform, Unsupported, Windows};

/// An environment made only of the variables a test names.
fn environment(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
        .collect();
    move |name: &str| map.get(name).cloned()
}

/// The three credential files a profile resolves, as `(path, label)`.
fn credential_files(platform: &dyn Platform, lookup: Lookup<'_>) -> [(PathBuf, String); 3] {
    let (codex, codex_label) = codex_auth_file(platform, lookup).expect("a Codex path");
    let (claude, claude_label) = claude_credentials_file(platform, lookup).expect("a Claude path");
    let (go, go_label) = opencode_auth_file(platform, lookup).expect("an OpenCode path");
    [(codex, codex_label), (claude, claude_label), (go, go_label)]
}

/// Every implementation names the variable it reads the profile from.
#[test]
fn every_platform_names_its_profile_variable() {
    assert_eq!(Windows.profile_variable(), "USERPROFILE");
    assert_eq!(Linux.profile_variable(), "HOME");
    assert_eq!(Macos.profile_variable(), "HOME");
    assert_eq!(Unsupported.profile_variable(), "HOME");
}

/// The profile comes from that variable alone, with nothing else set.
#[test]
fn the_profile_variable_alone_resolves_the_profile() {
    for (platform, profile) in [
        (&Windows as &dyn Platform, "C:\\Users\\someone"),
        (&Linux, "/home/someone"),
        (&Macos, "/home/someone"),
        (&Unsupported, "/home/someone"),
    ] {
        let lookup = environment(&[(platform.profile_variable(), profile)]);
        assert_eq!(
            platform.user_profile(&lookup),
            Some(PathBuf::from(profile)),
            "{} must resolve its profile from {} alone",
            platform.profile_variable(),
            platform.profile_variable()
        );
    }
}

/// The Windows defect: `USERPROFILE` alone, with `HOME` absent, still resolves.
#[test]
fn a_windows_profile_without_home_resolves_every_credential_file() {
    let lookup = environment(&[("USERPROFILE", "C:\\Users\\someone")]);
    assert_eq!(
        Windows.user_profile(&lookup),
        Some(PathBuf::from("C:\\Users\\someone"))
    );
    for (path, label) in credential_files(&Windows, &lookup) {
        assert!(path.starts_with("C:\\Users\\someone"), "{label}");
    }
}

/// A profile-only machine resolves every credential file, on every platform.
#[test]
fn a_profile_only_machine_resolves_every_credential_file() {
    for (platform, profile) in [
        (&Windows as &dyn Platform, "C:\\Users\\someone"),
        (&Linux, "/home/someone"),
        (&Macos, "/home/someone"),
        (&Unsupported, "/home/someone"),
    ] {
        let lookup = environment(&[(platform.profile_variable(), profile)]);
        let expected = PathBuf::from(profile);
        let [(codex, codex_label), (claude, claude_label), (go, go_label)] =
            credential_files(platform, &lookup);
        assert_eq!(
            codex,
            expected.join(".codex").join("auth.json"),
            "{codex_label}"
        );
        assert_eq!(
            claude,
            expected.join(".claude").join(".credentials.json"),
            "{claude_label}"
        );
        assert_eq!(
            go,
            expected
                .join(".local")
                .join("share")
                .join("opencode")
                .join("auth.json"),
            "{go_label}"
        );
    }
}

/// A provider's own variable wins over the profile, on every platform.
#[test]
fn an_explicit_override_wins_over_the_profile() {
    for platform in [&Linux as &dyn Platform, &Macos, &Unsupported] {
        let lookup = environment(&[
            ("HOME", "/profile"),
            ("CODEX_HOME", "/override/codex"),
            ("CLAUDE_CONFIG_DIR", "/override/claude"),
            ("XDG_DATA_HOME", "/override/xdg"),
        ]);
        let [(codex, codex_label), (claude, _), (go, _)] = credential_files(platform, &lookup);
        assert_eq!(codex, PathBuf::from("/override/codex").join("auth.json"));
        assert_eq!(codex_label, "codex-home-env");
        assert_eq!(
            claude,
            PathBuf::from("/override/claude").join(".credentials.json")
        );
        assert_eq!(
            go,
            PathBuf::from("/override/xdg")
                .join("opencode")
                .join("auth.json")
        );
    }
    let lookup = environment(&[("USERPROFILE", "/profile"), ("HOME", "/home")]);
    assert_eq!(
        Windows.user_profile(&lookup),
        Some(PathBuf::from("/profile")),
        "USERPROFILE wins over HOME on Windows"
    );
}

/// No profile and no override is an authentication state, never a guessed path.
#[test]
fn no_profile_and_no_override_is_an_authentication_state() {
    let lookup = environment(&[]);
    for platform in [&Windows as &dyn Platform, &Linux, &Macos, &Unsupported] {
        assert_eq!(platform.user_profile(&lookup), None);
        assert_eq!(
            profile_directory(platform, &lookup),
            Err(quota_core::ports::ProviderError::Authentication)
        );
        assert_eq!(
            codex_auth_file(platform, &lookup),
            Err(quota_core::ports::ProviderError::Authentication)
        );
        assert_eq!(
            claude_credentials_file(platform, &lookup),
            Err(quota_core::ports::ProviderError::Authentication)
        );
        assert_eq!(
            opencode_auth_file(platform, &lookup),
            Err(quota_core::ports::ProviderError::Authentication)
        );
    }
}

/// A blank value is never a path.
#[test]
fn a_blank_value_is_never_a_path() {
    let lookup = environment(&[
        ("HOME", "   "),
        ("USERPROFILE", ""),
        ("APPDATA", "\t"),
        ("XDG_CONFIG_HOME", " "),
    ]);
    for platform in [&Windows as &dyn Platform, &Linux, &Macos, &Unsupported] {
        assert_eq!(platform.user_profile(&lookup), None);
        assert_eq!(platform.application_data(&lookup), None);
    }
}

/// Each platform keeps its own application-data directory.
#[test]
fn each_platform_keeps_its_own_application_data() {
    let windows = environment(&[("APPDATA", "C:\\Users\\someone\\AppData\\Roaming")]);
    assert_eq!(
        Windows.application_data(&windows),
        Some(PathBuf::from("C:\\Users\\someone\\AppData\\Roaming"))
    );

    let xdg = environment(&[
        ("HOME", "/home/someone"),
        ("XDG_CONFIG_HOME", "/home/someone/.config-elsewhere"),
    ]);
    for platform in [&Linux as &dyn Platform, &Unsupported] {
        assert_eq!(
            platform.application_data(&xdg),
            Some(PathBuf::from("/home/someone/.config-elsewhere"))
        );
        let profile_only = environment(&[("HOME", "/home/someone")]);
        assert_eq!(
            platform.application_data(&profile_only),
            Some(PathBuf::from("/home/someone/.config")),
            "XDG_CONFIG_HOME falls back to .config in the profile"
        );
    }

    let mac = environment(&[("HOME", "/Users/someone")]);
    assert_eq!(
        Macos.application_data(&mac),
        Some(
            PathBuf::from("/Users/someone")
                .join("Library")
                .join("Application Support")
        )
    );
}

/// A store crate for a foreign system is never called.
///
/// Every implementation that is not the host's must refuse to open a store.
/// This test never calls the host platform's store, because that would open
/// the person's real credential store; the host store is exercised only by
/// the ignored test `the_system_store_keeps_the_contract` in
/// `crates/quota-providers/tests/secret_store_contract.rs`.
#[test]
fn a_foreign_platform_reports_the_store_unavailable() {
    let hosts: [(&str, &dyn Platform); 4] = [
        ("windows", &Windows),
        ("linux", &Linux),
        ("macos", &Macos),
        ("unsupported", &Unsupported),
    ];
    let host = if cfg!(windows) {
        "windows"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "unsupported"
    };
    let foreign: Vec<(&str, &dyn Platform)> = hosts
        .into_iter()
        .filter(|(name, _)| *name != host)
        .collect();
    assert_eq!(
        foreign.len(),
        3,
        "exactly three of the four implementations are foreign to {host}"
    );
    for (name, platform) in foreign {
        assert_eq!(
            platform
                .open_credential_store("app.quota.monitor.dev")
                .err(),
            Some(SecretStoreError::Unavailable),
            "{name} must refuse a store on a {host} host"
        );
    }
}
