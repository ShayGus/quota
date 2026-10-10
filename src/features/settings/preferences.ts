/**
 * Preference patches.
 *
 * A preferences change is sent as a whole confirmed object with the revision the
 * renderer believed was current. These helpers produce the next object; they do
 * not save it, and no screen shows a saved state before the backend confirms
 * (spec 13.2).
 */
import type {
  AccountId,
  AccountSnapshot,
  AccountSort,
  IndicatorStyle,
  Preferences,
  PrivacyAliasMode,
  Theme,
} from "../../generated/bindings";
import { canonicalOrder } from "../../shared/state/order";

/** The next preferences with a different colour scheme. */
export function withTheme(preferences: Preferences, theme: Theme): Preferences {
  return { ...preferences, theme };
}

/** The next preferences with a different allowance indicator. */
export function withIndicatorStyle(
  preferences: Preferences,
  indicator_style: IndicatorStyle,
): Preferences {
  return { ...preferences, indicator_style };
}

/** The next preferences with animations suppressed or restored. */
export function withReduceMotion(
  preferences: Preferences,
  reduce_motion: boolean,
): Preferences {
  return { ...preferences, reduce_motion };
}

/** The next preferences with a different identity-alias mode. */
export function withAliasMode(
  preferences: Preferences,
  alias_mode: PrivacyAliasMode,
): Preferences {
  return { ...preferences, privacy: { ...preferences.privacy, alias_mode } };
}

/** The next preferences with local history retained or dropped. */
export function withRetainHistory(
  preferences: Preferences,
  retain_history: boolean,
): Preferences {
  return { ...preferences, privacy: { ...preferences.privacy, retain_history } };
}

/** The next preferences with account labels included in exports or not. */
export function withExportIdentities(
  preferences: Preferences,
  export_identities: boolean,
): Preferences {
  return { ...preferences, privacy: { ...preferences.privacy, export_identities } };
}

/**
 * The next preferences with accounts listed in `account_sort`. The first time
 * the person chooses their own order, it starts from the order on screen, so
 * nothing jumps.
 */
export function withAccountSort(
  preferences: Preferences,
  account_sort: AccountSort,
  accounts: readonly AccountSnapshot[],
): Preferences {
  const seeded =
    account_sort === "manual" && preferences.account_order.length === 0
      ? canonicalOrder(accounts, preferences)
      : preferences.account_order;
  return { ...preferences, account_sort, account_order: [...seeded] };
}

/** The next preferences with the accounts arranged in `order`. */
export function withAccountOrder(
  preferences: Preferences,
  order: readonly AccountId[],
): Preferences {
  return { ...preferences, account_order: [...order] };
}
