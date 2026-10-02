/**
 * The overview surface.
 *
 * The list order is presentation state. A newer backend order is staged and
 * applied at a safe idle point: values, freshness, and warnings update
 * immediately, while row identity, focus, and the click target stay put until
 * the list is idle (spec 4.3, AC-51).
 */
import { useEffect, useRef, type JSX, type RefObject } from "react";

import type { AccountId, IndicatorStyle } from "../../generated/bindings";
import {
  applyOrder,
  placeAccounts,
  type PlacedAccount,
  type OverviewSection,
} from "../../shared/state/order";
import { applyPendingOrder } from "../../shared/state/store";
import type { RendererState } from "../../shared/state/types";
import { Icon, Logo } from "../../shared/ui/Icon";
import { useNow } from "../../shared/ui/useNow";
import { AccountColumns, AccountRow } from "./AccountRow";
import { OverviewToolbar, type OverviewFilter } from "./OverviewToolbar";
import { displayName } from "../../shared/format/alias";
import { providerLabel } from "../../shared/format/provider";
import { needsAttention } from "./status";
import { RefreshNotice } from "../../shared/ui/RefreshNotice";

/** How long the list must be idle before a staged order is applied. */
export const REORDER_IDLE_MS = 1200;

/**
 * The heading each section shows, its sub-label, and whether it names a count.
 *
 * The ranked group is the ordinary one, so it is only labelled once something
 * precedes it: a separator there would name a group the reader can already see.
 */
const SECTION_HEADING: Record<
  OverviewSection,
  readonly [heading: string, subLabel: string, countsItself: boolean]
