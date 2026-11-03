//! Telling the `TypeSafe` console's answers apart, as text.
//!
//! The console sits behind a bot check, and it can answer an ended session
//! with its sign-in page instead of a refusal. Everything here is a pure
//! function of text, so it is tested without a network.

/// Whether a page is the console's sign-in page, which it can serve with a
/// success status instead of a redirect.
pub(crate) fn is_login_landing(text: &str) -> bool {
    let unescaped = text.replace("\\\"", "\"");
    unescaped.contains("\"(auth)\",{\"children\":[\"login\"")
}

/// Whether an answer is a bot check or a challenge page rather than the
/// console itself. Quota never tries to pass one.
pub(crate) fn is_challenge(status: u16, mitigated: bool, text: &str) -> bool {
    if mitigated {
        return true;
    }
    if !matches!(status, 403 | 429 | 503) {
        return false;
    }
    let lower = text.to_ascii_lowercase();
    [
        "challenge-platform",
        "cf-chl",
        "cf_chl",
        "just a moment",
        "attention required",
        "vercel security checkpoint",
        "verify you are human",
        "captcha",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sign_in_page_is_recognised_escaped_or_not() {
        assert!(is_login_landing(
            r#"self.__next_f.push([1,"[\"(auth)\",{\"children\":[\"login\",{}]}]"])"#
        ));
        assert!(is_login_landing(r#"["(auth)",{"children":["login",{}]}]"#));
        assert!(!is_login_landing(
            r#"["(dashboard)",{"children":["settings"]}]"#
        ));
    }

    #[test]
    fn challenge_pages_are_told_apart_from_refusals() {
        assert!(is_challenge(200, true, ""));
        assert!(is_challenge(403, false, "<title>Just a moment...</title>"));
        assert!(is_challenge(
            403,
            false,
            "<title>Attention Required! | Cloudflare</title>"
        ));
        assert!(is_challenge(
            429,
            false,
            "<script src=\"/cdn-cgi/challenge-platform/h/b\"></script>"
        ));
        assert!(is_challenge(503, false, "Vercel Security Checkpoint"));
        assert!(!is_challenge(403, false, "{\"error\":\"forbidden\"}"));
        assert!(!is_challenge(200, false, "Just a moment"));
    }
}
