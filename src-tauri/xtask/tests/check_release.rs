//! `check-release` against trees that break one inspection rule at a time.
//!
//! The gate is the only thing standing between the development-only inspection
//! plugin and a shipping artifact, so each way it could leak is a test. The
//! gate is driven through its own `--root` option and its exit status, which is
//! what CI and every developer see.
#![expect(
    clippy::tests_outside_test_module,
    reason = "an integration test binary holds nothing but tests and compiles without cfg(test)"
)]

use std::fs;
#[cfg(unix)]
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
#[cfg(unix)]
use std::process::Stdio;
use std::result::Result;

/// The commit the repository pins the plugin to.
const PIN: &str = "c7d271a06469bdf4744bfdeadca7458a1f3d02e5";

/// What one gate call reports.
type Outcome = Result<(), String>;

/// The workspace half of the manifest a compliant tree carries.
fn workspace_manifest() -> String {
    format!(
        "[workspace]\nmembers = [\"crates/*\"]\n\n\
         [workspace.dependencies]\n\
         tauri-plugin-mcp = {{ git = \"https://github.com/P3GLEG/tauri-plugin-mcp\", rev = \"{PIN}\" }}\n"
    )
}

/// The desktop half of the manifest a compliant tree carries.
fn member_manifest() -> String {
    "[package]\nname = \"quota-desktop\"\nversion = \"0.1.0\"\n\n\
     [features]\nagent-inspection = [\"dep:tauri-plugin-mcp\"]\n\n\
     [dependencies]\ntauri-plugin-mcp = { workspace = true, optional = true }\n"
        .to_string()
}

/// Writes the smallest tree the gate reads: the workspace manifest, which is
/// also the desktop host's, `deny.toml`, one workflow with a pinned action,
/// a `tauri.conf.json`, and a minimal provider transport module, all where
/// the repository keeps them.
fn tree(root: &Path, workspace: &str, member: &str) -> Result<(), String> {
    let write = |path: PathBuf, body: &str| -> Result<(), String> {
        fs::write(&path, body).map_err(|error| format!("{}: {error}", path.display()))
    };
    fs::create_dir_all(root.join("src-tauri")).map_err(|e| e.to_string())?;
    fs::create_dir_all(root.join(".github/workflows")).map_err(|e| e.to_string())?;
    fs::create_dir_all(root.join("src-tauri/crates/quota-providers/src"))
        .map_err(|e| e.to_string())?;
    write(
        root.join("src-tauri/Cargo.toml"),
        &format!("{member}\n{workspace}"),
    )?;
    write(
        root.join("src-tauri/deny.toml"),
        "[licenses]\nallow = [\"MIT\", \"Apache-2.0\", \"Unicode-3.0\", \"BSD-3-Clause\"]\n",
    )?;
    write(
        root.join(".github/workflows/ci.yml"),
        &format!("jobs:\n  build:\n    steps:\n      - uses: actions/checkout@{PIN}\n"),
    )?;
    write(
        root.join("package.json"),
        "{\"name\": \"@quota/desktop\", \"version\": \"0.1.0\"}\n",
    )?;
    write(
        root.join("src-tauri/tauri.conf.json"),
        &configuration(ENDPOINT),
    )?;
    write(
        root.join("src-tauri/tauri.dev.conf.json"),
        "{\"bundle\": {\"createUpdaterArtifacts\": false}}\n",
    )?;
    updater_sources(root)?;
    write(
        root.join("src-tauri/crates/quota-providers/src/http.rs"),
        "//! Minimal provider transport fixture for the release gate.\n\
         //! The gate reads this file as text; it is never compiled.\n\
         #[cfg(not(feature = \"test-fixtures\"))]\n\
         pub(crate) fn endpoint(fixed: &str) -> &str {\n\
         fixed\n\
         }\n\
         \n\
         #[cfg(feature = \"test-fixtures\")]\n\
         pub(crate) fn retarget(base: &str) {\n\
         let _ = base;\n\
         }\n",
    )
}

/// The one address a release may check.
const ENDPOINT: &str = "https://github.com/ShayGus/quota/releases/latest/download/latest.json";

/// A public key `tauri signer generate` wrote, used only by these fixtures.
const TEST_PUBLIC_KEY: &str = include_str!("fixtures/update/test-updater.key.pub");

/// A shipping configuration with the updater set up as the repository does.
fn configuration(endpoint: &str) -> String {
    format!(
        "{{\"version\": \"0.1.0\", \"app\": {{\"security\": {{\"csp\": \"default-src 'self'\"}}}},\n\
         \"bundle\": {{\"createUpdaterArtifacts\": true}},\n\
         \"plugins\": {{\"updater\": {{\"pubkey\": \"{}\", \"endpoints\": [\"{endpoint}\"], \"requireSignedVersion\": true,\n\
         \"windows\": {{\"installMode\": \"passive\"}}}}}}}}\n",
        TEST_PUBLIC_KEY.trim()
    )
}

