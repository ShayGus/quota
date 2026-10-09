//! Account groups: several accounts the person treats as one provider account.
//!
//! A provider such as `OpenRouter` gives each API key its own limit and spend,
//! while the money belongs to the account behind the keys. Each key stays its
//! own monitored account, read and backed off on its own, and a group puts the
//! keys of one provider account together. The provider names no account, so
//! the person says which keys belong together; Quota never infers it.
//!
//! A group's total takes the account-wide balance once, from the member that
//! read it most recently, because every key of the account reads the same
//! balance. Only what is per key, the key spend, is added up.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::account::MAX_NICKNAME_LEN;
use crate::balance::{BalanceSummary, PeriodSpend};
use crate::error::DomainError;
use crate::ids::{AccountGroupId, AccountId};
use crate::provider::ProviderId;
use crate::snapshot::AccountSnapshot;

/// The longest group name, in characters, the same as an account name.
pub const MAX_GROUP_NAME_LEN: usize = MAX_NICKNAME_LEN;

/// The group an account belongs to.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct AccountGroup {
    /// The group's immutable identity.
    pub id: AccountGroupId,
    /// The person's name for the provider account.
    pub name: String,
}

impl AccountGroup {
    /// A group with a checked name.
    ///
    /// # Errors
    /// Returns [`DomainError::InvalidIdentifier`] for a blank name and
    /// [`DomainError::TooLong`] beyond [`MAX_GROUP_NAME_LEN`].
    pub fn new(id: AccountGroupId, name: &str) -> Result<Self, DomainError> {
        Ok(Self {
            id,
            name: group_name(name)?,
        })
    }
}

/// A trimmed, checked group name.
///
/// # Errors
/// Returns [`DomainError::InvalidIdentifier`] for a blank name and
/// [`DomainError::TooLong`] beyond [`MAX_GROUP_NAME_LEN`].
pub fn group_name(name: &str) -> Result<String, DomainError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(DomainError::InvalidIdentifier {
            field: "group name",
        });
    }
    if trimmed.chars().count() > MAX_GROUP_NAME_LEN {
        return Err(DomainError::TooLong {
            field: "group name",
            max: MAX_GROUP_NAME_LEN,
        });
    }
    Ok(trimmed.to_owned())
}

/// One group as the renderer sees it: its members and the account total.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
pub struct GroupSnapshot {
    /// The group's immutable identity.
    pub id: AccountGroupId,
    /// The provider every member belongs to.
    pub provider_id: ProviderId,
    /// The person's name for the provider account.
    pub name: String,
    /// The members, in the snapshot's account order.
    pub account_ids: Vec<AccountId>,
    /// The account-wide balance, from the member that read it last.
    pub balance: Option<BalanceSummary>,
    /// What every member key spent together, per period any member reports.
    pub key_spend: Option<PeriodSpend>,
}

/// Builds the group snapshots from `accounts`, given in display order.
///
/// A group appears where its first member appears, and lists its members in
/// the same order, so the groups keep the order the accounts already have.
#[must_use]
pub fn group_snapshots(accounts: &[&AccountSnapshot]) -> Vec<GroupSnapshot> {
    let mut groups: Vec<GroupSnapshot> = Vec::new();
    for account in accounts.iter().copied() {
        let Some(group) = &account.group else {
            continue;
        };
        if let Some(existing) = groups.iter_mut().find(|entry| entry.id == group.id) {
            existing.account_ids.push(account.account_id.clone());
        } else {
            groups.push(GroupSnapshot {
                id: group.id.clone(),
                provider_id: account.provider_id,
                name: group.name.clone(),
                account_ids: vec![account.account_id.clone()],
                balance: None,
                key_spend: None,
            });
        }
    }
    for group in &mut groups {
        let members: Vec<&AccountSnapshot> = accounts
            .iter()
            .copied()
            .filter(|account| group.account_ids.contains(&account.account_id))
            .collect();
        group.balance = latest_balance(&members);
        group.key_spend = total_spend(&members);
    }
    groups
}

/// The balance the most recent successful read saw. Every key of an account
/// reads the same balance, so it is taken once, never added up.
fn latest_balance(members: &[&AccountSnapshot]) -> Option<BalanceSummary> {
    members
        .iter()
        .filter(|account| account.balance.is_some())
        .max_by_key(|account| account.last_success_at)
        .and_then(|account| account.balance.clone())
}

