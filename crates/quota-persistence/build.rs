//! Embeds the Windows application manifest into this crate's test binaries.
//!
//! The tests link Tauri, which imports entry points only the version 6 common
//! controls provide. Without the manifest that selects them, a test binary
//! exits with `STATUS_ENTRYPOINT_NOT_FOUND` before any test runs. The manifest is
//! the desktop host's, so both select the same libraries.

fn main() {
    let windows = std::env::var("CARGO_CFG_TARGET_OS").is_ok_and(|os| os == "windows");
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").is_ok_and(|env| env == "msvc");
    if !(windows && msvc) {
        return;
    }
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../src-tauri/windows-app-manifest.xml");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
    println!("cargo:rustc-link-arg=/MANIFESTINPUT:{}", manifest.display());
}
