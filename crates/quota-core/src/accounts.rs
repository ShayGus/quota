//! The account registry.
//!
//! It owns which accounts exist, what each is bound to, and which generation a
//! read belongs to. It performs no I/O; every call goes through a repository
//! port that the host injects.

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
                    &stored.connection.id,
                    stored.connection.generation,
                    stored.connection.provider_id,
                    stored.connection.principal_id.clone(),
                    stored.connection.workspace_id.clone(),
                    stored.connection.entitlement_id.clone(),
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
        let binding = binding_from_connection(
            &stored.connection.id,
            stored.connection.generation,
            stored.connection.provider_id,
            stored.connection.principal_id.clone(),
            stored.connection.workspace_id.clone(),
            stored.connection.entitlement_id.clone(),
            new_account.profile_label,
        );
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

    fn entry_mut(&mut self, account_id: &AccountId) -> Result<&mut RegisteredAccount, CoreError> {
        self.accounts
            .get_mut(account_id)
            .ok_or_else(|| CoreError::AccountNotFound(account_id.clone()))
    }
}

#[allow(clippy::too_many_arguments)]
fn binding_from_connection(
    connection_id: &quota_domain::ids::ConnectionId,
    generation: u64,
    provider_id: quota_domain::provider::ProviderId,
    principal_id: Option<quota_domain::ids::ProviderPrincipalId>,
    workspace_id: Option<quota_domain::ids::WorkspaceId>,
    entitlement_id: Option<quota_domain::ids::EntitlementId>,
    profile_label: Option<String>,
) -> ConnectionBinding {
    ConnectionBinding {
        connection_id: connection_id.clone(),
        generation,
        provider_id,
        principal_id,
        workspace_id,
        entitlement_id,
        profile_label,
    }
}

fn same_entitlement(left: &StoredAccount, right: &StoredAccount) -> bool {
    left.connection.provider_id == right.connection.provider_id
        && left.connection.principal_id == right.connection.principal_id
        && left.connection.workspace_id == right.connection.workspace_id
        && left.connection.entitlement_id == right.connection.entitlement_id
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
