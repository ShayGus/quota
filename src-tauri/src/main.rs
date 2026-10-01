//! Desktop entry point. Composition and lifecycle live in the library.

// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    quota_desktop_lib::run();
}
