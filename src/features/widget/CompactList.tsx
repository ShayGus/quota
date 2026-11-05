/**
 * The compact list: one row per account, its name and then each limit.
 *
 * A row is its provider's icon and name, then each limit: its letter or model
 * name, its number, and a thin gauge under
 * it when there is a share to draw. The limits stand in columns of one width,
 * so every row's first limit starts at the same point, as wide as the widest
 * limit needs. The window is as wide as that and no wider, and stays that wide
 * when details open.
 *
 * A still click opens the row's details under it, in full words, with its
 * resets, when it was last checked, and the way to the full window. A press
 * that moves 4 px drags the window instead. Near the screen's bottom edge the
 * details open upward, and the rows stay where they are on the screen. They
 * close on the row, Esc, another application taking focus, or their account
 * leaving the widget.
 */
import {
  Fragment,
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type JSX,
} from "react";

import type { AccountId } from "../../generated/bindings";
import { launch } from "../../shared/ipc/report";
import type { FitWidget } from "../../shared/ipc/widget";
import { Icon } from "../../shared/ui/Icon";
import { ProviderMark } from "../../shared/ui/ProviderMark";
import type { WidgetAccount, WidgetRow } from "./model";
import { periodClass } from "./Rings";
import { reconnectLineOf, type ReconnectAsk, type ReconnectLine } from "./RingStrip";
import { useWidgetChrome } from "./Widget";

/** How long a lost focus waits for its return before the details close. */
const BLUR_GRACE_MS = 150;

/** Roughly how tall details are, to pick the side with room before they open. */
const DETAILS_BASE = 44;
const DETAILS_LIMIT = 34;
const DETAILS_RECONNECT = 34;
const ROOM_GAP = 8;

/** The most limits on one line; an account with more goes on to another. */
const MAX_COLUMNS = 4;

/** The page's class while the rows must hold their place at the window's bottom. */
const GROWS_UP = "widget-grows-up";

