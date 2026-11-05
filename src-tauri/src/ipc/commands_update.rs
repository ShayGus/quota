//! The update pop-up's two commands, and the what's new panel's one.
//!
//! Only the pop-up window holds the first two's permissions. The window asks what to show,
//! and says what the person pressed; the update flow in `updates` decides
//! everything else, so a window cannot start, skip, or redirect an update.

use quota_contracts::{CommandError, ReleaseNotes, UpdatePrompt, UpdateResponse};
use tauri::State;

use crate::updates::PromptSlot;

/// What the update pop-up shows, or nothing when no update is being offered.
#[tauri::command]
#[specta::specta]
#[must_use]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri hands a command its managed state by value"
)]
pub fn get_update_prompt(slot: State<'_, PromptSlot>) -> Option<UpdatePrompt> {
    slot.current()
}

/// The release notes built into this version, for the what's new panel, or
/// nothing when this version has none.
#[tauri::command]
#[specta::specta]
#[must_use]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri hands a command its application handle by value"
)]
pub fn get_release_notes(app: tauri::AppHandle) -> Option<ReleaseNotes> {
    crate::updates::release_notes::current(&app)
}

/// Passes the person's answer to the update flow.
///
/// An answer that does not fit what the pop-up shows, such as pressing OK twice
/// or any button during the install, is refused.
#[tauri::command]
#[specta::specta]
#[expect(
    clippy::needless_pass_by_value,
    reason = "Tauri hands a command its managed state by value"
)]
pub fn respond_to_update_prompt(
    slot: State<'_, PromptSlot>,
    response: UpdateResponse,
) -> Result<(), CommandError> {
    if slot.respond(response) {
        Ok(())
    } else {
        Err(CommandError::ValidationFailed {
            field: "response".to_owned(),
            reason: "that answer does not belong to the update pop-up as it is".to_owned(),
        })
    }
}
