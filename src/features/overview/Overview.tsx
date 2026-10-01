/**
 * The overview surface.
 *
 * The list order is presentation state. A newer backend order is staged and
 * applied at a safe idle point: values, freshness, and warnings update
 * immediately, while row identity, focus, and the click target stay put until
 * the list is idle (spec 4.3, AC-51).
 */
import { useEffect, useRef, useState, type JSX } from "react";

import type { AccountId, IndicatorStyle } from "../../generated/bindings";
import { applyOrder, placeAccounts } from "../../shared/state/order";
import { applyPendingOrder } from "../../shared/state/store";
import type { RendererState } from "../../shared/state/types";
import { useNow } from "../../shared/ui/useNow";
import { AccountColumns, AccountRow } from "./AccountRow";
import { OverviewToolbar, type OverviewFilter } from "./OverviewToolbar";
import { needsAttention } from "./status";

/** How long the list must be idle before a staged order is applied. */
export const REORDER_IDLE_MS = 1200;

/** The section heading and its explanatory sub-label. */
const SECTION_HEADING: Record<string, readonly [string, string]> = {
  needs_checking: ["Needs checking", "Not a prediction of exhaustion"],
  ranked: ["Least remaining first", "Lowest remaining allowance first"],
  monitoring_off: ["Monitoring off", "Not being checked"],
};

/** Whether the pointer or the keyboard focus is inside the account list. */
export function listIsEngaged(container: HTMLElement | null): boolean {
  if (container === null) {
    return false;
  }
  if (container.matches(":hover")) {
    return true;
  }
  const active = document.activeElement;
  return active !== null && container.contains(active);
}

/** The overview: every account, its limits, and its state. */
export function Overview({
  state,
  onOpenAccount,
  onReconnect,
}: {
  readonly state: RendererState;
  readonly onOpenAccount: (accountId: AccountId) => void;
  readonly onReconnect: (accountId: AccountId) => void;
}): JSX.Element {
  const [filter, setFilter] = useState<OverviewFilter>("all");
  const [search, setSearch] = useState("");
  const [style, setStyle] = useState<IndicatorStyle>("ring");
  const listRef = useRef<HTMLDivElement | null>(null);
  const now = useNow();

  const pending = state.pendingOrder !== null;

  // Apply a staged order once the list has been idle for the documented delay.
  useEffect(() => {
    if (!pending) {
      return;
    }
    const timer = window.setTimeout(() => {
      if (!listIsEngaged(listRef.current)) {
        applyPendingOrder();
      }
    }, REORDER_IDLE_MS);
    return () => {
      window.clearTimeout(timer);
    };
  }, [pending, state.snapshot]);

  // The placements are derived from the snapshot prop and the applied order, so
  // the render is a pure function of its inputs. Reading module state here would
  // be invisible to the React Compiler and could be memoized wrongly.
  const accounts = state.snapshot?.accounts ?? [];
  const placements = applyOrder(placeAccounts(accounts), state.appliedOrder);
  const query = search.trim().toLowerCase();
  const matches = placements.filter((entry) => {
    const { account } = entry;
    if (filter === "attention" && !needsAttention(account)) {
      return false;
    }
    if (query.length === 0) {
      return true;
    }
    const haystack = [
      account.nickname,
      account.provider_id,
      account.identity?.workspace_label ?? "",
      account.identity?.plan_label ?? "",
    ]
      .join(" ")
      .toLowerCase();
    return haystack.includes(query);
  });
  const attentionCount = placements.filter((entry) =>
    needsAttention(entry.account),
  ).length;

  let renderedSection: string | null = null;
  const rows: JSX.Element[] = [];
  for (const entry of matches) {
    if (entry.section !== renderedSection) {
      renderedSection = entry.section;
      const heading = SECTION_HEADING[entry.section] ?? [entry.section, ""];
      rows.push(
        <p key={`section-${entry.section}`} className="section-separator">
          <strong>{heading[0]}</strong>
          <span>{heading[1]}</span>
        </p>,
      );
    }
    rows.push(
      <AccountRow
        key={entry.account.account_id}
        account={entry.account}
        style={style}
        now={now}
        onOpen={onOpenAccount}
        onReconnect={onReconnect}
      />,
    );
  }

  return (
    <div className="overview" ref={listRef}>
      <OverviewToolbar
        filter={filter}
        onFilter={setFilter}
        search={search}
        onSearch={setSearch}
        attentionCount={attentionCount}
        allCount={placements.length}
        style={style}
        onStyle={setStyle}
        orderUpdatePending={pending}
        onApplyOrder={applyPendingOrder}
      />
      <p className="overview__rule">
        Position is relative depletion of the lowest known included allowance. It is not a
        forecast of when an allowance empties.
      </p>
      {matches.length === 0 ? (
        <div className="list-note">
          <span>No account matches the current filter or search text.</span>
        </div>
      ) : (
        <div className="table">
          <AccountColumns />
          {rows}
        </div>
      )}
      <p className="overview__count" data-testid="visible-count">
        {matches.length} / {placements.length} shown
      </p>
    </div>
  );
}
