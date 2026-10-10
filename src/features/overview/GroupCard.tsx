/**
 * One provider account with several keys, as one card.
 *
 * The top of the card is the account's total, drawn like any account's
 * reading: the balance every key reads the same, and what the keys spent
 * together. Under it each key has a ring of its own, its spend limit, so a
 * key running low stands out without the account's numbers repeated for it.
 * A key opens its own details, and one that must be reconnected says so on
 * its own ring. The person can leave a key out, or the spend line; a key left
 * out still counts in the total, which is the account's.
 */
import type { CSSProperties, JSX } from "react";

import type {
  AccountGroupId,
  AccountId,
  AccountSnapshot,
  GroupSnapshot,
  IndicatorStyle,
  QuotaWindowId,
} from "../../generated/bindings";
import { formatRemaining, moneyLeft } from "../../shared/format/allowance";
import {
  accountView,
  groupReader,
  groupSpendLine,
  keyLimitOf,
  keyMonthSpend,
} from "../../shared/format/group";
import { providerLabel } from "../../shared/format/provider";
import { Icon } from "../../shared/ui/Icon";
import { Bar, Ring } from "../../shared/ui/Meter";
import { ProviderMark } from "../../shared/ui/ProviderMark";
import { checkedLine, LedgerRow, QuotaButton, StatusBadge } from "./ProviderCard";
import {
  cardWindows,
  readingText,
  viewFraction,
  viewKnown,
  viewSeverity,
  viewValue,
  windowView,
} from "./reading";
import { statusOf, type StatusStatement } from "./status";

/** The most key rings in one row; more keys wrap to the next. */
const KEYS_PER_ROW = 4;

/** One key of the group, with the name it is shown with. */
export interface GroupKey {
  readonly account: AccountSnapshot;
  /** The key's own name, or its alias under the privacy setting. */
  readonly label: string;
}

/** How one key reads in its ring or row. */
interface KeyReading {
  readonly fraction: number | null;
  readonly severity: ReturnType<typeof viewSeverity>;
  /** The ring's centre: `58%`, or `—`. */
  readonly value: string;
  /** The word under the value: none, `no limit`, `off`. */
  readonly caption: string;
  /** The line under the ring: `$11.60`, `$9.00 this month`, a problem. */
  readonly line: string;
  /** Whether the line is a problem, drawn in the warning colour. */
  readonly problem: boolean;
  /** The limit to open, or `null` to open the key's details. */
  readonly windowId: QuotaWindowId | null;
  /** The sentence an assistive technology reads for the key. */
  readonly words: string;
}

/**
 * The statuses that are about the key's connection or checks, not its
 * limit. They replace the money line, because a number from a key that
 * cannot be checked is not what to read first.
 */
const KEY_PROBLEMS: ReadonlySet<string> = new Set([
  "Connecting",
  "Rate limited",
  "Offline",
  "Check failed",
  "Paused",
]);

/**
 * The group's badge: the account's status, unless it reads normally and a
 * key cannot be checked, which the badge then names with the key, so the
 * card is not
 * "Current" while one of its keys has stopped reporting.
 */
function groupStatus(
  total: AccountSnapshot,
  keys: readonly GroupKey[],
  now: number,
): StatusStatement {
  const status = statusOf(total, now);
  if (status.tone !== "good") {
    return status;
  }
  for (const { account, label } of keys) {
    const own = statusOf(account, now);
    if (own.text === "Reconnect" || KEY_PROBLEMS.has(own.text)) {
      return { ...own, text: `${label}: ${own.text.toLowerCase()}` };
    }
  }
  return status;
}

