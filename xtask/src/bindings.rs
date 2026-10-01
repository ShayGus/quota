//! `bindings --check`: the checked-in TypeScript mirror against the Rust IPC.

use std::collections::BTreeSet;
use std::path::Path;

use crate::outcome::Outcome;
use crate::scan;

/// The mirror the renderer imports.
const BINDINGS: &str = "apps/desktop/src/generated/bindings.ts";

/// Where the command and event definitions live.
const IPC_SOURCE: &str = "apps/desktop/src-tauri/src/ipc";

/// Command names the specification requires, used only when the host exists.
const REQUIRED_COMMANDS: [&str; 20] = [
    "get_snapshot",
    "list_provider_capabilities",
    "get_connection_progress",
    "begin_connection",
    "cancel_connection",
    "reconnect_account",
    "set_account_enabled",
    "rename_account",
    "disconnect_account",
    "refresh_accounts",
    "set_monitoring_state",
    "set_polling_preferences",
    "update_preferences",
    "set_overview_mode",
    "set_overview_always_on_top",
    "fit_overview_to_accounts",
    "reset_overview_position",
    "open_provider_usage_page",
    "clear_local_history",
    "export_sanitized_diagnostics",
];

/// Expected top-level event types emitted in the Rust IPC layer.
const REQUIRED_EVENTS: [&str; 6] = [
    "SnapshotUpdated",
    "ConnectionProgressChanged",
    "PreferencesChanged",
    "MonitoringStateChanged",
    "OverviewWindowStateChanged",
    "PersistenceStatusChanged",
];

/// Compares the mirror against the Rust IPC layer at `root`.
#[must_use]
pub(crate) fn run(root: &Path) -> Outcome {
    let mut outcome = Outcome::default();
    let bindings_path = root.join(BINDINGS);
    let ipc_dir = root.join(IPC_SOURCE);
    if !bindings_path.is_file() {
        outcome.fail(
            BINDINGS.to_string(),
            1,
            "the checked-in bindings mirror is missing; regenerate it from the Rust registry"
                .to_string(),
        );
        return outcome;
    }
    let Ok(text) = scan::read(&bindings_path) else {
        return outcome;
    };
    if !ipc_dir.is_dir() {
        outcome.note(format!(
            "{IPC_SOURCE} does not exist yet, so only the checked-in mirror is inspected; the host is another agent's slice"
        ));
        outcome.note(format!(
            "{BINDINGS}: {} exported declaration(s); the Rust side is not yet present",
            count_exports(&text)
        ));
        return outcome;
    }
    compare(&text, &ipc_dir, &mut outcome);
    outcome
}

/// Compares both directions and records what it inspected.
fn compare(text: &str, ipc_dir: &Path, outcome: &mut Outcome) {
    let rust_commands = command_names(ipc_dir, outcome);
    let rust_events = event_names(ipc_dir);
    if rust_commands.is_empty() && rust_events.is_empty() {
        outcome.fail(
            IPC_SOURCE.to_string(),
            1,
            "the IPC layer declares no #[tauri::command] function and no event type".to_string(),
        );
        return;
    }
    for name in &rust_commands {
        if !text.contains(&format!("\"{name}\"")) {
            outcome.fail(
                BINDINGS.to_string(),
                1,
                format!("the Rust command `{name}` has no wrapper in the bindings mirror"),
            );
        }
    }
    for name in &rust_events {
        if !text.contains(&format!("\"{name}\"")) {
            outcome.fail(
                BINDINGS.to_string(),
                1,
                format!("the Rust event `{name}` has no listener in the bindings mirror"),
            );
        }
    }
    for name in &REQUIRED_COMMANDS {
        if !rust_commands.contains(*name) {
            outcome.note(format!(
                "required command `{name}` is not yet defined in Rust"
            ));
        }
    }
    for name in &REQUIRED_EVENTS {
        if !rust_events.contains(*name) {
            outcome.note(format!(
                "required event `{name}` is not yet defined in Rust"
            ));
        }
    }
    for invented in wire_names(text)
        .iter()
        .filter(|name| !rust_commands.contains(*name) && !rust_events.contains(*name))
    {
        outcome.fail(
            BINDINGS.to_string(),
            1,
            format!(
                "the mirror uses the wire name `{invented}`, which the Rust IPC layer does not define"
            ),
        );
    }
    outcome.note(format!(
        "compared {} Rust command(s) and {} Rust event(s) with {} mirrored wire name(s)",
        rust_commands.len(),
        rust_events.len(),
        wire_names(text).len()
    ));
}

