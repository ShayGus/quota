//! `update-manifest` against release directories that break one rule at a time.
//!
//! The fixtures are tiny stand-ins for the packages, signed with a throwaway
//! key whose private half was discarded; `fixtures/update/test-updater.key.pub`
//! is its public half. The command is driven through its own arguments and exit
//! status, which is what the release workflow sees.
#![expect(
    clippy::tests_outside_test_module,
    reason = "an integration test binary holds nothing but tests and compiles without cfg(test)"
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

type Outcome = Result<(), String>;

const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/update");
const PUBLIC_KEY: &str = include_str!("fixtures/update/test-updater.key.pub");
const BASE: &str = "https://github.com/ShayGus/quota/releases/download/v0.1.0";

/// A repository root holding the configuration, and a release directory of the
/// fixture packages and signatures.
struct Release {
    _directory: tempfile::TempDir,
    root: PathBuf,
    packages: PathBuf,
}

fn release_with_version(version: &str) -> Result<Release, String> {
    let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
    let root = directory.path().to_path_buf();
    fs::create_dir_all(root.join("src-tauri")).map_err(|e| e.to_string())?;
    fs::write(
        root.join("src-tauri/tauri.conf.json"),
        format!(
            "{{\"version\": \"{version}\", \"plugins\": {{\"updater\": {{\"pubkey\": \"{}\"}}}}}}\n",
            PUBLIC_KEY.trim()
        ),
    )
    .map_err(|e| e.to_string())?;
    let packages = root.join("release");
    fs::create_dir_all(&packages).map_err(|e| e.to_string())?;
    for entry in fs::read_dir(Path::new(FIXTURES).join("release")).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        fs::copy(entry.path(), packages.join(entry.file_name())).map_err(|e| e.to_string())?;
    }
    Ok(Release {
        _directory: directory,
        root,
        packages,
    })
}

fn release() -> Result<Release, String> {
    release_with_version("0.1.0")
}

/// Runs `update-manifest <action>` and returns (succeeded, report).
fn run(release: &Release, action: &str, extra: &[&str]) -> Result<(bool, String), String> {
    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["update-manifest", action, "--dir"])
        .arg(&release.packages)
        .args(extra)
        .arg("--root")
        .arg(&release.root)
        .output()
        .map_err(|e| e.to_string())?;
    Ok((
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    ))
}

fn succeeds(release: &Release, action: &str, extra: &[&str]) -> Outcome {
    let (passed, report) = run(release, action, extra)?;
    if passed {
        Ok(())
    } else {
        Err(format!("`{action}` must succeed:\n{report}"))
    }
}

fn fails_with(release: &Release, action: &str, needle: &str) -> Outcome {
    let (passed, report) = run(release, action, &[])?;
    if passed || !report.contains(needle) {
        return Err(format!(
            "`{action}` must fail naming `{needle}`; passed={passed}:\n{report}"
        ));
    }
    Ok(())
}

fn assembled() -> Result<Release, String> {
    let release = release()?;
    succeeds(
        &release,
        "assemble",
        &["--version", "0.1.0", "--date", "2026-10-04T00:00:00Z"],
    )?;
    Ok(release)
}

