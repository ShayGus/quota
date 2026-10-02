/**
 * Overview behaviour.
 *
 * These tests exercise what a person sees and does: the readings on a card, the
 * order of the cards, and what happens to a card while a newer order is waiting
 * (spec 4.1-4.3, spec 17).
 */
import { fireEvent, render, screen } from "@testing-library/react";
import { act, useState } from "react";
import { describe, expect, it, vi } from "vitest";

import { Overview, REORDER_IDLE_MS } from "../src/features/overview/Overview";
import type { OverviewFilter } from "../src/features/overview/OverviewToolbar";
import type { IndicatorStyle } from "../src/generated/bindings";
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
  NOW,
  percent,
  preferences,
  snapshot,
  unavailable,
  window as quotaWindow,
} from "./fixtures";

/** The callbacks the harness records. */
const calls = {
  open: vi.fn(),
  openWindow: vi.fn(),
  reconnect: vi.fn(),
  enable: vi.fn(),
};

/** Renders the overview against the live store, as the application does. */
function Harness(): React.ReactElement {
  const state: RendererState = useRendererState();
  const [filter, setFilter] = useState<OverviewFilter>("all");
  return (
    <Overview
      state={state}
      filter={filter}
      onFilter={setFilter}
      onAddAccount={() => undefined}
      onIndicatorStyle={() => undefined}
      onOpenAccount={calls.open}
      onOpenWindow={calls.openWindow}
      onResume={() => undefined}
      onReconnect={calls.reconnect}
      onEnable={calls.enable}
    />
  );
}

/** The account identity of every card, in the order the DOM shows them. */
function cardOrder(): readonly (string | null)[] {
  const cards = screen.queryAllByRole("article");
  return cards.map((card) => card.getAttribute("data-account-id"));
}

/** The rendered rings of one card. */
function ringsOf(accountId: string): readonly HTMLElement[] {
  const card = document.querySelector(`[data-account-id="${accountId}"]`);
  if (card === null) {
    return [];
  }
  return [...card.querySelectorAll(".quota-button")].map((cell) => cell as HTMLElement);
}

/** The text at the centre of every ring on screen. */
function ringValues(): readonly (string | null)[] {
  return [...document.querySelectorAll(".ring-value")].map((value) => value.textContent);
}

