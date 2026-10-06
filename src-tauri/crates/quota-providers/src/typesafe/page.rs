//! Reading the `TypeSafe` console's billing page, as text.
//!
//! The console is a Next.js application with no documented API for billing.
//! Its billing page loads the overview through a server action, which is
//! called with a `Next-Action` header naming the action's identifier. The
//! identifier changes with every deployment, so it is found in the page's own
//! scripts, next to the action's name, as `CodexBar` finds it. Everything here
//! is a pure function of text, so it is tested without a network.

/// The action that answers the billing overview, by the name its scripts give it.
const ACTION_NAME: &str = "\"getBillingOverviewResult\"";

/// The most scripts a page may list before the rest are ignored.
const MAX_SCRIPTS: usize = 80;

/// How far before the action's name its identifier may stand.
const ACTION_SPAN: usize = 150;

/// The shortest identifier a server action has, in hexadecimal digits.
const MIN_ACTION_DIGITS: usize = 40;

/// The page's own scripts, as absolute addresses on `origin`, in page order.
///
/// Only scripts the console serves itself are kept: a third-party script
/// cannot name the console's actions.
pub(crate) fn script_sources(html: &str, origin: &str) -> Vec<String> {
    let mut sources = Vec::new();
    let lower = html.to_ascii_lowercase();
    let mut from = 0;
    while let Some(found) = lower.get(from..).and_then(|rest| rest.find("<script")) {
        let start = from + found;
        let Some(end) = lower.get(start..).and_then(|rest| rest.find('>')) else {
            break;
        };
        let tag = html.get(start..start + end).unwrap_or_default();
        from = start + end;
        let Some(source) = attribute(tag, "src") else {
            continue;
        };
        let absolute = if source.starts_with('/') && !source.starts_with("//") {
            format!("{origin}{source}")
        } else {
            source.to_owned()
        };
        let path = absolute.split(['?', '#']).next().unwrap_or_default();
        if absolute.starts_with(&format!("{origin}/"))
            && path.to_ascii_lowercase().ends_with(".js")
            && !sources.contains(&absolute)
        {
            sources.push(absolute);
            if sources.len() == MAX_SCRIPTS {
                break;
            }
        }
    }
    sources
}

/// One attribute of a tag, quoted with either quote.
fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(found) = lower.get(from..).and_then(|rest| rest.find(name)) {
        let at = from + found;
        from = at + name.len();
        let before = lower.get(..at).and_then(|head| head.chars().last());
        if !before.is_some_and(char::is_whitespace) {
            continue;
        }
        let rest = tag.get(from..)?.trim_start();
        let Some(rest) = rest.strip_prefix('=') else {
            continue;
        };
        let rest = rest.trim_start();
        let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
        let value = rest.get(1..)?;
        return value.find(quote).and_then(|end| value.get(..end));
    }
    None
}

/// The billing action's identifier in one script, when this script names it.
///
/// The identifier is a quoted hexadecimal string of at least 40 digits that
/// stands within 150 characters before the action's quoted name, with no
/// closing parenthesis between them.
pub(crate) fn action_id(script: &str) -> Option<String> {
    let mut from = 0;
    while let Some(found) = script.get(from..).and_then(|rest| rest.find(ACTION_NAME)) {
        let name_at = from + found;
        from = name_at + ACTION_NAME.len();
        let mut window_start = name_at.saturating_sub(ACTION_SPAN + 200);
        while !script.is_char_boundary(window_start) {
            window_start += 1;
        }
        let Some(before) = script.get(window_start..name_at) else {
            continue;
        };
        if let Some(id) = last_hex_string(before) {
            return Some(id);
        }
    }
    None
}

/// The last quoted hexadecimal string in `text` that ends close enough to
/// its end, with no `)` after it.
fn last_hex_string(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut end = bytes.len();
    while let Some(close) = text.get(..end).and_then(|head| head.rfind('"')) {
        let tail = text.get(close + 1..).unwrap_or_default();
        if tail.len() > ACTION_SPAN || tail.contains(')') {
            return None;
        }
        let open = text.get(..close).and_then(|head| head.rfind('"'))?;
        let inner = text.get(open + 1..close).unwrap_or_default();
        if inner.len() >= MIN_ACTION_DIGITS && inner.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Some(inner.to_ascii_lowercase());
        }
        end = open;
    }
    None
}

