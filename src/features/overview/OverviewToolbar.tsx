/**
 * The overview toolbar.
 *
 * The filter is local presentation state: it is not backend truth and is not
 * saved (spec 7.8). The layout buttons save the confirmed indicator style.
 */
import type { JSX } from "react";

import type { IndicatorStyle } from "../../generated/bindings";
import { Icon } from "../../shared/ui/Icon";

/** Which accounts the list shows. */
export type OverviewFilter = "all" | "attention";

/** The filter and layout controls. */
export function OverviewToolbar({
  filter,
  onFilter,
  attentionCount,
  allCount,
  indicatorStyle,
  onIndicatorStyle,
}: {
  readonly filter: OverviewFilter;
  readonly onFilter: (filter: OverviewFilter) => void;
  readonly attentionCount: number;
  readonly allCount: number;
  readonly indicatorStyle: IndicatorStyle;
  readonly onIndicatorStyle: (style: IndicatorStyle) => void;
}): JSX.Element {
  return (
    <div className="toolbar">
      <div className="filter-set" role="group" aria-label="Filter accounts">
        <button
          type="button"
          className={filter === "all" ? "selected" : ""}
          aria-pressed={filter === "all"}
          onClick={() => {
            onFilter("all");
          }}
        >
          All accounts<span className="count">{allCount}</span>
        </button>
        <button
          type="button"
          className={filter === "attention" ? "selected" : ""}
          aria-pressed={filter === "attention"}
          onClick={() => {
            onFilter("attention");
          }}
        >
          Attention<span className="count">{attentionCount}</span>
        </button>
      </div>
      <div className="layout-set" role="group" aria-label="Overview layout">
        <button
          type="button"
          className={`icon-btn${indicatorStyle === "ring" ? " active" : ""}`}
          aria-pressed={indicatorStyle === "ring"}
          aria-label="Donut layout"
          title="Donut layout"
          onClick={() => {
            onIndicatorStyle("ring");
          }}
        >
          <Icon name="donut" />
        </button>
        <button
          type="button"
          className={`icon-btn${indicatorStyle === "bar" ? " active" : ""}`}
          aria-pressed={indicatorStyle === "bar"}
          aria-label="Compact layout"
          title="Compact layout"
          onClick={() => {
            onIndicatorStyle("bar");
          }}
        >
          <Icon name="list" />
        </button>
      </div>
    </div>
  );
}