/** One key's reading: its spend limit, or why there is none to show. */
function keyReading(account: AccountSnapshot, now: number): KeyReading {
  const status = statusOf(account, now);
  if (!account.monitoring_enabled) {
    return none("off", "Checks disabled", false, "Checks disabled");
  }
  if (
    account.connection_state === "reauthentication_required" ||
    account.connection_state === "disconnected"
  ) {
    return none("no reading", status.text, true, status.text);
  }
  const limit = keyLimitOf(account);
  const problem = KEY_PROBLEMS.has(status.text) ? status.text : null;
  if (limit === null) {
    const spent = keyMonthSpend(account);
    const line = problem ?? (spent === null ? "Working" : `${spent} this month`);
    return none("no limit", line, problem !== null, `No spend limit, ${line}`);
  }
  const view = windowView(account, limit, now);
  // The percentage alone, by the same rules as every ring ("<1%" is never
  // "0%"); the money it stands for is the line under it.
  const value = viewKnown(view)
    ? (formatRemaining(limit.measurement).split(" · ")[0] ?? "—")
    : viewValue(view, limit);
  const money = viewKnown(view) ? moneyLeft(limit.measurement) : null;
  const line = problem ?? (view === "stale" ? "last known" : (money ?? "no reading"));
  return {
    fraction: viewFraction(view, limit),
    severity: viewSeverity(view, limit),
    value,
    caption: "",
    line,
    problem: problem !== null,
    windowId: limit.id,
    words: `${readingText(view, limit)}${problem === null ? "" : `, ${problem}`}`,
  };
}

/** A key with no limit to draw: an empty ring, and a word for why. */
function none(
  caption: string,
  line: string,
  problem: boolean,
  words: string,
): KeyReading {
  return {
    fraction: null,
    severity: "pending",
    value: "—",
    caption,
    line,
    problem,
    windowId: null,
    words,
  };
}

/** One key's ring, which opens its limit, or its details, or its reconnect. */
function KeyRing({
  item,
  provider,
  now,
  onOpen,
  onOpenWindow,
  onReconnect,
}: {
  readonly item: GroupKey;
  readonly provider: string;
  readonly now: number;
  readonly onOpen: (accountId: AccountId) => void;
  readonly onOpenWindow: (accountId: AccountId, windowId: QuotaWindowId) => void;
  readonly onReconnect: (accountId: AccountId) => void;
}): JSX.Element {
  const { account, label } = item;
  const reading = keyReading(account, now);
  const reconnect = account.connection_state === "reauthentication_required";
  return (
    <button
      type="button"
      className="quota-button key-button"
      aria-label={`${reconnect ? "Reconnect" : "Key"} ${provider} ${label}: ${reading.words}`}
      onClick={() => {
        if (reconnect) {
          onReconnect(account.account_id);
        } else if (reading.windowId === null) {
          onOpen(account.account_id);
        } else {
          onOpenWindow(account.account_id, reading.windowId);
        }
      }}
    >
      <div className="quota-label key-name" title={label}>
        {label}
      </div>
      <Ring
        fraction={reading.fraction}
        severity={reading.severity}
        label={reading.value}
        caption={reading.caption}
      />
      <div className={`reset-label${reading.problem ? " key-problem" : ""}`}>
        {reading.line}
      </div>
    </button>
  );
}

/** One key's row in the compact layout. */
function KeyRow({
  item,
  provider,
  now,
  onOpen,
  onOpenWindow,
  onReconnect,
}: {
  readonly item: GroupKey;
  readonly provider: string;
  readonly now: number;
  readonly onOpen: (accountId: AccountId) => void;
  readonly onOpenWindow: (accountId: AccountId, windowId: QuotaWindowId) => void;
  readonly onReconnect: (accountId: AccountId) => void;
}): JSX.Element {
  const { account, label } = item;
  const reading = keyReading(account, now);
  const reconnect = account.connection_state === "reauthentication_required";
  const tone = reading.problem
    ? "warn"
    : reading.fraction === null
      ? ""
      : reading.severity;
  return (
    <button
      type="button"
      className={`ledger-row${tone === "" ? "" : ` ${tone}`}`}
      aria-label={`${reconnect ? "Reconnect" : "Key"} ${provider} ${label}: ${reading.words}`}
      onClick={() => {
        if (reconnect) {
          onReconnect(account.account_id);
        } else if (reading.windowId === null) {
          onOpen(account.account_id);
        } else {
          onOpenWindow(account.account_id, reading.windowId);
        }
      }}
    >
      <span className="bar-label" title={label}>
        {label}
      </span>
      <Bar fraction={reading.fraction} />
      <span className="bar-value">{reading.value}</span>
      <span className="bar-time">{reading.line}</span>
    </button>
  );
}