describe("the account card", () => {
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

    const rings = ringsOf("a1");
    expect(rings).toHaveLength(3);
    expect(rings[0]?.textContent).toContain("5-hour");
    expect(rings[0]?.textContent).toContain("72%");
    expect(rings[1]?.textContent).toContain("41%");
    expect(rings[2]?.textContent).toContain("0%");
  });

  it("draws a ring only for the windows the account offers", () => {
    acceptSnapshot(
      snapshot("instance-1", 1, [
        account("a1", "codex", 1, [
          quotaWindow("session-window", "session", percent(72)),
        ]),
      ]),
    );
    render(<Harness />);

    const rings = ringsOf("a1");
    expect(rings).toHaveLength(1);
    expect(screen.queryByText("Weekly")).toBeNull();
  });

  it("opens the chosen window's detail from its ring", () => {
    acceptSnapshot(
      snapshot("instance-1", 1, [
        account("a1", "codex", 1, [
          quotaWindow("session-window", "session", percent(72)),
          quotaWindow("weekly-window", "weekly", percent(41)),
        ]),
      ]),
    );
    render(<Harness />);
    fireEvent.click(
      screen.getByRole("button", {
        name: "Codex a1, Weekly: 41% remaining. Resets in 2h 0m",
      }),
    );
    expect(calls.openWindow).toHaveBeenCalledWith("a1", "weekly-window");
    fireEvent.click(screen.getByRole("button", { name: "Details for Codex a1" }));
    expect(calls.open).toHaveBeenCalledWith("a1");
  });

  it("names each ring by its account, so two accounts of one provider differ", () => {
    acceptSnapshot(
      snapshot("instance-1", 1, [
        account("a1", "codex", 1, [quotaWindow("s1", "session", percent(72))], {
          nickname: "Work",
          rank: 72,
        }),
        account("a2", "codex", 2, [quotaWindow("s2", "session", percent(72))], {
          nickname: "Home",
          rank: 72,
        }),
      ]),
    );
    render(<Harness />);
    expect(
      screen.getByRole("button", { name: /^Codex Work, 5-hour: 72% remaining/ }),
    ).toBeTruthy();
    expect(
      screen.getByRole("button", { name: /^Codex Home, 5-hour: 72% remaining/ }),
    ).toBeTruthy();
  });

  it("draws the account-wide allowance as the ring and lists a narrower one", () => {
    acceptSnapshot(
      snapshot("instance-1", 1, [
        account(
          "a1",
          "claude",
          1,
          [
            quotaWindow("model-weekly", "weekly", percent(43), {
              label: "Model-specific weekly",
              resource: "one-model",
            }),
            quotaWindow("session", "session", percent(18), { resource: "account" }),
            quotaWindow("weekly", "weekly", percent(64), { resource: "account" }),
          ],
          { rank: 18 },
        ),
      ]),
    );
    render(<Harness />);
    expect(
      screen.getByRole("button", { name: /^Claude a1, Weekly: 64% remaining/ }),
    ).toBeTruthy();
    expect(screen.getByRole("button", { name: /1 model limit/ })).toBeTruthy();
  });

  it("lists other independent limits on request", () => {
    acceptSnapshot(
      snapshot("instance-1", 1, [
        account("a1", "claude", 1, [
          quotaWindow("session-window", "session", percent(72)),
          quotaWindow("weekly-window", "weekly", percent(64)),
          quotaWindow("model-window", "weekly", percent(43), {
            label: "Model-specific weekly",
            resource: "model-family",
          }),
        ]),
      ]),
    );
    render(<Harness />);
    expect(ringsOf("a1")).toHaveLength(2);
    expect(screen.queryByText("Model-specific weekly")).toBeNull();

    const toggle = screen.getByRole("button", { name: /1 model limit/ });
    expect(toggle.getAttribute("aria-expanded")).toBe("false");
    fireEvent.click(toggle);
    expect(screen.getByText("Model-specific weekly")).toBeTruthy();
    expect(screen.getByText("43% remaining")).toBeTruthy();
  });

  it("draws only account-wide allowances as rings, whatever their number", () => {
    // Two model-specific weekly allowances outnumber the account-wide one, and
    // the account has no account-wide 5-hour window at all.
    acceptSnapshot(
      snapshot("instance-1", 1, [
        account("a1", "claude", 1, [
          quotaWindow("opus-session", "session", percent(30), {
            label: "Opus",
            resource: "opus",
          }),
          quotaWindow("opus-weekly", "weekly", percent(20), {
            label: "Opus",
            resource: "opus",
          }),
          quotaWindow("sonnet-weekly", "weekly", percent(50), {
            label: "Sonnet",
            resource: "sonnet",
          }),
          quotaWindow("account-weekly", "weekly", percent(64), {
            label: "Claude account",
          }),
        ]),
      ]),
    );
    render(<Harness />);
    // One ring: the account-wide weekly. No model window takes a period ring.
    expect(ringsOf("a1")).toHaveLength(1);
    expect(screen.getByRole("button", { name: /3 model limits/ })).toBeTruthy();
  });

  it("offers Enable for a monitoring-off account and Reconnect for an expired one", () => {
    acceptSnapshot(
      snapshot("instance-1", 1, [
        account("off", "codex", 1, [], {
          rank: null,
          unrankedReason: "disabled",
          monitoringEnabled: false,
        }),
        account("expired", "claude", 2, [quotaWindow("w", "session", percent(40))], {
          rank: null,
          unrankedReason: "reconnect_required",
          connectionState: "reauthentication_required",
        }),
      ]),
    );
    applyPendingOrder();
    render(<Harness />);

    expect(screen.getByText("Monitoring off")).toBeTruthy();
    expect(screen.getByText("Checks disabled")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Enable" }));
    expect(calls.enable).toHaveBeenCalledWith("off");

    expect(screen.getByText("Connection expired")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Reconnect expired" }));
    expect(calls.reconnect).toHaveBeenCalledWith("expired");
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

    expect(cardOrder()).toEqual(["low", "middle", "high"]);
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
    expect(cardOrder()).toEqual(["alpha", "beta", "zebra"]);
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
    const before = cardOrder();

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

    expect(cardOrder()).toEqual(before);
  });

  it("places a stale account above the numeric order", () => {
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

    expect(cardOrder()).toEqual(["stale", "fresh"]);
  });

  it("places an account with no comparable rank above the ranked ones", () => {
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

    expect(cardOrder()).toEqual(["incomplete", "ranked"]);
  });

  it("places a monitoring-off account after the numeric order", () => {
    acceptSnapshot(
      snapshot("instance-1", 1, [
        account("off", "claude", 1, [], {
          rank: null,
          unrankedReason: "disabled",
          monitoringEnabled: false,
        }),
        account("ranked", "codex", 2, [quotaWindow("w", "session", percent(90))], {
          rank: 90,
        }),
      ]),
    );
    applyPendingOrder();
    render(<Harness />);

    expect(cardOrder()).toEqual(["ranked", "off"]);
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

    expect(ringValues()).toEqual(["<1%"]);
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
    expect(screen.getByText("no reading")).toBeTruthy();
    expect(ringValues()).toEqual(["—"]);
    const arcs = document.querySelectorAll(".ring-arc");
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

    const arc = document.querySelector(".ring-arc");
    expect(arc?.getAttribute("stroke-dasharray")).toBe("72 100");
  });

  it.each([
    [72, "", "Current"],
    [18, "warn", "5h low"],
    [8, "danger", "5h low"],
    [0, "danger", "5h exhausted"],
  ] as const)("colours %i%% %s and names it %s", (remaining, tone, badge) => {
    acceptSnapshot(
      snapshot("instance-1", 1, [
        account("a", "codex", 1, [quotaWindow("w", "session", percent(remaining))], {
          rank: remaining,
        }),
      ]),
    );
    render(<Harness />);
    const ring = document.querySelector(".ring");
    if (tone === "") {
      expect(ring?.classList.contains("warn")).toBe(false);
      expect(ring?.classList.contains("danger")).toBe(false);
    } else {
      expect(ring?.classList.contains(tone)).toBe(true);
    }
    expect(document.querySelector(".badge")?.textContent).toBe(badge);
  });
});

