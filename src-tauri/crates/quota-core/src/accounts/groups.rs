//! Account groups in the registry: which accounts are one provider account,
//! and what each group shows.

use quota_domain::group::{AccountGroup, group_name};
use quota_domain::ids::{AccountGroupId, AccountId};
use quota_domain::provider::ProviderId;

use super::AccountRegistry;
use crate::error::CoreError;

impl AccountRegistry {
    /// Puts `members` in a new group, taking each out of any group it was in.
    ///
    /// A group is one provider account, so every member must come from the
    /// same provider. Returns the members, so the caller can save each.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for no members, a bad name or mixed
    /// providers, and [`CoreError::AccountNotFound`] for an unknown member.
    pub fn create_group(
        &mut self,
        group_id: AccountGroupId,
        name: &str,
        members: &[AccountId],
    ) -> Result<Vec<AccountId>, CoreError> {
        let group = AccountGroup::new(group_id, name).map_err(|_| CoreError::Validation {
            field: "group name",
            reason: "must be a name of 1 to 64 characters",
        })?;
        let Some(first) = members.first() else {
            return Err(CoreError::Validation {
                field: "members",
                reason: "a group needs at least one account",
            });
        };
        let provider = self.provider_of(first)?;
        for member in members {
            if self.provider_of(member)? != provider {
                return Err(mixed_providers());
            }
        }
        for member in members {
            self.entry_mut(member)?.stored.group = Some(group.clone());
        }
        Ok(members.to_vec())
    }

    /// Moves one account into an existing group, or out of every group.
    ///
    /// # Errors
    /// Returns [`CoreError::AccountNotFound`] for an unknown account, and
    /// [`CoreError::Validation`] for an unknown group or one of another provider.
    pub fn set_group(
        &mut self,
        account_id: &AccountId,
        group_id: Option<&AccountGroupId>,
    ) -> Result<(), CoreError> {
        let provider = self.provider_of(account_id)?;
        let group = match group_id {
            None => None,
            Some(group_id) => Some(self.group_to_join(group_id, provider)?),
        };
        self.entry_mut(account_id)?.stored.group = group;
        Ok(())
    }

    /// The group a key of `provider` joins as `group_id`, shown.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an unknown group or one of
    /// another provider.
    pub fn group_to_join(
        &self,
        group_id: &AccountGroupId,
        provider: ProviderId,
    ) -> Result<AccountGroup, CoreError> {
        let member = self
            .accounts
            .values()
            .find(|entry| {
                entry
                    .stored
                    .group
                    .as_ref()
                    .is_some_and(|group| &group.id == group_id)
            })
            .ok_or_else(no_such_group)?;
        if member.stored.connection.provider_id != provider {
            return Err(mixed_providers());
        }
        let group = member.stored.group.clone().ok_or_else(no_such_group)?;
        // A key joins shown, whatever the member it was copied from.
        Ok(AccountGroup {
            key_shown: true,
            ..group
        })
    }

    /// Renames a group. Returns its members, so the caller can save each.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for a bad name or an unknown group.
    pub fn rename_group(
        &mut self,
        group_id: &AccountGroupId,
        name: &str,
    ) -> Result<Vec<AccountId>, CoreError> {
        let name = group_name(name).map_err(|_| CoreError::Validation {
            field: "group name",
            reason: "must be a name of 1 to 64 characters",
        })?;
        let members: Vec<AccountId> = self
            .accounts
            .values_mut()
            .filter_map(|entry| {
                let group = entry.stored.group.as_mut()?;
                (&group.id == group_id).then(|| {
                    group.name.clone_from(&name);
                    entry.stored.account_id.clone()
                })
            })
            .collect();
        if members.is_empty() {
            return Err(CoreError::Validation {
                field: "group",
                reason: "no such group",
            });
        }
        Ok(members)
    }

    /// Shows or hides what a group's keys spent together. Returns its
    /// members, so the caller can save each.
    ///
    /// # Errors
    /// Returns [`CoreError::Validation`] for an unknown group.
    pub fn set_group_spend_shown(
        &mut self,
        group_id: &AccountGroupId,
        shown: bool,
    ) -> Result<Vec<AccountId>, CoreError> {
        let members: Vec<AccountId> = self
            .accounts
            .values_mut()
            .filter_map(|entry| {
                let group = entry.stored.group.as_mut()?;
                (&group.id == group_id).then(|| {
                    group.spend_shown = shown;
                    entry.stored.account_id.clone()
                })
            })
            .collect();
        if members.is_empty() {
            return Err(no_such_group());
        }
        Ok(members)
    }

    /// Shows or hides one key in its group. A hidden key still counts in the
    /// account's total.
    ///
    /// # Errors
    /// Returns [`CoreError::AccountNotFound`] for an unknown account and
    /// [`CoreError::Validation`] for an account in no group.
    pub fn set_group_key_shown(
        &mut self,
        account_id: &AccountId,
        shown: bool,
    ) -> Result<(), CoreError> {
        let group = self
            .entry_mut(account_id)?
            .stored
            .group
            .as_mut()
            .ok_or_else(no_such_group)?;
        group.key_shown = shown;
        Ok(())
    }
}

/// The error for a group no account belongs to.
const fn no_such_group() -> CoreError {
    CoreError::Validation {
        field: "group",
        reason: "no such group",
    }
}

/// A group is one provider account, so its members share a provider.
const fn mixed_providers() -> CoreError {
    CoreError::Validation {
        field: "group",
        reason: "every account in a group must come from the same provider",
    }
}
