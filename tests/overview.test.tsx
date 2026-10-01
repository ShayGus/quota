/**
 * Overview behaviour.
 *
 * These tests exercise what a person sees and does: the cells in a row, the
 * section an account lands in, the order of the rows, and what happens to a row
 * while a newer order is waiting (spec 4.1-4.3, spec 17).
 */
import { fireEvent, render, screen } from "@testing-library/react";
import { act } from "react";
import { describe, expect, it, vi } from "vitest";

import { Overview, REORDER_IDLE_MS } from "../src/features/overview/Overview";
import type { RendererState } from "../src/shared/state/types";
import {
  applyPendingOrder,
  acceptPreferences,
  getRendererState,
} from "../src/shared/state/store";
import { acceptSnapshot } from "../src/shared/state/store";
import { useRendererState } from "../src/shared/state/useRendererState";
import {
  account,
  percent,
  preferences,
  snapshot,
  unavailable,
  window as quotaWindow,
} from "./fixtures";

/** Renders the overview against the live store, as the application does. */
function Harness(): React.ReactElement {
  const state: RendererState = useRendererState();
  return (
    <Overview
      state={state}
      onOpenAccount={() => undefined}
      onReconnect={() => undefined}
    />
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
    expect(cells[0]?.textContent).toContain("72%");
    expect(cells[1]?.textContent).toContain("41%");
    expect(cells[2]?.textContent).toContain("0%");
    // All three are in one row, so the values are compared together.
    expect(cells).toHaveLength(3);
  });

  it("says an offered column is absent rather than drawing a ring for it", () => {
    acceptSnapshot(
      snapshot("instance-1", 1, [
        account("a1", "codex", 1, [
          quotaWindow("session-window", "session", percent(72)),
        ]),
      ]),
    );
    render(<Harness />);

    const cells = cellsOf("a1");
    expect(cells[1]?.textContent).toContain("Not offered");
    expect(cells[1]?.querySelector(".ring")).toBeNull();
  });
});