/** The compact list. */
export function CompactList({
  accounts,
  onReconnect,
  onExpand,
}: {
  readonly accounts: readonly WidgetAccount[];
  /**
   * Reconnects one account, as the full window does. Answers the words for
   * why the host refused, or `null` once it took the reconnect.
   */
  readonly onReconnect: (accountId: AccountId) => Promise<string | null>;
  /** Switches to the full window. */
  readonly onExpand: () => void;
}): JSX.Element {
  const chrome = useWidgetChrome();
  const { growth, requestFit, setOnFitted } = chrome;
  const [opened, setOpened] = useState<AccountId | null>(null);
  const [up, setUp] = useState(false);
  const [cap, setCap] = useState<number | null>(null);
  const [ask, setAsk] = useState<ReconnectAsk | null>(null);
  const section = useRef<HTMLElement | null>(null);
  const rows = useRef(new Map<AccountId, HTMLButtonElement>());
  // The account leaving the widget closes its details, with nothing to wait for.
  const open = accounts.some((account) => account.id === opened) ? opened : null;
  const columns = Math.min(
    MAX_COLUMNS,
    Math.max(1, ...accounts.map((account) => account.rows.length)),
  );

  const close = useCallback((): void => {
    setOpened(null);
    setCap(null);
    setAsk(null);
  }, []);

  /** Opens one row, on the side with room for it. */
  const show = (account: WidgetAccount): void => {
    if (chrome.press.current !== null) {
      return;
    }
    setAsk((current) => (current?.id === account.id ? current : null));
    setCap(null);
    if (open !== null) {
      setOpened(account.id);
      return;
    }
    const height = section.current?.offsetHeight ?? 0;
    launch(
      requestFit(height, "down").then((answer) => {
        const wanted =
          DETAILS_BASE +
          account.rows.length * DETAILS_LIMIT +
          (account.reconnect === null ? 0 : DETAILS_RECONNECT);
        const upward =
          answer !== null &&
          roomBelow(answer) < wanted + ROOM_GAP &&
          (answer.room_above ?? 0) > roomBelow(answer);
        growth.current = upward ? "up" : "down";
        setUp(upward);
        setOpened(account.id);
      }),
    );
  };

  // The host has the last word: it may cap the height, or take the other side.
  useEffect(() => {
    setOnFitted((answer, asked) => {
      if (answer.height !== null && answer.height + 0.5 < asked) {
        setCap(answer.height);
      }
      if (answer.direction !== growth.current) {
        growth.current = answer.direction;
        setUp(answer.direction === "up");
      }
      if (open === null) {
        growth.current = "down";
        setUp(false);
      }
    });
    return () => {
      setOnFitted(null);
    };
  }, [growth, open, setOnFitted]);

  useLayoutEffect(() => {
    document.documentElement.classList.toggle(GROWS_UP, up);
    return () => {
      document.documentElement.classList.remove(GROWS_UP);
    };
  }, [up]);

  useEffect(() => {
    const onBlur = (): void => {
      window.setTimeout(() => {
        if (chrome.press.current === null && !document.hasFocus()) {
          close();
        }
      }, BLUR_GRACE_MS);
    };
    const onVisibility = (): void => {
      if (document.visibilityState === "hidden") {
        close();
      }
    };
    const onKey = (event: KeyboardEvent): void => {
      if (event.key === "Escape" && open !== null) {
        close();
        rows.current.get(open)?.focus();
      }
    };
    window.addEventListener("blur", onBlur);
    document.addEventListener("visibilitychange", onVisibility);
    document.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("blur", onBlur);
      document.removeEventListener("visibilitychange", onVisibility);
      document.removeEventListener("keydown", onKey);
    };
  }, [chrome, close, open]);

  return (
    <section
      className="widget-list"
      aria-label="Quota"
      ref={section}
      style={{
        gridTemplateColumns: `minmax(0, max-content) repeat(${String(columns)}, minmax(max-content, 1fr)) 0`,
        ...(cap === null ? {} : { maxHeight: cap, overflowY: "auto" }),
      }}
    >
      {accounts.map((account, index) => (
        <Fragment key={account.id}>
          <button
            type="button"
            ref={(row) => {
              if (row === null) {
                rows.current.delete(account.id);
              } else {
                rows.current.set(account.id, row);
              }
            }}
            className={`widget-row${account.id === open ? " open" : ""}${account.lastKnown ? " last-known" : ""}`}
            aria-label={
              account.reconnect === null
                ? account.description
                : `${account.name} needs reconnecting, open to reconnect`
            }
            aria-expanded={account.id === open}
            onClick={(event) => {
              // The second click of a double-click is ignored, so a double-click
              // never flashes the details open and closed.
              if (event.detail > 1) {
                return;
              }
              if (account.id === open) {
                close();
              } else {
                show(account);
              }
            }}
            onKeyDown={(event) => {
              const next = nextRow(event.key, index, accounts.length);
              if (next === null) {
                return;
              }
              event.preventDefault();
              const target = accounts[next];
              if (target !== undefined) {
                rows.current.get(target.id)?.focus();
              }
            }}
          >
            <span className="widget-row-name">
              <ProviderMark providerId={account.providerId} />
              <span className="widget-row-label">{account.name}</span>
            </span>
            <span className="widget-row-limits">
              {account.numbers ? (
                account.rows.map((row) => (
                  <Limit key={`${row.name}:${row.tag}`} row={row} />
                ))
              ) : (
                <span className="widget-row-status">
                  {account.reconnect === null ? null : <Icon name="link" />}
                  <span className={`widget-value${account.headline.low ? " low" : ""}`}>
                    {account.headline.value}
                  </span>
                </span>
              )}
            </span>
          </button>
          {account.id === open ? (
            <Details
              account={account}
              reconnect={reconnectLineOf(account, ask)}
              onExpand={onExpand}
              onReconnect={() => {
                const target = account.reconnect;
                const { id } = account;
                if (target === null || (ask?.id === id && ask.sending)) {
                  return;
                }
                setAsk({ id, sending: true, failure: null });
                launch(
                  onReconnect(target).then((failure) => {
                    setAsk((current) =>
                      current?.id === id ? { id, sending: false, failure } : current,
                    );
                  }),
                );
              }}
            />
          ) : null}
        </Fragment>
      ))}
    </section>
  );
}

