/**
 * What a manual refresh will actually do.
 *
 * Ordinary refreshes wait for each account's polling interval and backoff, so a
 * request can be deferred. The wireframe answers a refresh with a short toast,
 * so this is stated there, at the moment the person asks, rather than as a
 * banner that would stand on screen between every scheduled read.
 */
import { useEffect, useState, type JSX } from "react";

import type { AccountSnapshot, Preferences } from "../../generated/bindings";
import { displayName } from "../format/alias";
import { formatCountdown, instantOf } from "../format/duration";

/** The toast text for a refresh requested now. */
export function refreshMessage(
  accounts: readonly AccountSnapshot[],
  preferences: Preferences | null,
  now: number,
): string {
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
  if (waiting.length === 0) {
    return "Refreshing readings.";
  }
  return waiting
    .map(
      ({ account, next }) =>
        `Manual refreshes for ${displayName(preferences, accounts, account)} are deferred. Next eligible read in ${formatCountdown(next, now)}.`,
    )
    .join(" ");
}

/** How long a toast stays on screen, as in the wireframe. */
export const TOAST_MS = 3300;

/** One toast request. A new object shows the text again, even when it repeats. */
export interface ToastMessage {
  readonly text: string;
}

/** The wireframe's toast: one short status line that fades after a moment. */
export function Toast({
  message,
}: {
  readonly message: ToastMessage | null;
}): JSX.Element {
  // The message that has run its time. A new message object is a new toast,
  // so it shows until its own timer expires it.
  const [expired, setExpired] = useState<ToastMessage | null>(null);
  useEffect(() => {
    if (message === null) {
      return;
    }
    const timer = window.setTimeout(() => {
      setExpired(message);
    }, TOAST_MS);
    return () => {
      window.clearTimeout(timer);
    };
  }, [message]);
  const visible = message !== null && expired !== message;
  return (
    <div className={`toast${visible ? " show" : ""}`} role="status" aria-live="polite">
      {message?.text}
    </div>
  );
}
