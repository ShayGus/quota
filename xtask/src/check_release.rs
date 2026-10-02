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
    check_agent_inspection(root, &mut outcome);
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

/// The crate and feature that carry development-only agent inspection.
const INSPECTION_CRATE: &str = "tauri-plugin-mcp";
const INSPECTION_FEATURE: &str = "agent-inspection";

/// The only manifest allowed to declare either name.
const INSPECTION_OWNER: &str = "src-tauri/Cargo.toml";

/// Reports anything that could put the inspection plugin in a release build.
///
/// The plugin must never reach a shipping artifact, and each rule below closes
/// a way that could happen on its own:
///
/// - the dependency must be `optional = true`, because Cargo compiles and
///   links an ordinary dependency into every profile, release included;
/// - the feature must be declared, and only as a non-default feature, because
///   a feature in a `default = [...]` list is selected by every ordinary
///   build of the host;
/// - nothing may name the feature except that one declaration, so a dependent
///   cannot switch it on with a `features = [...]` dependency entry;
/// - the git source must be pinned to a 40-character commit, because a branch
///   or tag can move after review.
fn check_agent_inspection(root: &Path, outcome: &mut Outcome) {
    let mut dependencies = 0;
    let mut features = 0;
    let mut pins = 0;
    for manifest in scan::files_with_extension(root, "toml")
        .into_iter()
        .filter(|path| path.file_name().is_some_and(|name| name == "Cargo.toml"))
    {
        let Ok(text) = scan::read(&manifest) else {
            continue;
        };
        let file = scan::relative(root, &manifest);
        let document = Document::parse(&text);
        let (declared, pinned) = check_inspection_crate(&file, &document, outcome);
        dependencies += declared;
        pins += pinned;
        features += check_inspection_feature(&file, &document, outcome);
        check_default_sets(&file, &text, outcome);
    }
    if dependencies != 1 || features != 1 || pins != 1 {
        outcome.fail(
            INSPECTION_OWNER.to_string(),
            1,
            format!(
                "expected one `{INSPECTION_CRATE}` dependency, one workspace pin, and one `{INSPECTION_FEATURE}` feature; found {dependencies}, {pins}, and {features}"
            ),
        );
    }
    outcome.note(format!(
        "{INSPECTION_FEATURE}: {dependencies} optional dependency, {pins} workspace pin(s), {features} feature declaration(s)"
    ));
}

/// Counts and checks the crate declarations in one document, returning how many
/// real dependencies and how many workspace pins it holds.
fn check_inspection_crate(
    file: &str,
    document: &Document,
    outcome: &mut Outcome,
) -> (usize, usize) {
    let mut dependencies = 0;
    let mut pins = 0;
    for entry in document.all() {
        if !toml::is_dependency_table(&entry.table) {
            continue;
        }
        // A dependent selects the feature from its own dependency line, so
        // this runs before the plugin-specific checks skip the entry.
        if entry.value.contains(INSPECTION_FEATURE) {
            outcome.fail(
                file.to_string(),
                entry.line,
                format!(
                    "`{}` selects `{INSPECTION_FEATURE}`; a release build must never select it",
                    entry.key
                ),
            );
            continue;
        }
        if entry.key != INSPECTION_CRATE {
            continue;
        }
        if entry.table.starts_with("workspace") {
            // The workspace table is the pin, not a build edge. Its member
            // copy carries no `git`, so this is where the revision lives.
            pins += 1;
            if !git_revision(&entry.value).is_some_and(is_commit_sha) {
                outcome.fail(
                    file.to_string(),
                    entry.line,
                    format!("`{INSPECTION_CRATE}` must be pinned to a 40-character commit SHA"),
                );
            }
            continue;
        }
        dependencies += 1;
        if file != INSPECTION_OWNER {
            outcome.fail(
                file.to_string(),
                entry.line,
                format!("`{INSPECTION_CRATE}` may only be declared by `{INSPECTION_OWNER}`"),
            );
        }
        if !entry.value.contains("optional = true") {
            outcome.fail(
                file.to_string(),
                entry.line,
                format!(
                    "`{INSPECTION_CRATE}` is not optional, so every profile compiles and links it"
                ),
            );
        }
    }
    (dependencies, pins)
}

/// Counts and checks the `agent-inspection` declaration in one document.
fn check_inspection_feature(file: &str, document: &Document, outcome: &mut Outcome) -> usize {
    if !document.all().any(|entry| entry.key == INSPECTION_FEATURE) {
        return 0;
    }
    let only_declaration = file == INSPECTION_OWNER
        && document.get("features", INSPECTION_FEATURE) == Some("[\"dep:tauri-plugin-mcp\"]");
    if !only_declaration {
        outcome.fail(
            file.to_string(),
            1,
            format!(
                "`{INSPECTION_FEATURE}` may only be declared once, in `{INSPECTION_OWNER}` as a non-default feature"
            ),
        );
    }
    1
}

/// Reports an inspection feature inside a `default = [...]` list.
fn check_default_sets(file: &str, text: &str, outcome: &mut Outcome) {
    for (index, line) in text.lines().enumerate() {
        let trimmed = toml::strip_comment(line).trim();
        if trimmed.starts_with("default = [") && trimmed.contains(INSPECTION_FEATURE) {
            outcome.fail(
                file.to_string(),
                index + 1,
                format!(
                    "`{trimmed}` puts `{INSPECTION_FEATURE}` in a default feature set; release builds must exclude it"
                ),
            );
        }
    }
}

/// Reports the `rev` a `git = "..."` dependency value pins, without its quotes.
fn git_revision(value: &str) -> Option<&str> {
    if !value.contains("git = ") {
        return None;
    }
    let tail = value.split_once("rev = ")?.1.trim_start();
    Some(
        tail.trim_start_matches(['"', '\''])
            .split(['"', '\''])
            .next()
            .unwrap_or_default(),
    )
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
