/**
 * Overview behaviour.
 *
 * These tests exercise what a person sees and does: the cells in a row, the
 * section an account lands in, the order of the rows, and what happens to a row
 * while a newer order is waiting (spec 4.1-4.3, spec 17).
 */
import { render, screen, within } from "@testing-library/react";
import { act } from "react";
import { describe, expect, it } from "vitest";

import { Overview, REORDER_IDLE_MS } from "../src/features/overview/Overview";
import type { RendererState } from "../src/shared/state/types";
import { applyPendingOrder, getRendererState } from "../src/shared/state/store";
import { acceptSnapshot } from "../src/shared/state/store";
import { useRendererState } from "../src/shared/state/useRendererState";
import {
  account,
  percent,
  snapshot,
  unavailable,
  window as quotaWindow,
} from "./fixtures";

/** Renders the overview against the live store, as the application does. */
function Harness(): React.ReactElement {
  const state: RendererState = useRendererState();
  return (
    <Overview state={state} onOpenAccount={() => undefined} onReconnect={() => undefined} />
  );
}

/** The account identity of every row, in the order the DOM shows them. */
function rowOrder(): readonly (string | null)[] {
  const rows = screen.queryAllByRole("article");
  return rows.map((row) => row.getAttribute("data-account-id"));
}

/** The rendered session, weekly, and monthly cells of one row. */
function cellsOf(accountId: string): readonly HTMLElement[] {
  const row = document.querySelector(`[data-account-id="${accountId}"]`);
  if (row === null) {
    return [];
  }
  return [...row.querySelectorAll(".quota-cell")].map((cell) => cell as HTMLElement);
}

describe("the account row", () => {
  it("shows the session, weekly, and monthly allowance at the same time", () => {
    acceptSnapshot(
      snapshot("instance-1", 1, [
        account("a1", "codex", 1, [
          quotaWindow("session-window", "session", percent(72)),
          quotaWindow("weekly-window", "weekly", percent(41)),
          quotaWindow("monthly-window", "monthly", percent(0)),
        ]),
      ]),
    );
    render(<Harness />);

    const cells = cellsOf("a1");
    expect(cells).toHaveLength(3);
    expect(cells[0]).toHaveTextContent("72%");
    expect(cells[1]).toHaveTextContent("41%");
    expect(cells[2]).toHaveTextContent("0%");
    expect(within(cells[2] as HTMLElement).getAllByText("0%").length).toBeGreaterThan(0);
  });

  it("says an offered column is absent rather than drawing a ring for it", () => {
    acceptSnapshot(
      snapshot("instance-1", 1, [
        account("a1", "codex", 1, [quotaWindow("session-window", "session", percent(72))]),
      ]),
    );
    render(<Harness />);

    const cells = cellsOf("a1");
    expect(cells[1]).toHaveTextContent("Not offered");
    expect(within(cells[1] as HTMLElement).queryByRole("img")).toBeNull();
  });
});

describe("the presentation order", () => {
  it("places the lowest remaining allowance first", () => {
    acceptSnapshot(
      snapshot("instance-1", 1, [
        account("high", "codex", 1, [quotaWindow("w", "session", percent(86))], {
          rank: 86,
        }),
        account("low", "claude", 2, [quotaWindow("w", "session", percent(3))], { rank: 3 }),
        account("middle", "clinepass", 3, [quotaWindow("w", "session", percent(41))], {
          rank: 41,
        }),
      ]),
    );
    applyPendingOrder();
    render(<Harness />);

    expect(rowOrder()).toEqual(["low", "middle", "high"]);
  });

  it("breaks a tie by connection ordinal and then by account identity", () => {
    acceptSnapshot(
      snapshot("instance-1", 1, [
        account("zebra", "codex", 4, [quotaWindow("w", "session", percent(50))], {
          rank: 50,
        }),
        account("beta", "claude", 2, [quotaWindow("w", "session", percent(50))], {
          rank: 50,
        }),
        account("alpha", "clinepass", 2, [quotaWindow("w", "session", percent(50))], {
          rank: 50,
        }),
      ]),
    );
    applyPendingOrder();
    render(<Harness />);

    // Ordinal 2 comes first; inside ordinal 2 the identity decides.
    expect(rowOrder()).toEqual(["alpha", "beta", "zebra"]);
  });

  it("keeps a renamed account in place, because identity controls the tie", () => {
    acceptSnapshot(
      snapshot("instance-1", 1, [
        account("alpha", "codex", 2, [quotaWindow("w", "session", percent(50))], {
          rank: 50,
          nickname: "Personal",
        }),
        account("beta", "claude", 2, [quotaWindow("w", "session", percent(50))], {
          rank: 50,
          nickname: "Work",
        }),
      ]),
    );
    applyPendingOrder();
    render(<Harness />);
    const before = rowOrder();

    act(() => {
      acceptSnapshot(
        snapshot("instance-1", 2, [
          account("alpha", "codex", 2, [quotaWindow("w", "session", percent(50))], {
            rank: 50,
            nickname: "Zzz",
          }),
          account("beta", "claude", 2, [quotaWindow("w", "session", percent(50))], {
            rank: 50,
            nickname: "Aaa",
          }),
        ]),
      );
      applyPendingOrder();
    });

    expect(rowOrder()).toEqual(before);
  });
});

