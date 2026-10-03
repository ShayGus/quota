/**
 * The mini cards: two cards a row, side by side.
 *
 * With an odd number of accounts the last card takes the whole width at the
 * bottom, so the layout stays symmetric. Every card is the same height, with
 * a row for each of its first three limits: a bar in its period's colour for
 * an allowance, the amount itself for money.
 */
import type { JSX } from "react";

import { ProviderMark } from "../../shared/ui/ProviderMark";
import type { WidgetAccount, WidgetRow } from "./model";
import { periodClass } from "./Rings";

/** The rows a card has room for. */
const ROWS_PER_CARD = 3;

/** The mini cards. */
export function MiniCards({
  accounts,
}: {
  readonly accounts: readonly WidgetAccount[];
}): JSX.Element {
  const odd = accounts.length % 2 === 1;
  return (
    <section className="widget-cards" aria-label="Quota">
      {accounts.map((account, index) => (
        <article
          key={account.id}
          className={`widget-card${odd && index === accounts.length - 1 ? " wide" : ""}`}
          aria-label={account.description}
        >
          <span className="widget-card-title">
            <ProviderMark providerId={account.providerId} />
            <span className="widget-card-name">{account.name}</span>
          </span>
          {account.rows.slice(0, ROWS_PER_CARD).map((row) => (
            <CardRow key={`${row.name}:${row.tag}`} row={row} />
          ))}
          {account.rows.length === 0 ? (
            <span className="widget-card-row">
              <span className="widget-tag" />
              <span
                className={`widget-value amount${account.headline.low ? " low" : ""}`}
              >
                {account.headline.value}
              </span>
            </span>
          ) : null}
        </article>
      ))}
    </section>
  );
}

/** One limit: a bar and its share, or the amount. */
function CardRow({ row }: { readonly row: WidgetRow }): JSX.Element {
  return (
    <span className="widget-card-row">
      <span className="widget-tag">{row.tag}</span>
      {row.kind === "share" ? (
        <>
          <span className={`widget-bar ${periodClass(row.period)}`}>
            {row.fraction === null ? null : (
              <span style={{ width: `${String(row.fraction * 100)}%` }} />
            )}
          </span>
          <span className={`widget-value${row.low ? " low" : ""}`}>{row.value}</span>
        </>
      ) : (
        <span className="widget-value amount">{row.value}</span>
      )}
    </span>
  );
}