/// Every wire name the mirror passes to `invoke` or to `listen`.
fn wire_names(text: &str) -> BTreeSet<String> {
    const CALLS: [&str; 3] = ["call(\"", "callVoid(\"", "listen<unknown>(\""];
    let mut names = BTreeSet::new();
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("//") || trimmed.starts_with('*') || trimmed.starts_with("/*") {
            continue;
        }
        let _ = index;
        for call in CALLS {
            let mut search = line;
            while let Some(position) = search.find(call) {
                let rest = &search[position + call.len()..];
                let name = rest.split('"').next().unwrap_or("");
                if !name.is_empty() {
                    names.insert(name.to_string());
                }
                search = rest;
            }
        }
    }
    names
}

/// The number of declarations the mirror exports.
fn count_exports(text: &str) -> usize {
    text.lines()
        .filter(|line| {
            let trimmed = line.trim_start();
            trimmed.starts_with("export ")
        })
        .count()
}

/// The `#[tauri::command]` function names declared under `dir`.
fn command_names(dir: &Path, outcome: &mut Outcome) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for path in scan::files_with_extension(dir, "rs") {
        let Ok(text) = scan::read(&path) else {
            continue;
        };
        let lines = text.lines().collect::<Vec<_>>();
        for (index, line) in lines.iter().enumerate() {
            if !line.contains("tauri::command") {
                continue;
            }
            if let Some(name) = next_function_name(&lines, index) {
                names.insert(name);
            }
        }
        outcome.note(format!(
            "{}: {} command annotation(s)",
            scan::relative(dir, &path),
            lines
                .iter()
                .filter(|line| line.contains("tauri::command"))
                .count()
        ));
    }
    names
}

/// Scans forward from an attribute for the `fn` name it annotates.
fn next_function_name(lines: &[&str], attribute: usize) -> Option<String> {
    for line in lines.iter().skip(attribute + 1).take(8) {
        let trimmed = line.trim();
        if trimmed.starts_with("//") || trimmed.starts_with("#[") || trimmed.is_empty() {
            continue;
        }
        let position = trimmed.find("fn ")?;
        let rest = &trimmed[position + 3..];
        let name = rest
            .split(|character: char| !character.is_alphanumeric() && character != '_')
            .next()?;
        if !name.is_empty() {
            return Some(name.to_string());
        }
        return None;
    }
    None
}

/// Struct names that carry a `tauri_specta::Event` derive.
fn event_names(dir: &Path) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for path in scan::files_with_extension(dir, "rs") {
        let Ok(text) = scan::read(&path) else {
            continue;
        };
        let lines = text.lines().collect::<Vec<_>>();
        for (index, line) in lines.iter().enumerate() {
            if !line.contains("tauri_specta::Event") {
                continue;
            }
            if let Some(name) = declared_struct(&lines, index) {
                names.insert(name);
            }
        }
    }
    names
}

/// Scans forward from a derive for the struct it decorates.
fn declared_struct(lines: &[&str], derive: usize) -> Option<String> {
    for line in lines.iter().skip(derive + 1).take(10) {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with("#[") {
            continue;
        }
        return struct_name(line);
    }
    None
}

/// Extracts the name from `pub struct Name` or `pub struct Name {`.
fn struct_name(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let position = trimmed.find("struct ")?;
    let rest = &trimmed[position + 7..];
    let name = rest
        .split(|character: char| !character.is_alphanumeric())
        .next()?;
    if name.is_empty() || !name.starts_with(char::is_uppercase) {
        return None;
    }
    Some(name.to_string())
}
