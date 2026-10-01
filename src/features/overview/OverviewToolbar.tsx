/**
 * The overview toolbar.
 *
 * Search text and the filter are local presentation state: they are not
 * backend truth and are not saved (spec 7.8). The sort statement names the rule
 * the backend used, so a row's position can be explained (spec 4.1).
 */
import { useId, type JSX } from "react";

import { Icon } from "../../shared/ui/Icon";

/** Which accounts the list shows. */
export type OverviewFilter = "all" | "attention";

/** The filter and search controls. */
export function OverviewToolbar({
  filter,
  onFilter,
  search,
  onSearch,
  attentionCount,
  allCount,
  orderUpdatePending,
  onApplyOrder,
}: {
  readonly filter: OverviewFilter;
  readonly onFilter: (filter: OverviewFilter) => void;
  readonly search: string;
  readonly onSearch: (search: string) => void;
  readonly attentionCount: number;
  readonly allCount: number;
  /** Whether a newer order is waiting for a safe idle point. */
  readonly orderUpdatePending: boolean;
  readonly onApplyOrder: () => void;
}): JSX.Element {
  const searchId = useId();
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
              Needs attention
              <span className="segmented__count"> {attentionCount}</span>
            </button>
          </div>
        </div>
        <div className="toolbar__group">
          <span className="toolbar__sort">
            <Icon name="chevron-down" size={13} />
            Least remaining first
          </span>
          {orderUpdatePending ? (
            <button
              type="button"
              className="button button--small"
              onClick={() => {
                onApplyOrder();
              }}
            >
              <Icon name="layers" size={13} />
              Update order
            </button>
          ) : null}
        </div>
      </div>
      <div className="search-line">
        <label className="sr-only" htmlFor={searchId}>
          Search accounts
        </label>
        <Icon name="search" size={15} />
        <input
          id={searchId}
          type="search"
          value={search}
          placeholder="Search accounts"
          onChange={(event) => {
            onSearch(event.currentTarget.value);
          }}
        />
      </div>
    </>
  );
}
