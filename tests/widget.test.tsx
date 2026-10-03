/**
 * The mini widget.
 *
 * These tests hold the approved canvas to what the widget draws: one ring per
 * period with the shortest outside, so a lone ring is the outer one; the
 * number under the rings named after its ring; money as money, never a
 * share; mini cards in pairs with an odd last card across the width; and a
 * click that opens the account in the overview without closing the widget.
 */
import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { MiniCards } from "../src/features/widget/MiniCards";
import { widgetAccounts } from "../src/features/widget/model";
import { RingStrip } from "../src/features/widget/RingStrip";
import { Widget } from "../src/features/widget/Widget";
import type { AccountSnapshot, Measurement } from "../src/generated/bindings";
import type { RendererState } from "../src/shared/state/types";
import {
  account,
  NOW,
  percent,
  preferences,
  snapshot,
  window as quotaWindow,
} from "./fixtures";

const ipc = vi.hoisted(() => ({
  fitWidget: vi.fn(() => Promise.resolve()),
  dragWidget: vi.fn(() => Promise.resolve()),
  showInOverview: vi.fn(() => Promise.resolve()),
}));
vi.mock("../src/shared/ipc/widget", () => ipc);

/** A money reading, in cents. */
function money(remaining: number, limit: number | null = null): Measurement {
  return {
    kind: "money",
    value: {
      currency: "USD",
      scale: 2,
      used_minor_units: limit === null ? null : limit - remaining,
      remaining_minor_units: remaining,
      limit_minor_units: limit,
    },
  };
}

const kimi = account(
  "kimi",
  "kimi",
  1,
  [
    quotaWindow("k5", "session", percent(8)),
    quotaWindow("kw", "weekly", percent(52)),
    quotaWindow("km", "monthly", percent(77)),
  ],
  { rank: 8 },
);
const claude = account(
  "claude",
  "claude",
  2,
  [
    quotaWindow("c5", "session", percent(41)),
    quotaWindow("cw", "weekly", percent(66)),
    quotaWindow("cf", "weekly", percent(30), { resource: "fable", label: "Fable" }),
  ],
  { rank: 30 },
);
const cursor = account(
  "cursor",
  "cursor",
  3,
  [
    quotaWindow("ca", "monthly", percent(70), {
      resource: "cursor-models",
      label: "Cursor models",
    }),
    quotaWindow("co", "monthly", percent(48), {
      resource: "other-models",
      label: "Other models",
    }),
    quotaWindow("cx", "monthly", money(1500, 2000), {
      label: "On-demand",
      role: "extra_spend_cap",
    }),
  ],
  { rank: 48 },
);
const openrouter = account(
  "openrouter",
  "openrouter",
  4,
  [
    quotaWindow("oc", "custom", money(1754, 2500), {
      label: "Credits",
      role: "credit_balance",
      boundaryAt: null,
    }),
  ],
  { unrankedReason: "no_included_allowance" },
);

