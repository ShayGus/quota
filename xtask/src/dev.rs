//! Starts the desktop app on this machine.
//!
//! This is the only subcommand that builds or runs another package. It builds
//! the renderer, then runs the host with `custom-protocol` so the window loads
//! the built files instead of waiting for a dev server that is not running.

use std::path::Path;
use std::process::Command;

/// Builds the renderer, then runs the desktop host.
///
/// Returns the host's exit status, or `1` when a command cannot start.
#[must_use]
pub(crate) fn run(root: &Path) -> u8 {
    match build_renderer(root) {
        None => return 1,
        Some(0) => {}
        Some(code) => return code,
    }

    let mut command = Command::new(cargo());
    command.current_dir(root).args([
        "run",
        "-p",
        "quota-desktop",
        "--features",
        "custom-protocol",
    ]);
    announce(&command);
    match command.status() {
        Ok(status) => u8::try_from(status.code().unwrap_or(1)).unwrap_or(1),
        Err(error) => {
            println!("could not start the desktop host: {error}");
            1
        }
    }
}

/// Builds the renderer bundle into `apps/desktop/dist`.
///
/// Returns `None` when the command could not start, after printing the reason.
fn build_renderer(root: &Path) -> Option<u8> {
    let mut command = Command::new("pnpm");
    command.current_dir(root.join("apps/desktop")).arg("build");
    announce(&command);
    match command.status() {
        Ok(status) if status.success() => Some(0),
        Ok(status) => {
            println!("the renderer build failed with {status}");
            Some(1)
        }
        Err(error) => {
            println!("could not run pnpm: {error}");
            None
        }
    }
}

/// Prints the exact command so the person running it can copy it.
fn announce(command: &Command) {
    println!("$ {}", render(command));
}

/// Renders a command the way a shell would accept it.
fn render(command: &Command) -> String {
    let mut text = command.get_program().to_string_lossy().into_owned();
    for part in command.get_args() {
        let part = part.to_string_lossy();
        if part.contains(' ') {
            text.push(' ');
            text.push('\'');
            text.push_str(&part);
            text.push('\'');
        } else {
            text.push(' ');
            text.push_str(&part);
        }
    }
    text
}

/// The Cargo executable running this xtask, so the host uses the same toolchain.
fn cargo() -> std::ffi::OsString {
    std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_argument_with_a_space_is_quoted() {
        let mut command = Command::new("cargo");
        command.args(["run", "-p", "quota desktop"]);
        assert_eq!(render(&command), "cargo run -p 'quota desktop'");
    }

    #[test]
    fn an_argument_without_a_space_is_left_bare() {
        let mut command = Command::new("cargo");
        command.args(["run", "--features", "custom-protocol"]);
        assert_eq!(render(&command), "cargo run --features custom-protocol");
    }
}
