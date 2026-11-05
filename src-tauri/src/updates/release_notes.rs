//! What's new: the first run of a new version shows that version's release notes.
//!
//! The notes are `RELEASE_NOTES.md`, built into the program, so the panel needs
//! no network and shows only words that came with the signed build. The version
//! the notes were last handled for is saved in the preferences. A new install
//! has nothing to catch up on, so it only records its version.

use quota_contracts::ReleaseNotes;
use semver::Version;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder, Wry};

use crate::state::AppState;

/// The release notes, as they were when this build was made.
const NOTES: &str = include_str!("../../../RELEASE_NOTES.md");

/// The window's label. Its capability names the same word.
pub const LABEL: &str = "release-notes";

/// The window's size, in logical pixels.
const WIDTH: f64 = 460.0;
const HEIGHT: f64 = 520.0;

/// What a start does about the release notes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AfterStart {
    /// Nothing: they were handled for this version already.
    Nothing,
    /// Only saves this version: there is nothing to show.
    Record,
    /// Shows them, then saves this version.
    Show,
}

/// Decides what a start does. `last` is the version the notes were last handled
/// for, and `fresh` says that no preference has ever been saved. A saved profile
/// with no version comes from a build older than this panel, so it was updated.
#[must_use]
pub fn after_start(
    last: Option<&str>,
    current: &Version,
    fresh: bool,
    has_notes: bool,
) -> AfterStart {
    let updated = match last.map(Version::parse) {
        None => !fresh,
        Some(Ok(last)) if last == *current => return AfterStart::Nothing,
        Some(Ok(last)) => last < *current,
        Some(Err(_)) => false,
    };
    if updated && has_notes {
        AfterStart::Show
    } else {
        AfterStart::Record
    }
}

/// One version's list in the notes, or nothing when it has no section. A line
/// that is indented carries on the item above it.
#[must_use]
pub fn items_for(notes: &str, version: &str) -> Vec<String> {
    let heading = format!("## {version}");
    let mut items: Vec<String> = Vec::new();
    let lines = notes
        .lines()
        .skip_while(|line| line.trim_end() != heading)
        .skip(1)
        .take_while(|line| !line.starts_with("## "));
    for line in lines {
        if let Some(item) = line.strip_prefix("- ") {
            items.push(item.trim().to_owned());
        } else if line.starts_with(' ')
            && !line.trim().is_empty()
            && let Some(last) = items.last_mut()
        {
            last.push(' ');
            last.push_str(line.trim());
        }
    }
    items
}

/// This build's own notes, or `None` when its version has none.
#[must_use]
pub fn current(app: &AppHandle<Wry>) -> Option<ReleaseNotes> {
    let version = app.package_info().version.to_string();
    let items = items_for(NOTES, &version);
    (!items.is_empty()).then_some(ReleaseNotes { version, items })
}

/// Shows the notes if this is the first run of a new version, and saves that
/// they were handled. If the window can't open, nothing is saved, so the next
/// start tries again.
pub(crate) async fn show_if_updated(state: &AppState) {
    let version = state.app.package_info().version.clone();
    let (last, fresh) = {
        let preferences = state.preferences_state.read().await;
        (
            preferences.last_release_notes.clone(),
            preferences.revision == 0,
        )
    };
    let decision = after_start(
        last.as_deref(),
        &version,
        fresh,
        current(&state.app).is_some(),
    );
    if decision == AfterStart::Nothing {
        return;
    }
    if decision == AfterStart::Show
        && let Err(reason) = open_window(&state.app)
    {
        tracing::warn!(%reason, "the release notes could not be shown");
        return;
    }
    record(state, version.to_string()).await;
}

/// Saves that the notes were handled for this version.
async fn record(state: &AppState, version: String) {
    let saved = crate::ipc::commands_prefs::change_preferences(state, move |preferences| {
        preferences.last_release_notes = Some(version);
    })
    .await;
    if let Err(error) = saved {
        tracing::warn!(
            code = error.diagnostic_code(),
            "the release notes' version could not be saved"
        );
    }
}

/// Opens the window, centred and on top so it isn't lost behind the widget.
fn open_window(app: &AppHandle<Wry>) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(LABEL) {
        return window.set_focus().map_err(|error| error.to_string());
    }
    WebviewWindowBuilder::new(
        app,
        LABEL,
        WebviewUrl::App("index.html#/release-notes".into()),
    )
    .title("What's new in Quota")
    .inner_size(WIDTH, HEIGHT)
    .resizable(false)
    .maximizable(false)
    .minimizable(false)
    .decorations(false)
    .skip_taskbar(true)
    .always_on_top(true)
    .center()
    .focused(true)
    .build()
    .map(|_| ())
    .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(text: &str) -> Version {
        Version::parse(text).expect("a version")
    }

    const NEW: &str = "0.2.0";

    #[test]
    fn a_new_install_only_records_its_version() {
        assert_eq!(
            after_start(None, &version(NEW), true, true),
            AfterStart::Record
        );
    }

    #[test]
    fn an_update_from_a_build_older_than_the_panel_shows_the_notes() {
        assert_eq!(
            after_start(None, &version(NEW), false, true),
            AfterStart::Show
        );
    }

    #[test]
    fn an_update_shows_the_notes_once() {
        assert_eq!(
            after_start(Some("0.1.9"), &version(NEW), false, true),
            AfterStart::Show
        );
        assert_eq!(
            after_start(Some(NEW), &version(NEW), false, true),
            AfterStart::Nothing
        );
    }

    #[test]
    fn a_version_with_no_notes_shows_nothing_and_is_recorded() {
        assert_eq!(
            after_start(Some("0.1.9"), &version(NEW), false, false),
            AfterStart::Record
        );
    }

    #[test]
    fn going_back_a_version_or_an_unreadable_one_shows_nothing() {
        assert_eq!(
            after_start(Some("0.3.0"), &version(NEW), false, true),
            AfterStart::Record
        );
        assert_eq!(
            after_start(Some("not a version"), &version(NEW), false, true),
            AfterStart::Record
        );
    }

    const SAMPLE: &str = "# Release notes\n\nAbout this file.\n\n## 0.2.0\n\n- One\n- Two that\n  carries on\n\n## 0.1.0\n\n- Old\n";

    #[test]
    fn reads_only_the_asked_version() {
        assert_eq!(items_for(SAMPLE, "0.2.0"), ["One", "Two that carries on"]);
        assert_eq!(items_for(SAMPLE, "0.1.0"), ["Old"]);
    }

    #[test]
    fn a_version_with_no_section_has_no_items() {
        assert!(items_for(SAMPLE, "9.9.9").is_empty());
        assert!(items_for(SAMPLE, "0.2").is_empty());
    }
}
