//! Reporting a bug: a prefilled GitHub issue, or a prompt for an AI agent.
//!
//! Both carry the same environment block: Quota's version, the operating
//! system and architecture, how the app is shown, and which providers are
//! connected. Nothing else is gathered. Account names, nicknames, addresses,
//! credentials, profile paths and the user name never reach either, so the log
//! location is written in its unexpanded form, such as `%LOCALAPPDATA%`.

use quota_contracts::CommandError;
use quota_domain::preferences::{AppView, IndicatorStyle, OverviewMode};
use quota_domain::provider::ProviderId;

/// The repository a report is filed in.
pub const REPOSITORY: &str = "ShayGus/quota";

/// The new-issue form a report opens. Only this address is ever opened here.
const NEW_ISSUE: &str = "https://github.com/ShayGus/quota/issues/new";

/// The issue form in `.github/ISSUE_TEMPLATE`, whose field ids the query fills.
const TEMPLATE: &str = "bug_report.yml";

/// What a report says about this installation, and nothing more.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Environment {
    /// Quota's version.
    pub version: String,
    /// The operating system, as Rust names it: `windows`, `linux`, `macos`.
    pub os: &'static str,
    /// The processor architecture.
    pub arch: &'static str,
    /// How the app is shown: the full window or the mini widget, and its look.
    pub view: String,
    /// The connected providers, each once, in a stable order.
    pub providers: Vec<ProviderId>,
    /// Where the log is, with the home directory left as a variable.
    pub log_location: String,
}

impl Environment {
    /// Gathers the environment from the running app.
    pub async fn gather(state: &crate::state::AppState) -> Self {
        let mut providers: Vec<ProviderId> = state
            .registry
            .read()
            .await
            .iter()
            .map(|account| account.binding.provider_id)
            .collect();
        providers.sort_by_key(|provider| provider.as_str());
        providers.dedup();
        let preferences = state.preferences_state.read().await;
        let view = describe_view(
            preferences.view,
            preferences.overview_mode,
            preferences.indicator_style,
        );
        drop(preferences);
        Self {
            version: state.app.package_info().version.to_string(),
            os: std::env::consts::OS,
            arch: std::env::consts::ARCH,
            view,
            providers,
            log_location: log_location(std::env::consts::OS, &state.app.config().identifier),
        }
    }

    /// The providers as one line, or a word that says there are none.
    fn provider_list(&self) -> String {
        if self.providers.is_empty() {
            return "none".to_owned();
        }
        self.providers
            .iter()
            .map(|provider| provider.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// The operating system with its architecture.
    fn system(&self) -> String {
        format!("{} ({})", self.os, self.arch)
    }

    /// The environment as a short list, the same in the issue and the prompt.
    fn block(&self) -> String {
        format!(
            "- Quota version: {}\n- Operating system: {}\n- View: {}\n- Connected providers: {}\n",
            self.version,
            self.system(),
            self.view,
            self.provider_list()
        )
    }
}

/// How the app is shown, in words.
fn describe_view(view: AppView, mode: OverviewMode, style: IndicatorStyle) -> String {
    let look = match style {
        IndicatorStyle::Ring => "rings",
        IndicatorStyle::Bar => "bars",
    };
    match view {
        AppView::Widget => format!("mini widget, {look}"),
        AppView::Overview => {
            let place = match mode {
                OverviewMode::Tray => "docked to the tray",
                OverviewMode::Floating => "floating",
            };
            format!("full window, {place}, {look}")
        }
    }
}

/// The log file's location with the home directory unexpanded, as Tauri
/// resolves the application log directory on each platform.
fn log_location(os: &str, identifier: &str) -> String {
    match os {
        "windows" => format!("%LOCALAPPDATA%\\{identifier}\\logs\\quota.log"),
        "macos" => format!("~/Library/Logs/{identifier}/quota.log"),
        _ => format!("~/.local/share/{identifier}/logs/quota.log"),
    }
}

/// The new-issue address, with the form's environment fields filled in.
#[must_use]
pub fn issue_url(environment: &Environment) -> String {
    let fields = [
        ("template", TEMPLATE.to_owned()),
        ("version", environment.version.clone()),
        ("os", environment.system()),
        ("view", environment.view.clone()),
        ("providers", environment.provider_list()),
    ];
    let query = fields
        .iter()
        .map(|(key, value)| format!("{key}={}", encode(value)))
        .collect::<Vec<_>>()
        .join("&");
    format!("{NEW_ISSUE}?{query}")
}

/// A prompt that asks an AI coding agent to file the bug for the person.
#[must_use]
pub fn agent_prompt(environment: &Environment) -> String {
    format!(
        "Please help me report a bug in Quota, the desktop app that monitors AI \
         subscription allowances. Its repository is https://github.com/{REPOSITORY}.\n\n\
         1. Ask me what happened, the steps that lead to it, and what I expected instead. \
         Ask follow-up questions until the report is clear enough for a developer to \
         reproduce.\n\
         2. Write a short, specific title and a body with these sections: What happened, \
         Steps to reproduce, Expected behaviour, Environment.\n\
         3. Show me the full title and body and wait for my approval before filing \
         anything.\n\
         4. File it with the GitHub CLI: \
         `gh issue create --repo {REPOSITORY} --title \"...\" --body \"...\"`. \
         If `gh` is not installed or not signed in, open {NEW_ISSUE}?template={TEMPLATE} \
         instead and tell me what to paste into each field.\n\n\
         Never include credentials, API keys, tokens, cookies, email addresses, account \
         names, user names or file paths in the issue. If I share a log excerpt, remove \
         any of these from it first.\n\n\
         Optional evidence, only if I agree after reviewing it myself:\n\
         - Quota's log: {}\n\
         - Settings → Diagnostics → Export diagnostics writes a file with the account \
         count, the provider names and the polling settings; Quota shows where it saved \
         it.\n\n\
         Environment (include this in the issue):\n{}",
        environment.log_location,
        environment.block()
    )
}

/// Percent-encodes a query value: everything but the unreserved characters.
fn encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            for nibble in [byte >> 4, byte & 0x0f] {
                // A nibble is always below 16, so a digit always exists.
                if let Some(digit) = char::from_digit(u32::from(nibble), 16) {
                    encoded.push(digit.to_ascii_uppercase());
                }
            }
        }
    }
    encoded
}

