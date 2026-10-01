//! The account registry.
//!
//! It owns which accounts exist, what each is bound to, and which generation a
//! read belongs to. It performs no I/O; every call goes through a repository
//! port that the host injects.

use chrono::{DateTime, Utc};
use std::collections::BTreeMap;

use quota_domain::account::MAX_NICKNAME_LEN;
use quota_domain::ids::{AccountId, QuotaWindowId};
use quota_domain::quota::window::QuotaWindow;

use crate::error::CoreError;
use crate::ports::{AccountRepository, ConnectionBinding, StoredAccount};

/// One registered account and the binding a read must still match.
#[derive(Clone, Debug, PartialEq)]
pub struct RegisteredAccount {
    /// The stored account, including its immutable identity.
    pub stored: StoredAccount,
    /// The binding a read result is checked against.
    pub binding: ConnectionBinding,
}

impl RegisteredAccount {
    /// The immutable local identity.
    #[must_use]
    pub const fn account_id(&self) -> &AccountId {
        &self.stored.account_id
    }
}

/// The set of monitored accounts, keyed by immutable local identity.
#[derive(Clone, Debug, Default)]
pub struct AccountRegistry {
    accounts: BTreeMap<AccountId, RegisteredAccount>,
    next_ordinal: u32,
}

/// One account discovered by an adapter, ready to be registered.
#[derive(Clone, Debug, PartialEq)]
pub struct NewAccount {
    /// The local identity to assign.
    pub account_id: AccountId,
    /// The stored account the adapter discovered, without secret material.
    pub stored: StoredAccount,
    /// The adapter profile, for adapters with isolated profiles.
    pub profile_label: Option<String>,
    /// Whether the account starts monitored.
    pub monitoring_enabled: bool,
}

impl AccountRegistry {
    /// An empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Rebuilds the registry from durable state.
    #[must_use]
    pub fn from_stored(accounts: Vec<StoredAccount>) -> Self {
        let highest = accounts
            .iter()
            .map(|entry| entry.connection_ordinal)
            .max()
            .unwrap_or(0);
        let entries = accounts
            .into_iter()
            .map(|stored| {
                let binding = binding_from_connection(
                    &stored.connection,
                    stored.connection.profile_label.clone(),
                );
                let account_id = stored.account_id.clone();
                (account_id, RegisteredAccount { stored, binding })
            })
            .collect();
        Self {
            accounts: entries,
            next_ordinal: highest.saturating_add(1),
        }
    }

    /// The next stable tie-break ordinal.
    #[must_use]
    pub const fn next_ordinal(&self) -> u32 {
        self.next_ordinal
    }

    /// Borrows one account by immutable identity.
    #[must_use]
    pub fn get(&self, account_id: &AccountId) -> Option<&RegisteredAccount> {
        self.accounts.get(account_id)
    }

    /// Every registered account, in local-identity order.
    pub fn iter(&self) -> impl Iterator<Item = &RegisteredAccount> {
        self.accounts.values()
    }

    /// How many accounts are registered.
    #[must_use]
    pub fn len(&self) -> usize {
        self.accounts.len()
    }

