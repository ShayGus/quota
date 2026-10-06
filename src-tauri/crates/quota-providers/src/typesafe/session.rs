//! The console session Quota keeps: a `Cookie` header, nothing else.
//!
//! Quota signs in to the console in a window of its own and keeps the cookies
//! that window ends up with, as one header, in the system credential store.
//! The console may renew the session on any answer; a renewed cookie replaces
//! the old one in the header, and a cookie the console clears is dropped.

use quota_core::ports::{ProviderError, Secret};

/// A session header Quota can send, or the reason it cannot.
///
/// # Errors
/// Returns [`ProviderError::Authentication`] for an empty header, or one with
/// a line break or no cookie in it, which no sign-in produces.
pub(crate) fn checked(session: &Secret) -> Result<&str, ProviderError> {
    let header = session.expose().trim();
    if header.is_empty() || header.contains(['\r', '\n']) || !header.contains('=') {
        return Err(ProviderError::Authentication);
    }
    Ok(header)
}

/// The header with the cookies a response set or cleared applied to it, or
/// `None` when nothing changed.
pub(crate) fn renewed(header: &str, set_cookies: &[String]) -> Option<String> {
    let mut cookies: Vec<(String, String)> = header
        .split(';')
        .filter_map(|pair| {
            let (name, value) = pair.trim().split_once('=')?;
            (!name.is_empty()).then(|| (name.to_owned(), value.to_owned()))
        })
        .collect();
    let before = cookies.clone();
    for set_cookie in set_cookies {
        let mut parts = set_cookie.split(';');
        let Some((name, value)) = parts.next().and_then(|pair| pair.trim().split_once('=')) else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        let cleared = value.is_empty()
            || parts.any(|attribute| {
                let attribute = attribute.trim().to_ascii_lowercase();
                attribute == "max-age=0" || attribute.starts_with("max-age=-")
            });
        if cleared {
            cookies.retain(|(existing, _)| existing != name);
        } else if let Some(existing) = cookies.iter_mut().find(|(existing, _)| existing == name) {
            value.trim().clone_into(&mut existing.1);
        } else {
            cookies.push((name.to_owned(), value.trim().to_owned()));
        }
    }
    if cookies == before || cookies.is_empty() {
        return None;
    }
    Some(
        cookies
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; "),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_usable_header_is_sent() {
        assert_eq!(checked(&Secret::new("a=1; b=2".to_owned())), Ok("a=1; b=2"));
        for bad in ["", "  ", "no cookie", "a=1\r\nX: y"] {
            assert_eq!(
                checked(&Secret::new(bad.to_owned())),
                Err(ProviderError::Authentication),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn a_renewed_cookie_replaces_the_old_one_and_a_cleared_one_is_dropped() {
        let header = "session=old; theme=dark; tracking=x";
        let renewed = renewed(
            header,
            &[
                "session=new; Path=/; HttpOnly; Secure".to_owned(),
                "tracking=; Max-Age=0".to_owned(),
                "theme=dark".to_owned(),
            ],
        );
        assert_eq!(renewed.as_deref(), Some("session=new; theme=dark"));
    }

    #[test]
    fn nothing_new_is_no_change() {
        assert_eq!(renewed("a=1", &[]), None);
        assert_eq!(renewed("a=1", &["a=1; Path=/".to_owned()]), None);
        assert_eq!(renewed("a=1", &["a=; Max-Age=0".to_owned()]), None);
    }
}
