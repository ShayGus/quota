/**
 * The canonical account order, as the renderer presents it.
 *
 * The rank itself is backend truth: each account arrives with its own
 * `AccountOrder`, computed from the lowest known included remaining percentage.
 * This module only places the accounts in sections and applies the documented
 * tie-break, so nothing here averages, sums, or forecasts (spec 4.1, 4.2).
 */
import type { AccountId, AccountSnapshot } from "../../generated/bindings";

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

/** Places every account into its section, in presentation order. */
export function placeAccounts(
  accounts: readonly AccountSnapshot[],
): readonly PlacedAccount[] {
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
): readonly AccountId[] {
  return placeAccounts(accounts).map((entry) => entry.account.account_id);
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
