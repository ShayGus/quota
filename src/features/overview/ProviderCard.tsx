/**
 * One account's card in the popover.
 *
 * The card shows the identity, a status badge, every standard window at once
 * as a ring or a compact bar, and the account's other independent limits on
 * request. Its identity is the account ID, never its position, so a reorder
 * never moves a click target onto a different account (spec 4.3).
 */
import type { CSSProperties, JSX } from "react";

import type {
  AccountId,
  AccountSnapshot,
  IndicatorStyle,
  QuotaWindow,
  QuotaWindowId,
} from "../../generated/bindings";
import { remainingPercent } from "../../shared/format/allowance";
import { isPrepaidBalance, runwayLine } from "../../shared/format/balance";
import { formatAge, instantOf } from "../../shared/format/duration";
import { providerLabel } from "../../shared/format/provider";
import { Icon } from "../../shared/ui/Icon";
import { Bar, Ring } from "../../shared/ui/Meter";
import { ProviderMark } from "../../shared/ui/ProviderMark";
import {
  ACCOUNT_RESOURCE,
  boundarySentence,
  cardWindows,
  extraLabel,
  ledgerTime,
  readingText,
  resetLine,
  viewCaption,
  viewFraction,
  viewKnown,
  viewSeverity,
  viewValue,
  windowControlName,
  windowLabel,
  windowView,
} from "./reading";
import { lowestWindow, statusOf, type StatusStatement } from "./status";

/** An account's status badge. */
export function StatusBadge({
  status,
}: {
  readonly status: StatusStatement;
}): JSX.Element {
  return (
    <span className={`badge ${status.tone}`}>
      <Icon name={status.icon} />
      {status.text}
    </span>
  );
}

/** The provider tile, the provider's name, and the account's own label under it. */
export function Identity({
  account,
  label,
}: {
  readonly account: AccountSnapshot;
  readonly label: string;
}): JSX.Element {
  return (
    <>
      <ProviderMark providerId={account.provider_id} />
      <span className="provider-copy">
        <span className="provider-name">{providerLabel(account.provider_id)}</span>
        <span className="provider-meta">{label}</span>
      </span>
    </>
  );
}

/** When the account was last checked, as the card's footer states it. */
export function checkedLine(account: AccountSnapshot, now: number): string {
  if (!account.monitoring_enabled) {
    return "Checks disabled";
  }
  if (account.connection_state === "reauthentication_required") {
    return "Connection expired";
  }
  const since = instantOf(account.last_success_at);
  if (since === null) {
    return "Not checked yet";
  }
  // Under a minute the wireframe counts seconds; formatAge would say "under a
  // minute ago", which hides how fresh the reading is.
  const seconds = Math.floor((now - since) / 1000);
  const age =
    seconds >= 0 && seconds < 60
      ? seconds === 0
        ? "just now"
        : `${String(seconds)}s ago`
      : formatAge(since, now);
  return account.order.kind === "unranked" &&
    (account.order.value.reason === "stale" ||
      account.order.value.reason === "monitoring_paused")
    ? `Last checked ${age}`
    : `Checked ${age}`;
}

/** The note a card adds under its readings, when its state needs explaining. */
function cardNote(
  account: AccountSnapshot,
  main: readonly QuotaWindow[],
): { readonly tone: "" | "warn" | "danger"; readonly text: string } | null {
  if (account.order.kind === "unranked") {
    switch (account.order.value.reason) {
      case "stale":
        return {
          tone: "",
          text: "Showing last-known values. The source has not responded.",
        };
      case "reset_pending":
        return {
          tone: "",
          text: "No automatic refill. Waiting for a new provider reading.",
        };
      case "incomplete":
        return {
          tone: "warn",
          text: "Part of this account's allowance was absent from the latest provider response.",
        };
      case "disabled":
      case "monitoring_paused":
      case "native_units_only":
      case "no_included_allowance":
      case "reconnect_required":
      case "unlimited_only":
        return null;
    }
  }
  // An exhausted window ends work regardless of the others' balance, so the
  // card says so rather than leaving a healthy ring beside it to suggest more.
  const lowest = lowestWindow(account);
  if (lowest === null || remainingPercent(lowest.measurement) !== 0) {
    return null;
  }
  const others = main.filter(
    (window) => window !== lowest && (remainingPercent(window.measurement) ?? 0) > 0,
  );
  if (others.length === 0) {
    return null;
  }
  return {
    tone: "danger",
    text: `${windowLabel(lowest)} included quota is exhausted, regardless of the ${others
      .map(windowLabel)
      .join(" and ")} balance.`,
  };
}

