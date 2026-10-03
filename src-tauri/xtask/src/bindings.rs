//! `bindings --check`: the generated TypeScript against the Rust IPC layer.
//!
//! The renderer imports `src/generated/bindings.ts`, which Tauri Specta writes
//! from the command and event definitions themselves. This gate proves that file
//! is current: it runs the exporter and compares the result with what is
//! committed, so a hand edit or a changed signature fails here rather than at
//! runtime in the application.
//!
//! The comparison lives in `src-tauri/tests/bindings.rs` because only the host
//! crate can run the exporter. This subcommand runs that test and reports it.

use std::path::Path;
use std::process::Command;

use crate::outcome::Outcome;

/// The mirror the renderer imports, named in the report.
const BINDINGS: &str = "src/generated/bindings.ts";

/// Regenerates the bindings and compares them with the committed file.
#[must_use]
pub(crate) fn run(root: &Path) -> Outcome {
    let mut outcome = Outcome::default();
    outcome.note(format!("{BINDINGS} regenerated from the Rust IPC layer"));

    let result = Command::new(cargo())
        .current_dir(root.join(crate::scan::WORKSPACE))
        .args([
            "test",
            "--locked",
            "-p",
            "quota-desktop",
            "--test",
            "bindings",
        ])
        .output();

    match result {
        Ok(output) if output.status.success() => {
            outcome.note("the committed file matches the exporter byte for byte".to_owned());
        }
        Ok(output) => {
            let detail = String::from_utf8_lossy(&output.stdout);
            let failing = detail
                .lines()
                .rfind(|line| line.contains("panicked") || line.contains("assertion"))
                .unwrap_or("the regenerated bindings differ from the committed file")
                .trim()
                .to_owned();
            outcome.fail(
                BINDINGS.to_owned(),
                1,
                format!(
                    "{failing}. Regenerate it with `cargo test -p quota-desktop --test bindings` \
                     and copy the produced file; nothing in it is edited by hand."
                ),
            );
        }
        Err(error) => {
            outcome.fail(
                BINDINGS.to_owned(),
                1,
                format!("could not run the bindings test: {error}"),
            );
        }
    }

    outcome
}

/// The Cargo executable running this xtask, so the test uses the same toolchain.
fn cargo() -> std::ffi::OsString {
    std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into())
}