    /// Whether the registry holds no accounts.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.accounts.is_empty()
    }

    /// Registers one account, rejecting a duplicate verified binding.
    ///
    /// A second connection resolving to the same principal, workspace, and
    /// entitlement is a duplicate, not a new independent allowance.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] when the binding is already present.
    pub fn register(&mut self, new_account: NewAccount) -> Result<&RegisteredAccount, CoreError> {
        if self.find_duplicate(&new_account).is_some() {
            return Err(CoreError::Validation {
                field: "binding",
                reason: "this verified entitlement is already connected",
            });
        }
        let mut stored = new_account.stored;
        stored.account_id = new_account.account_id.clone();
        stored.connection_ordinal = self.next_ordinal;
        stored.monitoring_enabled = new_account.monitoring_enabled;
        self.next_ordinal = self.next_ordinal.saturating_add(1);
        let binding = binding_from_connection(&stored.connection, new_account.profile_label);
        let account_id = new_account.account_id;
        let entry = RegisteredAccount { stored, binding };
        Ok(self.accounts.entry(account_id).or_insert(entry))
    }

    /// Finds an existing account bound to the same verified entitlement.
    #[must_use]
    pub fn find_duplicate(&self, new_account: &NewAccount) -> Option<&AccountId> {
        self.accounts
            .values()
            .find(|entry| same_entitlement(&entry.stored, &new_account.stored))
            .map(RegisteredAccount::account_id)
    }

    /// Sets whether one account is monitored.
    ///
    /// # Errors
    /// Returns [`CoreError::AccountNotFound`] for an unknown identity.
    pub fn set_enabled(&mut self, account_id: &AccountId, enabled: bool) -> Result<(), CoreError> {
        self.entry_mut(account_id)?.stored.monitoring_enabled = enabled;
        Ok(())
    }

    /// Replaces a user-chosen name without touching identity, binding, or rank.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for a blank or oversized name.
    pub fn rename(
        &mut self,
        account_id: &AccountId,
        nickname: impl Into<String>,
    ) -> Result<(), CoreError> {
        let trimmed = nickname.into().trim().to_owned();
        if trimmed.is_empty() {
            return Err(CoreError::Validation {
                field: "nickname",
                reason: "must not be blank",
            });
        }
        if trimmed.chars().count() > MAX_NICKNAME_LEN {
            return Err(CoreError::Validation {
                field: "nickname",
                reason: "is too long",
            });
        }
        self.entry_mut(account_id)?.stored.nickname = trimmed;
        Ok(())
    }

    /// Removes one account, leaving its same-provider siblings untouched.
    ///
    /// # Errors
    /// Returns [`CoreError::AccountNotFound`] for an unknown identity.
    pub fn remove(&mut self, account_id: &AccountId) -> Result<RegisteredAccount, CoreError> {
        self.accounts
            .remove(account_id)
            .ok_or_else(|| CoreError::AccountNotFound(account_id.clone()))
    }

    /// Replaces one account's windows after an accepted read.
    ///
    /// # Errors
    /// Returns [`CoreError::AccountNotFound`] for an unknown identity.
    pub fn apply_reading(
        &mut self,
        account_id: &AccountId,
        windows: Vec<QuotaWindow>,
        expected_but_missing: Vec<QuotaWindowId>,
    ) -> Result<(), CoreError> {
        let entry = self.entry_mut(account_id)?;
        entry.stored.windows = windows;
        entry.stored.expected_but_missing_window_ids = expected_but_missing;
        Ok(())
    }

    /// Stores the provider-verified display identity for one account.
    ///
    /// # Errors
    /// Returns [`CoreError::AccountNotFound`] for an unknown identity.
    pub fn set_identity(
        &mut self,
        account_id: &AccountId,
        identity: quota_domain::account::VerifiedIdentity,
    ) -> Result<(), CoreError> {
        self.entry_mut(account_id)?.stored.identity = Some(identity);
        Ok(())
    }

    /// Records the result time and fetch state for one account.
    ///
    /// An accepted reading updates `last_success_at`; an error preserves the
    /// previous success time and records its next eligible instant.
    ///
    /// # Errors
    /// Returns [`CoreError::AccountNotFound`] for an unknown identity.
    pub fn record_attempt(
        &mut self,
        account_id: &AccountId,
        state: quota_domain::account::FetchState,
        attempted_at: DateTime<Utc>,
        next_attempt_at: Option<DateTime<Utc>>,
    ) -> Result<(), CoreError> {
        let entry = self.entry_mut(account_id)?;
        entry.stored.fetch_state = state;
        entry.stored.last_attempt_at = Some(attempted_at);
        entry.stored.next_attempt_at = next_attempt_at;
        if state == quota_domain::account::FetchState::Idle {
            entry.stored.last_success_at = Some(attempted_at);
        }
        Ok(())
    }

    /// Updates the binding generation for every account on one connection.
    ///
    /// # Errors
    /// Returns [`CoreError::ConnectionNotFound`] when the connection has no
    /// account in this registry.
    pub fn set_generation(
        &mut self,
        connection_id: &quota_domain::ids::ConnectionId,
        generation: u32,
    ) -> Result<(), CoreError> {
        let mut found = false;
        for entry in self.accounts.values_mut() {
            if &entry.stored.connection.id == connection_id {
                entry.stored.connection.generation = generation;
                entry.binding.generation = generation;
                found = true;
            }
        }
        if found {
            Ok(())
        } else {
            Err(CoreError::ConnectionNotFound(connection_id.clone()))
        }
    }

    /// Updates the connection state for every account on one connection.
    ///
    /// # Errors
    /// Returns [`CoreError::ConnectionNotFound`] when the connection has no
    /// account in this registry.
    pub fn set_connection_state(
        &mut self,
        connection_id: &quota_domain::ids::ConnectionId,
        state: quota_domain::account::ConnectionState,
    ) -> Result<(), CoreError> {
        let mut found = false;
        for entry in self.accounts.values_mut() {
            if &entry.stored.connection.id == connection_id {
                entry.stored.connection.state = state;
                entry.stored.connection_state = state;
                found = true;
            }
        }
        if found {
            Ok(())
        } else {
            Err(CoreError::ConnectionNotFound(connection_id.clone()))
        }
    }

    fn entry_mut(&mut self, account_id: &AccountId) -> Result<&mut RegisteredAccount, CoreError> {
        self.accounts
            .get_mut(account_id)
            .ok_or_else(|| CoreError::AccountNotFound(account_id.clone()))
    }
}

