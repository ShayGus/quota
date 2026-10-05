/**
 * Quota detail.
 *
 * One window at a time, chosen by tab, with its reading, its exact boundary,
 * its scope, and where it came from. Every other window is listed beneath as an
 * independent limit, so nothing is averaged or hidden (spec 3.2).
 */
import { useRef, useState, type JSX, type KeyboardEvent, type RefObject } from "react";

import type {
  AccountSnapshot,
  Preferences,
  QuotaWindow,
  QuotaWindowId,
} from "../../generated/bindings";
import { formatRemaining } from "../../shared/format/allowance";
import { displayName } from "../../shared/format/alias";
import {
  boundaryCountdown,
  boundaryLead,
  formatExactInstant,
} from "../../shared/format/duration";
import { Icon } from "../../shared/ui/Icon";
import { Ring } from "../../shared/ui/Meter";
import { checkedLine, Identity, StatusBadge } from "../overview/ProviderCard";
import {
  cardWindows,
  extraLabel,
  readingText,
  viewCaption,
  viewFraction,
  viewSeverity,
  viewValue,
  windowLabel,
  windowView,
  type WindowView,
} from "../overview/reading";
import { statusOf } from "../overview/status";
import { isPrepaidBalance } from "../../shared/format/balance";
import { BalanceDetail, balanceMeasurement, balanceSummaryCopy } from "./BalanceDetail";

/** The time zone used for exact boundary times. */
export const DISPLAY_TIME_ZONE = "Asia/Jerusalem";

/** The words for where a reading came from. */
const SOURCE_WORDS: Record<QuotaWindow["source"], string> = {
  documented_api: "Documented API",
  documented_cli_protocol: "Documented command line",
  observed_web_endpoint: "Observed web endpoint",
  local_capture: "Local capture",
  manual: "Entered by hand",
};

/** The words for what an allowance represents in the plan. */
const ROLE_WORDS: Record<QuotaWindow["metric_role"], string> = {
  included_allowance: "Included allowance",
  extra_spend_cap: "Extra-spend cap",
  credit_balance: "Credit balance",
  prepaid_balance: "Prepaid balance, measured from the last top-up",
};

/** The words for what a window's period is. */
const SEMANTICS_WORDS: Record<QuotaWindow["semantics"], string> = {
  anchored_period: "Anchored period",
  rolling_period: "Rolling window",
  calendar_cycle: "Calendar billing cycle",
  unknown: "Period not reported by the provider",
};

/** The words for what happens at a window's boundary. */
const BOUNDARY_WORDS: Record<NonNullable<QuotaWindow["boundary"]>["kind"], string> = {
  full_reset: "Full reset at the boundary",
  next_replenishment: "Partial replenishment",
  billing_boundary: "Billing boundary",
  unknown: "Boundary meaning not reported",
};

/** The eyebrow, headline, and explanation beside the large ring. */
function summaryCopy(
  view: WindowView,
  window: QuotaWindow,
  now: number,
): {
  readonly eyebrow: string;
  readonly headline: string;
  readonly lines: readonly string[];
} {
  switch (view) {
    case "pending":
      return {
        eyebrow: "RESET STATUS",
        headline: "Verifying",
        lines: ["The reset time passed.", "No fresh reading is available."],
      };
    case "stale":
      return {
        eyebrow: "LAST-KNOWN READING",
        headline: "Not current",
        lines: ["The number shown is historical.", "It may have changed."],
      };
    case "unavailable":
      return {
        eyebrow: "QUOTA STATUS",
        headline: "No reading",
        lines: ["This is unknown, not 0%."],
      };
    case "current":
      return window.boundary === null
        ? {
            eyebrow: "RESETS IN",
            headline: "Not reported",
            lines: ["The provider did not report a reset time."],
          }
        : {
            eyebrow: `${boundaryLead(window.boundary).toUpperCase()} IN`,
            headline: boundaryCountdown(window.boundary, now),
            lines: [
              `${formatExactInstant(window.boundary.at, DISPLAY_TIME_ZONE)} · ${DISPLAY_TIME_ZONE}`,
              "Reported by the provider",
            ],
          };
  }
}