describe("the needs-checking section", () => {
  it("places a stale account above the numeric order and outside it", () => {
    acceptSnapshot(
      snapshot("instance-1", 1, [
        account("fresh", "codex", 1, [quotaWindow("w", "session", percent(80))], {
          rank: 80,
        }),
        account("stale", "claude", 2, [quotaWindow("w", "session", percent(5))], {
          rank: null,
          unrankedReason: "stale",
        }),
      ]),
    );
    applyPendingOrder();
    render(<Harness />);

    expect(rowOrder()).toEqual(["stale", "fresh"]);
    expect(screen.getByText("Needs checking")).toBeInTheDocument();
    const sections = screen.getAllByText(/Needs checking|Least remaining first/);
    expect(sections[0]).toHaveTextContent("Needs checking");
  });

  it("places an account with no comparable rank in needs checking, not at the bottom", () => {
    acceptSnapshot(
      snapshot("instance-1", 1, [
        account("ranked", "codex", 1, [quotaWindow("w", "session", percent(90))], {
          rank: 90,
        }),
        account("incomplete", "claude", 2, [quotaWindow("w", "session", unavailable())], {
          rank: null,
          unrankedReason: "incomplete",
        }),
      ]),
    );
    applyPendingOrder();
    render(<Harness />);

    expect(rowOrder()).toEqual(["incomplete", "ranked"]);
  });

  it("separates a monitoring-off account from the numeric order", () => {
    acceptSnapshot(
      snapshot("instance-1", 1, [
        account("ranked", "codex", 1, [quotaWindow("w", "session", percent(90))], {
          rank: 90,
        }),
        account("off", "claude", 2, [], {
          rank: null,
          unrankedReason: "disabled",
          monitoringEnabled: false,
        }),
      ]),
    );
    applyPendingOrder();
    render(<Harness />);

    expect(rowOrder()).toEqual(["ranked", "off"]);
    expect(screen.getByText("Monitoring off")).toBeInTheDocument();
  });
});

describe("the remaining-allowance label", () => {
  it("reads a positive sub-one-percent remainder as <1%, never as zero", () => {
    acceptSnapshot(
      snapshot("instance-1", 1, [
        account("almost", "codex", 1, [quotaWindow("w", "session", percent(0.42))], {
          rank: 0.42,
        }),
      ]),
    );
    render(<Harness />);

    expect(screen.getAllByText("<1%").length).toBeGreaterThan(0);
    expect(screen.queryByText("0%")).toBeNull();
  });

  it("never draws a zero arc or a full arc for a reading that has no number", () => {
    acceptSnapshot(
      snapshot("instance-1", 1, [
        account("unknown", "codex", 1, [quotaWindow("w", "session", unavailable())], {
          rank: null,
          unrankedReason: "incomplete",
        }),
      ]),
    );
    render(<Harness />);

    expect(screen.getAllByText("Not reported").length).toBeGreaterThan(0);
    expect(screen.queryByText("0%")).toBeNull();
    expect(screen.queryByText("100%")).toBeNull();
    const arcs = document.querySelectorAll(".ring__arc");
    expect(arcs.length).toBeGreaterThan(0);
    for (const arc of arcs) {
      expect(arc.getAttribute("stroke-dasharray")).toBe("0 100");
    }
  });

  it("draws an arc proportional to a known reading", () => {
    acceptSnapshot(
      snapshot("instance-1", 1, [
        account("steady", "codex", 1, [quotaWindow("w", "session", percent(72))], {
          rank: 72,
        }),
      ]),
    );
    render(<Harness />);

    const arc = document.querySelector(".ring__arc");
    expect(arc?.getAttribute("stroke-dasharray")).toBe("72 100");
  });
});