fn manifest(release: &Release) -> Result<serde_json::Value, String> {
    let text =
        fs::read_to_string(release.packages.join("latest.json")).map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

fn write_manifest(release: &Release, manifest: &serde_json::Value) -> Outcome {
    fs::write(
        release.packages.join("latest.json"),
        serde_json::to_string_pretty(manifest).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())
}

#[test]
fn an_assembled_release_verifies() -> Outcome {
    let release = assembled()?;
    succeeds(&release, "verify", &["--version", "0.1.0"])
}

/// Compares two JSON values, naming what was compared when they differ.
fn same(what: &str, found: &serde_json::Value, expected: &str) -> Outcome {
    if found == expected {
        Ok(())
    } else {
        Err(format!("{what}: found {found}, expected {expected}"))
    }
}

#[test]
fn the_list_names_every_platform_and_installer_with_its_own_asset() -> Outcome {
    let release = assembled()?;
    let list = manifest(&release)?;
    same("version", &list["version"], "0.1.0")?;
    same("pub_date", &list["pub_date"], "2026-10-04T00:00:00Z")?;
    let platforms = &list["platforms"];
    for (key, asset) in [
        ("windows-x86_64", "Quota_0.1.0_x64-setup.exe"),
        ("windows-x86_64-nsis", "Quota_0.1.0_x64-setup.exe"),
        ("windows-x86_64-msi", "Quota_0.1.0_x64_en-US.msi"),
        ("linux-x86_64", "Quota_0.1.0_amd64.AppImage"),
        ("linux-x86_64-appimage", "Quota_0.1.0_amd64.AppImage"),
        ("darwin-aarch64", "Quota_0.1.0_aarch64.app.tar.gz"),
        ("darwin-x86_64", "Quota_0.1.0_x64.app.tar.gz"),
    ] {
        same(
            &format!("{key} url"),
            &platforms[key]["url"],
            &format!("{BASE}/{asset}"),
        )?;
        let signature = fs::read_to_string(release.packages.join(format!("{asset}.sig")))
            .map_err(|e| e.to_string())?;
        same(
            &format!("{key} signature"),
            &platforms[key]["signature"],
            signature.trim(),
        )?;
    }
    Ok(())
}

#[test]
fn a_package_without_a_signature_stops_the_assembly() -> Outcome {
    let release = release()?;
    fs::remove_file(release.packages.join("Quota_0.1.0_x64-setup.exe.sig"))
        .map_err(|e| e.to_string())?;
    fails_with(&release, "assemble", "has no signature file")
}

#[test]
fn a_missing_platform_fails_the_verification() -> Outcome {
    let release = release()?;
    for suffix in ["", ".sig"] {
        fs::remove_file(
            release
                .packages
                .join(format!("Quota_0.1.0_aarch64.app.tar.gz{suffix}")),
        )
        .map_err(|e| e.to_string())?;
    }
    succeeds(&release, "assemble", &[])?;
    fails_with(
        &release,
        "verify",
        "the platform `darwin-aarch64` is missing",
    )
}

#[test]
fn a_tampered_package_fails_the_verification() -> Outcome {
    let release = assembled()?;
    fs::write(
        release.packages.join("Quota_0.1.0_amd64.AppImage"),
        b"not what was signed",
    )
    .map_err(|e| e.to_string())?;
    fails_with(
        &release,
        "verify",
        "does not verify `Quota_0.1.0_amd64.AppImage`",
    )
}

#[test]
fn a_signature_that_is_not_the_one_on_disk_fails() -> Outcome {
    let release = assembled()?;
    let mut list = manifest(&release)?;
    let other = fs::read_to_string(release.packages.join("Quota_0.1.0_x64.app.tar.gz.sig"))
        .map_err(|e| e.to_string())?;
    list["platforms"]["linux-x86_64"]["signature"] = other.trim().into();
    write_manifest(&release, &list)?;
    fails_with(
        &release,
        "verify",
        "is not the contents of `Quota_0.1.0_amd64.AppImage.sig`",
    )
}

#[test]
fn a_signature_from_another_key_fails() -> Outcome {
    let release = assembled()?;
    // A real public key, but not the one these fixtures were signed with.
    let other_key = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDI2NzIyMDY4OENFM0ZDNUUKUldSZS9PT01hQ0J5SnFQUllGakd0RmQycTVtS1dUYzVkZ1lVdGt5OFo5cVgvNTUrN2Q5V0I0Sm8K";
    let config = fs::read_to_string(release.root.join("src-tauri/tauri.conf.json"))
        .map_err(|e| e.to_string())?;
    fs::write(
        release.root.join("src-tauri/tauri.conf.json"),
        config.replace(PUBLIC_KEY.trim(), other_key),
    )
    .map_err(|e| e.to_string())?;
    fails_with(&release, "verify", "does not verify")
}

#[test]
fn an_address_outside_this_release_fails() -> Outcome {
    for url in [
        "https://github.com/ShayGus/quota/releases/download/v0.0.9/Quota_0.1.0_amd64.AppImage",
        "https://example.com/v0.1.0/Quota_0.1.0_amd64.AppImage",
        "https://github.com/ShayGus/quota/releases/download/v0.1.0/../v0.0.9/x",
    ] {
        let release = assembled()?;
        let mut list = manifest(&release)?;
        list["platforms"]["linux-x86_64"]["url"] = url.into();
        write_manifest(&release, &list)?;
        fails_with(&release, "verify", "is not an asset of release v0.1.0")?;
    }
    Ok(())
}

#[test]
fn an_asset_that_is_not_in_the_release_fails() -> Outcome {
    let release = assembled()?;
    fs::remove_file(release.packages.join("Quota_0.1.0_x64.app.tar.gz"))
        .map_err(|e| e.to_string())?;
    fails_with(&release, "verify", "is not in the release directory")
}

#[test]
fn a_list_for_another_version_fails() -> Outcome {
    let release = assembled()?;
    let mut list = manifest(&release)?;
    list["version"] = "0.2.0".into();
    write_manifest(&release, &list)?;
    fails_with(&release, "verify", "`version` must be `0.1.0`")
}

#[test]
fn a_release_named_differently_from_the_configuration_fails() -> Outcome {
    let release = release_with_version("0.3.0")?;
    let (passed, report) = run(&release, "assemble", &["--version", "0.1.0"])?;
    if passed || !report.contains("the configuration declares `0.3.0`") {
        return Err(format!("expected a version mismatch:\n{report}"));
    }
    Ok(())
}

#[test]
fn a_missing_list_fails() -> Outcome {
    let release = release()?;
    fails_with(&release, "verify", "cannot read it")
}