describe("the presentation order", () => {
  it("places the lowest remaining allowance first", () => {
    acceptSnapshot(
      snapshot("instance-1", 1, [
        account("high", "codex", 1, [quotaWindow("w", "session", percent(86))], {
          rank: 86,
        }),
        account("low", "claude", 2, [quotaWindow("w", "session", percent(3))], {
          rank: 3,
        }),
        account("middle", "open_code_go", 3, [quotaWindow("w", "session", percent(41))], {
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
        account("alpha", "open_code_go", 2, [quotaWindow("w", "session", percent(50))], {
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
    const headings = document.querySelectorAll(".section-separator strong");
    expect([...headings].map((heading) => heading.textContent)).toEqual([
      "Needs checking",
      "Least remaining first",
    ]);
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
    expect(screen.getAllByText("Monitoring off").length).toBeGreaterThan(0);
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
  it("updates the value but keeps row identity, order, and focus stable", () => {
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
    expect(cellsOf("first")[0]?.textContent).toContain("2%");
    expect(cellsOf("second")[0]?.textContent).toContain("95%");
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
    expect(screen.getByRole("button", { name: /update order/i })).toBeTruthy();
  });

  it("holds the staged order while the list has focus, then applies it", () => {
    vi.useFakeTimers();
    try {
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

      // Someone is working inside the list, so focus is inside it.
      const focusedButton = document
        .querySelector('[data-account-id="second"]')
        ?.querySelector("button");
      focusedButton?.focus();
      expect(document.activeElement).toBe(focusedButton);

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

      // Idle for well past the delay, but the list is still engaged, so the
      // order holds and the values stay current.
      act(() => {
        vi.advanceTimersByTime(REORDER_IDLE_MS * 5);
      });
      expect(getRendererState().pendingOrder).not.toBeNull();
      expect(rowOrder()).toEqual(["second", "first"]);

      // They let go. The order lands on its own, without being asked for.
      act(() => {
        (document.activeElement as HTMLElement | null)?.blur();
      });
      act(() => {
        vi.advanceTimersByTime(REORDER_IDLE_MS * 5);
      });
      expect(rowOrder()).toEqual(["first", "second"]);
      expect(getRendererState().pendingOrder).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });
});

describe("the overview filters", () => {
  it("keeps every account visible by default and filters on request", () => {
    acceptSnapshot(
      snapshot("instance-1", 1, [
        account("calm", "codex", 1, [quotaWindow("w", "session", percent(90))], {
          rank: 90,
        }),
        account("low", "claude", 2, [quotaWindow("w", "session", percent(4))], {
          rank: 4,
        }),
      ]),
    );
    applyPendingOrder();
    render(<Harness />);
    expect(rowOrder()).toEqual(["low", "calm"]);

    const attention = screen.getByRole("button", { name: /needs attention/i });
    expect(attention.textContent).toContain("1");
    act(() => {
      fireEvent.click(attention);
    });
    expect(rowOrder()).toEqual(["low"]);
  });
});

describe("freshness", () => {
  it("draws a stale reading muted and dashed with a last-known caption", () => {
    acceptSnapshot(
      snapshot("instance-1", 1, [
        account("stale", "claude", 1, [quotaWindow("w", "session", percent(93))], {
          rank: null,
          unrankedReason: "stale",
        }),
      ]),
    );
    applyPendingOrder();
    render(<Harness />);

    const ring = document.querySelector(".ring");
    expect(ring?.className).toContain("ring--stale");
    expect(screen.getAllByText("last known").length).toBeGreaterThan(0);
    expect(screen.getAllByText("Stale reading").length).toBeGreaterThan(0);
    // The number stays visible; it is not hidden or turned into zero.
    expect(screen.getAllByText("93%").length).toBeGreaterThan(0);
  });

  it("draws a reset-pending reading as not current rather than refilled", () => {
    acceptSnapshot(
      snapshot("instance-1", 1, [
        account("due", "codex", 1, [quotaWindow("w", "session", percent(18))], {
          rank: null,
          unrankedReason: "reset_pending",
        }),
      ]),
    );
    applyPendingOrder();
    render(<Harness />);

    expect(document.querySelector(".ring")?.className).toContain("ring--stale");
    expect(screen.getAllByText("18%").length).toBeGreaterThan(0);
    expect(screen.queryByText("100%")).toBeNull();
  });

  it("keeps a known zero visible when another window is missing (AC-53)", () => {
    acceptSnapshot(
      snapshot("instance-1", 1, [
        account(
          "partial",
          "open_code_go",
          1,
          [
            quotaWindow("session-window", "session", percent(68)),
            quotaWindow("missing-week", "weekly", unavailable()),
            quotaWindow("model-week", "custom", percent(0), { label: "Model X weekly" }),
          ],
          { rank: null, unrankedReason: "incomplete" },
        ),
      ]),
    );
    applyPendingOrder();
    render(<Harness />);

    // Three states at once: a current reading, a missing one, and a known zero
    // that names the scope it belongs to.
    expect(screen.getAllByText("68%").length).toBeGreaterThan(0);
    expect(screen.getAllByText("Not reported").length).toBeGreaterThan(0);
    expect(screen.getAllByText("Model X weekly: 0%").length).toBeGreaterThan(0);
    expect(screen.getAllByText("Partial reading").length).toBeGreaterThan(0);
  });
});

describe("the privacy alias setting", () => {
  /** Two accounts whose displayed order differs from their identity order. */
  function twoAccounts(): void {
    acceptSnapshot(
      snapshot("instance-1", 1, [
        account("zulu", "codex", 1, [quotaWindow("w", "session", percent(80))], {
          rank: 80,
        }),
        account("alpha", "claude", 2, [quotaWindow("w", "session", percent(10))], {
          rank: 10,
        }),
      ]),
    );
    applyPendingOrder();
  }

  it("shows each account's own name when aliases are off", () => {
    acceptPreferences(preferences());
    twoAccounts();
    render(<Harness />);
    expect(screen.getByText("zulu")).toBeTruthy();
    expect(screen.getByText("alpha")).toBeTruthy();
  });

  it("replaces each name with its own stable label when aliases are on", () => {
    acceptPreferences(
      preferences({
        privacy: { ...preferences().privacy, alias_mode: "stable_aliases" },
      }),
    );
    twoAccounts();
    render(<Harness />);
    expect(screen.queryByText("zulu")).toBeNull();
    expect(screen.queryByText("alpha")).toBeNull();
    // Distinct labels, not one shared "Hidden account".
    expect(screen.getByText("Account 1")).toBeTruthy();
    expect(screen.getByText("Account 2")).toBeTruthy();
  });
});
