/**
 * A prepaid balance, measured from its last top-up.
 *
 * The card shows the money left at the centre of a ring that drains from the
 * last top-up, what it is measured from, and how long it lasts; the key's own
 * spend limit only when the person chose to show it. The detail adds the share,
 * the spending of each period and the top-ups.
 */
import { render, screen, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { AccountDetail } from "../src/features/accounts/AccountDetail";
import { ProviderCard } from "../src/features/overview/ProviderCard";
import { cardWindows } from "../src/features/overview/reading";
import { statusOf } from "../src/features/overview/status";
import { widgetAccounts } from "../src/features/widget/model";
import type { AccountSnapshot } from "../src/generated/bindings";
import {
  baselineLine,
  formatAmount,
  isWindowShown,
  runwayLine,
  runwayShort,
} from "../src/shared/format/balance";
import { account, keyLimit, NOW, preferences, prepaidBalance } from "./fixtures";

function openRouter(
  options: Parameters<typeof prepaidBalance>[0] & {
    readonly showKeyLimit?: boolean;
  } = {},
): AccountSnapshot {
  const { window, summary } = prepaidBalance(options);
  const share =
    summary.baseline_minor === null
      ? null
      : 100 * ((summary.balance_minor ?? 0) / summary.baseline_minor);
  return account("or", "openrouter", 1, [keyLimit(), window], {
    nickname: "Side project",
    rank: share,
    balance: summary,
    showKeyLimit: options.showKeyLimit ?? false,
  });
}

function card(shown: AccountSnapshot, style: "ring" | "bar" = "ring") {
  return render(
    <ProviderCard
      account={shown}
      label="Side project"
      style={style}
      now={NOW}
      expanded={false}
      onExpand={vi.fn()}
      onOpen={vi.fn()}
      onOpenWindow={vi.fn()}
      onReconnect={vi.fn()}
      onEnable={vi.fn()}
    />,
  );
}

describe("the words for a prepaid balance", () => {
  it("formats amounts from their minor units", () => {
    expect(formatAmount(3720, 2, "USD")).toBe("$37.20");
    expect(formatAmount(-150, 2, "USD")).toBe("-$1.50");
    expect(formatAmount(1999, 2, "EUR")).toBe("19.99 EUR");
    expect(formatAmount(null, 2, "USD")).toBeNull();
  });

  it("says what the gauge is measured from", () => {
    expect(baselineLine(prepaidBalance().summary)).toBe("of $50.00 loaded on Sep 28");
    expect(baselineLine(prepaidBalance({ baselineKind: "since_added" }).summary)).toBe(
      "of $50.00 since you added it",
    );
    expect(baselineLine(prepaidBalance({ baselineKind: "adjusted" }).summary)).toBe(
      "of $50.00 since Sep 28",
    );
    // A top-up onto a balance that was not empty says both amounts.
    const onTop = prepaidBalance({
      baseline: 5600,
      topUps: [
        {
          detected_at: "2026-09-28T12:00:00.000Z",
          amount_minor: 5000,
          balance_after_minor: 5600,
        },
      ],
    });
    expect(baselineLine(onTop.summary)).toBe("of $56.00 after $50.00 loaded on Sep 28");
  });

  it("says how long the balance lasts, and nothing without a pace", () => {
    expect(runwayLine(prepaidBalance().summary)).toBe("≈ 12 days at $3.10/day");
    expect(
      runwayLine(
        prepaidBalance({ runway: { spend_per_day_minor: 500, days_left: 0 } }).summary,
      ),
    ).toBe("under a day at $5.00/day");
    expect(runwayLine(prepaidBalance({ runway: null }).summary)).toBeNull();
    expect(runwayShort(prepaidBalance().summary)).toBe("≈ 12 d");
    expect(runwayShort(null)).toBeNull();
  });
});

describe("a prepaid balance on its card", () => {
  it("shows the money left, what it is measured from, and the pace", () => {
    card(openRouter());
    const rings = document.querySelectorAll(".quota-button");
    expect(rings).toHaveLength(1);
    const ring = rings[0] as HTMLElement;
    expect(within(ring).getByText("$37.20")).toBeTruthy();
    expect(within(ring).getByText("of $50.00 loaded on Sep 28")).toBeTruthy();
    expect(within(ring).getByText("≈ 12 days at $3.10/day")).toBeTruthy();
  });

  it("hides the key's spend limit until the person shows it", () => {
    const hidden = openRouter();
    expect(cardWindows(hidden).main.map((window) => window.id)).toEqual(["or-balance"]);
    expect(cardWindows(hidden).extra).toEqual([]);
    expect(isWindowShown(hidden, keyLimit())).toBe(false);
    const shown = openRouter({ showKeyLimit: true });
    expect(cardWindows(shown).main.map((window) => window.id)).toEqual([
      "or-balance",
      "or-key",
    ]);
  });

  it("always shows the key limit of a key that cannot read the balance", () => {
    const keyOnly = account("or", "openrouter", 1, [keyLimit()], { showKeyLimit: false });
    expect(cardWindows(keyOnly).main.map((window) => window.id)).toEqual(["or-key"]);
  });

  it("puts the pace in a compact row's time column", () => {
    card(openRouter(), "bar");
    expect(screen.getByText("≈ 12 d")).toBeTruthy();
    expect(screen.getByText("$37.20")).toBeTruthy();
  });

  it("is low at 20% of the last top-up, like an allowance", () => {
    expect(statusOf(openRouter(), NOW).text).toBe("Current");
    const low = openRouter({ balance: 900 });
    expect(statusOf(low, NOW)).toMatchObject({
      text: "Credit balance low",
      tone: "warn",
    });
  });
});

describe("a prepaid balance in the widget", () => {
  it("draws a ring of the share and shows the money left", () => {
    const [shown] = widgetAccounts([openRouter()], preferences(), NOW);
    expect(shown?.rings).toHaveLength(1);
    expect(shown?.rings[0]?.fraction).toBeCloseTo(0.744);
    expect(shown?.headline.value).toBe("$37.20");
    expect(shown?.rows.map((row) => row.tag)).toEqual(["Credit"]);
  });
});

describe("a prepaid balance in quota detail", () => {
  it("shows the share, the spending of each period and the top-ups", () => {
    const shown = openRouter();
    render(
      <AccountDetail
        account={shown}
        windowId={null}
        now={NOW}
        onBack={vi.fn()}
        onUsagePage={vi.fn()}
        onManageAccounts={vi.fn()}
        accounts={[shown]}
        preferences={preferences()}
      />,
    );
    expect(screen.getByText("LEFT OF LAST TOP-UP")).toBeTruthy();
    expect(screen.getByText("74% left")).toBeTruthy();
    expect(screen.getByText("$12.80 spent / $37.20 left of $50.00")).toBeTruthy();
    expect(screen.getByText(/\$0\.42 today/)).toBeTruthy();
    const topUps = screen.getByRole("list", { name: "Top-ups" });
    expect(within(topUps).getAllByRole("listitem")).toHaveLength(2);
    expect(within(topUps).getByText("$25.00")).toBeTruthy();
  });

  it("says when no top-up has been seen yet", () => {
    const shown = openRouter({ baselineKind: "since_added", topUps: [] });
    render(
      <AccountDetail
        account={shown}
        windowId={null}
        now={NOW}
        onBack={vi.fn()}
        onUsagePage={vi.fn()}
        onManageAccounts={vi.fn()}
        accounts={[shown]}
        preferences={preferences()}
      />,
    );
    expect(screen.getByText(/No top-up seen yet/)).toBeTruthy();
  });
});
