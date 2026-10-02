//! Desktop entry point. Composition and lifecycle live in the library.

// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::process::ExitCode;

fn main() -> ExitCode {
    quota_desktop_lib::run()
}