/// Opens the prefilled new-issue form in the system browser.
///
/// # Errors
/// Returns [`CommandError::NativeOperationFailed`] when the browser refuses.
pub async fn open_issue(state: &crate::state::AppState) -> Result<(), CommandError> {
    let url = issue_url(&Environment::gather(state).await);
    crate::bootstrap_helpers::open_external(&state.app, &url)
}

/// Copies the agent prompt to the system clipboard.
///
/// # Errors
/// Returns [`CommandError::NativeOperationFailed`] when the clipboard refuses.
pub async fn copy_prompt(state: &crate::state::AppState) -> Result<(), CommandError> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    let prompt = agent_prompt(&Environment::gather(state).await);
    state.app.clipboard().write_text(prompt).map_err(|_| {
        tracing::warn!("the bug-report prompt could not be copied");
        CommandError::NativeOperationFailed {
            operation: "copy_bug_report_prompt".into(),
            reason: format!(
                "the clipboard refused the prompt. The log is at {}",
                crate::file_log::location(&state.app)
            ),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn environment(providers: Vec<ProviderId>) -> Environment {
        Environment {
            version: "0.1.2".to_owned(),
            os: "windows",
            arch: "x86_64",
            view: describe_view(AppView::Overview, OverviewMode::Tray, IndicatorStyle::Ring),
            providers,
            log_location: log_location("windows", "app.quota.monitor"),
        }
    }

    #[test]
    fn the_issue_form_is_prefilled_with_the_environment_only() {
        let url = issue_url(&environment(vec![ProviderId::Claude, ProviderId::MuseCode]));
        assert_eq!(
            url,
            "https://github.com/ShayGus/quota/issues/new?template=bug_report.yml\
             &version=0.1.2&os=windows%20%28x86_64%29\
             &view=full%20window%2C%20docked%20to%20the%20tray%2C%20rings\
             &providers=claude%2C%20muse_code"
        );
    }

    #[test]
    fn no_connected_provider_is_said_in_words() {
        let url = issue_url(&environment(Vec::new()));
        assert!(url.ends_with("&providers=none"));
    }

    #[test]
    fn the_prompt_carries_the_environment_and_the_rules() {
        let prompt = agent_prompt(&environment(vec![ProviderId::Codex]));
        assert!(prompt.contains("gh issue create --repo ShayGus/quota"));
        assert!(prompt.contains("wait for my approval"));
        assert!(prompt.contains("- Quota version: 0.1.2\n"));
        assert!(prompt.contains("- Connected providers: codex\n"));
        assert!(prompt.contains("%LOCALAPPDATA%\\app.quota.monitor\\logs\\quota.log"));
        assert!(prompt.contains("Export diagnostics"));
    }

    #[test]
    fn nothing_personal_reaches_a_report() {
        let environment = environment(vec![ProviderId::Claude]);
        let user = std::env::var("USERNAME")
            .or_else(|_| std::env::var("USER"))
            .unwrap_or_default();
        for text in [issue_url(&environment), agent_prompt(&environment)] {
            assert!(!text.contains('@'), "no address: {text}");
            assert!(!text.contains(":\\"), "no expanded Windows path: {text}");
            assert!(!text.contains("/home/") && !text.contains("/Users/"));
            if user.len() > 2 {
                assert!(!text.contains(&user), "no user name");
            }
        }
    }

    #[test]
    fn the_log_location_leaves_the_home_directory_unexpanded() {
        assert_eq!(
            log_location("linux", "app.quota.monitor"),
            "~/.local/share/app.quota.monitor/logs/quota.log"
        );
        assert_eq!(
            log_location("macos", "app.quota.monitor"),
            "~/Library/Logs/app.quota.monitor/quota.log"
        );
    }

    #[test]
    fn the_widget_and_floating_window_are_described() {
        assert_eq!(
            describe_view(AppView::Widget, OverviewMode::Tray, IndicatorStyle::Bar),
            "mini widget, bars"
        );
        assert_eq!(
            describe_view(
                AppView::Overview,
                OverviewMode::Floating,
                IndicatorStyle::Bar
            ),
            "full window, floating, bars"
        );
    }

    #[test]
    fn query_values_are_percent_encoded() {
        assert_eq!(encode("a b&c=d/é"), "a%20b%26c%3Dd%2F%C3%A9");
    }
}
