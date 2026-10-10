/**
 * The overview surface.
 *
 * The list order is presentation state. A newer backend order is staged and
 * applied at a safe idle point: values, freshness, and warnings update
 * immediately, while row identity, focus, and the click target stay put until
 * the list is idle (spec 4.3, AC-51).
 */
import { useEffect, useRef, useState, type JSX, type RefObject } from "react";

import type {
  AccountGroupId,
  AccountId,
  AccountSnapshot,
  GroupSnapshot,
  IndicatorStyle,
  QuotaWindowId,
} from "../../generated/bindings";
import { applyOrder, placeAccounts, type PlacedAccount } from "../../shared/state/order";
import { applyPendingOrder } from "../../shared/state/store";
import type { RendererState } from "../../shared/state/types";
import { Icon, Logo } from "../../shared/ui/Icon";
import { useNow } from "../../shared/ui/useNow";
import { OverviewToolbar, type OverviewFilter } from "./OverviewToolbar";
import { displayName } from "../../shared/format/alias";
import { groupById, groupLabel } from "../../shared/format/group";
import { GroupCard } from "./GroupCard";
import { ProviderCard } from "./ProviderCard";
import { needsAttention } from "./status";

/** How long the list must be idle before a staged order is applied. */
export const REORDER_IDLE_MS = 1200;

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
  return applyOrder(
    placeAccounts(state.snapshot?.accounts ?? [], state.preferences),
    state.appliedOrder,
  );
}

/** The accounts the current filter leaves on screen, in the order on screen. */
export function overviewRows(
  state: RendererState,
  filter: OverviewFilter,
): readonly PlacedAccount[] {
  return overviewPlacements(state).filter(
    (entry) => filter === "all" || needsAttention(entry.account),
  );
}

/** One entry of the list: an account on its own, or a group and its keys. */
export type OverviewItem =
  | { readonly kind: "account"; readonly entry: PlacedAccount }
  | {
      readonly kind: "group";
      readonly group: GroupSnapshot;
      readonly members: readonly PlacedAccount[];
    };

/**
 * The list, with each group's keys gathered where its first key falls.
 *
 * The rows keep their order: a group takes the place of its first row and
 * lists its keys in row order. A key whose group the snapshot does not list
 * stays a row of its own, so no account is ever dropped.
 */
export function overviewItems(
  rows: readonly PlacedAccount[],
  groups: readonly GroupSnapshot[],
): readonly OverviewItem[] {
  const items: OverviewItem[] = [];
  const placed = new Set<string>();
  for (const entry of rows) {
    const id = entry.account.group?.id;
    const group = id === undefined ? undefined : groupById(groups, id);
    if (group === undefined) {
      items.push({ kind: "account", entry });
      continue;
    }
    if (placed.has(group.id)) {
      continue;
    }
    placed.add(group.id);
    items.push({
      kind: "group",
      group,
      members: rows.filter((row) => row.account.group?.id === group.id),
    });
  }
  return items;
}

