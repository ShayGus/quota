//! A test-only way to point the updater at a local server.
//!
//! Compiled only with the `sample-data` feature, which no release builds and
//! which [`super::policy::may_check_for_updates`] always keeps out of the
//! update flow. It exists so the real-app suite can prove that, even when an
//! update server is reachable and configured, such a build never contacts it.

use std::sync::OnceLock;

use tauri_plugin_updater::UpdaterBuilder;

/// The address the real-app launcher chose.
static ENDPOINT: OnceLock<String> = OnceLock::new();

/// Points the updater at `endpoint`, for the real-app suite only.
///
/// The sample-data build never checks for updates, so this changes nothing a
/// person can see; it lets a test prove that no request is made. The first call
/// wins.
pub fn retarget_for_tests(endpoint: &str) {
    if ENDPOINT.set(endpoint.to_owned()).is_err() {
        tracing::debug!("the test update address was already set");
    }
}

/// Applies the remembered address, if the launcher set one.
pub(super) fn apply(builder: UpdaterBuilder) -> Result<UpdaterBuilder, String> {
    let Some(endpoint) = ENDPOINT.get() else {
        return Ok(builder);
    };
    let url = endpoint
        .parse()
        .map_err(|error| format!("the test update address is not valid: {error}"))?;
    builder
        .endpoints(vec![url])
        .map_err(|error| error.to_string())
}