/// The host sources the update checks read, as a compliant tree has them.
fn updater_sources(root: &Path) -> Result<(), String> {
    let updates = root.join("src-tauri/src/updates");
    fs::create_dir_all(&updates).map_err(|e| e.to_string())?;
    let write =
        |name: &str, body: &str| fs::write(updates.join(name), body).map_err(|e| e.to_string());
    write(
        "mod.rs",
        "#[cfg(feature = \"sample-data\")]\nmod test_endpoint;\n\
         fn start() {\n    match may_check_for_updates(&facts) {\n\
         UpdateDecision::Skip(reason) => {}\n    UpdateDecision::Check => spawn(flow),\n    }\n}\n",
    )?;
    write(
        "host.rs",
        "fn check() {\n    let builder = app.updater_builder();\n\
         #[cfg(feature = \"sample-data\")]\n\
         let builder = super::test_endpoint::apply(builder)?;\n}\n",
    )?;
    write(
        "test_endpoint.rs",
        "fn apply(builder: B) { builder.endpoints(vec![url]); }\n",
    )?;
    fs::write(
        root.join("src-tauri/src/bootstrap.rs"),
        "fn start() { builder.plugin(tauri_plugin_updater::Builder::new().build()); }\n",
    )
    .map_err(|e| e.to_string())
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
        "[workspace]\nmembers = [\"crates/*\"]\n\n\
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
    fs::create_dir_all(root.join("src-tauri/crates/probe")).map_err(|e| e.to_string())?;
    fs::write(
        root.join("src-tauri/crates/probe/Cargo.toml"),
        "[package]\nname = \"probe\"\n\n\
         [dependencies]\n\
         quota-desktop = { path = \"../..\", features = [\"agent-inspection\"] }\n",
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
        "[dependencies]\ndesktop = { package = 'quota-desktop', path = '../..', features = ['inspect'] }\n",
        "[dependencies.desktop]\npackage = 'quota-desktop'\npath = '../..'\nfeatures = [\n 'inspect',\n]\n",
        "[build-dependencies]\ndesktop = { package = 'quota-desktop', path = '../..', features = ['inspect'] }\n",
        "[dev-dependencies]\ndesktop = { package = 'quota-desktop', path = '../..', features = ['inspect'] }\n",
        "[target.'cfg(unix)'.dependencies]\ndesktop = { package = 'quota-desktop', path = '../..', features = ['inspect'] }\n",
        "[dependencies]\ndesktop = { workspace = true }\n",
    ];
    for declaration in declarations {
        let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
        let workspace = if declaration == "[dependencies]\ndesktop = { workspace = true }\n" {
            format!(
                "{}desktop = {{ package = 'quota-desktop', path = '.', features = ['inspect'] }}\n",
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
        fs::create_dir_all(directory.path().join("src-tauri/crates/probe"))
            .map_err(|error| error.to_string())?;
        fs::write(
            directory.path().join("src-tauri/crates/probe/Cargo.toml"),
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
    fs::create_dir_all(directory.path().join("src-tauri/crates/probe"))
        .map_err(|error| error.to_string())?;
    fs::write(directory.path().join("src-tauri/crates/probe/Cargo.toml"),
        "[package]\nname = 'probe'\n[features]\ndefault = ['indirect']\nindirect = ['desktop/inspect']\n[dependencies]\ndesktop = { package = 'quota-desktop', path = '../..', optional = true }\n"
    ).map_err(|error| error.to_string())?;
    fails_with(directory.path(), "in a default feature set")
}

#[test]
fn equivalent_toml_and_unselected_aliases_remain_compliant() -> Outcome {
    let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
    let workspace = format!(
        "[workspace]\nmembers = ['crates/*']\n[workspace.dependencies.inspection]\npackage = 'tauri-plugin-mcp'\ngit = 'https://github.com/P3GLEG/tauri-plugin-mcp'\nrev = '{PIN}' # branch = 'main'\n"
    );
    let member = "[package]\nname = 'quota-desktop'\nversion = '0.1.0'\n[features]\ndefault = ['safe', 'inspection?/screenshot']\nsafe = ['cycle']\ncycle = ['safe']\ninspect = ['agent-inspection']\nagent-inspection = [\n 'dep:inspection',\n]\n[dependencies.inspection]\nworkspace = true\noptional = true # optional = false\n";
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
    fs::create_dir_all(directory.path().join("src-tauri/crates/probe"))
        .map_err(|error| error.to_string())?;
    let probe = "[package]\nname = 'probe'\n[features]\ndefault = ['desktop?/inspect']\n[dependencies]\ndesktop = { package = 'quota-desktop', path = '../..', optional = true, default-features = false }\n";
    fs::write(
        directory.path().join("src-tauri/crates/probe/Cargo.toml"),
        probe,
    )
    .map_err(|error| error.to_string())?;
    let (passed, report) = gate(directory.path())?;
    if !passed {
        return Err(format!(
            "weak forwarding must leave an inactive dependency disabled:\n{report}"
        ));
    }
    fs::write(
        directory.path().join("src-tauri/crates/probe/Cargo.toml"),
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
        "{}desktop = {{ package = 'quota-desktop', path = '.', default-features = false, features = ['inspect'] }}\n",
        workspace_manifest()
    );
    let member = member_manifest().replace(
        "[features]\n",
        "[features]\ninspect = ['agent-inspection']\n",
    );
    tree(directory.path(), &workspace, &member)?;
    fs::create_dir_all(directory.path().join("src-tauri/crates/probe"))
        .map_err(|error| error.to_string())?;
    fs::write(directory.path().join("src-tauri/crates/probe/Cargo.toml"),
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
        fs::create_dir_all(directory.path().join("src-tauri/crates/probe"))
            .map_err(|error| error.to_string())?;
        fs::write(directory.path().join("src-tauri/crates/probe/Cargo.toml"),
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
            "[workspace]\nmembers = ['crates/*']\n[workspace.dependencies]\ntauri-plugin-mcp = {{ {source} }}\n"
        );
        tree(directory.path(), &workspace, &member_manifest())?;
        fails_with(directory.path(), "40-character commit SHA")?;
    }
    Ok(())
}

fn write_manifest(root: &Path, name: &str, body: &str) -> Outcome {
    let directory = root.join("src-tauri/crates").join(name);
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    fs::write(directory.join("Cargo.toml"), body).map_err(|error| error.to_string())
}

#[test]
fn incoming_features_are_unified_before_weak_forwarding() -> Outcome {
    let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
    let root = directory.path();
    tree(root, &workspace_manifest(), &member_manifest())?;
    write_manifest(
        root,
        "bridge",
        "[package]\nname = 'bridge'\n[features]\nactivate = ['dep:desktop']\ninspect = ['desktop?/agent-inspection']\n[dependencies]\ndesktop = { package = 'quota-desktop', path = '../..', optional = true, default-features = false }\n",
    )?;
    write_manifest(
        root,
        "left",
        "[package]\nname = 'left'\n[dependencies]\nbridge = { path = '../bridge', features = ['activate'] }\n",
    )?;
    write_manifest(
        root,
        "right",
        "[package]\nname = 'right'\n[dependencies]\nbridge = { path = '../bridge', features = ['inspect'] }\n",
    )?;
    write_manifest(
        root,
        "probe",
        "[package]\nname = 'probe'\n[dependencies]\nleft = { path = '../left' }\nright = { path = '../right' }\n",
    )?;
    fails_with(root, "a release build must never select it")
}

#[test]
fn optional_dependencies_without_default_features_still_receive_features() -> Outcome {
    for activation in ["dep:bridge", "bridge/activate"] {
        let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
        let root = directory.path();
        tree(root, &workspace_manifest(), &member_manifest())?;
        write_manifest(
            root,
            "bridge",
            "[package]\nname = 'bridge'\n[features]\nactivate = ['dep:desktop']\ninspect = ['desktop?/agent-inspection']\n[dependencies]\ndesktop = { package = 'quota-desktop', path = '../..', optional = true, default-features = false }\n",
        )?;
        write_manifest(
            root,
            "probe",
            &format!(
                "[package]\nname = 'probe'\n[features]\ndefault = ['{activation}']\n[dependencies]\nbridge = {{ path = '../bridge', optional = true, default-features = false, features = ['activate', 'inspect'] }}\n"
            ),
        )?;
        fails_with(root, "in a default feature set")?;
    }
    Ok(())
}

#[test]
fn unselected_optional_dependency_features_do_not_activate_inspection() -> Outcome {
    let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
    let root = directory.path();
    tree(root, &workspace_manifest(), &member_manifest())?;
    write_manifest(
        root,
        "probe",
        "[package]\nname = 'probe'\n[dependencies]\ndesktop = { package = 'quota-desktop', path = '../..', optional = true, default-features = false, features = ['agent-inspection'] }\n",
    )?;
    let (passed, report) = gate(root)?;
    if !passed {
        return Err(format!(
            "an inactive optional dependency must stay disabled:\n{report}"
        ));
    }
    Ok(())
}

#[test]
fn identically_named_packages_from_different_paths_do_not_unify() -> Outcome {
    let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
    let root = directory.path();
    tree(root, &workspace_manifest(), &member_manifest())?;
    let bridge = "[package]\nname = 'bridge'\n[features]\nactivate = ['dep:desktop']\ninspect = ['desktop?/agent-inspection']\n[dependencies]\ndesktop = { package = 'quota-desktop', path = '../..', optional = true, default-features = false }\n";
    write_manifest(
        root,
        "bridge-left",
        &bridge.replace("name = 'bridge'", "name = 'bridge'\nversion = '1.0.0'"),
    )?;
    write_manifest(
        root,
        "bridge-right",
        &bridge.replace("name = 'bridge'", "name = 'bridge'\nversion = '2.0.0'"),
    )?;
    write_manifest(
        root,
        "probe",
        "[package]\nname = 'probe'\n[dependencies]\nleft = { package = 'bridge', path = '../bridge-left', features = ['activate'] }\nright = { package = 'bridge', path = '../bridge-right', features = ['inspect'] }\n",
    )?;
    let (passed, report) = gate(root)?;
    if !passed {
        return Err(format!(
            "different package sources must not share features:\n{report}"
        ));
    }
    Ok(())
}

#[test]
fn inspection_source_overrides_are_rejected_in_all_manifest_forms() -> Outcome {
    let overrides = [
        "[patch.'https://github.com/P3GLEG/tauri-plugin-mcp']\ntauri-plugin-mcp = { path = 'plugin' }\n",
        "[patch.'https://github.com/P3GLEG/tauri-plugin-mcp'.inspection]\npackage = 'tauri-plugin-mcp'\npath = 'plugin'\n",
        "[patch.crates-io]\ninspection = { package = 'tauri-plugin-mcp', git = 'https://example.org/plugin', rev = 'other' }\n",
        "[replace]\n\"tauri-plugin-mcp:0.3.1\" = { path = 'plugin' }\n",
        "[replace]\n\"inspection:0.3.1\" = { package = 'tauri-plugin-mcp', path = 'plugin' }\n",
        "[replace]\n\"https://github.com/P3GLEG/tauri-plugin-mcp#tauri-plugin-mcp:0.3.1\" = { path = 'plugin' }\n",
        "[replace]\n\"https://github.com/P3GLEG/tauri-plugin-mcp#0.3.1\" = { path = 'plugin' }\n",
    ];
    for source_override in overrides {
        let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
        tree(
            directory.path(),
            &format!("{}\n{source_override}", workspace_manifest()),
            &member_manifest(),
        )?;
        fails_with(directory.path(), "source override is forbidden")?;
    }
    Ok(())
}

#[test]
fn unrelated_source_overrides_remain_allowed() -> Outcome {
    let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
    tree(
        directory.path(),
        &format!(
            "{}\n[patch.crates-io]\nserde = {{ path = 'serde' }}\n",
            workspace_manifest()
        ),
        &member_manifest(),
    )?;
    let (passed, report) = gate(directory.path())?;
    if !passed {
        return Err(format!(
            "unrelated source overrides must remain allowed:\n{report}"
        ));
    }
    Ok(())
}

fn renderer_tree(root: &Path, main: &str) -> Outcome {
    tree(root, &workspace_manifest(), &member_manifest())?;
    fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
    fs::write(root.join("src/main.tsx"), main).map_err(|error| error.to_string())
}

#[test]
fn renderer_source_rejects_unguarded_inspection_without_javascript_runtime() -> Outcome {
    for main in [
        "import { setupPluginListeners } from 'tauri-plugin-mcp'; if (import.meta.env.DEV) setupPluginListeners();",
        "import('tauri-plugin-mcp');",
        "export { setupPluginListeners } from 'tauri-plugin-mcp';",
        "import '../node_modules/tauri-plugin-mcp/index.js';",
        "if (!import.meta.env.DEV) { import('tauri-plugin-mcp'); }",
        "if (import.meta.env.DEV) {} else { import('tauri-plugin-mcp'); }",
        "if (import.meta.env.DEV || true) { import('tauri-plugin-mcp'); }",
        "if (import.meta.env.DEV) { console.log('development'); } import('tauri-plugin-mcp');",
        "function load() { return import('tauri-plugin-mcp'); } if (import.meta.env.DEV) load();",
        "require('tauri-plugin-mcp');",
        "import inspection = require('tauri-plugin-mcp');",
        r"import 'tauri\u002dplugin-mcp';",
        r"import('tauri\u002dplugin-mcp');",
        "import('tauri-' + 'plugin-mcp');",
        "import(`tauri-plugin-mcp`);",
    ] {
        let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
        renderer_tree(directory.path(), main)?;
        let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
            .args(["check-release", "--root"])
            .arg(directory.path())
            .env("PATH", "")
            .output()
            .map_err(|error| error.to_string())?;
        let report = String::from_utf8_lossy(&output.stdout);
        if output.status.success() || !report.contains("inspection guest imports must be dynamic") {
            return Err(format!("unguarded guest import passed: {main}\n{report}"));
        }
    }
    Ok(())
}

#[test]
fn renderer_source_accepts_development_import_and_ordinary_renderer() -> Outcome {
    for main in [
        "if (import.meta.env.DEV) { import('tauri-plugin-mcp').then(({ setupPluginListeners }) => setupPluginListeners()).catch(console.error); } document.title = 'Quota';",
        "if (import.meta.env.DEV) { if (document.hidden) { import('tauri-plugin-mcp'); } }",
        "// import 'tauri-plugin-mcp'\ndocument.title = 'tauri-plugin-mcp is a development tool';",
        "/* if (import.meta.env.DEV) { */ import { invoke } from '@tauri-apps/api/core';",
        r#"const view = <div title="tauri-plugin-mcp">Quota</div>;"#,
    ] {
        let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
        renderer_tree(directory.path(), main)?;
        let output = Command::new(env!("CARGO_BIN_EXE_xtask"))
            .args(["check-release", "--root"])
            .arg(directory.path())
            .env("PATH", "")
            .output()
            .map_err(|error| error.to_string())?;
        if !output.status.success() {
            return Err(format!(
                "ordinary renderer failed: {main}\n{}",
                String::from_utf8_lossy(&output.stdout)
            ));
        }
    }
    Ok(())
}

#[test]
fn renderer_source_checks_sibling_modules_and_rejects_parse_errors() -> Outcome {
    let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
    renderer_tree(directory.path(), "import './inspection.js';")?;
    fs::write(
        directory.path().join("src/inspection.js"),
        "import 'tauri-plugin-mcp';",
    )
    .map_err(|error| error.to_string())?;
    fails_with(directory.path(), "inspection guest imports must be dynamic")?;
    fs::write(
        directory.path().join("src/inspection.js"),
        "if (import.meta.env.DEV { import('tauri-plugin-mcp'); }",
    )
    .map_err(|error| error.to_string())?;
    fails_with(directory.path(), "cannot parse renderer source")
}

#[test]
fn strong_optional_forwarding_activates_same_named_parent_feature() -> Outcome {
    for table in [
        "dependencies",
        "build-dependencies",
        "target.'cfg(unix)'.dependencies",
    ] {
        let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
        let root = directory.path();
        tree(root, &workspace_manifest(), &member_manifest())?;
        write_manifest(
            root,
            "bridge",
            &format!(
                "[package]\nname = 'bridge'\n[features]\ndefault = ['desktop/custom-protocol']\ndesktop = ['dep:desktop', 'desktop?/agent-inspection']\n[{table}]\ndesktop = {{ package = 'quota-desktop', path = '../..', optional = true, default-features = false }}\n"
            ),
        )?;
        fails_with(root, "a release build must never select it")?;
    }
    Ok(())
}

#[test]
fn strong_optional_forwarding_activates_implicit_parent_feature() -> Outcome {
    let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
    let root = directory.path();
    tree(root, &workspace_manifest(), &member_manifest())?;
    write_manifest(
        root,
        "bridge",
        "[package]\nname = 'bridge'\n[features]\ndefault = ['desktop/custom-protocol', 'desktop?/agent-inspection']\n[dependencies]\ndesktop = { package = 'quota-desktop', path = '../..', optional = true, default-features = false }\n",
    )?;
    fails_with(root, "a release build must never select it")
}

#[test]
fn weak_optional_forwarding_does_not_activate_same_named_parent_feature() -> Outcome {
    for selection in ["desktop?/custom-protocol", "dep:desktop"] {
        let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
        let root = directory.path();
        tree(root, &workspace_manifest(), &member_manifest())?;
        write_manifest(
            root,
            "bridge",
            &format!(
                "[package]\nname = 'bridge'\n[features]\ndefault = ['{selection}']\ndesktop = ['dep:desktop', 'desktop?/agent-inspection']\n[dependencies]\ndesktop = {{ package = 'quota-desktop', path = '../..', optional = true, default-features = false }}\n"
            ),
        )?;
        let (passed, report) = gate(root)?;
        if !passed {
            return Err(format!(
                "weak or dep: forwarding activated a parent feature:\n{report}"
            ));
        }
    }
    Ok(())
}

fn capability_tree(root: &Path) -> Outcome {
    tree(root, &workspace_manifest(), &member_manifest())?;
    fs::create_dir_all(root.join("src-tauri/capabilities")).map_err(|error| error.to_string())
}

#[test]
fn inspection_grants_cannot_borrow_settings_command_authority() -> Outcome {
    let grants = [
        r#"{"identifier":"inspection","windows":["overview","settings"],"permissions":["mcp:default"]}"#,
        r#"{"identifier":"inspection","windows":["settings"],"permissions":["mcp:default"]}"#,
        r#"{"identifier":"inspection","windows":["*"],"permissions":["mcp:default"]}"#,
        r#"{"identifier":"inspection","windows":["over*"],"permissions":["mcp:default"]}"#,
        r#"{"identifier":"inspection","permissions":["mcp:default"]}"#,
        r#"{"identifier":"inspection","webviews":["settings"],"permissions":["mcp:default"]}"#,
        r#"{"identifier":"inspection","windows":["overview"],"webviews":["settings"],"permissions":["mcp:default"]}"#,
        r#"{"identifier":"inspection","windows":["settings"],"permissions":[{"identifier":"mcp:allow-push-ipc"}]}"#,
        r#"[{"identifier":"inspection","windows":["settings"],"permissions":["mcp:allow-push-log"]}]"#,
        r#"{"capabilities":[{"identifier":"inspection","windows":["settings"],"permissions":["mcp:default"]}]}"#,
    ];
    for grant in grants {
        let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
        capability_tree(directory.path())?;
        fs::write(
            directory.path().join("src-tauri/capabilities/other.json"),
            grant,
        )
        .map_err(|error| error.to_string())?;
        fails_with(
            directory.path(),
            "inspection permissions must be restricted to the overview window",
        )?;
    }
    Ok(())
}

#[test]
fn inspection_scope_is_checked_in_toml_and_inline_capabilities() -> Outcome {
    let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
    capability_tree(directory.path())?;
    let grant = directory.path().join("src-tauri/capabilities/other.toml");
    fs::write(
        &grant,
        "identifier = 'inspection'\nwindows = ['settings']\npermissions = ['mcp:default']\n",
    )
    .map_err(|error| error.to_string())?;
    fails_with(
        directory.path(),
        "inspection permissions must be restricted to the overview window",
    )?;
    fs::remove_file(grant).map_err(|error| error.to_string())?;
    for name in ["tauri.conf.json", "tauri.linux.conf.json"] {
        fs::write(directory.path().join("src-tauri").join(name),
            r#"{"version":"0.1.0","app":{"security":{"csp":"default-src 'self'","capabilities":[{"identifier":"inspection","windows":["settings"],"permissions":["mcp:allow-push-ipc"]}]}}}"#)
            .map_err(|error| error.to_string())?;
        fails_with(
            directory.path(),
            "inspection permissions must be restricted to the overview window",
        )?;
        fs::remove_file(directory.path().join("src-tauri").join(name))
            .map_err(|error| error.to_string())?;
    }
    for name in ["Tauri.toml", "Tauri.linux.toml"] {
        fs::write(directory.path().join("src-tauri").join(name),
            "[app.security]\ncapabilities = [{ identifier = 'inspection', windows = ['settings'], permissions = ['mcp:default'] }]\n")
            .map_err(|error| error.to_string())?;
        fails_with(
            directory.path(),
            "inspection permissions must be restricted to the overview window",
        )?;
        fs::remove_file(directory.path().join("src-tauri").join(name))
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[test]
fn overview_inspection_leaves_settings_capabilities_independent() -> Outcome {
    let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
    capability_tree(directory.path())?;
    fs::write(
        directory
            .path()
            .join("src-tauri/capabilities/inspection.json"),
        r#"{"identifier":"inspection","windows":["overview"],"permissions":["mcp:default"]}"#,
    )
    .map_err(|error| error.to_string())?;
    fs::write(directory.path().join("src-tauri/capabilities/settings.json"),
        r#"{"identifier":"settings","windows":["settings"],"permissions":["allow-clear-local-history","allow-update-preferences"]}"#)
        .map_err(|error| error.to_string())?;
    let (passed, report) = gate(directory.path())?;
    if !passed {
        return Err(format!(
            "overview-only inspection must preserve settings authority:\n{report}"
        ));
    }
    Ok(())
}

/// The version a compliant tree declares in all three files.
const VERSION: &str = "0.1.0";

#[test]
fn a_drifted_package_manifest_version_fails() -> Outcome {
    let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
    tree(directory.path(), &workspace_manifest(), &member_manifest())?;
    fs::write(
        directory.path().join("package.json"),
        "{\"version\": \"9.9.9\"}\n",
    )
    .map_err(|error| error.to_string())?;
    fails_with(directory.path(), "does not match src-tauri/tauri.conf.json")
}

#[test]
fn a_drifted_desktop_manifest_version_fails() -> Outcome {
    let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
    let member = member_manifest().replace(VERSION, "9.9.9");
    tree(directory.path(), &workspace_manifest(), &member)?;
    fails_with(directory.path(), "does not match src-tauri/tauri.conf.json")
}

#[test]
fn a_mirror_without_a_version_fails() -> Outcome {
    let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
    tree(directory.path(), &workspace_manifest(), &member_manifest())?;
    fs::remove_file(directory.path().join("package.json")).map_err(|error| error.to_string())?;
    fails_with(directory.path(), "cannot be read")
}

#[test]
fn an_authority_without_a_version_fails() -> Outcome {
    let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
    tree(directory.path(), &workspace_manifest(), &member_manifest())?;
    fs::write(
        directory.path().join("src-tauri/tauri.conf.json"),
        "{\"app\": {\"security\": {\"csp\": \"default-src 'self'\"}}}\n",
    )
    .map_err(|error| error.to_string())?;
    fails_with(directory.path(), "declares no `version`")
}

#[cfg(unix)]
fn ci_runs_pass(runs: &str) -> Result<bool, String> {
    if runs.is_empty() {
        return Ok(false);
    }
    let mut grep = Command::new("grep")
        .args(["-qvFx", "completed success"])
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;
    let Some(mut stdin) = grep.stdin.take() else {
        return Err("grep input was not piped".to_string());
    };
    stdin
        .write_all(runs.as_bytes())
        .map_err(|error| error.to_string())?;
    drop(stdin);
    match grep.wait().map_err(|error| error.to_string())?.code() {
        Some(0) => Ok(false),
        Some(1) => Ok(true),
        code => Err(format!("grep returned unexpected exit code {code:?}")),
    }
}

#[cfg(unix)]
#[test]
fn release_preflight_requires_all_ci_runs_to_succeed() -> Outcome {
    let workflow = include_str!("../../../.github/workflows/release.yml");
    if !workflow.contains("grep -qvFx 'completed success'") {
        return Err("release preflight must match the full successful CI line".to_string());
    }
    if !workflow.contains("if [ -z \"$runs\" ]; then") {
        return Err("release preflight must reject commits with no CI runs".to_string());
    }
    for (runs, expected) in [
        ("completed success\ncompleted success", true),
        ("completed success\ncompleted failure", false),
        ("in_progress null", false),
        ("completed cancelled", false),
        ("", false),
    ] {
        let passed = ci_runs_pass(runs)?;
        if passed != expected {
            return Err(format!(
                "CI run output {runs:?} passed as {passed}, expected {expected}"
            ));
        }
    }
    Ok(())
}

/// Replaces one file of a compliant tree and asks the gate about it.
fn rejects_after(edit: impl FnOnce(&Path) -> Result<(), String>, needle: &str) -> Outcome {
    let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
    tree(directory.path(), &workspace_manifest(), &member_manifest())?;
    edit(directory.path())?;
    fails_with(directory.path(), needle)
}

fn write_file(root: &Path, file: &str, body: &str) -> Outcome {
    let path = root.join(file);
    fs::create_dir_all(path.parent().ok_or("no parent")?).map_err(|e| e.to_string())?;
    fs::write(path, body).map_err(|e| e.to_string())
}

#[test]
fn another_update_address_fails() -> Outcome {
    rejects_after(
        |root| {
            write_file(
                root,
                "src-tauri/tauri.conf.json",
                &configuration("https://example.com/latest.json"),
            )
        },
        "must be exactly",
    )
}

#[test]
fn a_missing_or_malformed_public_key_fails() -> Outcome {
    rejects_after(
        |root| {
            let broken = configuration(ENDPOINT).replace(TEST_PUBLIC_KEY.trim(), "bm90IGEga2V5");
            write_file(root, "src-tauri/tauri.conf.json", &broken)
        },
        "plugins.updater.pubkey",
    )
}

#[test]
fn a_weakened_updater_setting_fails() -> Outcome {
    for setting in ["dangerousInsecureTransportProtocol", "allowDowngrades"] {
        rejects_after(
            |root| {
                let weakened = configuration(ENDPOINT)
                    .replace("\"windows\"", &format!("\"{setting}\": true, \"windows\""));
                write_file(root, "src-tauri/tauri.conf.json", &weakened)
            },
            setting,
        )?;
    }
    Ok(())
}

#[test]
fn an_update_list_that_is_not_bound_to_the_signed_version_fails() -> Outcome {
    rejects_after(
        |root| {
            let unbound = configuration(ENDPOINT).replace("\"requireSignedVersion\": true,", "");
            write_file(root, "src-tauri/tauri.conf.json", &unbound)
        },
        "requireSignedVersion",
    )
}

#[test]
fn a_release_that_makes_no_signatures_fails() -> Outcome {
    rejects_after(
        |root| {
            let unsigned = configuration(ENDPOINT).replace("true", "false");
            write_file(root, "src-tauri/tauri.conf.json", &unsigned)
        },
        "createUpdaterArtifacts",
    )
}

#[test]
fn an_overlay_cannot_set_plugins() -> Outcome {
    rejects_after(
        |root| {
            write_file(
                root,
                "src-tauri/tauri.dev.conf.json",
                "{\"bundle\": {\"createUpdaterArtifacts\": false}, \"plugins\": {\"updater\": {}}}\n",
            )
        },
        "must not set `plugins`",
    )
}

#[test]
fn the_development_overlay_needs_no_signing_key() -> Outcome {
    rejects_after(
        |root| write_file(root, "src-tauri/tauri.dev.conf.json", "{}\n"),
        "createUpdaterArtifacts",
    )
}

#[test]
fn the_updates_module_cannot_set_an_address_or_read_the_environment() -> Outcome {
    for (line, token) in [
        ("builder.endpoints(vec![url]);", ".endpoints("),
        ("builder.pubkey(key);", ".pubkey("),
        ("let target = std::env::var(\"X\");", "std::env"),
        ("let _ = env::var(\"X\");", "env::var"),
        ("let proxy = builder.proxy(url);", ".proxy("),
    ] {
        rejects_after(
            |root| {
                write_file(
                    root,
                    "src-tauri/src/updates/flow.rs",
                    &format!("fn f() {{\n    {line}\n}}\n"),
                )
            },
            token,
        )?;
    }
    Ok(())
}

#[test]
fn only_the_host_file_may_build_an_updater() -> Outcome {
    rejects_after(
        |root| {
            write_file(
                root,
                "src-tauri/src/updates/flow.rs",
                "fn f() { let updater = app.updater(); }\n",
            )
        },
        "only `host.rs` may build an updater",
    )
}

#[test]
fn the_updater_is_registered_the_one_way_outside_its_module() -> Outcome {
    rejects_after(
        |root| {
            write_file(
                root,
                "src-tauri/src/bootstrap.rs",
                "fn start() { tauri_plugin_updater::Builder::new().pubkey(k).build(); }\n",
            )
        },
        "may only be registered as",
    )?;
    rejects_after(
        |root| {
            write_file(
                root,
                "src-tauri/src/other.rs",
                "fn f() { let u = app.updater_builder(); }\n",
            )
        },
        "may only be registered as",
    )
}

#[test]
fn the_test_endpoint_must_be_compiled_out_of_a_release() -> Outcome {
    rejects_after(
        |root| {
            write_file(
                root,
                "src-tauri/src/updates/host.rs",
                "fn check() {\n    let builder = super::test_endpoint::apply(builder)?;\n}\n",
            )
        },
        "the test endpoint must be gated",
    )?;
    rejects_after(
        |root| {
            write_file(
                root,
                "src-tauri/src/updates/mod.rs",
                "mod test_endpoint;\nfn start() { may_check_for_updates(); UpdateDecision::Skip; spawn(); }\n",
            )
        },
        "the test endpoint must be gated",
    )
}

#[test]
fn no_window_may_be_granted_the_updater() -> Outcome {
    for permission in [
        "updater:default",
        "process:allow-restart",
        "dialog:allow-ask",
    ] {
        rejects_after(
            |root| {
                write_file(
                    root,
                    "src-tauri/capabilities/rogue-capability.json",
                    &format!(
                        "{{\"identifier\": \"rogue\", \"windows\": [\"overview\"], \"permissions\": [\"core:default\", \"{permission}\"]}}\n"
                    ),
                )
            },
            "must not be granted to a window",
        )?;
    }
    Ok(())
}

#[test]
fn the_flow_must_decide_before_it_starts() -> Outcome {
    rejects_after(
        |root| {
            write_file(
                root,
                "src-tauri/src/updates/mod.rs",
                "fn start() { spawn(flow); }\n",
            )
        },
        "must call `may_check_for_updates`",
    )
}

#[test]
fn the_sample_data_feature_cannot_be_a_default() -> Outcome {
    let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
    tree(
        directory.path(),
        &workspace_manifest(),
        "[package]\nname = \"quota-desktop\"\n\n\
         [features]\ndefault = [\"sample-data\"]\nsample-data = []\n\
         agent-inspection = [\"dep:tauri-plugin-mcp\"]\n\n\
         [dependencies]\ntauri-plugin-mcp = { workspace = true, optional = true }\n",
    )?;
    fails_with(directory.path(), "test-only feature in the default set")
}

#[test]
fn a_workflow_cannot_reconfigure_a_build() -> Outcome {
    for line in [
        "      TAURI_CONFIG: '{\"plugins\": {}}'",
        "        run: bun tauri build --config other.json",
    ] {
        rejects_after(
            |root| {
                write_file(
                    root,
                    ".github/workflows/release.yml",
                    &format!(
                        "jobs:\n  build:\n    steps:\n      - uses: actions/checkout@{PIN}\n{line}\n"
                    ),
                )
            },
            "would let a build use another updater address",
        )?;
    }
    Ok(())
}

#[test]
fn a_release_build_selects_no_features() -> Outcome {
    rejects_after(
        |root| {
            write_file(
                root,
                ".github/workflows/release.yml",
                &format!(
                    "jobs:\n  build:\n    steps:\n      - uses: actions/checkout@{PIN}\n      - run: cargo build --features sample-data\n"
                ),
            )
        },
        "a release build selects no features",
    )
}

#[test]
fn the_signing_key_reaches_only_a_build_step() -> Outcome {
    let workflow = |step: &str| {
        format!("jobs:\n  build:\n    steps:\n      - uses: actions/checkout@{PIN}\n{step}")
    };
    let key = "          TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}\n";
    let build = format!("      - name: Build\n        env:\n{key}        run: bun tauri build\n");
    let test_step = format!("      - name: Test\n        env:\n{key}        run: cargo test\n");

    let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
    tree(directory.path(), &workspace_manifest(), &member_manifest())?;
    write_file(
        directory.path(),
        ".github/workflows/release.yml",
        &workflow(&build),
    )?;
    let (passed, report) = gate(directory.path())?;
    if !passed {
        return Err(format!("a signing build step must pass:\n{report}"));
    }
    rejects_after(
        |root| write_file(root, ".github/workflows/release.yml", &workflow(&test_step)),
        "may only reach a step that runs `tauri build`",
    )?;
    rejects_after(
        |root| write_file(root, ".github/workflows/ci.yml", &workflow(&build)),
        "may only reach a step that runs `tauri build`",
    )
}
