//! `check-release`: the audited release surface, verified statically.

use std::collections::BTreeSet;
use std::path::Path;

use cargo_toml::Value;

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
    let manifests: Vec<(String, Value)> = scan::files_with_extension(root, "toml")
        .into_iter()
        .filter(|path| path.file_name().is_some_and(|name| name == "Cargo.toml"))
        .filter_map(|path| {
            let file = scan::relative(root, &path);
            let result = scan::read(&path).and_then(|text| {
                cargo_toml::from_str::<Value>(&text).map_err(|error| error.to_string())
            });
            match result {
                Ok(document) => Some((file, document)),
                Err(error) => {
                    outcome.fail(file, 1, format!("cannot parse Cargo manifest: {error}"));
                    None
                }
            }
        })
        .collect();
    let workspace = manifests
        .iter()
        .find(|(file, _)| file == "Cargo.toml")
        .and_then(|(_, document)| document.get("workspace"))
        .and_then(|workspace| workspace.get("dependencies"))
        .and_then(Value::as_table);
    let mut dependencies = 0;
    let mut features = 0;
    let mut pins = 0;
    for (file, document) in &manifests {
        pins += check_inspection_pin(file, document, outcome);
        let plugin_aliases = check_inspection_crate(file, document, workspace, outcome);
        dependencies += plugin_aliases.len();
        let workspace_entries = document
            .get("workspace")
            .and_then(|workspace| workspace.get("dependencies"))
            .and_then(Value::as_table)
            .into_iter()
            .flatten()
            .map(|(alias, value)| (alias.as_str(), value));
        for (alias, specification) in dependency_entries(document)
            .into_iter()
            .chain(workspace_entries)
        {
            if dependency_selects_inspection(
                alias,
                specification,
                &[],
                &manifests,
                workspace,
                &mut BTreeSet::new(),
            ) {
                outcome.fail(file.clone(), 1,
                    format!("`{alias}` selects `{INSPECTION_FEATURE}`; a release build must never select it"));
            }
        }
        features += check_inspection_feature(file, document, &plugin_aliases, outcome);
        if selects_inspection(
            file,
            document,
            &["default"],
            &manifests,
            workspace,
            &mut BTreeSet::new(),
        ) {
            outcome.fail(file.clone(), 1,
                format!("`{INSPECTION_FEATURE}` is in a default feature set; release builds must exclude it"));
        }
    }
    if dependencies != 1 || features != 1 || pins != 1 {
        outcome.fail(INSPECTION_OWNER.to_string(), 1,
            format!("expected one `{INSPECTION_CRATE}` dependency, one workspace pin, and one `{INSPECTION_FEATURE}` feature; found {dependencies}, {pins}, and {features}"));
    }
    outcome.note(format!("{INSPECTION_FEATURE}: {dependencies} optional dependency, {pins} workspace pin(s), {features} feature declaration(s)"));
}

fn check_inspection_pin(file: &str, document: &Value, outcome: &mut Outcome) -> usize {
    let mut pins = 0;
    let entries = document
        .get("workspace")
        .and_then(|workspace| workspace.get("dependencies"))
        .and_then(Value::as_table)
        .into_iter()
        .flatten();
    for (alias, specification) in entries {
        if dependency_package(alias, specification) != INSPECTION_CRATE {
            continue;
        }
        pins += 1;
        let revision = specification.get("rev").and_then(Value::as_str);
        let pinned = file == "Cargo.toml"
            && specification
                .get("git")
                .and_then(Value::as_str)
                .is_some_and(|git| !git.is_empty())
            && revision.is_some_and(|sha| {
                sha.len() == 40 && sha.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
            && [
                "branch",
                "tag",
                "path",
                "version",
                "registry",
                "registry-index",
                "workspace",
            ]
            .iter()
            .all(|key| specification.get(*key).is_none());
        if !pinned {
            outcome.fail(file.to_string(), 1,
                format!("`{INSPECTION_CRATE}` must be pinned to a 40-character commit SHA in the root workspace"));
        }
    }
    pins
}

fn check_inspection_crate(
    file: &str,
    document: &Value,
    workspace: Option<&cargo_toml::Table>,
    outcome: &mut Outcome,
) -> Vec<String> {
    let mut plugin_aliases = Vec::new();
    for (alias, specification) in dependency_entries(document) {
        let source = dependency_source(alias, specification, workspace);
        if dependency_package(alias, source) != INSPECTION_CRATE {
            continue;
        }
        plugin_aliases.push(format!("dep:{alias}"));
        if file != INSPECTION_OWNER {
            outcome.fail(
                file.to_string(),
                1,
                format!("`{INSPECTION_CRATE}` may only be declared by `{INSPECTION_OWNER}`"),
            );
        }
        if specification.get("optional").and_then(Value::as_bool) != Some(true) {
            outcome.fail(
                file.to_string(),
                1,
                format!(
                    "`{INSPECTION_CRATE}` is not optional, so every profile compiles and links it"
                ),
            );
        }
        if specification.get("workspace").and_then(Value::as_bool) != Some(true)
            || workspace.and_then(|table| table.get(alias)).is_none()
            || [
                "package",
                "git",
                "rev",
                "branch",
                "tag",
                "path",
                "version",
                "registry",
                "registry-index",
            ]
            .iter()
            .any(|key| specification.get(*key).is_some())
        {
            outcome.fail(file.to_string(), 1,
                format!("`{INSPECTION_CRATE}` must inherit the audited workspace dependency without source overrides"));
        }
    }
    plugin_aliases
}

fn check_inspection_feature(
    file: &str,
    document: &Value,
    plugin_aliases: &[String],
    outcome: &mut Outcome,
) -> usize {
    let Some(declaration) = document
        .get("features")
        .and_then(|table| table.get(INSPECTION_FEATURE))
    else {
        return 0;
    };
    let selected = string_array(Some(declaration));
    if file != INSPECTION_OWNER
        || selected.len() != 1
        || !plugin_aliases.iter().any(|alias| selected[0] == alias)
    {
        outcome.fail(file.to_string(), 1,
            format!("`{INSPECTION_FEATURE}` may only be declared once, in `{INSPECTION_OWNER}` as a non-default feature enabling the inspection dependency"));
    }
    1
}

fn dependency_package<'a>(alias: &'a str, specification: &'a Value) -> &'a str {
    specification
        .get("package")
        .and_then(Value::as_str)
        .unwrap_or(alias)
}

fn dependency_entries(document: &Value) -> Vec<(&str, &Value)> {
    let mut entries = Vec::new();
    for kind in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(table) = document.get(kind).and_then(Value::as_table) {
            entries.extend(table.iter().map(|(alias, value)| (alias.as_str(), value)));
        }
    }
    if let Some(targets) = document.get("target").and_then(Value::as_table) {
        for target in targets.values() {
            entries.extend(dependency_entries(target));
        }
    }
    entries
}

fn string_array(value: Option<&Value>) -> Vec<&str> {
    value
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect()
}

fn dependency_source<'a>(
    alias: &str,
    specification: &'a Value,
    workspace: Option<&'a cargo_toml::Table>,
) -> &'a Value {
    if specification.get("workspace").and_then(Value::as_bool) == Some(true) {
        workspace
            .and_then(|table| table.get(alias))
            .unwrap_or(specification)
    } else {
        specification
    }
}