function accountsOf(...accounts: AccountSnapshot[]) {
  return widgetAccounts(accounts, preferences(), NOW);
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe("the widget's model", () => {
  it("draws one ring per period, the shortest outside", () => {
    const [shown] = accountsOf(kimi);
    expect(shown?.rings.map((ring) => ring.period)).toEqual([
      "session",
      "weekly",
      "monthly",
    ]);
  });

  it("draws a lone ring as the outer ring", () => {
    const [shown] = accountsOf(cursor);
    expect(shown?.rings).toEqual([{ period: "monthly", fraction: 0.48 }]);
  });

  it("lets two limits of one period share a ring that shows the tighter", () => {
    const [shown] = accountsOf(claude);
    expect(shown?.rings.map((ring) => [ring.period, ring.fraction])).toEqual([
      ["session", 0.41],
      ["weekly", 0.3],
    ]);
    expect(shown?.rows.map((row) => row.tag)).toEqual(["5h", "Wk", "Fable"]);
  });

  it("names the number after the ring it belongs to, and flags it low", () => {
    const [first, second] = accountsOf(kimi, claude);
    expect(first?.headline).toEqual({ tag: "5h", value: "8%", low: true });
    expect(second?.headline).toEqual({ tag: "Fable", value: "30%", low: false });
  });

  it("shows money as money and never as a share", () => {
    const [shown] = accountsOf(openrouter);
    expect(shown?.rings).toEqual([]);
    expect(shown?.headline.value).toBe("$17.54");
    expect(shown?.rows).toMatchObject([
      { tag: "Credit", kind: "amount", value: "$17.54 left" },
    ]);
    const [withCap] = accountsOf(cursor);
    expect(withCap?.rows[2]).toMatchObject({
      tag: "Extra",
      kind: "amount",
      value: "$15.00 left",
    });
  });

  it("puts an account with only money after the ranked ones", () => {
    expect(accountsOf(openrouter, kimi, claude).map((shown) => shown.id)).toEqual([
      "kimi",
      "claude",
      "openrouter",
    ]);
  });

  it("leaves out accounts whose monitoring is off", () => {
    const off = account("off", "codex", 5, [quotaWindow("x", "session", percent(50))], {
      monitoringEnabled: false,
    });
    expect(accountsOf(kimi, off).map((shown) => shown.id)).toEqual(["kimi"]);
  });

  it("tells two accounts of one provider apart by name", () => {
    const work = account("work", "claude", 6, claude.windows, {
      rank: 30,
      nickname: "Work",
    });
    expect(accountsOf(claude, work).map((shown) => shown.name)).toEqual([
      "Claude · claude",
      "Claude · Work",
    ]);
  });

  it("says what is wrong instead of a number it cannot trust", () => {
    const lapsed = account(
      "lapsed",
      "grok",
      7,
      [quotaWindow("g", "weekly", percent(83))],
      {
        rank: 83,
        connectionState: "reauthentication_required",
      },
    );
    const [shown] = accountsOf(lapsed);
    expect(shown?.headline).toEqual({ tag: "", value: "Reconnect", low: true });
  });
});

describe("the mini cards", () => {
  it("pair the cards, with an odd last card across the width", () => {
    const { container, rerender } = render(
      <MiniCards accounts={accountsOf(kimi, claude, cursor)} onOpen={vi.fn()} />,
    );
    const wide = (): string[] =>
      [...container.querySelectorAll(".widget-card")].map((card) =>
        card.classList.contains("wide") ? "wide" : "half",
      );
    expect(wide()).toEqual(["half", "half", "wide"]);
    rerender(
      <MiniCards
        accounts={accountsOf(kimi, claude, cursor, openrouter)}
        onOpen={vi.fn()}
      />,
    );
    expect(wide()).toEqual(["half", "half", "half", "half"]);
  });

  it("show money in a card as an amount", () => {
    render(<MiniCards accounts={accountsOf(openrouter)} onOpen={vi.fn()} />);
    expect(screen.getByText("$17.54 left")).toBeTruthy();
    expect(screen.queryByText(/%/)).toBeNull();
  });
});

describe("the ring strip", () => {
  it("balances its rows: nine tiles make rows of five and four", () => {
    const nine = Array.from({ length: 9 }, (_, index) =>
      account(
        `a${String(index)}`,
        "codex",
        index,
        [quotaWindow(`w${String(index)}`, "session", percent(50))],
        {
          rank: 50,
        },
      ),
    );
    const { container } = render(
      <RingStrip accounts={accountsOf(...nine)} onOpen={vi.fn()} />,
    );
    const tiles = container.querySelector<HTMLElement>(".widget-tiles");
    expect(tiles?.style.width).toBe("296px");
  });

  it("names every ring colour in its key", () => {
    render(<RingStrip accounts={accountsOf(kimi)} onOpen={vi.fn()} />);
    for (const name of ["5-hour", "Weekly", "Monthly"]) {
      expect(screen.getByText(name)).toBeTruthy();
    }
  });

  it("breaks an account down when it is pointed at", () => {
    render(<RingStrip accounts={accountsOf(kimi, claude)} onOpen={vi.fn()} />);
    fireEvent.pointerEnter(screen.getByRole("button", { name: /^Claude/ }));
    const breakdown = screen.getByRole("tooltip");
    expect(breakdown.textContent).toContain("Fable weekly");
    expect(breakdown.textContent).toContain("30%");
  });
});

describe("the widget window", () => {
  it("opens a clicked account in the overview and keeps its own size fitted", () => {
    globalThis.ResizeObserver = class {
      observe(): void {}
      unobserve(): void {}
      disconnect(): void {}
    };
    const state = {
      snapshot: snapshot("instance", 1, [kimi]),
      preferences: preferences({ indicator_style: "bar" }),
    } as unknown as RendererState;
    render(<Widget state={state} />);
    expect(ipc.fitWidget).toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: /^Kimi/ }));
    expect(ipc.showInOverview).toHaveBeenCalledWith({
      view: "detail",
      accountId: "kimi",
      windowId: null,
    });
  });
});
