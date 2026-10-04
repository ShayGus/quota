//! The update surface of `check-release`.
//!
//! An update is code that replaces the installed program, so the address it is
//! fetched from and the key it is checked with are the two things an attacker
//! would change. This gate keeps them in one place, `plugins.updater` in
//! `src-tauri/tauri.conf.json`, and keeps every other route to them closed:
//!
//! - the configuration names exactly the published release feed, the public
//!   key, and no setting that weakens transport or version checks;
//! - no Rust source outside the updates module touches the updater, and the
//!   module itself never sets an address, key, target, header, proxy, or reads
//!   the environment (one test-only file may set an address, and it is compiled
//!   only with `sample-data`, which the policy keeps out of the update flow);
//! - no window is granted an updater, process, or dialog permission;
//! - no workflow reconfigures a build, selects features, or hands the signing
//!   key to a step that is not a build.

use std::path::Path;

use serde_json::Value;

use crate::outcome::Outcome;
use crate::scan;

/// The only address a release may check.
pub(crate) const ENDPOINT: &str =
    "https://github.com/ShayGus/quota/releases/latest/download/latest.json";

/// Where the release's update list and packages are published.
pub(crate) const DOWNLOAD_BASE: &str = "https://github.com/ShayGus/quota/releases/download";

const CONFIG: &str = "src-tauri/tauri.conf.json";
const DEV_CONFIG: &str = "src-tauri/tauri.dev.conf.json";
const UPDATES: &str = "src-tauri/src/updates";
const TEST_ENDPOINT: &str = "test_endpoint.rs";
const HOST: &str = "host.rs";
const SAMPLE_GATE: &str = "#[cfg(feature = \"sample-data\")]";

/// Updater settings that weaken a check, in any spelling the plugin accepts.
const WEAKENING_SETTINGS: [&str; 4] = [
    "dangerousInsecureTransportProtocol",
    "dangerousAcceptInvalidCerts",
    "dangerousAcceptInvalidHostnames",
    "allowDowngrades",
];

/// Source that sets what the configuration alone must decide.
const FORBIDDEN_IN_UPDATES: [&str; 17] = [
    ".endpoints(",
    ".pubkey(",
    ".target(",
    ".header(",
    ".headers(",
    ".proxy(",
    ".no_proxy(",
    ".installer_arg",
    ".version_comparator(",
    "default_version_comparator",
    "configure_client",
    "dangerous",
    "allow_downgrades",
    "env::var",
    "std::env",
    "option_env!",
    "env!(",
];

/// The one place the plugin is registered, and the only way it may be.
const REGISTRATION: &str = "tauri_plugin_updater::Builder::new().build()";

/// Runs every update check against the tree at `root`.
pub(crate) fn check(root: &Path, outcome: &mut Outcome) {
    check_configuration(root, outcome);
    check_other_configurations(root, outcome);
    check_capabilities(root, outcome);
    check_sources(root, outcome);
    check_workflows(root, outcome);
}

fn read_json(root: &Path, file: &str, outcome: &mut Outcome) -> Option<Value> {
    let parsed = scan::read(&root.join(file))
        .and_then(|text| serde_json::from_str::<Value>(&text).map_err(|error| error.to_string()));
    match parsed {
        Ok(value) => Some(value),
        Err(error) => {
            outcome.fail(file.to_string(), 1, format!("cannot read {file}: {error}"));
            None
        }
    }
}

/// `plugins.updater`: the feed, the key, and the settings around them.
fn check_configuration(root: &Path, outcome: &mut Outcome) {
    let Some(config) = read_json(root, CONFIG, outcome) else {
        return;
    };
    let fail = |outcome: &mut Outcome, message: &str| {
        outcome.fail(CONFIG.to_string(), 1, message.to_string());
    };
    let Some(updater) = config.pointer("/plugins/updater") else {
        fail(
            outcome,
            "`plugins.updater` is missing, so the release cannot update",
        );
        return;
    };
    let endpoints = updater.get("endpoints").and_then(Value::as_array);
    if endpoints.map(|list| list.iter().map(Value::as_str).collect::<Vec<_>>())
        != Some(vec![Some(ENDPOINT)])
    {
        fail(
            outcome,
            &format!("`plugins.updater.endpoints` must be exactly [\"{ENDPOINT}\"]"),
        );
    }
    let key_is_minisign = updater
        .get("pubkey")
        .and_then(Value::as_str)
        .and_then(crate::update_manifest::decode_base64_text)
        .is_some_and(|text| text.starts_with("untrusted comment: minisign public key"));
    if !key_is_minisign {
        fail(
            outcome,
            "`plugins.updater.pubkey` must be the base64 public key `tauri signer generate` writes",
        );
    }
    for setting in WEAKENING_SETTINGS {
        if updater.get(setting).is_some() {
            fail(
                outcome,
                &format!("`plugins.updater.{setting}` must not be set in a shipping configuration"),
            );
        }
    }
    if updater.get("requireSignedVersion") != Some(&Value::Bool(true)) {
        fail(
            outcome,
            "`plugins.updater.requireSignedVersion` must be true, so a stale signature cannot be paired with a newer version number",
        );
    }
    if updater
        .pointer("/windows/installMode")
        .and_then(Value::as_str)
        != Some("passive")
    {
        fail(
            outcome,
            "`plugins.updater.windows.installMode` must be \"passive\"",
        );
    }
    if config.pointer("/bundle/createUpdaterArtifacts") != Some(&Value::Bool(true)) {
        fail(
            outcome,
            "`bundle.createUpdaterArtifacts` must be true, or a release produces no signatures",
        );
    }
    outcome.note(
        "updater configuration: one feed, one public key, passive Windows install".to_string(),
    );
}

