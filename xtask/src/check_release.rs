//! `check-release`: the audited release surface, verified statically.

use std::path::Path;

use crate::cargo_manifest;
use crate::outcome::Outcome;
use crate::scan;
use crate::toml::{self, Document};

/// Licences the dependency record requires to be allowed.
const REQUIRED_LICENCES: [&str; 4] = ["MIT", "Apache-2.0", "Unicode-3.0", "BSD-3-Clause"];

/// Keys whose value is a Git reference in a workflow step.
const USES_KEY: &str = "uses:";

/// Runs the gate against the repository at `root`.
#[must_use]
pub(crate) fn run(root: &Path) -> Outcome {
    let mut outcome = Outcome::default();
    cargo_manifest::check_release_features(root, &mut outcome);
    cargo_manifest::check_dependency_feature_selection(root, &mut outcome);
    check_deny_config(root, &mut outcome);
    check_workflow_pins(root, &mut outcome);
    check_tauri_config(root, &mut outcome);
    outcome
}

/// `deny.toml` must exist and allow the required licences.
fn check_deny_config(root: &Path, outcome: &mut Outcome) {
    let path = root.join("deny.toml");
    if !path.is_file() {
        outcome.fail(
            "deny.toml".to_string(),
            1,
            "deny.toml is missing".to_string(),
        );
        return;
    }
    let Ok(text) = scan::read(&path) else {
        outcome.fail(
            "deny.toml".to_string(),
            1,
            "deny.toml cannot be read".to_string(),
        );
        return;
    };
    let document = Document::parse(&text);
    let allow = document
        .entry("licenses", "allow")
        .map_or(String::new(), |entry| entry.value.clone());
    outcome.note(format!(
        "deny.toml: {} licence(s) allowed",
        allow.matches('"').count() / 2
    ));
    for licence in REQUIRED_LICENCES {
        if !allow.contains(&format!("\"{licence}\"")) {
            outcome.fail(
                "deny.toml".to_string(),
                document.table_line("licenses").unwrap_or(1),
                format!("licence `{licence}` is not in the allowed set"),
            );
        }
    }
}

/// Every workflow `uses:` must name a full 40-character commit SHA.
fn check_workflow_pins(root: &Path, outcome: &mut Outcome) {
    let directory = root.join(".github/workflows");
    if !directory.is_dir() {
        outcome.fail(
            ".github/workflows".to_string(),
            1,
            "no workflow directory exists".to_string(),
        );
        return;
    }
    let workflows = scan::files_with_extension(&directory, "yml")
        .into_iter()
        .chain(scan::files_with_extension(&directory, "yaml"))
        .collect::<Vec<_>>();
    if workflows.is_empty() {
        outcome.fail(
            ".github/workflows".to_string(),
            1,
            "no workflow files exist".to_string(),
        );
        return;
    }
    let mut pins = 0;
    let mut unpinned = 0;
    for workflow in &workflows {
        let Ok(text) = scan::read(workflow) else {
            continue;
        };
        let file = scan::relative(root, workflow);
        for (index, line) in text.lines().enumerate() {
            // A comment explains the pin policy; it is not a pin.
            if line.trim_start().starts_with('#') {
                continue;
            }
            let Some(position) = line.find(USES_KEY) else {
                continue;
            };
            let reference = toml::strip_comment(&line[position + USES_KEY.len()..])
                .trim()
                .trim_matches('"')
                .trim_matches('\'');
            if reference.is_empty() {
                continue;
            }
            pins += 1;
            if !is_commit_sha(reference) {
                unpinned += 1;
                outcome.fail(
                    file.clone(),
                    index + 1,
                    format!(
                        "action `{reference}` is not pinned to a 40-character commit SHA; a tag or branch can be moved"
                    ),
                );
            }
        }
    }
    outcome.note(format!(
        "{} workflow file(s), {pins} action reference(s), {unpinned} unpinned",
        workflows.len()
    ));
}

/// Reports whether a `uses:` value ends in a full commit SHA.
fn is_commit_sha(reference: &str) -> bool {
    let Some(sha) = reference.rsplit('@').next() else {
        return false;
    };
    sha.len() == 40 && sha.chars().all(|character| character.is_ascii_hexdigit())
}

/// No Tauri configuration may ship devtools or a wildcard content policy.
fn check_tauri_config(root: &Path, outcome: &mut Outcome) {
    let configs = scan::files_with_extension(root, "json")
        .into_iter()
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name == "tauri.conf.json")
        })
        .collect::<Vec<_>>();
    if configs.is_empty() {
        outcome.note(
            "no tauri.conf.json yet; the desktop host is not scaffolded, so this item is pending"
                .to_string(),
        );
        return;
    }
    for config in &configs {
        let Ok(text) = scan::read(config) else {
            continue;
        };
        let file = scan::relative(root, config);
        for (index, line) in text.lines().enumerate() {
            if line.contains("devtools") && line.contains("true") {
                outcome.fail(
                    file.clone(),
                    index + 1,
                    "`devtools: true` must not be in a shipping configuration".to_string(),
                );
            }
            if line.contains("\"csp\"") && (line.contains('*') || line.contains("null")) {
                outcome.fail(
                    file.clone(),
                    index + 1,
                    "the content security policy must not be a wildcard or null".to_string(),
                );
            }
        }
    }
    outcome.note(format!("{} tauri.conf.json file(s) checked", configs.len()));
}
