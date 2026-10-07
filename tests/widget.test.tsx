/**
 * The mini widget.
 *
 * These tests hold the approved canvas to what the widget draws: one ring per
 * period with the shortest outside, so a lone ring is the outer one; the
 * number under the rings named after its ring; money as money, never a
 * share; mini cards in pairs with an odd last card across the width; and a
 * corner button that switches back to the full window. Pointing at a tile
 * only shows its peek line, a still click opens its details drawer, and a
 * press that moves 4 px drags the window instead.
 */
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
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
  fitWidget: vi.fn((contentHeight: number, direction: string) =>
    Promise.resolve({
      height: contentHeight,
      direction,
      room_above: 800,
      room_below: 800,
    }),
  ),
  dragWidget: vi.fn(() => Promise.resolve()),
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
    expect(shown?.rings).toEqual([
      { period: "monthly", fraction: 0.48, letter: "M", value: "48%", low: false },
    ]);
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

  it("marks last-known values as stale", () => {
    const stale = account(
      "stale",
      "codex",
      8,
      [
        {
          ...quotaWindow("s", "session", percent(60)),
          valid_until: "2026-10-01T11:00:00.000Z",
        },
      ],
      { rank: 60 },
    );
    const [shown] = accountsOf(stale);
    expect(shown?.rows).toMatchObject([{ reset: "last known", stale: true }]);
  });

  it("chips a problem status and peeks it with its last-known note", () => {
    const connecting = account(
      "ollama",
      "ollama_cloud",
      9,
      [
        {
          ...quotaWindow("o5", "session", percent(100)),
          valid_until: "2026-10-01T11:00:00.000Z",
        },
      ],
      { connectionState: "connecting", rank: 10 },
    );
    const [shown] = accountsOf(connecting);
    expect(shown?.chip).toEqual({ text: "Connecting", tone: "pending" });
    expect(shown?.headline.value).toBe("Connecting");
    expect(shown?.peek).toBe("Connecting · last known values");
  });

  it("chips a warning status with its warning tone", () => {
    const limited = {
      ...account("limited", "cursor", 10, [quotaWindow("l", "session", percent(40))], {
        rank: 40,
      }),
      fetch_state: "backoff" as const,
    };
    const [shown] = accountsOf(limited);
    expect(shown?.chip).toEqual({ text: "Rate limited", tone: "warn" });
  });

  it("peeks the tightest limit, an amount, or no reading", () => {
    const [first, second] = accountsOf(claude, openrouter);
    expect(second?.peek).toBe("$17.54 left");
    expect(first?.peek).toBe("Fable weekly resets in 2h 0m");
    const silent = account(
      "silent",
      "codex",
      11,
      [quotaWindow("v", "session", { kind: "unavailable", value: "not_reported" })],
      { rank: null },
    );
    const [quiet] = accountsOf(silent);
    expect(quiet?.peek).toBe("no reading reported");
  });
});

describe("the mini cards", () => {
  it("pair the cards, with an odd last card across the width", () => {
    const { container, rerender } = render(
      <MiniCards accounts={accountsOf(kimi, claude, cursor)} />,
    );
    const wide = (): string[] =>
      [...container.querySelectorAll(".widget-card")].map((card) =>
        card.classList.contains("wide") ? "wide" : "half",
      );
    expect(wide()).toEqual(["half", "half", "wide"]);
    rerender(<MiniCards accounts={accountsOf(kimi, claude, cursor, openrouter)} />);
    expect(wide()).toEqual(["half", "half", "half", "half"]);
  });

  it("show money in a card as an amount", () => {
    render(<MiniCards accounts={accountsOf(openrouter)} />);
    expect(screen.getByText("$17.54 left")).toBeTruthy();
    expect(screen.queryByText(/%/)).toBeNull();
  });
});

