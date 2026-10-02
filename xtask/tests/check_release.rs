//! `check-release` against trees that break one inspection rule at a time.
//!
//! The gate is the only thing standing between the development-only inspection
//! plugin and a shipping artifact, so each way it could leak is a test. The
//! gate is driven through its own `--root` option and its exit status, which is
//! what CI and every developer see.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::result::Result;

/// The commit the repository pins the plugin to.
const PIN: &str = "c7d271a06469bdf4744bfdeadca7458a1f3d02e5";

/// What one gate call reports.
type Outcome = Result<(), String>;

/// The workspace manifest a compliant tree carries.
fn workspace_manifest() -> String {
    format!(
        "[workspace]\nmembers = [\"src-tauri\"]\n\n\
         [workspace.dependencies]\n\
         tauri-plugin-mcp = {{ git = \"https://github.com/P3GLEG/tauri-plugin-mcp\", rev = \"{PIN}\" }}\n"
    )
}

/// The desktop manifest a compliant tree carries.
fn member_manifest() -> String {
    "[package]\nname = \"quota-desktop\"\n\n\
     [features]\nagent-inspection = [\"dep:tauri-plugin-mcp\"]\n\n\
     [dependencies]\ntauri-plugin-mcp = { workspace = true, optional = true }\n"
        .to_string()
}

/// Writes the smallest tree the gate reads: two manifests, `deny.toml`, one
/// workflow with a pinned action, and a `tauri.conf.json`.
fn tree(root: &Path, workspace: &str, member: &str) -> Result<(), String> {
    let write = |path: PathBuf, body: &str| -> Result<(), String> {
        fs::write(&path, body).map_err(|error| format!("{}: {error}", path.display()))
    };
    fs::create_dir_all(root.join("src-tauri")).map_err(|e| e.to_string())?;
    fs::create_dir_all(root.join(".github/workflows")).map_err(|e| e.to_string())?;
    write(root.join("Cargo.toml"), workspace)?;
    write(root.join("src-tauri/Cargo.toml"), member)?;
    write(
        root.join("deny.toml"),
        "[licenses]\nallow = [\"MIT\", \"Apache-2.0\", \"Unicode-3.0\", \"BSD-3-Clause\"]\n",
    )?;
    write(
        root.join(".github/workflows/ci.yml"),
        &format!("jobs:\n  build:\n    steps:\n      - uses: actions/checkout@{PIN}\n"),
    )?;
    write(
        root.join("src-tauri/tauri.conf.json"),
        "{\"app\": {\"security\": {\"csp\": \"default-src 'self'\"}}}\n",
    )
}

/// Runs the gate over `root` and returns whether it exited successfully.
fn gate(root: &Path) -> Result<(bool, String), String> {
    let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .args(["check-release", "--root"])
        .arg(root)
        .output()
        .map_err(|error| error.to_string())?;
    let report = String::from_utf8_lossy(&output.stdout).into_owned();
    Ok((output.status.success(), report))
}

/// Asserts the gate rejects `root` for the reason named by `needle`.
fn fails_with(root: &Path, needle: &str) -> Outcome {
    let (passed, report) = gate(root)?;
    let named = report.contains(needle);
    if passed == named {
        return Err(format!(
            "expected a failure naming `{needle}`, got status {passed} and:\n{report}"
        ));
    }
    Ok(())
}

#[test]
fn a_compliant_tree_passes() -> Result<(), String> {
    let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
    let root = directory.path();
    tree(root, &workspace_manifest(), &member_manifest())?;
    let (passed, report) = gate(root)?;
    if !passed {
        return Err(format!("a compliant tree must pass:\n{report}"));
    }
    Ok(())
}

#[test]
fn a_non_optional_dependency_fails() -> Result<(), String> {
    let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
    let root = directory.path();
    tree(
        root,
        &workspace_manifest(),
        "[package]\nname = \"quota-desktop\"\n\n\
         [features]\nagent-inspection = [\"dep:tauri-plugin-mcp\"]\n\n\
         [dependencies]\ntauri-plugin-mcp = { workspace = true }\n",
    )?;
    fails_with(root, "is not optional")
}

#[test]
fn the_feature_in_a_default_set_fails() -> Result<(), String> {
    let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
    let root = directory.path();
    tree(
        root,
        &workspace_manifest(),
        "[package]\nname = \"quota-desktop\"\n\n\
         [features]\ndefault = [\"agent-inspection\"]\n\
         agent-inspection = [\"dep:tauri-plugin-mcp\"]\n\n\
         [dependencies]\ntauri-plugin-mcp = { workspace = true, optional = true }\n",
    )?;
    fails_with(root, "in a default feature set")
}

#[test]
fn a_branch_instead_of_a_commit_fails() -> Result<(), String> {
    let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
    let root = directory.path();
    tree(
        root,
        "[workspace]\nmembers = [\"src-tauri\"]\n\n\
         [workspace.dependencies]\n\
         tauri-plugin-mcp = { git = \"https://github.com/P3GLEG/tauri-plugin-mcp\", branch = \"main\" }\n",
        &member_manifest(),
    )?;
    fails_with(root, "40-character commit SHA")
}

#[test]
fn a_dependent_selecting_the_feature_fails() -> Result<(), String> {
    let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
    let root = directory.path();
    tree(root, &workspace_manifest(), &member_manifest())?;
    fs::create_dir_all(root.join("crates/probe")).map_err(|e| e.to_string())?;
    fs::write(
        root.join("crates/probe/Cargo.toml"),
        "[package]\nname = \"probe\"\n\n\
         [dependencies]\n\
         quota-desktop = { path = \"../../src-tauri\", features = [\"agent-inspection\"] }\n",
    )
    .map_err(|e| e.to_string())?;
    fails_with(root, "a release build must never select it")
}

#[test]
fn a_dropped_declaration_fails() -> Result<(), String> {
    let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
    let root = directory.path();
    tree(
        root,
        &workspace_manifest(),
        "[package]\nname = \"quota-desktop\"\n\n[dependencies]\nanyhow = \"1.0.92\"\n",
    )?;
    fails_with(
        root,
        "one workspace pin, and one `agent-inspection` feature",
    )
}
