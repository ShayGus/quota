//! Bug-report commands: the prefilled GitHub issue and the agent prompt.
//!
//! The renderer supplies nothing; the host gathers the environment itself, so
//! no window can put an address, a path, or an account name into a report.

#![expect(
    clippy::unreachable,
    clippy::let_underscore_must_use,
    reason = "`#[tauri::command]` expands to `let _check: ReturnType = unreachable!()`, which both lints report against the handler signature"
)]

use quota_contracts::CommandError;
use tauri::State;

use crate::state::AppState;

/// Opens GitHub's new-issue form for Quota, prefilled with the environment.
#[tauri::command]
#[specta::specta]
pub async fn open_bug_report_issue(state: State<'_, AppState>) -> Result<(), CommandError> {
    crate::bug_report::open_issue(&state).await
}

/// Copies a prompt that asks an AI agent to file the bug for the person.
#[tauri::command]
#[specta::specta]
pub async fn copy_bug_report_prompt(state: State<'_, AppState>) -> Result<(), CommandError> {
    crate::bug_report::copy_prompt(&state).await
}
