/**
 * Account groups: the keys of one provider account, shown together.
 *
 * Each key stays its own account, with its own reading and status. A group
 * is one card: the account's total once, the balance every key reads the
 * same and what the keys spent together, then one ring for each key, its own
 * spend limit.
 */
import type {
  AccountGroupId,
  AccountSnapshot,
  GroupSnapshot,
  Preferences,
  ProviderId,
  QuotaWindow,
} from "../../generated/bindings";
import { formatAmount } from "./balance";

/**
 * The providers whose accounts can be grouped: those connected with an API
 * key, where one provider account can hold several keys.
 */
const GROUPABLE: readonly ProviderId[] = ["openrouter", "open_code_go"];

/** Whether accounts of this provider can be put in a group. */
export function isGroupable(provider: ProviderId): boolean {
  return GROUPABLE.includes(provider);
}

/** Whether a window is one key's own spend limit, not the account's. */
function isKeyLimit(window: QuotaWindow): boolean {
  return window.metric_role === "extra_spend_cap";
}

/** One key's own spend limit, or `null` for a key without one. */
export function keyLimitOf(account: AccountSnapshot): QuotaWindow | null {
  return account.windows.find(isKeyLimit) ?? null;
}

/**
 * A key as the account's total: only what belongs to the whole account, its
 * balance and included allowance, without the key's own limit.
 */
export function accountView(account: AccountSnapshot): AccountSnapshot {
  const windows = account.windows.filter((window) => !isKeyLimit(window));
  return windows.length === account.windows.length ? account : { ...account, windows };
}

/**
 * The key a group's total is read from: the one that read the account's
 * balance last, else the one checked last, as the host takes the balance.
 */
export function groupReader<T extends AccountSnapshot>(
  members: readonly T[],
): T | undefined {
  const newest = (candidates: readonly T[]): T | undefined =>
    candidates.reduce<T | undefined>(
      (best, member) =>
        best === undefined ||
        (member.last_success_at ?? "") > (best.last_success_at ?? "")
          ? member
          : best,
      undefined,
    );
  return newest(members.filter((member) => member.balance !== null)) ?? newest(members);
}

/** What one key spent this month, `$9.00`, or `null` when it reports none. */
export function keyMonthSpend(account: AccountSnapshot): string | null {
  const balance = account.balance;
  const month = balance?.key_spend?.month_minor ?? null;
  return balance === null ? null : formatAmount(month, balance.scale, balance.currency);
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