> = {
  needs_checking: ["Needs checking", "Unknown is not zero or full", true],
  ranked: ["Ranked accounts", "Least remaining first", false],
  monitoring_off: ["Monitoring off", "", true],
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

/**
 * Applies a staged order once the list has been idle long enough, or as soon as
 * it becomes idle after that.
 *
 * A single timer that gives up while someone is interacting with the list drops
 * the update on the floor: nothing re-arms it, so the order silently stops
 * matching the readings. Listening for the interaction that ends instead means
 * the update lands as soon as the person lets go, which is the point of staging
 * it. An unmount or a newer snapshot cancels it.
 */
function useApplyWhenIdle(
  container: RefObject<HTMLElement | null>,
  apply: () => void,
  pending: boolean,
): void {
  useEffect(() => {
    if (!pending) {
      return;
    }
    let cancelled = false;
    let timer = 0;
    const attempt = (): void => {
      timer = window.setTimeout(() => {
        if (cancelled) {
          return;
        }
        if (listIsEngaged(container.current)) {
          // Still engaged. Look again after the same delay, so the update lands
          // as soon as the person lets go rather than never.
          attempt();
          return;
        }
        apply();
      }, REORDER_IDLE_MS);
    };
    attempt();
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [pending, container, apply]);
}

/**
 * The accounts in the order currently on screen.
 *
 * The state is handed in rather than read from the store, because the React
 * Compiler treats an argument-free store read as pure and memoises it for the
 * life of the component. The list would then keep showing the accounts of the
 * first render for ever.
 */
export function overviewPlacements(state: RendererState): readonly PlacedAccount[] {
  return applyOrder(placeAccounts(state.snapshot?.accounts ?? []), state.appliedOrder);
}

/**
 * The accounts the current filter and search text leave on screen.
 *
 * The footer measures the rendered rows, so filtering here also determines its
 * denominator without a second copy of the matching rule.
 */
export function overviewRows(
  state: RendererState,
  filter: OverviewFilter,
  search: string,
): readonly PlacedAccount[] {
  const query = search.trim().toLowerCase();
  return overviewPlacements(state).filter((entry) => {
    const { account } = entry;
    if (filter === "attention" && !needsAttention(account)) {
      return false;
    }
    if (query.length === 0) {
      return true;
    }
    const haystack = [
      account.nickname,
      providerLabel(account.provider_id),
      account.identity?.workspace_label ?? "",
      account.identity?.plan_label ?? "",
    ]
      .join(" ")
      .toLowerCase();
    return haystack.includes(query);
  });
}

/** The overview: every account, its limits, and its state. */
export function Overview({
  state,
  filter,
  onFilter,
  search,
  onSearch,
  searchOpen,
  onSearchOpen,
  onFit,
  onAddAccount,
  onIndicatorStyle,
  onOpenAccount,
  onReconnect,
  onResume,
}: {
  readonly state: RendererState;
  readonly filter: OverviewFilter;
  readonly onFilter: (filter: OverviewFilter) => void;
  readonly search: string;
  readonly onSearch: (search: string) => void;
  readonly searchOpen: boolean;
  readonly onSearchOpen: (open: boolean) => void;
  readonly onFit: () => void;
  /** Opens the settings surface, where accounts are added. */
  readonly onAddAccount: () => void;
  readonly onIndicatorStyle: (style: IndicatorStyle) => void;
  readonly onOpenAccount: (accountId: AccountId) => void;
  readonly onResume: () => void;
  readonly onReconnect: (accountId: AccountId) => void;
}): JSX.Element {
  // The indicator style is a confirmed preference, so the overview reads it
  // rather than keeping an unsaved local copy that never reaches the host.
  const style: IndicatorStyle = state.preferences?.indicator_style ?? "ring";
  const listRef = useRef<HTMLDivElement | null>(null);
  const now = useNow();

  const pending = state.pendingOrder !== null;

  useApplyWhenIdle(listRef, applyPendingOrder, pending);

  // Everything below is derived from the snapshot prop and the applied order, so
  // the render is a pure function of its inputs. Reading module state here would
  // be invisible to the React Compiler and could be memoized wrongly.
  const accounts = state.snapshot?.accounts ?? [];
  const placements = overviewPlacements(state);
  const matches = overviewRows(state, filter, search);
  const attentionCount = placements.filter((entry) =>
    needsAttention(entry.account),
  ).length;
  let renderedSection: string | null = null;
  const rows: JSX.Element[] = [];
  for (const [index, entry] of matches.entries()) {
    if (entry.section !== renderedSection) {
      renderedSection = entry.section;
      const [heading, subLabel, countsItself] = SECTION_HEADING[entry.section];
      let runEnd = index + 1;
      while (runEnd < matches.length && matches[runEnd]?.section === entry.section) {
        runEnd += 1;
      }
      if (entry.section !== "ranked" || index > 0) {
        rows.push(
          <p
            key={`section-${entry.section}-${entry.account.account_id}`}
            className={`section-separator${entry.section === "needs_checking" ? " section-separator--needs" : ""}`}
          >
            <strong>
              {heading}
              {countsItself ? ` · ${String(runEnd - index)}` : ""}
            </strong>
            <span>{subLabel}</span>
          </p>,
        );
      }
    }
    rows.push(
      <AccountRow
        key={entry.account.account_id}
        account={entry.account}
        label={displayName(state.preferences, accounts, entry.account)}
        style={style}
        now={now}
        onOpen={onOpenAccount}
        onReconnect={onReconnect}
      />,
    );
  }

  if (placements.length === 0) {
    return (
      <div className="overview" ref={listRef}>
        <FirstLaunch onAddAccount={onAddAccount} />
      </div>
    );
  }

  return (
    <div className="overview" ref={listRef}>
      <OverviewToolbar
        filter={filter}
        onFilter={onFilter}
        searchOpen={searchOpen}
        onSearchOpen={onSearchOpen}
        search={search}
        onSearch={onSearch}
        onClearSearch={() => {
          onSearch("");
        }}
        attentionCount={attentionCount}
        allCount={placements.length}
        orderUpdatePending={pending}
        onApplyOrder={applyPendingOrder}
        onFit={onFit}
        indicatorStyle={style}
        onIndicatorStyle={onIndicatorStyle}
      />
      <RefreshNotice accounts={accounts} preferences={state.preferences} now={now} />
      {state.monitoring?.kind === "paused" ? (
        <div className="list-note">
          <span>Monitoring paused. These are last-known readings, not ranked.</span>
          <button type="button" className="text-button" onClick={onResume}>
            Resume
          </button>
        </div>
      ) : null}
      {matches.length === 0 ? (
        <div className="empty empty--compact">
          <h2>
            {search.length > 0 ? "No matching accounts" : "No accounts need attention"}
          </h2>
          <p>Change the filter to return to all subscriptions.</p>
          <button
            type="button"
            className="button"
            onClick={() => {
              onFilter("all");
              onSearchOpen(false);
              onSearch("");
            }}
          >
            Show all accounts
          </button>
        </div>
      ) : (
        <div className="table">
          <AccountColumns />
          {rows}
        </div>
      )}
    </div>
  );
}

/** The first launch: no account yet, so the window explains what it is for. */
function FirstLaunch({
  onAddAccount,
}: {
  readonly onAddAccount: () => void;
}): JSX.Element {
  return (
    <div className="empty">
      <div className="empty__art">
        <Logo size={42} />
      </div>
      <h2>
        Your subscriptions,
        <br />
        in one small window.
      </h2>
      <p>
        See what is left in each quota window, when it resets, and how recently it was
        checked.
      </p>
      <button
        type="button"
        className="button button--primary button--full"
        onClick={onAddAccount}
      >
        <Icon name="plus" size={15} />
        Add your first account
      </button>
    </div>
  );
}