/// No other Tauri configuration may carry updater settings, so no overlay can
/// redirect the feed or replace the key.
fn check_other_configurations(root: &Path, outcome: &mut Outcome) {
    let mut inspected = 0;
    for path in scan::files_with_extension(root, "json") {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        let file = scan::relative(root, &path);
        if !(name.starts_with("tauri.") && name.ends_with("conf.json")) || file == CONFIG {
            continue;
        }
        inspected += 1;
        let Some(overlay) = read_json(root, &file, outcome) else {
            continue;
        };
        if overlay.get("plugins").is_some() {
            outcome.fail(
                file.clone(),
                1,
                "an overlay configuration must not set `plugins`; the updater feed and key live in tauri.conf.json alone".to_string(),
            );
        }
        if file == DEV_CONFIG
            && overlay.pointer("/bundle/createUpdaterArtifacts") != Some(&Value::Bool(false))
        {
            outcome.fail(
                file,
                1,
                "the development configuration must set `bundle.createUpdaterArtifacts` to false, so a local build needs no signing key".to_string(),
            );
        }
    }
    outcome.note(format!(
        "{inspected} overlay configuration(s) checked for updater settings"
    ));
}

/// Permission prefixes of the plugins that could run an update from a window.
const UPDATE_PLUGIN_PERMISSIONS: [&str; 3] = ["updater:", "process:", "dialog:"];

/// No window may be granted the updater, the process plugin, or a dialog: the
/// host runs every update, and a window can only answer the host's own pop-up.
fn check_capabilities(root: &Path, outcome: &mut Outcome) {
    let directory = root.join(scan::WORKSPACE).join("capabilities");
    let mut inspected = 0;
    for path in scan::files_with_extension(&directory, "json") {
        inspected += 1;
        let file = scan::relative(root, &path);
        let Some(capability) = read_json(root, &file, outcome) else {
            continue;
        };
        let permissions = capability
            .get("permissions")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|permission| {
                permission
                    .as_str()
                    .or_else(|| permission.get("identifier").and_then(Value::as_str))
            });
        for permission in permissions {
            if UPDATE_PLUGIN_PERMISSIONS
                .iter()
                .any(|prefix| permission.starts_with(prefix))
            {
                outcome.fail(
                    file.clone(),
                    1,
                    format!("`{permission}` must not be granted to a window; the host runs every update and a window only answers its pop-up"),
                );
            }
        }
    }
    outcome.note(format!(
        "{inspected} capability file(s) checked for update permissions"
    ));
}

/// The Rust host: who may touch the updater, and what they may set.
fn check_sources(root: &Path, outcome: &mut Outcome) {
    let source = root.join(scan::WORKSPACE).join("src");
    let updates = root.join(UPDATES);
    let mut inspected = 0;
    for path in scan::files_with_extension(&source, "rs") {
        let file = scan::relative(root, &path);
        let Ok(text) = scan::read(&path) else {
            continue;
        };
        inspected += 1;
        if path.starts_with(&updates) {
            check_updates_file(&file, &text, outcome);
        } else {
            check_outside_file(&file, &text, outcome);
        }
    }
    check_module_wiring(root, outcome);
    outcome.note(format!(
        "{inspected} host source file(s) checked for updater use"
    ));
}

fn code_lines(text: &str) -> impl Iterator<Item = (usize, &str)> {
    text.lines()
        .enumerate()
        .map(|(index, line)| (index + 1, line.trim()))
        .filter(|(_, line)| !line.starts_with("//"))
}

