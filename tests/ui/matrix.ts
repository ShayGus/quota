/**
 * The coverage matrix for the application's display modes.
 *
 * Quota has two independent choices that decide what is on screen, both
 * confirmed preferences:
 *
 * - `view` (`AppView`): the full window (`overview`) or the mini widget
 *   (`widget`). They never show together.
 * - `overview_mode` (`OverviewMode`): whether the full window is docked to the
 *   tray (`tray`) or a floating window (`floating`). The widget ignores it.
 * - `indicator_style` (`IndicatorStyle`): rings or bars. In the full window it
 *   picks the card layout. In the widget it picks the look: rings give the ring
 *   strip, bars give the mini cards. There is no third widget variant.
 *
 * So the real surfaces are the six below. The owner's names map to them as:
 * "Full window" is the four `full-*` surfaces, "Floating bar" is the ring strip
 * (`widget-strip`), and "Floating cards" is the mini cards (`widget-cards`).
 */
import type {
  AccountSnapshot,
  Preferences,
  ProviderId,
} from "../../src/generated/bindings";
import { account, percent, window as quotaWindow } from "../fixtures";

/** One way the application can be on screen. */
export interface Surface {
  readonly id: string;
  /** The webview the renderer runs in. */
  readonly window: "overview" | "widget";
  readonly view: Preferences["view"];
  readonly mode: Preferences["overview_mode"];
  readonly style: Preferences["indicator_style"];
  /** What the owner calls it, for the coverage table. */
  readonly name: string;
}

export const SURFACES: readonly Surface[] = [
  {
    id: "full-tray-ring",
    window: "overview",
    view: "overview",
    mode: "tray",
    style: "ring",
    name: "Full window, docked, rings",
  },
  {
    id: "full-tray-bar",
    window: "overview",
    view: "overview",
    mode: "tray",
    style: "bar",
    name: "Full window, docked, bars",
  },
  {
    id: "full-floating-ring",
    window: "overview",
    view: "overview",
    mode: "floating",
    style: "ring",
    name: "Full window, floating, rings",
  },
  {
    id: "full-floating-bar",
    window: "overview",
    view: "overview",
    mode: "floating",
    style: "bar",
    name: "Full window, floating, bars",
  },
  {
    id: "widget-strip",
    window: "widget",
    view: "widget",
    mode: "tray",
    style: "ring",
    name: "Floating bar (ring strip)",
  },
  {
    id: "widget-cards",
    window: "widget",
    view: "widget",
    mode: "tray",
    style: "bar",
    name: "Floating cards (mini cards)",
  },
];

/** The account counts: none, one, two, three (odd), and seven. */
export const COUNTS = [0, 1, 2, 3, 7] as const;

/** The states an account can be in that the interface draws differently. */
export const STATES = [
  "healthy",
  "low",
  "rate limited",
  "offline",
  "check failed",
  "reconnect needed",
] as const;
export type AccountState = (typeof STATES)[number];

export const THEMES = ["light", "dark"] as const;

/** A name no real fixture contains, so a leak is easy to search for. */
export const NICKNAME_MARK = "Nick";

function windows(prefix: string, session: number, weekly: number) {
  return [
    quotaWindow(`${prefix}-5h`, "session", percent(session), {
      label: "5-hour",
      boundaryAt: "2026-10-01T14:00:00.000Z",
    }),
    quotaWindow(`${prefix}-week`, "weekly", percent(weekly), {
      label: "Weekly",
      boundaryAt: "2026-10-05T09:00:00.000Z",
    }),
  ];
}

/** One account in one state. */
export function accountIn(
  state: AccountState,
  provider: ProviderId,
  ordinal: number,
  nickname = `${NICKNAME_MARK}-${provider}`,
): AccountSnapshot {
  const id = `acct-${provider}-${String(ordinal)}`;
  const options = { nickname };
  switch (state) {
    case "healthy":
      return account(id, provider, ordinal, windows(id, 82, 61), {
        ...options,
        rank: 61,
      });
    case "low":
      return account(id, provider, ordinal, windows(id, 8, 35), { ...options, rank: 8 });
    case "rate limited":
      return {
        ...account(id, provider, ordinal, windows(id, 40, 40), { ...options, rank: 40 }),
        fetch_state: "backoff",
        next_attempt_at: "2026-10-01T12:05:00.000Z",
      };
    case "offline":
      return {
        ...account(id, provider, ordinal, windows(id, 70, 70), { ...options, rank: 70 }),
        fetch_state: "offline",
      };
    case "check failed":
      return {
        ...account(id, provider, ordinal, windows(id, 55, 55), { ...options, rank: 55 }),
        fetch_state: "error",
      };
    case "reconnect needed":
      return account(id, provider, ordinal, [], {
        ...options,
        connectionState: "reauthentication_required",
        unrankedReason: "reconnect_required",
      });
  }
}

/** Seven different providers, so no account needs its nickname to be told apart. */
const PROVIDER_ORDER: readonly ProviderId[] = [
  "claude",
  "codex",
  "cursor",
  "openrouter",
  "zai",
  "kimi",
  "minimax",
];

/** The first `count` of seven accounts that cycle through every state. */
export function mixedAccounts(count: number): AccountSnapshot[] {
  const states: readonly AccountState[] = [...STATES, "healthy"];
  return PROVIDER_ORDER.slice(0, count).map((provider, index) =>
    accountIn(states[index] ?? "healthy", provider, index + 1),
  );
}

/** The preferences a surface needs. */
export function surfacePreferences(
  surface: Surface,
  base: Preferences,
  overrides: Partial<Preferences> = {},
): Preferences {
  return {
    ...base,
    view: surface.view,
    overview_mode: surface.mode,
    indicator_style: surface.style,
    ...overrides,
  };
}