/** The measurement row: used and left for a current percentage, else what is known. */
function measurementText(view: WindowView, window: QuotaWindow): string {
  const left = formatRemaining(window.measurement);
  switch (view) {
    case "current":
      return window.measurement.kind === "percentage" &&
        window.measurement.value.used_percent !== null
        ? `${String(Math.round(window.measurement.value.used_percent))}% used / ${left} left`
        : `${left} left`;
    case "stale":
      return `${left} left · last known`;
    case "pending":
      return `${left} left before reset · historical`;
    case "unavailable":
      return "Not available";
  }
}

/** The account's windows in tab order: the card's rings, then its other limits. */
function orderedWindows(account: AccountSnapshot): readonly QuotaWindow[] {
  const { main, extra } = cardWindows(account);
  return [...main, ...extra];
}

/** A window's tab label. */
function tabLabel(account: AccountSnapshot, window: QuotaWindow): string {
  return cardWindows(account).main.includes(window)
    ? windowLabel(window)
    : extraLabel(window);
}

/** The quota detail surface. */
export function AccountDetail({
  account,
  windowId,
  now,
  onBack,
  accounts,
  preferences,
  onUsagePage,
  onManageAccounts,
}: {
  readonly account: AccountSnapshot;
  /** The window to show first, or `null` for the first one. */
  readonly windowId: QuotaWindowId | null;
  readonly now: number;
  readonly onBack: () => void;
  readonly onUsagePage: () => void;
  readonly onManageAccounts: () => void;
  readonly accounts: readonly AccountSnapshot[];
  readonly preferences: Preferences | null;
}): JSX.Element {
  const label = displayName(preferences, accounts, account);
  const windows = orderedWindows(account);
  const [selectedId, setSelectedId] = useState<QuotaWindowId | null>(windowId);
  const tabs = useRef<HTMLDivElement | null>(null);
  const selected = windows.find((window) => window.id === selectedId) ?? windows[0];

  const onTabKey = (event: KeyboardEvent<HTMLButtonElement>): void => {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") {
      return;
    }
    event.preventDefault();
    const index = windows.findIndex((window) => window.id === selected?.id);
    const step = event.key === "ArrowRight" ? 1 : -1;
    const next = windows[(index + step + windows.length) % windows.length];
    if (next === undefined) {
      return;
    }
    setSelectedId(next.id);
    tabs.current
      ?.querySelector<HTMLButtonElement>(`[data-window="${CSS.escape(next.id)}"]`)
      ?.focus();
  };

  return (
    <section aria-label={`Quota detail for ${label}`}>
      <div className="back-row">
        <button type="button" className="back-button" onClick={onBack}>
          <Icon name="arrow-left" />
          All subscriptions
        </button>
        <span className="eyebrow" style={{ fontSize: "8px", letterSpacing: "1px" }}>
          QUOTA DETAIL
        </span>
      </div>
      <div className="detail-body">
        <div className="detail-identity">
          <div className="identity">
            <Identity account={account} label={label} />
          </div>
          <StatusBadge status={statusOf(account, now)} />
        </div>
        {selected === undefined ? (
          <div className="note">This account reports no allowance windows yet.</div>
        ) : (
          <SelectedWindow
            account={account}
            windows={windows}
            selected={selected}
            now={now}
            tabsRef={tabs}
            onSelect={setSelectedId}
            onTabKey={onTabKey}
          />
        )}
        <div className="note">
          Reset times tell you when an allowance changes, not how long you can keep
          working. Limits are never averaged.
        </div>
        <div className="detail-bottom">
          <button type="button" className="text-btn" onClick={onUsagePage}>
            <Icon name="external" />
            Provider usage page
          </button>
          <button type="button" className="text-btn" onClick={onManageAccounts}>
            Manage account
          </button>
        </div>
      </div>
    </section>
  );
}

