import type { JSX } from "react";

import type { AccountSnapshot } from "../../generated/bindings";
import { formatCountdown, instantOf } from "../format/duration";

export function RefreshNotice({
  accounts,
  now,
}: {
  readonly accounts: readonly AccountSnapshot[];
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
          Manual refreshes for {account.nickname} are deferred. Next eligible read in{" "}
          {formatCountdown(next, now)}.
        </p>
      ))}
    </>
  );
}
