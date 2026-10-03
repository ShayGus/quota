//! The one version every shipped artifact is named from, compared across the
//! three files that carry it.
//!
//! The Tauri configuration is the authority: it is what the application
//! reports and what names the installers. `package.json` and the desktop
//! manifest mirror it so the frontend package and the host crate answer the
//! same question. A release whose mirrors disagree is a release whose file
//! names, bundle names, and upgrade code do not describe the same build.

use std::path::Path;

use serde_json::Value;

use crate::outcome::Outcome;
use crate::scan;
use crate::toml;

/// The file that decides the version.
const AUTHORITY: &str = "src-tauri/tauri.conf.json";

/// The frontend package manifest, which mirrors it.
const PACKAGE_JSON: &str = "package.json";

/// The desktop host manifest, which mirrors it.
const CARGO_MANIFEST: &str = "src-tauri/Cargo.toml";

/// Reports a version that is missing from a file, or different in one.
pub(crate) fn check(root: &Path, outcome: &mut Outcome) {
    let authority = json_version(root, AUTHORITY, outcome);
    let package = json_version(root, PACKAGE_JSON, outcome);
    let manifest = manifest_version(root, outcome);
    let (Some(authority), Some(package), Some(manifest)) = (authority, package, manifest) else {
        return;
    };
    for (file, version) in [
        (PACKAGE_JSON, package.as_str()),
        (CARGO_MANIFEST, manifest.as_str()),
    ] {
        if version != authority {
            outcome.fail(
                file.to_string(),
                1,
                format!(
                    "version `{version}` does not match {AUTHORITY}, which declares `{authority}`"
                ),
            );
        }
    }
    outcome.note(format!(
        "version `{authority}` agrees across the Tauri configuration, package.json, and the desktop manifest"
    ));
}

/// Reads the version from a JSON file, or records why it has none.
fn json_version(root: &Path, file: &str, outcome: &mut Outcome) -> Option<String> {
    let text = read(root, file, outcome)?;
    let Ok(document) = serde_json::from_str::<Value>(text.as_str()) else {
        outcome.fail(
            file.to_string(),
            1,
            format!("{file} is not readable JSON, so its version cannot be compared"),
        );
        return None;
    };
    document
        .get("version")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            outcome.fail(
                file.to_string(),
                1,
                format!("{file} declares no `version`, which {AUTHORITY} must match"),
            );
            None
        })
}

/// Reads the version from the desktop manifest's package table.
fn manifest_version(root: &Path, outcome: &mut Outcome) -> Option<String> {
    let text = read(root, CARGO_MANIFEST, outcome)?;
    let document = toml::Document::parse(&text);
    document
        .get("package", "version")
        .map(|value| value.trim_matches('"').trim_matches('\'').to_string())
        .or_else(|| {
            outcome.fail(
                CARGO_MANIFEST.to_string(),
                1,
                format!(
                    "{CARGO_MANIFEST} declares no `[package] version`, which {AUTHORITY} must match"
                ),
            );
            None
        })
}

/// Reads one file as text, or records why it cannot be read.
fn read(root: &Path, file: &str, outcome: &mut Outcome) -> Option<String> {
    if let Ok(text) = scan::read(&root.join(file)) {
        return Some(text);
    }
    outcome.fail(
        file.to_string(),
        1,
        format!("{file} cannot be read, so its version cannot be compared"),
    );
    None
}