/** The overview: every account's card, its limits, and its state. */
export function Overview({
  state,
  filter,
  onFilter,
  onAddAccount,
  onIndicatorStyle,
  onOpenAccount,
  onOpenWindow,
  onReconnect,
  onEnable,
  onResume,
  onAddKey,
}: {
  readonly state: RendererState;
  readonly filter: OverviewFilter;
  readonly onFilter: (filter: OverviewFilter) => void;
  /** Opens the add-account wizard. */
  readonly onAddAccount: () => void;
  readonly onIndicatorStyle: (style: IndicatorStyle) => void;
  readonly onOpenAccount: (accountId: AccountId) => void;
  readonly onOpenWindow: (accountId: AccountId, windowId: QuotaWindowId) => void;
  readonly onResume: () => void;
  readonly onReconnect: (accountId: AccountId) => void;
  readonly onEnable: (accountId: AccountId) => void;
  /** Opens the add-account wizard, adding a key to one group. */
  readonly onAddKey: (groupId: AccountGroupId) => void;
}): JSX.Element {
  // The indicator style is a confirmed preference, so the overview reads it
  // rather than keeping an unsaved local copy that never reaches the host.
  const style: IndicatorStyle = state.preferences?.indicator_style ?? "ring";
  const listRef = useRef<HTMLDivElement | null>(null);
  const now = useNow();
  // Which cards list their other independent limits. Presentation only.
  const [expanded, setExpanded] = useState<ReadonlySet<AccountId>>(new Set());

  const pending = state.pendingOrder !== null;

  useApplyWhenIdle(listRef, applyPendingOrder, pending);

  // Everything below is derived from the snapshot prop and the applied order, so
  // the render is a pure function of its inputs. Reading module state here would
  // be invisible to the React Compiler and could be memoized wrongly.
  const accounts = state.snapshot?.accounts ?? [];
  const placements = overviewPlacements(state);
  const matches = overviewRows(state, filter);
  const attentionCount = placements.filter((entry) =>
    needsAttention(entry.account),
  ).length;
  const paused = state.monitoring?.kind === "paused";
  const groups = state.snapshot?.groups ?? [];
  const card = (account: AccountSnapshot): JSX.Element => (
    <ProviderCard
      key={account.account_id}
      account={account}
      label={displayName(state.preferences, accounts, account)}
      style={style}
      now={now}
      expanded={expanded.has(account.account_id)}
      onExpand={(accountId) => {
        const next = new Set(expanded);
        if (next.has(accountId)) {
          next.delete(accountId);
        } else {
          next.add(accountId);
        }
        setExpanded(next);
      }}
      onOpen={onOpenAccount}
      onOpenWindow={onOpenWindow}
      onReconnect={onReconnect}
      onEnable={onEnable}
    />
  );

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
        attentionCount={attentionCount}
        allCount={placements.length}
        indicatorStyle={style}
        onIndicatorStyle={onIndicatorStyle}
      />
      <div className="overview-label">
        <span>REMAINING ALLOWANCE</span>
        <span>{style === "bar" ? "RESET IN" : "Each window is independent"}</span>
      </div>
      {paused ? (
        <div className="banner neutral">
          <Icon name="pause" />
          <div>
            Monitoring is paused. Values are last known.{" "}
            <button type="button" onClick={onResume}>
              Resume
            </button>
          </div>
        </div>
      ) : null}
      {matches.length === 0 ? (
        <div className="empty compact">
          <div className="empty-art">
            <Icon name="check" />
          </div>
          <h2>No accounts need attention</h2>
          <p>
            {paused
              ? "Monitoring is paused. Resume to check allowance status."
              : "All enabled accounts have current, non-low readings."}
          </p>
          <button
            type="button"
            className="button"
            onClick={() => {
              onFilter("all");
            }}
          >
            Show all accounts
          </button>
        </div>
      ) : (
        <div className="cards">
          {overviewItems(matches, groups).map((item) =>
            item.kind === "account" ? (
              card(item.entry.account)
            ) : (
              <GroupCard
                key={item.group.id}
                group={item.group}
                name={groupLabel(state.preferences, groups, item.group)}
                keys={item.members.map((entry) => ({
                  account: entry.account,
                  label: displayName(state.preferences, accounts, entry.account),
                }))}
                style={style}
                now={now}
                onOpen={onOpenAccount}
                onOpenWindow={onOpenWindow}
                onReconnect={onReconnect}
                onAddKey={onAddKey}
              />
            ),
          )}
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
      <div className="empty-art">
        <Logo />
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
      <button type="button" className="button primary full" onClick={onAddAccount}>
        <Icon name="plus" />
        Add your first account
      </button>
      <small>
        Use the sign-in your AI apps already have, sign in in your browser, or paste an
        API key. Quota never asks for your password.
      </small>
    </div>
  );
}
