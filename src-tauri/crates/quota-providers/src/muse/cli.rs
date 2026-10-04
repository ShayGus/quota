//! The sign-in the Muse CLI keeps on this computer.
//!
//! `muse` stores its sign-in in `auth.json` at `MUSE_AUTH_PATH` when set,
//! otherwise in `.config/muse` in the user profile, under
//! `providers.meta.access_token`. A Meta account token starts `dca:`. Quota
//! only reads the file and never changes it.

use quota_core::ports::{ProviderError, Secret};

use crate::credentials::{process_lookup, profile_directory, read_json, string_at};
use crate::platform::Lookup;

/// The profile label a connection that reads the CLI's sign-in carries.
pub(crate) const PROFILE: &str = "muse-cli";

/// The CLI's account token.
pub(crate) async fn token() -> Result<Secret, ProviderError> {
    let lookup: Lookup<'_> = &process_lookup;
    let path = match lookup("MUSE_AUTH_PATH") {
        Some(path) => std::path::PathBuf::from(path),
        None => profile_directory(crate::platform::system(), lookup)?
            .join(".config")
            .join("muse")
            .join("auth.json"),
    };
    let document = read_json(&path).await?;
    string_at(&document, &[&["providers", "meta", "access_token"]])
        .filter(|token| token.starts_with("dca:"))
        .map(Secret::new)
        .ok_or(ProviderError::Authentication)
}