describe("the ring strip", () => {
  it("balances its rows: nine tiles make three rows of three", () => {
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
    const { container } = render(<RingStrip accounts={accountsOf(...nine)} />);
    const tiles = container.querySelector<HTMLElement>(".widget-tiles");
    expect(tiles?.style.width).toBe("292px");
  });

  it("writes every ring's value under its tile, after its period's letter", () => {
    const { container } = render(<RingStrip accounts={accountsOf(kimi, claude)} />);
    const readings = [...container.querySelectorAll(".widget-readings")].map(
      (element) => element.textContent,
    );
    expect(readings).toEqual(["H8%W52%M77%", "H41%W30%"]);
    expect(
      container.querySelector(".widget-reading .widget-value.low")?.textContent,
    ).toBe("8%");
  });

  it("says what is wrong instead of the values when an account needs attention", () => {
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
    const { container } = render(<RingStrip accounts={accountsOf(lapsed)} />);
    expect(container.querySelector(".widget-readings")).toBeNull();
    expect(container.querySelector(".widget-headline")?.textContent).toBe("Reconnect");
  });

  it("names every ring colour in its key", () => {
    render(<RingStrip accounts={accountsOf(kimi)} />);
    for (const name of ["5-hour", "Weekly", "Monthly"]) {
      expect(screen.getByText(name)).toBeTruthy();
    }
  });

  it("shows the peek and no drawer when a tile is pointed at", () => {
    render(<RingStrip accounts={accountsOf(kimi, claude)} />);
    fireEvent.pointerEnter(screen.getByRole("button", { name: /^Claude/ }));
    const foot = document.querySelector(".widget-foot");
    expect(foot?.classList.contains("peeking")).toBe(true);
    expect(foot?.querySelector(".widget-peek")?.textContent).toContain(
      "Claude · Fable weekly resets in 2h 0m",
    );
    expect(screen.queryByRole("region", { name: "Claude" })).toBeNull();
    expect(ipc.fitWidget).not.toHaveBeenCalled();
    expect(
      screen.getByRole("button", { name: /^Claude/ }).getAttribute("aria-expanded"),
    ).toBe("false");
  });

  it("returns the key after the pointer leaves the tiles", async () => {
    render(<RingStrip accounts={accountsOf(kimi, claude)} />);
    const tile = screen.getByRole("button", { name: /^Claude/ });
    fireEvent.pointerEnter(tile);
    fireEvent.pointerLeave(tile);
    await waitFor(() => {
      expect(document.querySelector(".widget-foot.peeking")).toBeNull();
    });
  });
});

