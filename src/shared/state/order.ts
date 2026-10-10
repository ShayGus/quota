/**
 * The canonical account order, as the renderer presents it.
 *
 * The rank itself is backend truth: each account arrives with its own
 * `AccountOrder`, computed from the lowest known included remaining percentage.
 * This module only places the accounts in sections and applies the documented
 * tie-break, so nothing here averages, sums, or forecasts (spec 4.1, 4.2).
 *
 * The person can choose another order: their own arrangement, or by provider.
 * Either keeps every account where it is while readings change; only an
 * account whose monitoring is off still follows the rest.
 */
import type {
  AccountId,
  AccountSnapshot,
  AccountSort,
  Preferences,
} from "../../generated/bindings";
import { providerLabel } from "../format/provider";

/** The three presentation sections, in the order they appear. */
export type OverviewSection = "needs_checking" | "ranked" | "monitoring_off";

/** The section order on screen: problems first, then the numeric ranking. */
export const SECTION_ORDER: readonly OverviewSection[] = [
  "needs_checking",
  "ranked",
  "monitoring_off",
];

/** The section an account belongs to, from its own recorded order result. */
export function sectionOf(account: AccountSnapshot): OverviewSection {
  if (!account.monitoring_enabled || account.connection_state === "disconnected") {
    return "monitoring_off";
  }
  if (account.order.kind === "ranked") {
    return "ranked";
  }
  return account.order.value.reason === "disabled" ||
    account.order.value.reason === "monitoring_paused"
    ? "monitoring_off"
    : "needs_checking";
}

/** The unrounded order value, or `null` when the account has no comparable rank. */
export function orderValue(account: AccountSnapshot): number | null {
  return account.order.kind === "ranked" ? account.order.value.remaining_percent : null;
}

/**
 * Sorts one section.
 *
 * Ranked accounts go lowest remaining first. Ties break by the stable
 * connection ordinal and then by account identity, so a rename or a value
 * change above or below never reshuffles two equal rows (spec AC-50).
 */
function compare(
  a: AccountSnapshot,
  b: AccountSnapshot,
  section: OverviewSection,
): number {
  if (section === "ranked") {
    const left = orderValue(a);
    const right = orderValue(b);
    if (left !== null && right !== null && left !== right) {
      return left - right;
    }
  }
  if (a.connection_ordinal !== b.connection_ordinal) {
    return a.connection_ordinal - b.connection_ordinal;
  }
  return a.account_id < b.account_id ? -1 : a.account_id > b.account_id ? 1 : 0;
}

/** One account with the section it belongs to. */
export interface PlacedAccount {
  readonly account: AccountSnapshot;
  readonly section: OverviewSection;
}

/** The order the person chose, the least remaining first unless they chose. */
export function sortOf(preferences: Preferences | null): AccountSort {
  return preferences?.account_sort ?? "least_remaining";
}

/** By stable connection ordinal, then identity: the order accounts were added. */
function byOrdinal(a: AccountSnapshot, b: AccountSnapshot): number {
  if (a.connection_ordinal !== b.connection_ordinal) {
    return a.connection_ordinal - b.connection_ordinal;
  }
  return a.account_id < b.account_id ? -1 : a.account_id > b.account_id ? 1 : 0;
}

/**
 * The person's arrangement of `accounts`: the accounts `order` lists, in its
 * order, then the ones it does not, such as a new account, in the order they
 * were added. A group's keys are kept together where its first key falls, so
 * a group always moves as one.
 */
export function arrangedOrder(
  accounts: readonly AccountSnapshot[],
  order: readonly AccountId[],
): readonly AccountId[] {
  const byId = new Map(accounts.map((account) => [account.account_id, account]));
  const listed = order.filter((id, index) => byId.has(id) && order.indexOf(id) === index);
  const rest = accounts
    .filter((account) => !listed.includes(account.account_id))
    .sort(byOrdinal)
    .map((account) => account.account_id);
  const flat = [...listed, ...rest];
  const arranged: AccountId[] = [];
  for (const id of flat) {
    if (arranged.includes(id)) continue;
    const group = byId.get(id)?.group?.id;
    if (group === undefined) {
      arranged.push(id);
      continue;
    }
    arranged.push(...flat.filter((other) => byId.get(other)?.group?.id === group));
  }
  return arranged;
}

