//! Rules about first-party Cargo manifests.

use std::path::Path;

use crate::outcome::Outcome;
use crate::scan;
use crate::toml::{self, Document};

/// Packages that must not reach for a UI toolkit or a database driver.
const PURE_PACKAGES: [&str; 2] = ["quota-domain", "quota-core"];

/// Dependency names a pure package may not declare.
const FORBIDDEN_FOR_PURE: [&str; 4] = ["tauri", "tauri-plugin", "react", "sqlx"];

/// The package keys every first-party member must inherit.
const INHERITED_KEYS: [&str; 3] = ["edition", "rust-version", "publish"];

/// Checks inheritance, forbidden dependency edges, and provider features.
pub(crate) fn check(root: &Path, outcome: &mut Outcome) {
    let manifests = scan::files_with_extension(root, "toml")
        .into_iter()
        .filter(|path| path.file_name().is_some_and(|name| name == "Cargo.toml"))
        .collect::<Vec<_>>();
    for manifest in &manifests {
        check_one(root, manifest, outcome);
    }
    outcome.note(format!("{} Cargo manifests", manifests.len()));
    check_provider_features(root, outcome);
}

/// Rules for one first-party Cargo manifest.
fn check_one(root: &Path, path: &Path, outcome: &mut Outcome) {
    let file = scan::relative(root, path);
    let Ok(text) = scan::read(path) else {
        outcome.fail(file, 1, "manifest cannot be read".to_string());
        return;
    };
    let document = Document::parse(&text);
    let Some(name) = document.get("package", "name") else {
        return;
    };
    let name = name.trim().trim_matches('"').to_string();
    let package_line = document.table_line("package").unwrap_or(1);
    for key in INHERITED_KEYS {
        if !inherits(&document, key) {
            outcome.fail(
                file.clone(),
                package_line,
                format!("`{name}` must declare `{key}.workspace = true`"),
            );
        }
    }
    let lints = document
        .get("lints", "workspace")
        .is_some_and(|value| value.contains("true"));
    if !lints {
        outcome.fail(
            file.clone(),
            package_line,
            format!("`{name}` must declare `[lints] workspace = true`"),
        );
    }
    if !PURE_PACKAGES.contains(&name.as_str()) {
        return;
    }
    for entry in document.all() {
        if !toml::is_dependency_table(&entry.table) {
            continue;
        }
        if let Some(dependency) = forbidden_dependency(&entry.key, &entry.value) {
            outcome.fail(
                file.clone(),
                entry.line,
                format!(
                    "`{name}` declares `{dependency}`, which a domain/core package must not depend on"
                ),
            );
        }
    }
}

/// Reports whether `key` is inherited from the workspace, dotted or inline.
fn inherits(document: &Document, key: &str) -> bool {
    let dotted = format!("{key}.workspace");
    document
        .get("package", &dotted)
        .is_some_and(|value| value.trim() == "true")
        || document
            .get("package", key)
            .is_some_and(|value| value.contains("workspace = true"))
}

/// Returns the forbidden dependency name when `key` names one.
fn forbidden_dependency(key: &str, value: &str) -> Option<String> {
    let key = key.trim().trim_matches('"');
    if key == "package"
        && let Some(real) = package_rename(value)
    {
        return FORBIDDEN_FOR_PURE
            .iter()
            .find(|forbidden| real.starts_with(**forbidden))
            .map(|forbidden| (*forbidden).to_string());
    }
    FORBIDDEN_FOR_PURE
        .iter()
        .find(|forbidden| key.starts_with(**forbidden))
        .map(|forbidden| (*forbidden).to_string())
}

/// Reads the `package = "..."` rename inside an inline dependency table.
fn package_rename(value: &str) -> Option<String> {
    let (_, after_key) = value.split_once("package")?;
    let (_, after_quote) = after_key.split_once('"')?;
    let (name, _) = after_quote.split_once('"')?;
    Some(name.to_string())
}

/// Features that must never reach a release artifact.
const TEST_FEATURES: [&str; 1] = ["test-fixtures"];

