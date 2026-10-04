//! Preference, policy, and local-data commands.
//!
//! A command returns the confirmed durable state, never an optimistic success
//! badge. A save that failed returns a typed persistence error.

#![expect(
    clippy::unreachable,
    clippy::let_underscore_must_use,
    reason = "`#[tauri::command]` expands to `let _check: ReturnType = unreachable!()`, which both lints report against the handler signature"
)]

use quota_contracts::CommandError;
use quota_contracts::preferences::Preferences;
use quota_contracts::refs::AccountRef;
use quota_core::ports::{OperationalPreferencesRepository, PreferenceRepository};
use quota_domain::polling::ProviderPollingPolicy;
use quota_domain::preferences::IndicatorStyle;
use quota_domain::provider::ProviderId;
use tauri::State;

use crate::state::AppState;

/// Saves committed preferences and returns what was actually persisted.
#[tauri::command]
#[specta::specta]
pub async fn update_preferences(
    state: State<'_, AppState>,
    preferences: Preferences,
) -> Result<Preferences, CommandError> {
    persist_preferences(&state, preferences).await
}

/// Saves only the allowance indicator style.
#[tauri::command]
#[specta::specta]
pub async fn set_indicator_style(
    state: State<'_, AppState>,
    style: IndicatorStyle,
) -> Result<Preferences, CommandError> {
    change_preferences(&state, |preferences| preferences.indicator_style = style).await
}

/// Saves a full preference aggregate after locking the preference writers.
pub(crate) async fn persist_preferences(
    state: &AppState,
    preferences: Preferences,
) -> Result<Preferences, CommandError> {
    let _write = state.preferences_write.lock().await;
    persist_preferences_locked(state, preferences).await
}

/// Changes one preference field without overwriting a concurrent field update.
pub(crate) async fn change_preferences<F>(
    state: &AppState,
    change: F,
) -> Result<Preferences, CommandError>
where
    F: FnOnce(&mut Preferences) + Send,
{
    let _write = state.preferences_write.lock().await;
    let mut preferences = state.preferences_state.read().await.clone();
    change(&mut preferences);
    persist_preferences_locked(state, preferences).await
}

async fn persist_preferences_locked(
    state: &AppState,
    preferences: Preferences,
) -> Result<Preferences, CommandError> {
    if preferences.schema_version != crate::bootstrap_helpers::PREFERENCES_SCHEMA_VERSION {
        return Err(CommandError::ValidationFailed {
            field: "schema_version".into(),
            reason: "the renderer sent an unsupported preference schema".into(),
        });
    }
    // Two settings changed before the first confirmation arrived both carry the
    // same revision, so the second whole-object save would silently restore the
    // first one's other fields. A stale aggregate is refused instead.
    let confirmed = state.preferences_state.read().await.clone();
    if preferences.revision < confirmed.revision {
        return Err(CommandError::RevisionConflict {
            expected: preferences.revision,
            actual: confirmed.revision,
        });
    }
    let confirmed = persist_preference_owners(
        state.preferences.as_ref(),
        state.operational_preferences.as_ref(),
        &preferences,
        &confirmed,
    )
    .await?;
    *state.preferences_state.write().await = confirmed.clone();
    *state.policies.write().await = confirmed
        .polling
        .iter()
        .filter(|policy| crate::bootstrap::is_compiled(policy.provider_id))
        .cloned()
        .collect();
    crate::ipc::events::publish_preferences(&state.app, &state.app_instance_id, &confirmed);
    Ok(confirmed)
}

