/**
 * Account aliases for the privacy setting.
 *
 * With aliases on, a person's real account name is replaced everywhere it would
 * otherwise be shown. Each account gets its own distinguishable label, because a
 * single shared word such as "Hidden account" tells a person with several
 * accounts nothing about which one they are looking at.
 *
 * Aliases are keyed on a sort of the account identities, not on the displayed
 * order, because the displayed order moves every time a reading changes. Keying
 * on it would rename accounts underneath the person mid-session.
 */
import type { AccountSnapshot, Preferences } from "../../generated/bindings";

/** The label shown instead of a real account name. */
export function accountLabel(
  preferences: Preferences | null,
  accounts: readonly AccountSnapshot[],
  accountId: AccountSnapshot["account_id"],
): string {
  if (preferences === null || preferences.privacy.alias_mode !== "stable_aliases") {
    return "";
  }
  const index = [...accounts]
    .map((candidate) => candidate.account_id)
    .sort()
    .indexOf(accountId);
  // An account outside the given set still gets a label of its own.
  return `Account ${index === -1 ? "0" : String(index + 1)}`;
}

/**
 * The label for one account, or its real nickname when aliases are off.
 *
 * The empty string from `accountLabel` means "no alias applies", so a caller
 * never has to know which mode is in force.
 */
export function displayName(
  preferences: Preferences | null,
  accounts: readonly AccountSnapshot[],
  account: AccountSnapshot,
): string {
  return accountLabel(preferences, accounts, account.account_id) || account.nickname;
}
