//! Repository automation. Run through `cargo xtask <command>`.
//! Architecture and release checks inspect repository files. The bindings
//! check delegates to the desktop exporter test; see [`bindings`]. Application
//! launch belongs to the Tauri CLI through `bun tauri dev`.

#![forbid(unsafe_code)]

mod bindings;
mod cargo_manifest;
mod check_architecture;
mod check_release;
mod outcome;
mod scan;
mod toml;

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

/// Exit status for a gate that found nothing, or a command that succeeded.
const OK: u8 = 0;

/// Exit status for a gate that found a violation, or a misuse of the command.
const FAILED: u8 = 1;

fn main() -> ExitCode {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    ExitCode::from(run(&arguments))
}

/// Dispatches one command line. Returns the process exit status.
fn run(arguments: &[String]) -> u8 {
    let command = arguments.first().map(String::as_str);
    let rest = arguments.get(1..).unwrap_or_default();
    let root = match root_override(rest) {
        Ok(root) => root,
        Err(message) => {
            println!("{message}");
            usage();
            return FAILED;
        }
    };
    match command {
        Some("check-architecture") => gate("check-architecture", &check_architecture::run(&root)),
        Some("check-release") => gate("check-release", &check_release::run(&root)),
        Some("bindings") => {
            if rest.first().map(String::as_str) == Some("--check") {
                gate("bindings --check", &bindings::run(&root))
            } else {
                usage();
                FAILED
            }
        }
        Some("--help" | "-h" | "help") => {
            usage();
            OK
        }
        Some(other) => {
            println!("unknown subcommand `{other}`");
            usage();
            FAILED
        }
        None => {
            usage();
            FAILED
        }
    }
}

/// Reads an optional `--root <path>` so a gate can be pointed at a test tree.
fn root_override(arguments: &[String]) -> Result<PathBuf, String> {
    let Some(position) = arguments.iter().position(|argument| argument == "--root") else {
        return Ok(root());
    };
    let Some(path) = arguments.get(position + 1) else {
        return Err("`--root` needs a path argument".to_string());
    };
    Ok(PathBuf::from(path))
}

/// Prints a gate's report and maps it to an exit status.
fn gate(name: &str, outcome: &outcome::Outcome) -> u8 {
    print!("{}", outcome.report(name));
    if outcome.is_clean() { OK } else { FAILED }
}

/// The repository root, taken from this package's manifest directory.
fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map_or_else(|| PathBuf::from("."), PathBuf::from)
}

/// Prints the command list and options.
fn usage() {
    println!(
        "\
usage: cargo xtask <command>

commands:
  check-architecture   fail when a package, dependency, file size, raw IPC call,
                       duplicated IPC model, or provider feature violates policy.
  check-release        fail when the release surface is not audited: test
                       features, licence allow list, workflow action pins, and
                       the Tauri devtools/content-security settings.
  bindings --check     compare src/generated/bindings.ts with the
                       Rust IPC layer.

  --help               print this text

Options:
  --root <path>        read the tree at <path> instead of this repository. The
                       gates use it for their own tests.

The bindings check runs the exporter and compares what it generates with the
committed file, byte for byte, so a stale bindings.ts cannot pass. The exporter
lives in the host crate because only it can run tauri-specta, so the check
compiles and runs src-tauri/tests/bindings.rs. It never opens a window or reads
an account. CI runs the same command."
    );
}
