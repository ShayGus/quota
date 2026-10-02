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
    if passed || !named {
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

#[test]
fn default_feature_closures_cannot_enable_inspection() -> Outcome {
    let cases = [
        "default = [\n  'agent-inspection',\n]\n",
        "default = ['inspect']\ninspect = ['agent-inspection']\n",
        "default = ['first']\nfirst = ['second']\nsecond = ['first', 'agent-inspection']\n",
        "default = ['dep:tauri-plugin-mcp']\n",
        "default = ['tauri-plugin-mcp/screenshot']\n",
    ];
    for features in cases {
        let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
        let member = member_manifest().replace("[features]\n", &format!("[features]\n{features}"));
        tree(directory.path(), &workspace_manifest(), &member)?;
        fails_with(directory.path(), "in a default feature set")?;
    }
    Ok(())
}

#[test]
fn optional_false_cannot_be_hidden_by_a_comment() -> Outcome {
    let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
    let member =
        member_manifest().replace("optional = true }", "optional = false } # optional = true");
    tree(directory.path(), &workspace_manifest(), &member)?;
    fails_with(directory.path(), "is not optional")
}

#[test]
fn renamed_inspection_dependencies_are_checked_by_package_identity() -> Outcome {
    let cases = [
        (
            "default = ['dep:inspection']\n",
            "optional = true",
            "in a default feature set",
        ),
        ("", "optional = false", "is not optional"),
    ];
    for (default, optional, diagnostic) in cases {
        let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
        let workspace = workspace_manifest().replace(
            "tauri-plugin-mcp = {",
            "inspection = { package = 'tauri-plugin-mcp',",
        );
        let member = format!(
            "[package]\nname = 'quota-desktop'\n[features]\n{default}agent-inspection = ['dep:inspection']\n[dependencies.inspection]\nworkspace = true\n{optional}\n"
        );
        tree(directory.path(), &workspace, &member)?;
        fails_with(directory.path(), diagnostic)?;
    }
    Ok(())
}

#[test]
fn member_sources_cannot_bypass_the_workspace_pin() -> Outcome {
    let sources = [
        "git = 'https://github.com/P3GLEG/tauri-plugin-mcp', branch = 'main'",
        "version = '0.3.1'",
        "path = '../plugin'",
        "workspace = true, package = 'tauri-plugin-mcp'",
        "workspace = true, git = 'https://github.com/P3GLEG/tauri-plugin-mcp'",
        "workspace = true, rev = 'other'",
        "workspace = true, branch = 'main'",
        "workspace = true, tag = 'latest'",
        "workspace = true, path = '../plugin'",
        "workspace = true, version = '0.3.1'",
        "workspace = true, registry = 'alternate'",
    ];
    for source in sources {
        let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
        let member = member_manifest().replace(
            "workspace = true, optional = true",
            &format!("{source}, optional = true"),
        );
        tree(directory.path(), &workspace_manifest(), &member)?;
        fails_with(
            directory.path(),
            "must inherit the audited workspace dependency",
        )?;
    }
    Ok(())
}

#[test]
fn a_renamed_member_source_is_not_a_workspace_inheritance() -> Outcome {
    let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
    let member = member_manifest()
        .replace("dep:tauri-plugin-mcp", "dep:inspection")
        .replace(
            "tauri-plugin-mcp = { workspace = true",
            "inspection = { package = 'tauri-plugin-mcp', version = '0.3.1'",
        );
    tree(directory.path(), &workspace_manifest(), &member)?;
    fails_with(
        directory.path(),
        "must inherit the audited workspace dependency",
    )
}

#[test]
fn dependent_aliases_are_resolved_across_dependency_representations() -> Outcome {
    let declarations = [
        "[dependencies]\ndesktop = { package = 'quota-desktop', path = '../../src-tauri', features = ['inspect'] }\n",
        "[dependencies.desktop]\npackage = 'quota-desktop'\npath = '../../src-tauri'\nfeatures = [\n 'inspect',\n]\n",
        "[build-dependencies]\ndesktop = { package = 'quota-desktop', features = ['inspect'] }\n",
        "[dev-dependencies]\ndesktop = { package = 'quota-desktop', features = ['inspect'] }\n",
        "[target.'cfg(unix)'.dependencies]\ndesktop = { package = 'quota-desktop', features = ['inspect'] }\n",
        "[dependencies]\ndesktop = { workspace = true }\n",
    ];
    for declaration in declarations {
        let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
        let workspace = if declaration == "[dependencies]\ndesktop = { workspace = true }\n" {
            format!(
                "{}desktop = {{ package = 'quota-desktop', path = 'src-tauri', features = ['inspect'] }}\n",
                workspace_manifest()
            )
        } else {
            workspace_manifest()
        };
        let member = member_manifest().replace(
            "[features]\n",
            "[features]\ninspect = ['nested']\nnested = ['agent-inspection']\n",
        );
        tree(directory.path(), &workspace, &member)?;
        fs::create_dir_all(directory.path().join("crates/probe"))
            .map_err(|error| error.to_string())?;
        fs::write(
            directory.path().join("crates/probe/Cargo.toml"),
            format!("[package]\nname = 'probe'\n{declaration}"),
        )
        .map_err(|error| error.to_string())?;
        fails_with(
            directory.path(),
            "crates/probe/Cargo.toml:1: `desktop` selects",
        )?;
    }
    Ok(())
}

#[test]
fn default_features_follow_forwarded_dependency_aliases() -> Outcome {
    let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
    let member = member_manifest().replace(
        "[features]\n",
        "[features]\ninspect = ['agent-inspection']\n",
    );
    tree(directory.path(), &workspace_manifest(), &member)?;
    fs::create_dir_all(directory.path().join("crates/probe")).map_err(|error| error.to_string())?;
    fs::write(directory.path().join("crates/probe/Cargo.toml"),
        "[package]\nname = 'probe'\n[features]\ndefault = ['indirect']\nindirect = ['desktop/inspect']\n[dependencies]\ndesktop = { package = 'quota-desktop', path = '../../src-tauri', optional = true }\n"
    ).map_err(|error| error.to_string())?;
    fails_with(directory.path(), "in a default feature set")
}

#[test]
fn equivalent_toml_and_unselected_aliases_remain_compliant() -> Outcome {
    let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
    let workspace = format!(
        "[workspace]\nmembers = ['src-tauri']\n[workspace.dependencies.inspection]\npackage = 'tauri-plugin-mcp'\ngit = 'https://github.com/P3GLEG/tauri-plugin-mcp'\nrev = '{PIN}' # branch = 'main'\n"
    );
    let member = "[package]\nname = 'quota-desktop'\n[features]\ndefault = ['safe', 'inspection?/screenshot']\nsafe = ['cycle']\ncycle = ['safe']\ninspect = ['agent-inspection']\nagent-inspection = [\n 'dep:inspection',\n]\n[dependencies.inspection]\nworkspace = true\noptional = true # optional = false\n";
    tree(directory.path(), &workspace, member)?;
    let (passed, report) = gate(directory.path())?;
    if !passed {
        return Err(format!(
            "equivalent TOML and unselected aliases must pass:\n{report}"
        ));
    }
    Ok(())
}

#[test]
fn rejection_assertions_reject_success_and_missing_diagnostics() -> Outcome {
    let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
    tree(directory.path(), &workspace_manifest(), &member_manifest())?;
    if fails_with(directory.path(), "missing diagnostic").is_ok()
        || fails_with(directory.path(), "agent-inspection").is_ok()
    {
        return Err("a successful gate cannot satisfy a rejection assertion".to_string());
    }
    tree(
        directory.path(),
        &workspace_manifest(),
        &member_manifest().replace("optional = true", "optional = false"),
    )?;
    if fails_with(directory.path(), "missing diagnostic").is_ok() {
        return Err("a rejection assertion must require its diagnostic".to_string());
    }
    fails_with(directory.path(), "is not optional")
}

#[test]
fn weak_forwarding_only_enables_inspection_when_the_dependency_is_active() -> Outcome {
    let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
    let member = member_manifest().replace(
        "[features]\n",
        "[features]\ninspect = ['agent-inspection']\n",
    );
    tree(directory.path(), &workspace_manifest(), &member)?;
    fs::create_dir_all(directory.path().join("crates/probe")).map_err(|error| error.to_string())?;
    let probe = "[package]\nname = 'probe'\n[features]\ndefault = ['desktop?/inspect']\n[dependencies]\ndesktop = { package = 'quota-desktop', path = '../../src-tauri', optional = true, default-features = false }\n";
    fs::write(directory.path().join("crates/probe/Cargo.toml"), probe)
        .map_err(|error| error.to_string())?;
    let (passed, report) = gate(directory.path())?;
    if !passed {
        return Err(format!(
            "weak forwarding must leave an inactive dependency disabled:\n{report}"
        ));
    }
    fs::write(
        directory.path().join("crates/probe/Cargo.toml"),
        probe.replace(
            "['desktop?/inspect']",
            "['dep:desktop', 'desktop?/inspect']",
        ),
    )
    .map_err(|error| error.to_string())?;
    fails_with(directory.path(), "in a default feature set")
}

#[test]
fn workspace_and_member_feature_selections_are_additive() -> Outcome {
    let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
    let workspace = format!(
        "{}desktop = {{ package = 'quota-desktop', path = 'src-tauri', default-features = false, features = ['inspect'] }}\n",
        workspace_manifest()
    );
    let member = member_manifest().replace(
        "[features]\n",
        "[features]\ninspect = ['agent-inspection']\n",
    );
    tree(directory.path(), &workspace, &member)?;
    fs::create_dir_all(directory.path().join("crates/probe")).map_err(|error| error.to_string())?;
    fs::write(directory.path().join("crates/probe/Cargo.toml"),
        "[package]\nname = 'probe'\n[dependencies]\ndesktop = { workspace = true, default-features = false, features = [] }\n")
        .map_err(|error| error.to_string())?;
    fails_with(
        directory.path(),
        "crates/probe/Cargo.toml:1: `desktop` selects",
    )
}

#[test]
fn inspection_dependency_ownership_covers_target_and_build_tables() -> Outcome {
    for kind in [
        "dependencies",
        "dev-dependencies",
        "build-dependencies",
        "target.'cfg(unix)'.dependencies",
    ] {
        let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
        tree(directory.path(), &workspace_manifest(), &member_manifest())?;
        fs::create_dir_all(directory.path().join("crates/probe"))
            .map_err(|error| error.to_string())?;
        fs::write(directory.path().join("crates/probe/Cargo.toml"),
            format!("[package]\nname = 'probe'\n[{kind}.inspection]\npackage = 'tauri-plugin-mcp'\nversion = '0.3.1'\noptional = true\n"))
            .map_err(|error| error.to_string())?;
        fails_with(directory.path(), "may only be declared by")?;
    }
    Ok(())
}

#[test]
fn workspace_pins_are_parsed_as_sources_and_commit_values() -> Outcome {
    for source in [
        format!("git = 'https://github.com/P3GLEG/tauri-plugin-mcp', rev = 'prefix@{PIN}'"),
        format!(
            "git = 'https://github.com/P3GLEG/tauri-plugin-mcp', rev = '{PIN}', branch = 'main'"
        ),
        format!("path = 'plugin', rev = '{PIN}', metadata = \"git = repository\""),
    ] {
        let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
        let workspace = format!(
            "[workspace]\nmembers = ['src-tauri']\n[workspace.dependencies]\ntauri-plugin-mcp = {{ {source} }}\n"
        );
        tree(directory.path(), &workspace, &member_manifest())?;
        fails_with(directory.path(), "40-character commit SHA")?;
    }
    Ok(())
}