/// The read binding a stored account implies.
///
/// It is derived from the connection summary, so the binding and the summary
/// cannot disagree about which login, provider or generation they describe.
fn binding_from_connection(
    connection: &quota_domain::account::ConnectionSummary,
    profile_label: Option<String>,
) -> ConnectionBinding {
    ConnectionBinding {
        connection_id: connection.id.clone(),
        generation: connection.generation,
        provider_id: connection.provider_id,
        principal_id: connection.principal_id.clone(),
        workspace_id: connection.workspace_id.clone(),
        entitlement_id: connection.entitlement_id.clone(),
        profile_label,
    }
}

fn same_entitlement(left: &StoredAccount, right: &StoredAccount) -> bool {
    if left.connection.provider_id != right.connection.provider_id {
        return false;
    }
    let left_has_verified_identity = left.connection.principal_id.is_some()
        || left.connection.workspace_id.is_some()
        || left.connection.entitlement_id.is_some();
    let right_has_verified_identity = right.connection.principal_id.is_some()
        || right.connection.workspace_id.is_some()
        || right.connection.entitlement_id.is_some();
    if left_has_verified_identity || right_has_verified_identity {
        return left.connection.principal_id == right.connection.principal_id
            && left.connection.workspace_id == right.connection.workspace_id
            && left.connection.entitlement_id == right.connection.entitlement_id;
    }
    // Sources without identity fields (OpenCode Go) can only be distinguished
    // by their local credential profile label.
    left.connection.profile_label == right.connection.profile_label
}

/// Loads the registry from durable state.
///
/// # Errors
/// Returns [`CoreError::Persistence`] when the durable owner cannot be reached.
pub async fn load_registry(
    repository: &dyn AccountRepository,
) -> Result<AccountRegistry, CoreError> {
    let stored = repository
        .load_accounts()
        .await
        .map_err(|error| CoreError::Persistence { owner: error.owner })?;
    Ok(AccountRegistry::from_stored(stored))
}