/// The action's result in a React Server Components answer: the first row
/// whose value is a JSON object with an `ok` member.
///
/// Each row is `<id>:<json>`; rows that are not JSON objects are skipped.
pub(crate) fn action_result(text: &str) -> Option<serde_json::Value> {
    text.lines().find_map(|line| {
        let (id, value) = line.split_once(':')?;
        if id.is_empty() {
            return None;
        }
        let parsed: serde_json::Value = serde_json::from_str(value).ok()?;
        parsed
            .as_object()
            .is_some_and(|object| object.contains_key("ok"))
            .then_some(parsed)
    })
}

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

    const ORIGIN: &str = "https://console.typesafe.ai";

    #[test]
    fn only_the_consoles_own_scripts_are_kept_in_order() {
        let html = r#"<html><head>
            <script src="/_next/static/chunks/a.js"></script>
            <SCRIPT async src='/_next/static/chunks/b.js?v=2'></SCRIPT>
            <script src="https://cdn.example.test/c.js"></script>
            <script src="//cdn.example.test/d.js"></script>
            <script>inline()</script>
            <script data-src="/x.js" src="/_next/static/chunks/a.js"></script>
            <script src="https://console.typesafe.ai/_next/static/chunks/e.js"></script>
            <link href="/style.css">
        </head></html>"#;
        assert_eq!(
            script_sources(html, ORIGIN),
            [
                "https://console.typesafe.ai/_next/static/chunks/a.js",
                "https://console.typesafe.ai/_next/static/chunks/b.js?v=2",
                "https://console.typesafe.ai/_next/static/chunks/e.js",
            ]
        );
    }

    #[test]
    fn at_most_eighty_scripts_are_listed() {
        let tags: Vec<String> = (0..100)
            .map(|index| format!(r#"<script src="/s{index}.js"></script>"#))
            .collect();
        let html = tags.concat();
        assert_eq!(script_sources(&html, ORIGIN).len(), 80);
    }

    #[test]
    fn the_action_id_is_the_hex_string_just_before_its_name() {
        let id = "0f".repeat(21);
        let chunk = format!(
            r#"let a=(0,s.createServerReference)("{other}",s.callServer,void 0,s.findSourceMapURL,"getUsage");let b=(0,s.createServerReference)("{id}",s.callServer,void 0,s.findSourceMapURL,"getBillingOverviewResult");"#,
            other = "ab".repeat(21),
        );
        assert_eq!(action_id(&chunk), Some(id));
    }

    #[test]
    fn a_hex_string_too_far_away_or_behind_a_parenthesis_is_not_the_action() {
        let id = "ab".repeat(21);
        let far = format!(r#""{id}"{}"getBillingOverviewResult""#, " ".repeat(200));
        assert_eq!(action_id(&far), None);
        let closed = format!(r#"x("{id}"),y("getBillingOverviewResult")"#);
        assert_eq!(action_id(&closed), None);
        let short = r#"("abc123",s.callServer,"getBillingOverviewResult")"#;
        assert_eq!(action_id(short), None);
        assert_eq!(action_id("nothing here"), None);
    }

    #[test]
    fn the_result_is_the_first_row_with_an_ok_member() {
        let answer = "0:{\"a\":\"$@1\",\"f\":\"\",\"b\":\"build\"}\n1:{\"ok\":true,\"data\":{\"billing\":{\"balance\":4.2}}}\n2:{\"ok\":false}\n";
        let result = action_result(answer).expect("a result");
        assert_eq!(result["ok"], true);
        assert_eq!(result["data"]["billing"]["balance"], 4.2);
        assert_eq!(action_result("0:[1,2]\n1:\"text\"\nnot a row"), None);
    }

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
            429,
            false,
            "<script src=\"/cdn-cgi/challenge-platform/h/b\"></script>"
        ));
        assert!(is_challenge(503, false, "Vercel Security Checkpoint"));
        assert!(!is_challenge(403, false, "{\"error\":\"forbidden\"}"));
        assert!(!is_challenge(200, false, "Just a moment"));
    }
}