/// A file inside the updates module.
fn check_updates_file(file: &str, text: &str, outcome: &mut Outcome) {
    let is_test_endpoint = file.ends_with(&format!("/{TEST_ENDPOINT}"));
    let lines = code_lines(text).collect::<Vec<_>>();
    for (number, line) in &lines {
        if !is_test_endpoint {
            for token in FORBIDDEN_IN_UPDATES {
                if line.contains(token) {
                    outcome.fail(
                        file.to_string(),
                        *number,
                        format!("`{token}` must not appear in the updates module; the feed, key, target and transport come from tauri.conf.json alone"),
                    );
                }
            }
        }
        if (line.contains("updater_builder(") || line.contains(".updater()"))
            && !file.ends_with(&format!("/{HOST}"))
        {
            outcome.fail(
                file.to_string(),
                *number,
                format!("only `{HOST}` may build an updater"),
            );
        }
    }
    // Anything that reaches the test endpoint must be compiled out of a release.
    for (position, (number, line)) in lines.iter().enumerate() {
        if line.contains("test_endpoint::") || *line == "mod test_endpoint;" {
            let gated = position
                .checked_sub(1)
                .and_then(|before| lines.get(before))
                .is_some_and(|(_, previous)| *previous == SAMPLE_GATE);
            if !gated {
                outcome.fail(
                    file.to_string(),
                    *number,
                    format!("the test endpoint must be gated by `{SAMPLE_GATE}` on the line above, so no release build contains it"),
                );
            }
        }
    }
}

/// A host file outside the updates module may register the plugin and nothing more.
fn check_outside_file(file: &str, text: &str, outcome: &mut Outcome) {
    for (number, line) in code_lines(text) {
        let without_registration = line.replace(REGISTRATION, "");
        if without_registration.contains("tauri_plugin_updater")
            || line.contains("updater_builder(")
        {
            outcome.fail(
                file.to_string(),
                number,
                format!("the updater may only be registered as `{REGISTRATION}`; everything else belongs in the updates module"),
            );
        }
    }
}

/// The module must decide through the one policy function before it starts.
fn check_module_wiring(root: &Path, outcome: &mut Outcome) {
    let file = format!("{UPDATES}/mod.rs");
    let Ok(text) = scan::read(&root.join(&file)) else {
        outcome.fail(file, 1, "the updates module is missing".to_string());
        return;
    };
    let policy_first = text.find("may_check_for_updates(").zip(text.find("spawn("));
    if policy_first.is_none_or(|(decision, spawn)| decision >= spawn)
        || !text.contains("UpdateDecision::Skip")
    {
        outcome.fail(
            file,
            1,
            "`start` must call `may_check_for_updates` and handle `UpdateDecision::Skip` before it spawns anything".to_string(),
        );
    }
}

/// Workflows: no reconfigured build, no feature selection, no loose secrets.
fn check_workflows(root: &Path, outcome: &mut Outcome) {
    let directory = root.join(".github/workflows");
    let mut inspected = 0;
    for path in scan::files_with_extension(&directory, "yml") {
        let Ok(text) = scan::read(&path) else {
            continue;
        };
        inspected += 1;
        let file = scan::relative(root, &path);
        let lines = text.lines().collect::<Vec<_>>();
        for (index, line) in lines.iter().enumerate() {
            let code = line.trim();
            if code.starts_with('#') {
                continue;
            }
            check_workflow_line(&file, index, &lines, outcome);
        }
    }
    outcome.note(format!(
        "{inspected} workflow(s) checked for update-signing hygiene"
    ));
}

fn check_workflow_line(file: &str, index: usize, lines: &[&str], outcome: &mut Outcome) {
    let line = lines.get(index).copied().unwrap_or("");
    let number = index + 1;
    let release = file.ends_with("release.yml");
    for token in ["TAURI_CONFIG", "--config"] {
        if line.contains(token) {
            outcome.fail(
                file.to_string(),
                number,
                format!("`{token}` would let a build use another updater address or key; the configuration is the only source"),
            );
        }
    }
    if release && (line.contains("--features") || line.contains("--all-features")) {
        outcome.fail(
            file.to_string(),
            number,
            "a release build selects no features; it is built from the default set".to_string(),
        );
    }
    if line.contains("TAURI_SIGNING_PRIVATE_KEY") && !signing_step_builds(file, index, lines) {
        outcome.fail(
            file.to_string(),
            number,
            "the signing key may only reach a step that runs `tauri build`, in release.yml"
                .to_string(),
        );
    }
}

/// Whether the step around line `index` is a build in the release workflow.
fn signing_step_builds(file: &str, index: usize, lines: &[&str]) -> bool {
    if !file.ends_with("release.yml") {
        return false;
    }
    let is_step_start = |line: &&str| line.trim_start().starts_with("- ");
    let start = (0..=index)
        .rev()
        .find(|at| lines.get(*at).is_some_and(is_step_start))
        .unwrap_or(0);
    let end = (index + 1..lines.len())
        .find(|at| lines.get(*at).is_some_and(is_step_start))
        .unwrap_or(lines.len());
    lines
        .get(start..end)
        .unwrap_or_default()
        .iter()
        .any(|line| line.contains("tauri build"))
}