describe("a value update while the list is busy", () => {
  it("updates the value but keeps row identity, order, and focus stable", async () => {
    acceptSnapshot(
      snapshot("instance-1", 1, [
        account("first", "codex", 1, [quotaWindow("w", "session", percent(80))], {
          rank: 80,
        }),
        account("second", "claude", 2, [quotaWindow("w", "session", percent(10))], {
          rank: 10,
        }),
      ]),
    );
    applyPendingOrder();
    render(<Harness />);
    expect(rowOrder()).toEqual(["second", "first"]);

    const focusedRow = document.querySelector('[data-account-id="second"]');
    const focusedButton = focusedRow?.querySelector("button");
    focusedButton?.focus();
    expect(document.activeElement).toBe(focusedButton);

    // The backend now says the second account recovered and the first dropped.
    act(() => {
      acceptSnapshot(
        snapshot("instance-1", 2, [
          account("first", "codex", 1, [quotaWindow("w", "session", percent(2))], {
            rank: 2,
          }),
          account("second", "claude", 2, [quotaWindow("w", "session", percent(95))], {
            rank: 95,
          }),
        ]),
      );
    });

    // Values are current immediately.
    expect(cellsOf("first")[0]).toHaveTextContent("2%");
    expect(cellsOf("second")[0]).toHaveTextContent("95%");
    // The displayed order has not moved yet, and focus stayed on its row.
    expect(rowOrder()).toEqual(["second", "first"]);
    expect(document.activeElement).toBe(focusedButton);
    expect(getRendererState().pendingOrder).not.toBeNull();
  });

  it("applies the staged order once it is asked to", () => {
    acceptSnapshot(
      snapshot("instance-1", 1, [
        account("first", "codex", 1, [quotaWindow("w", "session", percent(80))], {
          rank: 80,
        }),
        account("second", "claude", 2, [quotaWindow("w", "session", percent(10))], {
          rank: 10,
        }),
      ]),
    );
    applyPendingOrder();
    render(<Harness />);
    expect(rowOrder()).toEqual(["second", "first"]);

    act(() => {
      acceptSnapshot(
        snapshot("instance-1", 2, [
          account("first", "codex", 1, [quotaWindow("w", "session", percent(2))], {
            rank: 2,
          }),
          account("second", "claude", 2, [quotaWindow("w", "session", percent(95))], {
            rank: 95,
          }),
        ]),
      );
    });
    expect(rowOrder()).toEqual(["second", "first"]);

    act(() => {
      applyPendingOrder();
    });
    expect(rowOrder()).toEqual(["first", "second"]);
  });

  it("offers the update-order control while an order is staged", () => {
    acceptSnapshot(
      snapshot("instance-1", 1, [
        account("only", "codex", 1, [quotaWindow("w", "session", percent(50))], {
          rank: 50,
        }),
      ]),
    );
    applyPendingOrder();
    render(<Harness />);
    expect(screen.queryByRole("button", { name: /update order/i })).toBeNull();

    act(() => {
      acceptSnapshot(
        snapshot("instance-1", 2, [
          account("only", "codex", 1, [quotaWindow("w", "session", percent(40))], {
            rank: 40,
          }),
        ]),
      );
    });
    expect(screen.getByRole("button", { name: /update order/i })).toBeInTheDocument();
  });

  it("has a documented idle delay before an unattended reorder", () => {
    expect(REORDER_IDLE_MS).toBe(1200);
  });
});

describe("the overview filters", () => {
  it("keeps every account visible by default and filters on request", async () => {
    acceptSnapshot(
      snapshot("instance-1", 1, [
        account("calm", "codex", 1, [quotaWindow("w", "session", percent(90))], {
          rank: 90,
        }),
        account("low", "claude", 2, [quotaWindow("w", "session", percent(4))], { rank: 4 }),
      ]),
    );
    applyPendingOrder();
    render(<Harness />);
    expect(rowOrder()).toEqual(["low", "calm"]);

    const attention = screen.getByRole("button", { name: /needs attention/i });
    expect(attention).toHaveTextContent("1");
    attention.click();
    expect(rowOrder()).toEqual(["low"]);
  });
});