/// Reports every default feature set that selects a test-only feature.
pub(crate) fn check_release_features(root: &Path, outcome: &mut Outcome) {
    let mut inspected = 0;
    for manifest in scan::files_with_extension(root, "toml")
        .into_iter()
        .filter(|path| path.file_name().is_some_and(|name| name == "Cargo.toml"))
    {
        let Ok(text) = scan::read(&manifest) else {
            continue;
        };
        for (index, line) in text.lines().enumerate() {
            let trimmed = line.trim();
            if !trimmed.starts_with("default = [") {
                continue;
            }
            inspected += 1;
            if TEST_FEATURES
                .iter()
                .any(|feature| trimmed.contains(feature))
            {
                outcome.fail(
                    scan::relative(root, &manifest),
                    index + 1,
                    format!(
                        "`{trimmed}` puts a test-only feature in the default set; release builds must exclude it"
                    ),
                );
            }
        }
    }
    outcome.note(format!("{inspected} default feature declaration(s)"));
}

/// Reports a `test-fixtures` feature that is not explicitly non-default.
fn check_provider_features(root: &Path, outcome: &mut Outcome) {
    let manifest = root.join("crates/quota-providers/Cargo.toml");
    if !manifest.is_file() {
        outcome
            .note("crates/quota-providers/Cargo.toml is absent; feature check skipped".to_string());
        return;
    }
    let Ok(text) = scan::read(&manifest) else {
        return;
    };
    let document = Document::parse(&text);
    let file = scan::relative(root, &manifest);
    let Some(features) = document.get("features", "test-fixtures") else {
        outcome.fail(
            file,
            1,
            "`test-fixtures` feature is not declared".to_string(),
        );
        return;
    };
    if !features.trim().is_empty() && features.trim() != "[]" {
        outcome.fail(
            file.clone(),
            document
                .entry("features", "test-fixtures")
                .map_or(1, |entry| entry.line),
            format!("`test-fixtures` must be an empty feature list, found `{features}`"),
        );
    }
    // The feature must be non-default. An explicit opt-in in a dependent's
    // dependency table is the audited way to select it; a default entry is not.
    let default = document.get("features", "default").unwrap_or("");
    if toml::strip_comment(default).contains("test-fixtures") {
        outcome.fail(
            file,
            document
                .entry("features", "default")
                .map_or(1, |entry| entry.line),
            "`test-fixtures` is in the default feature set; release builds must exclude it"
                .to_string(),
        );
    }
}

/// Reports a first-party dependency declaration that enables a test-only
/// feature on `quota-providers`.
///
/// `check_release_features` reads only `default = [...]` declarations, so a
/// feature selected from a dependent's dependency table would pass it. This
/// reads the other half: an ordinary build of the desktop host must not compile
/// the fixture adapter, which is reachable only through `quota-providers`'
/// non-default `test-fixtures` feature.
pub(crate) fn check_dependency_feature_selection(root: &Path, outcome: &mut Outcome) {
    let mut inspected = 0;
    for manifest in scan::files_with_extension(root, "toml")
        .into_iter()
        .filter(|path| path.file_name().is_some_and(|name| name == "Cargo.toml"))
    {
        let Ok(text) = scan::read(&manifest) else {
            continue;
        };
        let document = Document::parse(&text);
        for entry in document.all() {
            // A dev-dependency is a test-only route to the feature and is the
            // audited way to reach the fixture adapter, so it is not a finding.
            if !toml::is_dependency_table(&entry.table) || entry.table.contains("dev-") {
                continue;
            }
            if dependency_package(&entry.key, &entry.value).as_deref() != Some("quota-providers") {
                continue;
            }
            inspected += 1;
            for feature in TEST_FEATURES {
                if !entry.value.contains(feature) {
                    continue;
                }
                outcome.fail(
                    scan::relative(root, &manifest),
                    entry.line,
                    format!(
                        "`{}` enables the test-only `{feature}` feature on `quota-providers`; an ordinary build must not compile the fixture adapter",
                        entry.key
                    ),
                );
            }
        }
    }
    outcome.note(format!(
        "{inspected} quota-providers dependency declaration(s)"
    ));
}

/// Returns the package a dependency key names, following a `package = "..."` rename.
fn dependency_package(key: &str, value: &str) -> Option<String> {
    let key = key.trim().trim_matches('"');
    if key == "quota-providers" {
        return Some(key.to_string());
    }
    package_rename(value)
}