/** The free room under the window; none is known as plenty. */
function roomBelow(answer: FitWidget): number {
  return answer.room_below ?? Number.POSITIVE_INFINITY;
}

/** The row an arrow key or Home or End moves to, or `null` for any other key. */
function nextRow(key: string, index: number, count: number): number | null {
  switch (key) {
    case "ArrowDown":
      return Math.min(count - 1, index + 1);
    case "ArrowUp":
      return Math.max(0, index - 1);
    case "Home":
      return 0;
    case "End":
      return count - 1;
    default:
      return null;
  }
}

/** The colour a limit goes by: its period's, or its kind's. */
function toneClass(row: WidgetRow): string {
  return row.tone === "period" ? periodClass(row.period) : `tone-${row.tone}`;
}

/** One limit in a row: its label, its number, and a gauge when it has a share. */
function Limit({ row }: { readonly row: WidgetRow }): JSX.Element {
  return (
    <span className={`widget-limit ${toneClass(row)}${row.low ? " low" : ""}`}>
      <span className="widget-limit-tag">{row.tag}</span>
      <span className="widget-value">{row.brief}</span>
      {row.kind === "share" && row.fraction !== null ? (
        <span className="widget-limit-gauge">
          <span style={{ width: `${String(row.fraction * 100)}%` }} />
        </span>
      ) : null}
    </span>
  );
}

/** One account in full words, under its row. */
function Details({
  account,
  reconnect,
  onReconnect,
  onExpand,
}: {
  readonly account: WidgetAccount;
  /** The reconnect line, or `null` when the account needs no reconnect. */
  readonly reconnect: ReconnectLine | null;
  readonly onReconnect: () => void;
  readonly onExpand: () => void;
}): JSX.Element {
  return (
    <div className="widget-details" role="region" aria-label={account.name}>
      {account.chip === null ? null : (
        <span className={`widget-chip${account.chip.tone === "warn" ? " warn" : ""}`}>
          <Icon name={account.chip.tone === "warn" ? "warning" : "clock"} />
          {account.chip.text}
        </span>
      )}
      {reconnect === null ? null : (
        <div className="widget-details-reconnect">
          <span
            className={`widget-drawer-why${reconnect.failure === null ? "" : " failed"}`}
            role={reconnect.failure === null ? undefined : "alert"}
          >
            {reconnect.failure ?? "Reconnect to resume checks."}
          </span>
          <button
            type="button"
            className="button primary widget-drawer-action"
            aria-label={`${reconnect.busy ? "Reconnecting" : "Reconnect"} ${account.name}`}
            aria-disabled={reconnect.busy}
            aria-busy={reconnect.busy}
            onClick={() => {
              // A busy button keeps its focus but does nothing, so the keyboard
              // is never thrown out of the details.
              if (!reconnect.busy) {
                onReconnect();
              }
            }}
          >
            {reconnect.busy ? "Reconnecting…" : "Reconnect"}
          </button>
        </div>
      )}
      {account.rows.map((row) => (
        <div className={`widget-detail ${toneClass(row)}`} key={`${row.name}:${row.tag}`}>
          <span className="widget-detail-name">
            <span className="widget-detail-dot" />
            {row.name}
          </span>
          <span
            className={`widget-detail-value${row.low ? " low" : ""}${row.stale ? " stale" : ""}`}
          >
            {row.detail}
          </span>
          {row.note === "" ? null : (
            <span className="widget-detail-note">{row.note}</span>
          )}
        </div>
      ))}
      <div className="widget-details-foot">
        <span>{account.checked}</span>
        <button type="button" className="widget-details-full" onClick={onExpand}>
          Open full window
        </button>
      </div>
    </div>
  );
}