/**
 * The label of the button that lists a card's other limits. An included
 * allowance the provider scoped to something narrower than the account is a
 * model limit, as the wireframe calls it; anything else is named generically.
 */
function extraSummary(extra: readonly QuotaWindow[]): string {
  const models = extra.every(
    (window) =>
      window.metric_role === "included_allowance" &&
      window.scope.resource !== ACCOUNT_RESOURCE,
  );
  const noun = models ? "model limit" : "other limit";
  return `${String(extra.length)} ${noun}${extra.length === 1 ? "" : "s"}`;
}

/** One ring and the line under it. */
export function QuotaButton({
  account,
  accountLabel,
  window,
  now,
  onOpenWindow,
}: {
  readonly account: AccountSnapshot;
  /** The account's own label, or its alias, so the name says whose it is. */
  readonly accountLabel: string;
  readonly window: QuotaWindow;
  readonly now: number;
  readonly onOpenWindow: (accountId: AccountId, windowId: QuotaWindowId) => void;
}): JSX.Element {
  const view = windowView(account, window, now);
  const label = windowLabel(window);
  const reset = resetLine(view, window, now, account.balance);
  const runway =
    view === "current" && isPrepaidBalance(window) && account.balance !== null
      ? runwayLine(account.balance)
      : null;
  return (
    <button
      type="button"
      className="quota-button"
      aria-label={windowControlName(
        providerLabel(account.provider_id),
        accountLabel,
        label,
        view,
        window,
        now,
        account.balance,
      )}
      onClick={() => {
        onOpenWindow(account.account_id, window.id);
      }}
    >
      <div className="quota-label">{label}</div>
      <Ring
        fraction={viewFraction(view, window)}
        severity={viewSeverity(view, window)}
        label={viewValue(view, window)}
        caption={viewCaption(view, window)}
      />
      <div className="reset-label">
        {reset.lead}
        {reset.time === null ? null : (
          <>
            {" "}
            <strong>{reset.time}</strong>
          </>
        )}
      </div>
      {runway === null ? null : <div className="reset-label runway-label">{runway}</div>}
    </button>
  );
}

/** One compact row: label, bar, value, and reset time. */
export function LedgerRow({
  account,
  accountLabel,
  window,
  now,
  onOpenWindow,
}: {
  readonly account: AccountSnapshot;
  /** The account's own label, or its alias, so the name says whose it is. */
  readonly accountLabel: string;
  readonly window: QuotaWindow;
  readonly now: number;
  readonly onOpenWindow: (accountId: AccountId, windowId: QuotaWindowId) => void;
}): JSX.Element {
  const view = windowView(account, window, now);
  const label = windowLabel(window);
  const tone =
    view === "stale" ? "stale" : viewKnown(view) ? viewSeverity(view, window) : "";
  return (
    <button
      type="button"
      className={`ledger-row${tone === "" ? "" : ` ${tone}`}`}
      aria-label={windowControlName(
        providerLabel(account.provider_id),
        accountLabel,
        label,
        view,
        window,
        now,
        account.balance,
      )}
      onClick={() => {
        onOpenWindow(account.account_id, window.id);
      }}
    >
      <span className="bar-label">{label}</span>
      <Bar fraction={viewFraction(view, window)} />
      <span className="bar-value">{viewValue(view, window)}</span>
      <span
        className="bar-time"
        title={boundarySentence(view, window, now, account.balance)}
      >
        {ledgerTime(view, window, now, account.balance)}
      </span>
    </button>
  );
}

