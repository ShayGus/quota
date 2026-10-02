//! Where the credentials other clients keep are found on this machine.
//!
//! Windows need not supply `HOME`; default credential discovery must work from
//! its native profile variable without a shell-provided home override.
//!
//! One rule holds: a provider's own directory override always wins, then the
//! operating system's user directory, then a shell variable. A profile path is
//! never assembled from a user name, because a guessed path is worse than an
//! honest "not found".

use std::path::PathBuf;

/// Reads one environment variable, without treating a blank value as one.
pub(crate) type Lookup<'a> = &'a dyn Fn(&str) -> Option<String>;

/// The first non-blank value of a variable, as a path.
fn variable(lookup: Lookup<'_>, name: &str) -> Option<PathBuf> {
    let value = lookup(name)?;
    if value.trim().is_empty() {
        return None;
    }
    Some(PathBuf::from(value))
}

/// The current user's profile directory, or `None` when nothing declares one.
///
/// On Windows this is `USERPROFILE`, which Windows exports to every process
/// from the same profile its known-folder API returns, with `HOME` as a fallback
/// if `USERPROFILE` is absent or blank. Everywhere else it is `HOME`.
///
/// Every credential this crate reads lives directly under this directory
/// (`.codex`, `.claude`, `.local/share/opencode`). The roaming and local
/// application-data folders are not needed by any reader here, so they are not
/// resolved: a resolver for them belongs with the first reader that needs one.
pub(crate) fn user_profile(lookup: Lookup<'_>) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        variable(lookup, "USERPROFILE").or_else(|| variable(lookup, "HOME"))
    }
    #[cfg(not(windows))]
    {
        variable(lookup, "HOME")
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::collections::HashMap;

    /// The profile variable this platform actually exports to a process.
    pub(crate) const NATIVE_PROFILE_VARIABLE: &str =
        if cfg!(windows) { "USERPROFILE" } else { "HOME" };

    /// An environment made only of the variables a test names.
    pub(crate) fn environment(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(name, value)| ((*name).to_owned(), (*value).to_owned()))
            .collect();
        move |name: &str| map.get(name).cloned()
    }

    #[test]
    fn a_blank_or_unset_variable_is_never_a_path() {
        let lookup = environment(&[("BLANK", "   ")]);
        assert_eq!(variable(&lookup, "BLANK"), None);
        assert_eq!(variable(&lookup, "SET"), None);
    }

    /// A Windows process is given its profile and no home at all.
    #[test]
    #[cfg(windows)]
    fn a_windows_profile_resolves_without_home() {
        let lookup = environment(&[("USERPROFILE", "C:\\Users\\someone")]);
        assert_eq!(
            user_profile(&lookup),
            Some(PathBuf::from("C:\\Users\\someone"))
        );
    }
}