async fn persist_preference_owners(
    presentation_repository: &dyn PreferenceRepository,
    operational_repository: &dyn OperationalPreferencesRepository,
    preferences: &Preferences,
    confirmed: &Preferences,
) -> Result<Preferences, CommandError> {
    let current = presentation_repository.load().await.map_err(|error| {
        CommandError::PersistenceUnavailable {
            owner: error.owner.into(),
        }
    })?;
    let operational = crate::bootstrap_helpers::to_operational(preferences, confirmed.revision);
    let presentation_only =
        operational == crate::bootstrap_helpers::to_operational(confirmed, confirmed.revision);
    let current_operational = if presentation_only {
        operational
    } else {
        operational_repository.load().await.map_err(|error| {
            CommandError::PersistenceUnavailable {
                owner: error.owner.into(),
            }
        })?
    };
    let mut presentation = crate::bootstrap_helpers::to_presentation(preferences);
    presentation.revision = current.revision.max(current_operational.revision);
    let presentation = presentation_repository
        .save(&presentation)
        .await
        .map_err(|error| CommandError::PersistenceUnavailable {
            owner: error.owner.into(),
        })?;
    let operational = if presentation_only {
        current_operational
    } else {
        let operational =
            crate::bootstrap_helpers::to_operational(preferences, presentation.revision);
        operational_repository
            .save(&operational)
            .await
            .map_err(|error| CommandError::PersistenceUnavailable {
                owner: error.owner.into(),
            })?
    };
    Ok(crate::bootstrap_helpers::from_persisted(
        &presentation,
        &operational,
    ))
}

/// Saves one polling policy for a compiled provider.
///
/// # Errors
/// Returns [`CommandError::UnsupportedProvider`] for a provider this build does
/// not compile, and [`CommandError::ValidationFailed`] when the policy names
/// another provider.
#[tauri::command]
#[specta::specta]
pub async fn set_polling_preferences(
    state: State<'_, AppState>,
    provider_id: ProviderId,
    policy: ProviderPollingPolicy,
) -> Result<ProviderPollingPolicy, CommandError> {
    if !crate::bootstrap::is_compiled(provider_id) {
        return Err(CommandError::UnsupportedProvider { provider_id });
    }
    if policy.provider_id != provider_id {
        return Err(CommandError::ValidationFailed {
            field: "provider_id".into(),
            reason: "the policy names another provider".into(),
        });
    }
    change_preferences(&state, move |preferences| {
        preferences
            .polling
            .retain(|existing| existing.provider_id != provider_id);
        preferences.polling.push(policy);
    })
    .await?
    .polling
    .into_iter()
    .find(|saved| saved.provider_id == provider_id)
    .ok_or_else(|| CommandError::Internal {
        code: "polling_policy_not_saved".into(),
    })
}

/// Deletes one account's local reading history.
///
/// It never touches a same-provider sibling, and it never touches an active
/// account binding or a persisted backoff deadline.
#[tauri::command]
#[specta::specta]
pub async fn clear_local_history(
    state: State<'_, AppState>,
    account_ref: AccountRef,
) -> Result<(), CommandError> {
    let known = state.registry.read().await.get(account_ref.id()).is_some();
    if !known {
        return Err(CommandError::AccountNotFound);
    }
    state
        .history
        .clear_history(account_ref.id())
        .await
        .map_err(|error| CommandError::PersistenceUnavailable {
            owner: error.owner.into(),
        })
}

/// Writes a redacted diagnostics export through the host-owned export helper.
///
/// `destination` is a file-name label, not a path. See
/// [`crate::bootstrap_helpers::write_diagnostics`] for containment and privacy.
#[tauri::command]
#[specta::specta]
pub async fn export_sanitized_diagnostics(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    destination: String,
) -> Result<String, CommandError> {
    if destination.trim().is_empty() || destination.len() > 4096 {
        return Err(CommandError::ValidationFailed {
            field: "destination".into(),
            reason: "the export label is empty or too long".into(),
        });
    }
    // The renderer supplies a label, never a path. The host reduces it to a safe
    // stem and writes inside its own application data directory.
    let label = crate::bootstrap_helpers::safe_export_label(&destination);
    crate::bootstrap_helpers::write_diagnostics(&app, &state, &label).await
}

#[cfg(test)]
#[path = "commands_prefs_tests.rs"]
mod tests;
