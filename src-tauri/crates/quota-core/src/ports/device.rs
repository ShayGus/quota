//! A sign-in the person completes in their browser with a short code.
//!
//! This is the OAuth device authorization grant (RFC 8628): Quota asks the
//! provider for a code, the person opens the provider's page and enters it,
//! and Quota polls until the provider grants a token. The token is then a
//! credential Quota owns, held and stored exactly like a pasted key.

use chrono::{DateTime, Utc};

use super::Secret;

/// A started device sign-in, waiting for the person.
#[derive(Debug)]
pub struct DeviceAuthorization {
    /// The code the person enters on the provider's page.
    pub user_code: String,
    /// The page where they enter it.
    pub verification_uri: String,
    /// The page with the code already filled in, when the provider offers it.
    /// Quota opens this one.
    pub verification_uri_complete: Option<String>,
    /// The code Quota polls with. It never leaves this process.
    pub device_code: Secret,
    /// How long to wait between polls, in seconds.
    pub interval_seconds: u64,
    /// When the code stops working.
    pub expires_at: DateTime<Utc>,
}

/// What one poll found.
#[derive(Debug)]
pub enum DevicePoll {
    /// The person has not finished yet.
    Pending,
    /// The provider asked Quota to poll less often.
    SlowDown,
    /// The person approved; this is the credential to verify and store.
    Granted(Secret),
    /// The person declined on the provider's page.
    Denied,
    /// The code expired before the person finished.
    Expired,
}

/// A sign-in the person completes on the provider's own website, in a window
/// Quota opens for it, for a provider that offers no other way to read usage.
///
/// The window keeps its browser storage apart from every other browser, and
/// the session it ends up with is the credential: Quota builds a `Cookie`
/// header from the window's cookies for `cookie_url` and keeps that, exactly
/// like a pasted key. The page in the window has no access to the app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsoleSignIn {
    /// The page the window opens, the provider's own sign-in.
    pub sign_in_url: &'static str,
    /// The address whose cookies make up the session.
    pub cookie_url: &'static str,
}
