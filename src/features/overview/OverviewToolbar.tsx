/**
 * The overview toolbar.
 *
 * Search text and the filter are local presentation state: they are not
 * backend truth and are not saved (spec 7.8). The sort statement names the rule
 * the backend used, so a row's position can be explained (spec 4.1).
 */
import { useId, useState, type JSX } from "react";

import type { IndicatorStyle } from "../../generated/bindings";
import { Icon } from "../../shared/ui/Icon";

/** Which accounts the list shows. */
export type OverviewFilter = "all" | "attention";

/** The filter, search, indicator, and window controls. */
export function OverviewToolbar({
  filter,
  onFilter,
  searchOpen,
  onSearchOpen,
  search,
  onSearch,
  onClearSearch,
  attentionCount,
  allCount,
  orderUpdatePending,
  onApplyOrder,
  onFit,
  indicatorStyle,
  onIndicatorStyle,
}: {
  readonly filter: OverviewFilter;
  readonly onFilter: (filter: OverviewFilter) => void;
  /** Whether the search line is open. It stays open until it is dismissed. */
  readonly searchOpen: boolean;
  readonly onSearchOpen: (open: boolean) => void;
  readonly search: string;
  readonly onSearch: (search: string) => void;
  readonly onClearSearch: () => void;
  readonly attentionCount: number;
  readonly allCount: number;
  /** Whether a newer order is waiting for a safe idle point. */
  readonly orderUpdatePending: boolean;
  readonly onApplyOrder: () => void;
  readonly onFit: () => void;
  readonly indicatorStyle: IndicatorStyle;
  readonly onIndicatorStyle: (style: IndicatorStyle) => void;
}): JSX.Element {
  const searchId = useId();
  const [helpOpen, setHelpOpen] = useState(false);
  return (
    <>
      <div className="toolbar">
        <div className="toolbar__group">
          <div className="segmented" role="group" aria-label="Filter accounts">
            <button
              type="button"
              aria-pressed={filter === "all"}
              onClick={() => {
                onFilter("all");
              }}
            >
              All accounts
              <span className="segmented__count"> {allCount}</span>
            </button>
            <button
              type="button"
              aria-pressed={filter === "attention"}
              onClick={() => {
                onFilter("attention");
              }}
            >
              Attention
              <span className="segmented__count"> {attentionCount}</span>
            </button>
          </div>
          <button
            type="button"
            className="toolbar__sort"
            title={
              orderUpdatePending
                ? "Readings changed; row order is held while you interact. Apply the new order now."
                : "How ordering works"
            }
            onClick={() => {
              if (orderUpdatePending) {
                onApplyOrder();
              } else {
                setHelpOpen(!helpOpen);
              }
            }}
          >
            <Icon name={orderUpdatePending ? "refresh" : "list"} size={13} />
            {orderUpdatePending ? "Update order" : "Least remaining first"}
          </button>
        </div>
        <div className="toolbar__group">
          <button
            type="button"
            className="icon-button"
            aria-pressed={searchOpen}
            aria-label="Find an account"
            onClick={() => {
              onSearchOpen(!searchOpen);
            }}
          >
            <Icon name="search" size={16} />
          </button>
          <button
            type="button"
            className="button button--small"
            title="Widen the window to show all accounts and limits"
            onClick={onFit}
          >
            Fit {allCount}
          </button>
          <div className="layout-set" role="group" aria-label="Allowance indicators">
            <button
              type="button"
              className="icon-button"
              aria-pressed={indicatorStyle === "ring"}
              aria-label="Ring indicators"
              title="Ring indicators"
              onClick={() => {
                onIndicatorStyle("ring");
              }}
            >
              <Icon name="donut" size={15} />
            </button>
            <button
              type="button"
              className="icon-button"
              aria-pressed={indicatorStyle === "bar"}
              aria-label="Bar indicators"
              title="Bar indicators"
              onClick={() => {
                onIndicatorStyle("bar");
              }}
            >
              <Icon name="list" size={15} />
            </button>
          </div>
        </div>
      </div>
      {helpOpen ? (
        <div className="note" role="status">
          Accounts are ranked by their lowest current remaining allowance. Unknown and
          stale readings need checking; monitoring-off accounts are separate. Row order
          stays put while you interact.{" "}
          <button
            type="button"
            className="text-button"
            onClick={() => {
              setHelpOpen(false);
            }}
          >
            Close ordering help
          </button>
        </div>
      ) : null}
      {searchOpen ? (
        <div className="search-line">
          <label className="sr-only" htmlFor={searchId}>
            Search accounts
          </label>
          <Icon name="search" size={15} />
          <input
            id={searchId}
            type="search"
            value={search}
            placeholder="Find provider, nickname, workspace…"
            onChange={(event) => {
              onSearch(event.currentTarget.value);
            }}
          />
          <button type="button" className="text-button" onClick={onClearSearch}>
            Clear
          </button>
        </div>
      ) : null}
    </>
  );
}