/// What the members spent together, per period. A period no member reports
/// stays unreported rather than reading as nothing spent.
fn total_spend(members: &[&AccountSnapshot]) -> Option<PeriodSpend> {
    let spends: Vec<PeriodSpend> = members
        .iter()
        .filter_map(|account| account.balance.as_ref()?.key_spend)
        .collect();
    if spends.is_empty() {
        return None;
    }
    let sum = |period: fn(&PeriodSpend) -> Option<i64>| {
        spends.iter().filter_map(period).reduce(i64::saturating_add)
    };
    Some(PeriodSpend {
        today_minor: sum(|spend| spend.today_minor),
        week_minor: sum(|spend| spend.week_minor),
        month_minor: sum(|spend| spend.month_minor),
    })
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, TimeZone, Utc};

    use super::*;
    use crate::account::{ConnectionState, FetchState};
    use crate::balance::BaselineKind;
    use crate::ids::ConnectionId;
    use crate::quota::units::CurrencyCode;
    use crate::ranking::{AccountOrder, UnrankedOrder, UnrankedReason};

    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 9, hour, 0, 0).unwrap()
    }

    fn group(id: &str) -> AccountGroup {
        AccountGroup::new(AccountGroupId::new(id).unwrap(), "Work").unwrap()
    }

    fn balance(balance_minor: i64, spend: Option<PeriodSpend>) -> BalanceSummary {
        BalanceSummary {
            currency: CurrencyCode::new("USD").unwrap(),
            scale: 2,
            balance_minor,
            baseline_minor: 5_000,
            baseline_at: at(0),
            baseline_kind: BaselineKind::TopUp,
            loaded_minor: 5_000,
            spent_minor: 5_000 - balance_minor,
            top_ups: Vec::new(),
            runway: None,
            key_spend: spend,
            credits: Vec::new(),
            cycle_spend: None,
        }
    }

    fn spend(today: i64, week: Option<i64>) -> PeriodSpend {
        PeriodSpend {
            today_minor: Some(today),
            week_minor: week,
            month_minor: None,
        }
    }

    fn key(
        id: &str,
        group: Option<AccountGroup>,
        read_at: u32,
        balance: Option<BalanceSummary>,
    ) -> AccountSnapshot {
        AccountSnapshot {
            account_id: AccountId::new(id).unwrap(),
            connection_id: ConnectionId::new(id).unwrap(),
            connection_generation: 1,
            provider_id: ProviderId::Openrouter,
            nickname: id.into(),
            identity: None,
            connection_ordinal: 1,
            monitoring_enabled: true,
            connection_state: ConnectionState::Connected,
            fetch_state: FetchState::Idle,
            last_attempt_at: None,
            last_success_at: Some(at(read_at)),
            next_attempt_at: None,
            windows: Vec::new(),
            expected_but_missing_window_ids: Vec::new(),
            order: AccountOrder::Unranked(UnrankedOrder {
                reason: UnrankedReason::Incomplete,
                rule_version: 1,
            }),
            balance,
            show_key_limit: false,
            group,
        }
    }

    #[test]
    fn a_group_name_is_trimmed_and_never_blank_or_too_long() {
        assert_eq!(group_name("  Work  ").unwrap(), "Work");
        group_name("   ").unwrap_err();
        group_name(&"a".repeat(MAX_GROUP_NAME_LEN + 1)).unwrap_err();
    }

    #[test]
    fn groups_follow_the_account_order_and_skip_ungrouped_accounts() {
        let ci = key("ci", Some(group("g")), 1, None);
        let alone = key("alone", None, 1, None);
        let personal = key("personal", Some(group("g")), 1, None);
        let groups = group_snapshots(&[&ci, &alone, &personal]);
        assert_eq!(groups.len(), 1);
        let ids: Vec<&str> = groups[0]
            .account_ids
            .iter()
            .map(AccountId::as_str)
            .collect();
        assert_eq!(ids, ["ci", "personal"]);
        assert_eq!(groups[0].name, "Work");
        assert_eq!(groups[0].provider_id, ProviderId::Openrouter);
    }

    #[test]
    fn the_balance_is_taken_once_from_the_newest_read_never_added_up() {
        let older = key("a", Some(group("g")), 1, Some(balance(4_000, None)));
        let newer = key("b", Some(group("g")), 3, Some(balance(3_900, None)));
        let groups = group_snapshots(&[&older, &newer]);
        assert_eq!(groups[0].balance.as_ref().unwrap().balance_minor, 3_900);
    }

    #[test]
    fn key_spend_adds_up_per_period_and_an_unreported_period_stays_unreported() {
        let a = key(
            "a",
            Some(group("g")),
            1,
            Some(balance(4_000, Some(spend(120, Some(900))))),
        );
        let b = key(
            "b",
            Some(group("g")),
            2,
            Some(balance(4_000, Some(spend(30, None)))),
        );
        let groups = group_snapshots(&[&a, &b]);
        let total = groups[0].key_spend.unwrap();
        assert_eq!(total.today_minor, Some(150));
        assert_eq!(total.week_minor, Some(900));
        assert_eq!(total.month_minor, None);
    }

    #[test]
    fn a_group_with_no_balance_reports_none_rather_than_zero() {
        let a = key("a", Some(group("g")), 1, None);
        let groups = group_snapshots(&[&a]);
        assert_eq!(groups[0].balance, None);
        assert_eq!(groups[0].key_spend, None);
    }
}