describe("the widget window", () => {
  beforeEach(() => {
    Element.prototype.setPointerCapture = vi.fn();
  });

  const report = {
    openIssue: vi.fn(() => Promise.resolve()),
    copyPrompt: vi.fn(() => Promise.resolve(true)),
  };

  function widgetState(accounts: readonly AccountSnapshot[]): RendererState {
    return {
      snapshot: snapshot("instance", 1, [...accounts]),
      preferences: preferences({ view: "widget" }),
    } as unknown as RendererState;
  }

  function renderWidget(
    onExpand: () => void,
    accounts: readonly AccountSnapshot[] = [kimi, claude],
  ) {
    globalThis.ResizeObserver = class {
      observe(): void {}
      unobserve(): void {}
      disconnect(): void {}
    };
    return render(
      <Widget state={widgetState(accounts)} onExpand={onExpand} report={report} />,
    );
  }

  function press(target: Element, x: number, y: number, pointerId = 7): void {
    fireEvent.pointerDown(target, { button: 0, pointerId, clientX: x, clientY: y });
  }

  function move(target: Element, x: number, y: number, pointerId = 7): void {
    fireEvent.pointerMove(target, { pointerId, clientX: x, clientY: y, buttons: 1 });
  }

  function sleep(ms: number): Promise<unknown> {
    const { promise, resolve } = Promise.withResolvers<unknown>();
    window.setTimeout(resolve, ms);
    return promise;
  }

  it("keeps its own size fitted to its accounts", () => {
    renderWidget(vi.fn());
    expect(ipc.fitWidget).toHaveBeenCalled();
  });

  it("switches back to the full window from its corner button", () => {
    const onExpand = vi.fn();
    renderWidget(onExpand);
    fireEvent.click(screen.getByRole("button", { name: "Open the full window" }));
    expect(onExpand).toHaveBeenCalledOnce();
  });

  it("opens the drawer when an account is clicked, not the full window", async () => {
    const onExpand = vi.fn();
    renderWidget(onExpand);
    fireEvent.click(screen.getByRole("button", { name: /^Claude/ }));
    const drawer = await screen.findByRole("region", { name: "Claude" });
    expect(within(drawer).getByText("Fable weekly")).toBeTruthy();
    expect(onExpand).not.toHaveBeenCalled();
  });

  it("opens the drawer on a still click, without dragging", async () => {
    renderWidget(vi.fn());
    const tile = screen.getByRole("button", { name: /^Claude/ });
    press(tile, 50, 50);
    move(tile, 53, 52);
    fireEvent.pointerUp(tile);
    fireEvent.click(tile, { detail: 1 });
    expect(await screen.findByRole("region", { name: "Claude" })).toBeTruthy();
    expect(tile.getAttribute("aria-expanded")).toBe("true");
    expect(ipc.dragWidget).not.toHaveBeenCalled();
    expect(ipc.fitWidget).toHaveBeenLastCalledWith(224, "down");
  });

  it("drags on a 4 px move and swallows the click after it", async () => {
    renderWidget(vi.fn());
    const tile = screen.getByRole("button", { name: /^Claude/ });
    press(tile, 50, 50);
    move(tile, 54, 50);
    expect(ipc.dragWidget).toHaveBeenCalledOnce();
    move(tile, 60, 50);
    expect(ipc.dragWidget).toHaveBeenCalledOnce();
    fireEvent.click(tile, { detail: 1 });
    expect(screen.queryByRole("region", { name: "Claude" })).toBeNull();
    press(tile, 60, 50, 8);
    fireEvent.pointerUp(tile);
    fireEvent.click(tile, { detail: 1 });
    expect(await screen.findByRole("region", { name: "Claude" })).toBeTruthy();
  });

  it("drags from the drawer, the key line and the corner button", async () => {
    const onExpand = vi.fn();
    const view = renderWidget(onExpand);
    fireEvent.click(screen.getByRole("button", { name: /^Claude/ }));
    const drawer = await screen.findByRole("region", { name: "Claude" });
    const head = within(drawer).getByText("Claude");
    press(head, 50, 200);
    move(head, 54, 200);
    expect(ipc.dragWidget).toHaveBeenCalledOnce();
    fireEvent.pointerUp(head);
    fireEvent.click(screen.getByRole("button", { name: "Open the full window" }), {
      detail: 1,
    });
    expect(onExpand).not.toHaveBeenCalled();
    const foot = view.container.querySelector(".widget-foot");
    expect(foot).toBeTruthy();
    press(foot as Element, 50, 110);
    move(foot as Element, 50, 114);
    expect(ipc.dragWidget).toHaveBeenCalledTimes(2);
    fireEvent.pointerUp(foot as Element);
  });

  it("defers a resize while pressed and sends it when the press ends", async () => {
    const view = renderWidget(vi.fn());
    fireEvent.click(screen.getByRole("button", { name: /^Claude/ }));
    await screen.findByRole("region", { name: "Claude" });
    expect(ipc.fitWidget).toHaveBeenCalledTimes(2);
    const tile = screen.getByRole("button", { name: /^Claude/ });
    press(tile, 50, 50);
    const grown = {
      ...claude,
      windows: [...claude.windows, quotaWindow("cd", "daily", percent(90))],
    };
    view.rerender(
      <Widget state={widgetState([kimi, grown])} onExpand={vi.fn()} report={report} />,
    );
    await sleep(50);
    expect(ipc.fitWidget).toHaveBeenCalledTimes(2);
    fireEvent.pointerUp(tile);
    await waitFor(() => {
      expect(ipc.fitWidget).toHaveBeenCalledTimes(3);
    });
    expect(ipc.fitWidget).toHaveBeenLastCalledWith(244, "down");
  });

  it.each(["pointerup", "pointercancel"])(
    "ends a captured drag on %s at the window boundary",
    async (release) => {
      const view = renderWidget(vi.fn());
      const tile = screen.getByRole("button", { name: /^Claude/ });
      fireEvent.click(tile);
      await screen.findByRole("region", { name: "Claude" });
      press(tile, 50, 50);
      expect(tile.setPointerCapture).toHaveBeenCalledWith(7);
      move(tile, 50, 54);
      move(tile, 50, 58);
      expect(ipc.dragWidget).toHaveBeenCalledOnce();
      view.rerender(
        <Widget
          state={widgetState([
            kimi,
            {
              ...claude,
              windows: [...claude.windows, quotaWindow("cd", "daily", percent(90))],
            },
          ])}
          onExpand={vi.fn()}
          report={report}
        />,
      );
      await sleep(50);
      expect(ipc.fitWidget).toHaveBeenCalledTimes(2);
      fireEvent(window, new window.PointerEvent(release, { pointerId: 7 }));
      await waitFor(() => {
        expect(ipc.fitWidget).toHaveBeenLastCalledWith(244, "down");
      });
      move(tile, 50, 64);
      expect(ipc.dragWidget).toHaveBeenCalledOnce();
    },
  );

  it.each([116, 144])(
    "caps the drawer and rows for a host height of %i",
    async (height) => {
      renderWidget(vi.fn());
      ipc.fitWidget.mockResolvedValueOnce({
        height,
        direction: "down",
        room_above: 8,
        room_below: height - 116,
      });
      fireEvent.click(screen.getByRole("button", { name: /^Claude/ }));
      const drawer = await screen.findByRole("region", { name: "Claude" });
      await waitFor(() => {
        expect(drawer.style.height).toBe(`${Math.max(0, height - 120)}px`);
      });
      const rows = drawer.querySelector<HTMLElement>(".widget-drawer-rows.scroll");
      expect(rows?.style.maxHeight).toBe("0px");
    },
  );

  it("grows first and shrinks after, once per change", async () => {
    renderWidget(vi.fn(), [kimi, claude, openrouter]);
    fireEvent.click(screen.getByRole("button", { name: /^Claude/ }), { detail: 1 });
    await screen.findByRole("region", { name: "Claude" });
    fireEvent.click(screen.getByRole("button", { name: /^OpenRouter/ }), {
      detail: 1,
    });
    await screen.findByRole("region", { name: "OpenRouter" });
    await waitFor(() => {
      expect(ipc.fitWidget).toHaveBeenCalledTimes(3);
    });
    fireEvent.click(screen.getByRole("button", { name: /^OpenRouter/ }), {
      detail: 1,
    });
    await waitFor(() => {
      expect(screen.queryByRole("region", { name: "OpenRouter" })).toBeNull();
    });
    expect(ipc.fitWidget.mock.calls.map((call) => [call[0], call[1]])).toEqual([
      [0, "down"],
      [224, "down"],
      [184, "down"],
      [116, "down"],
    ]);
  });

  it("opens upward when the host has no room below", async () => {
    renderWidget(vi.fn());
    ipc.fitWidget.mockResolvedValueOnce({
      height: 224,
      direction: "up",
      room_above: 800,
      room_below: 10,
    });
    fireEvent.click(screen.getByRole("button", { name: /^Claude/ }), { detail: 1 });
    await screen.findByRole("region", { name: "Claude" });
    expect(document.querySelector(".widget-strip.up")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Close the details" }));
    await waitFor(() => {
      expect(screen.queryByRole("region", { name: "Claude" })).toBeNull();
    });
    expect(ipc.fitWidget).toHaveBeenLastCalledWith(116, "up");
  });

  it("keeps the drawer on its account across a re-rank and closes it when it leaves", async () => {
    const onExpand = vi.fn();
    const view = renderWidget(onExpand);
    fireEvent.click(screen.getByRole("button", { name: /^Claude/ }));
    await screen.findByRole("region", { name: "Claude" });
    view.rerender(
      <Widget state={widgetState([claude, kimi])} onExpand={onExpand} report={report} />,
    );
    expect(screen.getByRole("region", { name: "Claude" })).toBeTruthy();
    view.rerender(
      <Widget state={widgetState([kimi])} onExpand={onExpand} report={report} />,
    );
    await waitFor(() => {
      expect(screen.queryByRole("region", { name: "Claude" })).toBeNull();
    });
  });

  it("closes on Esc and puts focus back on the tile", async () => {
    renderWidget(vi.fn());
    const tile = screen.getByRole("button", { name: /^Claude/ });
    fireEvent.click(tile, { detail: 1 });
    await screen.findByRole("region", { name: "Claude" });
    fireEvent.keyDown(document, { key: "Escape" });
    await waitFor(() => {
      expect(screen.queryByRole("region", { name: "Claude" })).toBeNull();
    });
    expect(document.activeElement).toBe(tile);
  });

  it("ignores the second click of a double-click", async () => {
    renderWidget(vi.fn());
    const tile = screen.getByRole("button", { name: /^Claude/ });
    fireEvent.click(tile, { detail: 1 });
    await screen.findByRole("region", { name: "Claude" });
    fireEvent.click(tile, { detail: 2 });
    expect(screen.getByRole("region", { name: "Claude" })).toBeTruthy();
  });

  it("closes the drawer from its close button", async () => {
    renderWidget(vi.fn());
    fireEvent.click(screen.getByRole("button", { name: /^Claude/ }), { detail: 1 });
    await screen.findByRole("region", { name: "Claude" });
    fireEvent.click(screen.getByRole("button", { name: "Close the details" }));
    await waitFor(() => {
      expect(screen.queryByRole("region", { name: "Claude" })).toBeNull();
    });
  });

  it("closes when another application takes focus, never mid-press", async () => {
    vi.spyOn(document, "hasFocus").mockReturnValue(false);
    renderWidget(vi.fn());
    const tile = screen.getByRole("button", { name: /^Claude/ });
    fireEvent.click(tile, { detail: 1 });
    await screen.findByRole("region", { name: "Claude" });
    press(tile, 50, 50);
    fireEvent.blur(window);
    await sleep(200);
    expect(screen.getByRole("region", { name: "Claude" })).toBeTruthy();
    fireEvent.pointerUp(tile);
    fireEvent.blur(window);
    await waitFor(() => {
      expect(screen.queryByRole("region", { name: "Claude" })).toBeNull();
    });
  });

  it("moves focus with the keyboard and opens from it", async () => {
    renderWidget(vi.fn());
    const kimiTile = screen.getByRole("button", { name: /^Kimi/ });
    const claudeTile = screen.getByRole("button", { name: /^Claude/ });
    fireEvent.keyDown(kimiTile, { key: "Tab" });
    fireEvent.focus(kimiTile);
    expect(document.querySelector(".widget-foot.peeking")).toBeTruthy();
    fireEvent.keyDown(kimiTile, { key: "ArrowRight" });
    expect(document.activeElement).toBe(claudeTile);
    fireEvent.keyDown(claudeTile, { key: "Home" });
    expect(document.activeElement).toBe(kimiTile);
    fireEvent.keyDown(kimiTile, { key: "End" });
    expect(document.activeElement).toBe(claudeTile);
    fireEvent.click(claudeTile);
    expect(await screen.findByRole("region", { name: "Claude" })).toBeTruthy();
  });

  it("reports a bug from beside the expand button and confirms a copied prompt", async () => {
    renderWidget(vi.fn());
    fireEvent.click(screen.getByRole("button", { name: "Report a bug" }));
    fireEvent.click(
      screen.getByRole("menuitem", { name: "Copy a prompt for an AI agent" }),
    );
    expect(report.copyPrompt).toHaveBeenCalled();
    expect((await screen.findByRole("status")).textContent).toContain("Prompt copied");
    expect(screen.queryByRole("menu")).toBeNull();
  });
});