fn dependency_selects_inspection(
    alias: &str,
    specification: &Value,
    forwarded: &[&str],
    manifests: &[(String, Value)],
    workspace: Option<&cargo_toml::Table>,
    visited: &mut BTreeSet<(String, String)>,
) -> bool {
    let source = dependency_source(alias, specification, workspace);
    let package = dependency_package(alias, source);
    let Some((target_file, target)) = manifests.iter().find(|(_, target)| {
        target
            .get("package")
            .and_then(|package| package.get("name"))
            .and_then(Value::as_str)
            == Some(package)
    }) else {
        return false;
    };
    let mut selected = string_array(specification.get("features"));
    selected.extend_from_slice(forwarded);
    let inherited = specification.get("workspace").and_then(Value::as_bool) == Some(true);
    if inherited {
        selected.extend(string_array(source.get("features")));
    }
    let defaults = if inherited {
        source.get("default-features").and_then(Value::as_bool) != Some(false)
            || specification
                .get("default-features")
                .and_then(Value::as_bool)
                == Some(true)
    } else {
        specification
            .get("default-features")
            .and_then(Value::as_bool)
            != Some(false)
    };
    if defaults {
        selected.push("default");
    }
    selects_inspection(
        target_file,
        target,
        &selected,
        manifests,
        workspace,
        visited,
    )
}

fn selects_inspection(
    file: &str,
    document: &Value,
    selected: &[&str],
    manifests: &[(String, Value)],
    workspace: Option<&cargo_toml::Table>,
    visited: &mut BTreeSet<(String, String)>,
) -> bool {
    let mut expanded = BTreeSet::new();
    let mut pending = selected.to_vec();
    while let Some(feature) = pending.pop() {
        if !expanded.insert(feature) {
            continue;
        }
        if feature == INSPECTION_FEATURE {
            return true;
        }
        pending.extend(string_array(
            document
                .get("features")
                .and_then(|table| table.get(feature)),
        ));
    }
    if !visited.insert((
        file.to_string(),
        expanded.iter().copied().collect::<Vec<_>>().join("\0"),
    )) {
        return false;
    }
    dependency_entries(document)
        .into_iter()
        .any(|(alias, specification)| {
            let forwarded: Vec<&str> = expanded
                .iter()
                .filter_map(|feature| {
                    feature
                        .split_once('/')
                        .filter(|(name, _)| name.trim_end_matches('?') == alias)
                        .map(|(_, feature)| feature)
                })
                .collect();
            let active = specification.get("optional").and_then(Value::as_bool) != Some(true)
                || expanded.contains(format!("dep:{alias}").as_str())
                || (expanded.contains(alias)
                    && document
                        .get("features")
                        .and_then(|table| table.get(alias))
                        .is_none())
                || expanded.iter().any(|feature| {
                    feature
                        .split_once('/')
                        .is_some_and(|(name, _)| name == alias)
                });
            if !active {
                return false;
            }
            let source = dependency_source(alias, specification, workspace);
            dependency_package(alias, source) == INSPECTION_CRATE
                || dependency_selects_inspection(
                    alias,
                    specification,
                    &forwarded,
                    manifests,
                    workspace,
                    visited,
                )
        })
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