describe("a value update while the list is busy", () => {
  it("updates the value but keeps card identity, order, and focus stable", () => {
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
    expect(cardOrder()).toEqual(["second", "first"]);

    const focusedCard = document.querySelector('[data-account-id="second"]');
    const focusedButton = focusedCard?.querySelector("button");
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
    expect(ringsOf("first")[0]?.textContent).toContain("2%");
    expect(ringsOf("second")[0]?.textContent).toContain("95%");
    // The displayed order has not moved yet, and focus stayed on its card.
    expect(cardOrder()).toEqual(["second", "first"]);
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
    expect(cardOrder()).toEqual(["second", "first"]);

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
    expect(cardOrder()).toEqual(["second", "first"]);

    act(() => {
      applyPendingOrder();
    });
    expect(cardOrder()).toEqual(["first", "second"]);
  });

  it("holds the staged order while the list has focus, then applies it", () => {
    vi.useFakeTimers();
    vi.setSystemTime(NOW);
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
      expect(cardOrder()).toEqual(["second", "first"]);

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
      expect(cardOrder()).toEqual(["second", "first"]);

      // They let go. The order lands on its own, without being asked for.
      act(() => {
        (document.activeElement as HTMLElement | null)?.blur();
      });
      act(() => {
        vi.advanceTimersByTime(REORDER_IDLE_MS * 5);
      });
      expect(cardOrder()).toEqual(["first", "second"]);
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
    expect(cardOrder()).toEqual(["low", "calm"]);

    const attention = screen.getByRole("button", { name: /^attention/i });
    expect(attention.textContent).toContain("1");
    act(() => {
      fireEvent.click(attention);
    });
    expect(cardOrder()).toEqual(["low"]);
  });

  it("does not count a monitoring-off account as needing attention", () => {
    acceptSnapshot(
      snapshot("instance-1", 1, [
        account("off", "claude", 1, [], {
          rank: null,
          unrankedReason: "disabled",
          monitoringEnabled: false,
        }),
      ]),
    );
    render(<Harness />);
    expect(screen.getByRole("button", { name: /^attention/i }).textContent).toBe(
      "Attention0",
    );
  });

  it("restores every account from the empty attention filter", () => {
    acceptSnapshot(
      snapshot("instance-1", 1, [
        account("calm", "codex", 1, [quotaWindow("w", "session", percent(90))], {
          rank: 90,
        }),
      ]),
    );
    render(<Harness />);
    fireEvent.click(screen.getByRole("button", { name: /^Attention/ }));
    expect(cardOrder()).toHaveLength(0);
    expect(screen.getByText("No accounts need attention")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Show all accounts" }));
    expect(cardOrder()).toEqual(["calm"]);
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
    expect(ring?.classList.contains("stale")).toBe(true);
    expect(screen.getAllByText("last known").length).toBeGreaterThan(0);
    expect(document.querySelector(".badge")?.textContent).toMatch(/^Stale/);
    expect(screen.getByText(/Showing last-known values/)).toBeTruthy();
    // The number stays visible; it is not hidden or turned into zero.
    expect(ringValues()).toEqual(["93%"]);
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

    expect(document.querySelector(".ring")?.classList.contains("stale")).toBe(true);
    expect(ringValues()).toEqual(["18%"]);
    expect(screen.getByText("Verifying reset")).toBeTruthy();
    expect(screen.getByText(/No automatic refill/)).toBeTruthy();
  });

  it("shows a window whose boundary has passed as verifying, not refilled", () => {
    acceptSnapshot(
      snapshot("instance-1", 1, [
        account(
          "due",
          "codex",
          1,
          [
            quotaWindow("w", "session", percent(18), {
              boundaryAt: "2026-10-01T11:00:00.000Z",
            }),
          ],
          { rank: null, unrankedReason: "reset_pending" },
        ),
      ]),
    );
    render(<Harness />);

    expect(ringValues()).toEqual(["—"]);
    expect(screen.getByText("verifying")).toBeTruthy();
    expect(screen.getByText("Reset due · verifying")).toBeTruthy();
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
    expect(ringValues()).toEqual(["68%", "—"]);
    expect(screen.getAllByText("Not reported").length).toBeGreaterThan(0);
    expect(screen.getByText("Partially reported")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /1 other limit/ }));
    expect(screen.getByText("Model X weekly")).toBeTruthy();
    expect(screen.getByText("0% remaining")).toBeTruthy();
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

describe("boundary meanings", () => {
  const kinds = [
    "full_reset",
    "next_replenishment",
    "billing_boundary",
    "unknown",
  ] as const;
  const words = [
    "Resets in",
    "Next replenishment in",
    "Billing boundary in",
    "Boundary in",
  ];

  it.each(kinds.map((kind, index) => [kind, words[index]] as const))(
    "names a %s boundary under its ring",
    (kind, word) => {
      const window = {
        ...quotaWindow("w", "session", percent(72)),
        boundary: { kind, at: "2026-10-01T14:00:00Z" },
      };
      acceptSnapshot(
        snapshot("instance-1", 1, [account("a", "claude", 1, [window], { rank: 72 })]),
      );
      render(<Harness />);
      expect(document.querySelector(".reset-label")?.textContent).toBe(
        `${String(word)} 2h 0m`,
      );
    },
  );

  it.each(kinds.map((kind, index) => [kind, words[index]] as const))(
    "keeps a %s boundary's meaning on its compact row",
    (kind, word) => {
      acceptPreferences(preferences({ indicator_style: "bar" }));
      const window = {
        ...quotaWindow("w", "session", percent(72)),
        boundary: { kind, at: "2026-10-01T14:00:00Z" },
      };
      acceptSnapshot(
        snapshot("instance-1", 1, [account("a", "claude", 1, [window], { rank: 72 })]),
      );
      render(<Harness />);
      expect(document.querySelector(".bar-time")?.getAttribute("title")).toBe(
        `${String(word)} 2h 0m`,
      );
      expect(
        screen.getByRole("button", {
          name: `Claude a, 5-hour: 72% remaining. ${String(word)} 2h 0m`,
        }),
      ).toBeTruthy();
    },
  );

  it("shows the countdown in the compact layout", () => {
    const style: IndicatorStyle = "bar";
    acceptPreferences(preferences({ indicator_style: style }));
    acceptSnapshot(
      snapshot("instance-1", 1, [
        account("a", "claude", 1, [quotaWindow("w", "session", percent(72))], {
          rank: 72,
        }),
      ]),
    );
    render(<Harness />);
    expect(document.querySelector(".ring")).toBeNull();
    expect(document.querySelector(".bar-value")?.textContent).toBe("72%");
    expect(document.querySelector(".bar-time")?.textContent).toBe("2h 0m");
    expect(screen.getByText("RESET IN")).toBeTruthy();
  });
});
