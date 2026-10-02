import type { JSX } from "react";

import type { AccountSnapshot, Preferences } from "../../generated/bindings";
import { displayName } from "../format/alias";
import { formatCountdown, instantOf } from "../format/duration";

export function RefreshNotice({
  accounts,
  preferences,
  now,
}: {
  readonly accounts: readonly AccountSnapshot[];
  readonly preferences: Preferences | null;
  readonly now: number;
}): JSX.Element {
  const waiting = accounts.flatMap((account) => {
    const next = instantOf(account.next_attempt_at);
    return account.monitoring_enabled &&
      account.connection_state !== "connecting" &&
      account.fetch_state !== "fetching" &&
      next !== null &&
      next > now
      ? [{ account, next }]
      : [];
  });
  return (
    <>
      {waiting.map(({ account, next }) => (
        <p key={account.account_id} className="note" role="status">
          Manual refreshes for {displayName(preferences, accounts, account)} are deferred.
          Next eligible read in{" "}
          {formatCountdown(next, now)}.
        </p>
      ))}
    </>
  );
}
