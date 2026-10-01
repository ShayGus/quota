//! `check-architecture`: static rules that need no compiler.

use std::path::Path;

use crate::cargo_manifest;
use crate::outcome::Outcome;
use crate::scan;

/// Largest number of code lines a first-party production file may hold.
const FILE_LIMIT: usize = 400;

/// Raw IPC entry points the renderer may only reach through generated code.
const RAW_IPC: [&str; 3] = ["invoke(", "listen(", "emit("];

/// Prefixes under `src/` that may call IPC directly.
const IPC_EXEMPT_PREFIXES: [&str; 2] = ["src/generated/", "src/shared/ipc/"];

/// Trees that must not carry a second copy of the IPC model.
const IPC_OWNER_TREES: [&str; 2] = ["crates/quota-contracts/src/", "crates/quota-domain/src/"];

/// Markers that identify a hand-written command or event union.
const UNION_MARKERS: [&str; 6] = [
    "enum CommandError",
    "enum Command",
    "enum IpcCommand",
    "enum AppEvent",
    "enum IpcEvent",
    "enum BackendEvent",
];

/// Runs the gate against the repository at `root`.
#[must_use]
pub(crate) fn run(root: &Path) -> Outcome {
    let mut outcome = Outcome::default();
    cargo_manifest::check(root, &mut outcome);
    let sources = scan::files_with_extension(root, "rs");
    for source in &sources {
        check_source_size(root, source, &mut outcome);
    }
    check_raw_ipc(root, &mut outcome);
    check_duplicate_models(root, &mut outcome);
    outcome.note(format!("{} Rust sources", sources.len()));
    outcome
}

/// Reports a production file over the hard ceiling.
fn check_source_size(root: &Path, path: &Path, outcome: &mut Outcome) {
    if scan::is_generated(path) || path.components().any(|part| part.as_os_str() == "tests") {
        return;
    }
    let Ok(text) = scan::read(path) else {
        return;
    };
    let lines = scan::code_lines(&text);
    if lines > FILE_LIMIT {
        outcome.fail(
            scan::relative(root, path),
            1,
            format!("{lines} code lines exceeds the hard ceiling of {FILE_LIMIT}"),
        );
    }
}

/// Reports raw IPC calls in renderer code outside the audited boundary.
fn check_raw_ipc(root: &Path, outcome: &mut Outcome) {
    let directory = root.join("src");
    if !directory.is_dir() {
        outcome.note("src does not exist yet; raw IPC scan skipped".to_string());
        return;
    }
    let mut inspected = 0;
    for path in scan::files_with_extension(&directory, "ts")
        .into_iter()
        .chain(scan::files_with_extension(&directory, "tsx"))
    {
        inspected += 1;
        let file = scan::relative(root, &path);
        if IPC_EXEMPT_PREFIXES
            .iter()
            .any(|prefix| file.starts_with(prefix))
        {
            continue;
        }
        let Ok(text) = scan::read(&path) else {
            continue;
        };
        for (index, line) in text.lines().enumerate() {
            let Some(call) = RAW_IPC.iter().find(|call| line.contains(**call)) else {
                continue;
            };
            outcome.fail(
                file.clone(),
                index + 1,
                format!("raw IPC call `{call}` outside generated code and the IPC wrapper"),
            );
        }
    }
    outcome.note(format!("{inspected} renderer TypeScript sources"));
}

/// Reports a second, hand-written copy of the IPC command or event model.
///
/// The transport contract is declared once, in `quota-contracts`, and the
/// renderer's `bindings.ts` is generated from it. This gate therefore rejects a
/// TypeScript model written by hand beside the Rust source, an IPC union
/// declared in the pure domain tree, and one union name declared twice inside a
/// tree.
fn check_duplicate_models(root: &Path, outcome: &mut Outcome) {
    for tree in IPC_OWNER_TREES {
        let directory = root.join(tree);
        if !directory.is_dir() {
            continue;
        }
        for path in scan::files_with_extension(&directory, "ts")
            .into_iter()
            .chain(scan::files_with_extension(&directory, "tsx"))
            .chain(scan::files_with_extension(&directory, "d.ts"))
        {
            outcome.fail(
                scan::relative(root, &path),
                1,
                "a hand-written TypeScript IPC model must not sit beside the Rust contract; the renderer mirror is generated".to_string(),
            );
        }
        check_union_declarations(root, &directory, tree, outcome);
    }
}

/// Rejects duplicated and misplaced IPC unions inside one crate tree.
fn check_union_declarations(root: &Path, directory: &Path, tree: &str, outcome: &mut Outcome) {
    let mut seen: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    for path in scan::files_with_extension(directory, "rs") {
        if scan::is_generated(&path) {
            continue;
        }
        let file = scan::relative(root, &path);
        let Ok(text) = scan::read(&path) else {
            continue;
        };
        for (index, line) in text.lines().enumerate() {
            let Some(name) = ipc_union_name(line) else {
                continue;
            };
            if tree == IPC_OWNER_TREES[1] {
                outcome.fail(
                    file.clone(),
                    index + 1,
                    format!(
                        "`{name}` is an IPC union in the pure domain tree; transport types belong in quota-contracts"
                    ),
                );
                continue;
            }
            if let Some(first) = seen.insert(name.clone(), file.clone()) {
                outcome.fail(
                    file.clone(),
                    index + 1,
                    format!(
                        "`{name}` is already declared in {first}; one union has one definition"
                    ),
                );
            }
        }
    }
}

/// Returns the union name when a line declares an IPC command or event type.
fn ipc_union_name(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if !trimmed.starts_with("pub enum ") {
        return None;
    }
    let name = trimmed["pub enum ".len()..]
        .split(|character: char| !character.is_alphanumeric())
        .next()?;
    UNION_MARKERS
        .iter()
        .find(|marker| marker.contains(name))
        .map(|_| name.to_string())
}