/** The tabs, the selected window's summary and facts, and the other limits. */
function SelectedWindow({
  account,
  windows,
  selected,
  now,
  tabsRef,
  onSelect,
  onTabKey,
}: {
  readonly account: AccountSnapshot;
  readonly windows: readonly QuotaWindow[];
  readonly selected: QuotaWindow;
  readonly now: number;
  readonly tabsRef: RefObject<HTMLDivElement | null>;
  readonly onSelect: (windowId: QuotaWindowId) => void;
  readonly onTabKey: (event: KeyboardEvent<HTMLButtonElement>) => void;
}): JSX.Element {
  const view = windowView(account, selected, now);
  // A prepaid balance never resets; it is described by what it is measured from.
  const balance =
    isPrepaidBalance(selected) && view === "current" ? account.balance : null;
  const summary =
    balance === null
      ? summaryCopy(view, selected, now)
      : balanceSummaryCopy(selected, balance);
  const others = windows.filter((window) => window.id !== selected.id);
  return (
    <>
      <div className="tabs" role="tablist" aria-label="Quota window" ref={tabsRef}>
        {windows.map((window) => (
          <button
            key={window.id}
            type="button"
            role="tab"
            data-window={window.id}
            aria-selected={window.id === selected.id}
            tabIndex={window.id === selected.id ? 0 : -1}
            className={window.id === selected.id ? "selected" : ""}
            onClick={() => {
              onSelect(window.id);
            }}
            onKeyDown={onTabKey}
          >
            {tabLabel(account, window)}
          </button>
        ))}
      </div>
      <div
        className="detail-summary"
        role="group"
        aria-label={`${tabLabel(account, selected)}: ${readingText(view, selected)}`}
      >
        <Ring
          fraction={viewFraction(view, selected)}
          severity={viewSeverity(view, selected)}
          label={viewValue(view, selected)}
          caption={viewCaption(view, selected)}
        />
        <div className="detail-time">
          <div className="eyebrow">{summary.eyebrow}</div>
          <strong className="mono">{summary.headline}</strong>
          <p>
            {summary.lines.map((line, index) => (
              <span key={line}>
                {index > 0 ? <br /> : null}
                {line}
              </span>
            ))}
          </p>
        </div>
      </div>
      <dl className="detail-list">
        <div>
          <dt>Allowance scope</dt>
          <dd>{selected.scope.label || "Not reported"}</dd>
        </div>
        <div>
          <dt>Measurement</dt>
          <dd>
            {balance === null
              ? measurementText(view, selected)
              : balanceMeasurement(balance)}
          </dd>
        </div>
        <div>
          <dt>Last checked</dt>
          <dd>{checkedLine(account, now)}</dd>
        </div>
        <div>
          <dt>Window semantics</dt>
          <dd>
            {selected.boundary === null
              ? "No reported reset"
              : BOUNDARY_WORDS[selected.boundary.kind]}
            <br />
            <span className="muted">{SEMANTICS_WORDS[selected.semantics]}</span>
          </dd>
        </div>
        <div>
          <dt>Reading source</dt>
          <dd>
            {SOURCE_WORDS[selected.source]}
            <br />
            <span className="muted">{ROLE_WORDS[selected.metric_role]}</span>
          </dd>
        </div>
      </dl>
      {balance === null ? null : <BalanceDetail balance={balance} />}
      {others.length === 0 ? null : (
        <>
          <h3 className="section-title">Other independent limits</h3>
          {others.map((window) => {
            const otherView = windowView(account, window, now);
            const tone = otherView === "current" ? viewSeverity(otherView, window) : "";
            return (
              <button
                key={window.id}
                type="button"
                className={`other-limit${tone === "" ? "" : ` ${tone}`}`}
                onClick={() => {
                  onSelect(window.id);
                }}
              >
                <span>{tabLabel(account, window)}</span>
                <strong>{readingText(otherView, window)}</strong>
              </button>
            );
          })}
        </>
      )}
    </>
  );
}
