//! The real-app test launcher: Quota with its provider transport pointed at a
//! local fake server.
//!
//! Only the real-app suite (`bun run build:e2e`) builds this, and only with the
//! non-default `sample-data` feature, which compiles in `test-fixtures` and with
//! it `quota_providers::retarget`. A default or release build has neither the
//! feature nor this target, and the shipped `quota` binary is not changed by it:
//! it is the same `quota_desktop_lib::run()` behind one extra call.
//!
//! The base address is read here, in the test launcher, from
//! `QUOTA_E2E_PROVIDER_BASE`. The provider transport itself never reads the
//! environment, and `cargo xtask check-release` keeps it that way. The update
//! address is read the same way, from `QUOTA_E2E_UPDATE_ENDPOINT`; the updater
//! never reads the environment either.

use std::process::ExitCode;

fn main() -> ExitCode {
    if let Ok(base) = std::env::var("QUOTA_E2E_PROVIDER_BASE") {
        quota_providers::retarget(&base);
    }
    // An update server the suite watches. This build never checks for updates, so
    // the address only lets a test prove that no request is ever made.
    if let Ok(endpoint) = std::env::var("QUOTA_E2E_UPDATE_ENDPOINT") {
        quota_desktop_lib::updates::retarget_for_tests(&endpoint);
    }
    quota_desktop_lib::run()
}
