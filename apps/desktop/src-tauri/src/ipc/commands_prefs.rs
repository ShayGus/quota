//! Preference, policy, and local-data commands.
//!
//! A command returns the confirmed durable state, never an optimistic success
//! badge. A save that failed returns a typed persistence error.

use quota_contracts::CommandError;
use quota_contracts::preferences::{NotificationPolicy, Preferences, PrivacyPolicy};
use quota_contracts::refs::AccountRef;
use quota_domain::polling::ProviderPollingPolicy;
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
    if preferences.schema_version != crate::bootstrap::PREFERENCES_SCHEMA_VERSION {
        return Err(CommandError::ValidationFailed {
            field: "schema_version".into(),
            reason: "the renderer sent an unsupported preference schema".into(),
        });
    }
    state
        .preferences
        .save(&crate::bootstrap::to_presentation(&preferences))
        .await
        .map(|saved| crate::bootstrap::from_presentation(&saved))
        .map_err(|error| CommandError::PersistenceUnavailable {
            owner: error.owner.into(),
        })
}

/// Replaces the validated polling policy for one provider.
///
/// # Errors
/// Returns [`CommandError::UnsupportedProvider`] for a provider this build does
/// not compile, and [`CommandError::ValidationFailed`] for an empty policy set.
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
    let mut policies = state.policies.write().await;
    policies.retain(|existing| existing.provider_id != provider_id);
    policies.push(policy.clone());
    Ok(policy)
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

/// Writes a redacted diagnostics export to a path the user chose.
///
/// The export carries adapter identifiers, error categories, timings, and
/// sanitized status only. It never carries a token, a cookie, an account
/// address, a profile path, or a raw provider body.
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
            reason: "the export path is empty or too long".into(),
        });
    }
    crate::bootstrap_helpers::write_diagnostics(&app, &state, &destination).await
}

/// The notification and privacy policy the renderer currently displays.
#[tauri::command]
#[specta::specta]
pub async fn notification_and_privacy_policy() -> (NotificationPolicy, PrivacyPolicy) {
    (
        NotificationPolicy {
            enabled: true,
            thresholds: Default::default(),
            quiet_hours: quota_contracts::preferences::QuietHours::Never,
            recovery_enabled: true,
        },
        PrivacyPolicy {
            alias_mode: Default::default(),
            retain_history: true,
            export_identities: false,
        },
    )
}
