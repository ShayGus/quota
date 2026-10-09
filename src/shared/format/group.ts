/**
 * Account groups: the keys of one provider account, shown together.
 *
 * Each key stays its own account, with its own reading and status. A group
 * shows the account total once, above its keys: the balance every key reads
 * the same, and what the keys spent together. Each key then shows what is its
 * own, its spend limit, rather than repeating the account's balance.
 */
import type {
  AccountGroupId,
  AccountSnapshot,
  GroupSnapshot,
  Preferences,
  ProviderId,
} from "../../generated/bindings";
import { formatAmount, isPrepaidBalance } from "./balance";

/**
 * The providers whose accounts can be grouped: those connected with an API
 * key, where one provider account can hold several keys.
 */
const GROUPABLE: readonly ProviderId[] = ["openrouter", "open_code_go"];

/** Whether accounts of this provider can be put in a group. */
export function isGroupable(provider: ProviderId): boolean {
  return GROUPABLE.includes(provider);
}

/**
 * One key as its group shows it: without the account-wide balance, which the
 * group shows once, and with the key's own spend limit always on, because
 * inside a group the limit is what tells the keys apart.
 */
export function memberView(account: AccountSnapshot): AccountSnapshot {
  const windows = account.windows.filter(
    (window) => !isPrepaidBalance(window) && window.metric_role !== "credit_balance",
  );
  if (windows.length === account.windows.length) {
    return account;
  }
  return { ...account, windows, show_key_limit: true };
}

/** The group with this identity, when the snapshot has it. */
export function groupById(
  groups: readonly GroupSnapshot[],
  id: AccountGroupId,
): GroupSnapshot | undefined {
  return groups.find((group) => group.id === id);
}

/**
 * The name a group is shown with. Under "Hide account names" a group is
 * "Group 1", "Group 2", numbered in a stable order, because the name the
 * person chose can say as much as an account name.
 */
export function groupLabel(
  preferences: Preferences | null,
  groups: readonly GroupSnapshot[],
  group: GroupSnapshot,
): string {
  if (preferences === null || preferences.privacy.alias_mode !== "stable_aliases") {
    return group.name;
  }
  const index = groups
    .map((candidate) => candidate.id)
    .sort()
    .indexOf(group.id);
  return `Group ${index === -1 ? "0" : String(index + 1)}`;
}

/** The account's balance line: "$37.20 left", or `null` without a balance. */
export function groupBalanceLine(group: GroupSnapshot): string | null {
  const balance = group.balance;
  if (balance === null) {
    return null;
  }
  const amount = formatAmount(balance.balance_minor, balance.scale, balance.currency);
  return amount === null ? null : `${amount} left`;
}

/**
 * What the keys spent together: "Keys spent $1.50 today · $20.00 this month".
 * A period no key reports is left out, and `null` when none is reported.
 */
export function groupSpendLine(group: GroupSnapshot): string | null {
  const spend = group.key_spend;
  const balance = group.balance;
  if (spend === null || balance === null) {
    return null;
  }
  const parts = [
    [spend.today_minor, "today"],
    [spend.week_minor, "this week"],
    [spend.month_minor, "this month"],
  ] as const;
  const shown = parts.flatMap(([minor, period]) => {
    const amount = formatAmount(minor, balance.scale, balance.currency);
    return amount === null ? [] : [`${amount} ${period}`];
  });
  return shown.length === 0 ? null : `Keys spent ${shown.join(" · ")}`;
}