/** One step of an arrangement: an account alone, or a group's keys together. */
function arrangedItems(
  accounts: readonly AccountSnapshot[],
  arranged: readonly AccountId[],
): AccountId[][] {
  const groupOf = (id: AccountId): string | undefined =>
    accounts.find((account) => account.account_id === id)?.group?.id;
  const items: AccountId[][] = [];
  for (const id of arranged) {
    const last = items.at(-1);
    const group = groupOf(id);
    if (last !== undefined && group !== undefined && groupOf(last[0] ?? "") === group) {
      last.push(id);
    } else {
      items.push([id]);
    }
  }
  return items;
}

/**
 * The arrangement after moving one account a step up (-1) or down (1), or
 * `null` when it cannot move that way. A key moves among its group's keys;
 * at the group's edge, or for an account on its own, the whole step moves
 * past its neighbour, so a group always moves as one.
 */
export function moveInOrder(
  accounts: readonly AccountSnapshot[],
  order: readonly AccountId[],
  accountId: AccountId,
  direction: -1 | 1,
): readonly AccountId[] | null {
  const items = arrangedItems(accounts, arrangedOrder(accounts, order));
  const index = items.findIndex((item) => item.includes(accountId));
  const item = items[index];
  if (item === undefined) {
    return null;
  }
  const within = item.indexOf(accountId) + direction;
  if (item.length > 1 && within >= 0 && within < item.length) {
    const next = [...item];
    [next[within - direction], next[within]] = [
      next[within] ?? "",
      next[within - direction] ?? "",
    ];
    items[index] = next;
    return items.flat();
  }
  const target = index + direction;
  const neighbour = items[target];
  if (neighbour === undefined) {
    return null;
  }
  items[target] = item;
  items[index] = neighbour;
  return items.flat();
}

/**
 * Places every account into its section, in presentation order.
 *
 * By the least remaining (the default), problems come first, then the
 * ranking. In the person's arrangement or by provider, every account keeps
 * its place whatever it reads, and only those with monitoring off follow.
 */
export function placeAccounts(
  accounts: readonly AccountSnapshot[],
  preferences: Preferences | null = null,
): readonly PlacedAccount[] {
  const sort = sortOf(preferences);
  if (sort !== "least_remaining") {
    const position =
      sort === "manual"
        ? new Map(
            arrangedOrder(accounts, preferences?.account_order ?? []).map((id, index) => [
              id,
              index,
            ]),
          )
        : null;
    const compareChosen = (a: AccountSnapshot, b: AccountSnapshot): number => {
      if (position !== null) {
        return (position.get(a.account_id) ?? 0) - (position.get(b.account_id) ?? 0);
      }
      return (
        providerLabel(a.provider_id).localeCompare(providerLabel(b.provider_id)) ||
        byOrdinal(a, b)
      );
    };
    const off = (account: AccountSnapshot): boolean =>
      sectionOf(account) === "monitoring_off";
    return [
      ...accounts.filter((account) => !off(account)).sort(compareChosen),
      ...accounts.filter(off).sort(compareChosen),
    ].map((account) => ({ account, section: sectionOf(account) }));
  }
  const placed: PlacedAccount[] = [];
  for (const section of SECTION_ORDER) {
    const members = accounts
      .filter((account) => sectionOf(account) === section)
      .sort((a, b) => compare(a, b, section));
    for (const account of members) {
      placed.push({ account, section });
    }
  }
  return placed;
}

/** The account identities in canonical presentation order. */
export function canonicalOrder(
  accounts: readonly AccountSnapshot[],
  preferences: Preferences | null = null,
): readonly AccountId[] {
  return placeAccounts(accounts, preferences).map((entry) => entry.account.account_id);
}

/**
 * Reorders a placed list to match the currently displayed order.
 *
 * An account the applied order does not know about keeps its canonical place
 * rather than disappearing from the table.
 */
export function applyOrder(
  placed: readonly PlacedAccount[],
  order: readonly AccountId[],
): readonly PlacedAccount[] {
  if (order.length === 0) {
    return placed;
  }
  const position = new Map<AccountId, number>();
  order.forEach((id, index) => position.set(id, index));
  const fallback = order.length;
  return [...placed].sort((a, b) => {
    const left =
      position.get(a.account.account_id) ?? fallback + a.account.connection_ordinal;
    const right =
      position.get(b.account.account_id) ?? fallback + b.account.connection_ordinal;
    return left - right;
  });
}