/** A group's card: the account's total, then a ring for each key. */
export function GroupCard({
  group,
  name,
  keys,
  style,
  now,
  onOpen,
  onOpenWindow,
  onReconnect,
  onAddKey,
}: {
  readonly group: GroupSnapshot;
  /** The group's name, or an alias under the privacy setting. */
  readonly name: string;
  /** The group's keys, in the overview's order. */
  readonly keys: readonly GroupKey[];
  readonly style: IndicatorStyle;
  readonly now: number;
  readonly onOpen: (accountId: AccountId) => void;
  readonly onOpenWindow: (accountId: AccountId, windowId: QuotaWindowId) => void;
  readonly onReconnect: (accountId: AccountId) => void;
  /** Opens the add-account wizard on this group. */
  readonly onAddKey: (groupId: AccountGroupId) => void;
}): JSX.Element {
  const provider = providerLabel(group.provider_id);
  const reader = groupReader(keys.map((item) => item.account));
  const total = reader === undefined ? undefined : accountView(reader);
  const main = total === undefined ? [] : cardWindows(total).main;
  const spend = group.spend_shown ? groupSpendLine(group) : null;
  const count = group.account_ids.length;
  const shown = keys.filter((item) => item.account.group?.key_shown !== false);
  const hidden = keys.length - shown.length;
  const keyProps = { provider, now, onOpen, onOpenWindow, onReconnect };
  return (
    <article
      className="provider-card group-card"
      data-group-id={group.id}
      aria-label={`${provider} ${name} account`}
    >
      <div className="provider-head">
        <button
          type="button"
          className="identity identity-button"
          aria-label={`Details for ${provider} ${name}`}
          disabled={reader === undefined}
          onClick={() => {
            if (reader !== undefined) {
              onOpen(reader.account_id);
            }
          }}
        >
          <ProviderMark providerId={group.provider_id} />
          <span className="provider-copy">
            <span className="provider-name">{name}</span>
            <span className="provider-meta">
              {provider} · {count} {count === 1 ? "key" : "keys"}
              {hidden === 0 ? null : ` · ${String(hidden)} hidden`}
            </span>
          </span>
        </button>
        {total === undefined ? null : (
          <StatusBadge status={groupStatus(total, keys, now)} />
        )}
      </div>
      {total === undefined || main.length === 0 ? null : style === "bar" ? (
        <div className="ledger-rows">
          {main.map((window) => (
            <LedgerRow
              key={window.id}
              account={total}
              accountLabel={name}
              window={window}
              now={now}
              onOpenWindow={onOpenWindow}
            />
          ))}
        </div>
      ) : (
        <div
          className="quota-grid"
          style={{ "--cols": String(main.length) } as CSSProperties}
        >
          {main.map((window) => (
            <QuotaButton
              key={window.id}
              account={total}
              accountLabel={name}
              window={window}
              now={now}
              onOpenWindow={onOpenWindow}
            />
          ))}
        </div>
      )}
      {spend === null ? null : <div className="group-spend">{spend}</div>}
      {shown.length === 0 ? null : (
        <div className="group-keys-title">
          {shown.length === 1 ? "Key" : "Keys"} · spend limit
        </div>
      )}
      {shown.length === 0 ? null : style === "bar" ? (
        <div className="ledger-rows group-key-rows">
          {shown.map((item) => (
            <KeyRow key={item.account.account_id} item={item} {...keyProps} />
          ))}
        </div>
      ) : (
        <div
          className="quota-grid key-grid"
          style={
            { "--cols": String(Math.min(shown.length, KEYS_PER_ROW)) } as CSSProperties
          }
        >
          {shown.map((item) => (
            <KeyRing key={item.account.account_id} item={item} {...keyProps} />
          ))}
        </div>
      )}
      <div className="card-foot">
        <span>{reader === undefined ? "Not checked yet" : checkedLine(reader, now)}</span>
        <span className="card-foot-actions">
          <button
            type="button"
            aria-label={`Add a key to ${provider} ${name}`}
            onClick={() => {
              onAddKey(group.id);
            }}
          >
            <Icon name="plus" />
            Add key
          </button>
          {reader === undefined ? null : (
            <button
              type="button"
              onClick={() => {
                onOpen(reader.account_id);
              }}
            >
              Details
              <Icon name="chevron-right" />
            </button>
          )}
        </span>
      </div>
    </article>
  );
}