/** One account's card. */
export function ProviderCard({
  account,
  label,
  style,
  now,
  expanded,
  onExpand,
  onOpen,
  onOpenWindow,
  onReconnect,
  onEnable,
}: {
  readonly account: AccountSnapshot;
  /** The name to show: the account's own, or its alias under the privacy setting. */
  readonly label: string;
  readonly style: IndicatorStyle;
  readonly now: number;
  /** Whether the account's other independent limits are listed. */
  readonly expanded: boolean;
  readonly onExpand: (accountId: AccountId) => void;
  readonly onOpen: (accountId: AccountId) => void;
  readonly onOpenWindow: (accountId: AccountId, windowId: QuotaWindowId) => void;
  readonly onReconnect: (accountId: AccountId) => void;
  readonly onEnable: (accountId: AccountId) => void;
}): JSX.Element {
  const { main, extra } = cardWindows(account);
  const status = statusOf(account, now);
  const note = cardNote(account, main);
  const reconnect =
    account.connection_state === "reauthentication_required" ||
    account.connection_state === "disconnected";
  let body: JSX.Element;
  if (!account.monitoring_enabled) {
    body = (
      <div className="connection-message">
        <p>This account is still saved. Its quota monitoring is turned off.</p>
        <button
          type="button"
          className="button"
          onClick={() => {
            onEnable(account.account_id);
          }}
        >
          Enable
        </button>
      </div>
    );
  } else if (reconnect) {
    body = (
      <div className="connection-message">
        <p>No current reading is available. Reconnect to resume checks.</p>
        <button
          type="button"
          className="button"
          aria-label={`Reconnect ${label}`}
          onClick={() => {
            onReconnect(account.account_id);
          }}
        >
          Reconnect
        </button>
      </div>
    );
  } else if (main.length === 0) {
    body = (
      <div className="card-note">This account reports no allowance windows yet.</div>
    );
  } else if (style === "bar") {
    body = (
      <div className="ledger-rows">
        {main.map((window) => (
          <LedgerRow
            key={window.id}
            account={account}
            accountLabel={label}
            window={window}
            now={now}
            onOpenWindow={onOpenWindow}
          />
        ))}
      </div>
    );
  } else {
    body = (
      <div
        className="quota-grid"
        style={{ "--cols": String(main.length) } as CSSProperties}
      >
        {main.map((window) => (
          <QuotaButton
            key={window.id}
            account={account}
            accountLabel={label}
            window={window}
            now={now}
            onOpenWindow={onOpenWindow}
          />
        ))}
      </div>
    );
  }
  const showNote = account.monitoring_enabled && !reconnect && note !== null;
  return (
    <article
      className="provider-card"
      data-account-id={account.account_id}
      aria-label={`${providerLabel(account.provider_id)} ${label} allowance`}
    >
      <div className="provider-head">
        <button
          type="button"
          className="identity identity-button"
          aria-label={`Details for ${providerLabel(account.provider_id)} ${label}`}
          onClick={() => {
            onOpen(account.account_id);
          }}
        >
          <Identity account={account} label={label} />
        </button>
        <StatusBadge status={status} />
      </div>
      {body}
      {showNote ? (
        <div className={`card-note${note.tone === "" ? "" : ` ${note.tone}`}`}>
          {note.text}
        </div>
      ) : null}
      <div className="card-foot">
        <span>{checkedLine(account, now)}</span>
        {extra.length > 0 ? (
          <button
            type="button"
            aria-expanded={expanded}
            onClick={() => {
              onExpand(account.account_id);
            }}
          >
            {extraSummary(extra)}
            <Icon name={expanded ? "chevron-up" : "chevron-down"} />
          </button>
        ) : (
          <button
            type="button"
            onClick={() => {
              onOpen(account.account_id);
            }}
          >
            Details
            <Icon name="chevron-right" />
          </button>
        )}
      </div>
      {expanded
        ? extra.map((window) => {
            const view = windowView(account, window, now);
            const reset = resetLine(view, window, now);
            return (
              <button
                key={window.id}
                type="button"
                className="extra-limit"
                onClick={() => {
                  onOpenWindow(account.account_id, window.id);
                }}
              >
                <span>{extraLabel(window)}</span>
                <strong>{readingText(view, window)}</strong>
                <span>
                  {view === "current"
                    ? reset.time === null
                      ? reset.lead
                      : `${reset.lead} ${reset.time}`
                    : "Not a current reading"}
                </span>
              </button>
            );
          })
        : null}
    </article>
  );
}
