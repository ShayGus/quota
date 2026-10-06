/**
 * Realistic host states for the interface tests.
 *
 * Built from the same typed helpers as the unit-test fixtures, so a change to
 * the generated contract breaks these at compile time. Every account address
 * uses the reserved `example.test` domain; no fixture carries a real credential.
 */
import type {
  AccountSnapshot,
  AppSnapshot,
  OverviewWindowState,
  Preferences,
  ProviderCapabilities,
  ProviderId,
  RegisteredProvider,
} from "../../src/generated/bindings";
import {
  account,
  candidate,
  keyLimit,
  percent,
  prepaidBalance,
  preferences as basePreferences,
  snapshot as baseSnapshot,
  unavailable,
  window as quotaWindow,
} from "../fixtures";
import type { ConnectionScript, FakeConfig, WindowLabel } from "./fake-backend";

export { NOW } from "../fixtures";

const INSTANCE = "ui-test-instance";

/** Five-hour and weekly windows, as the subscription providers report them. */
function sessionAndWeekly(prefix: string, session: number, weekly: number) {
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

/**
 * An OpenRouter account as the host shows it: a prepaid balance of $37.20 out
 * of a $50.00 top-up, with its key's monthly spend limit, which is hidden until
 * the person chooses to show it.
 */
export function openRouterAccount(
  options: { readonly showKeyLimit?: boolean } = {},
): AccountSnapshot {
  const { window, summary } = prepaidBalance();
  return account("acct-openrouter", "openrouter", 4, [window, keyLimit()], {
    nickname: "Side project",
    rank: 74.4,
    balance: summary,
    showKeyLimit: options.showKeyLimit ?? false,
  });
}

/**
 * A TypeSafe console account: a credit balance made of two grants, a free one
 * that runs out first and a purchased one, all fictional.
 */
export function typeSafeAccount(): AccountSnapshot {
  const { window, summary } = prepaidBalance({
    balance: 3720,
    baseline: 5000,
    baselineKind: "credits",
    topUps: [],
    runway: { spend_per_day_minor: 310, days_left: 12 },
    credits: [
      {
        kind: "free",
        amount_minor: 1000,
        remaining_minor: 420,
        expires_at: "2026-10-31T12:00:00.000Z",
      },
      {
        kind: "purchased",
        amount_minor: 4000,
        remaining_minor: 3300,
        expires_at: "2027-09-28T12:00:00.000Z",
      },
    ],
    cycleSpend: { label: "October 2026", spent_minor: 1280 },
  });
  const balance = {
    ...window,
    id: "ts-balance",
    source: "observed_web_endpoint" as const,
  };
  return account("acct-typesafe", "typesafe", 8, [balance], {
    nickname: "TypeSafe",
    rank: 74.4,
    balance: { ...summary, loaded_minor: 5000, spent_minor: 1280, key_spend: null },
  });
}

/**
 * The README's overview: a Codex account running low, a Claude account with
 * room left, and an OpenRouter prepaid balance.
 */
export function heroAccounts(): AccountSnapshot[] {
  const [claude, codex] = overviewAccounts();
  return [claude, codex, openRouterAccount()].filter(
    (entry): entry is AccountSnapshot => entry !== undefined,
  );
}

/** The seven accounts the overview scenario shows, one per interesting state. */
export function overviewAccounts(): AccountSnapshot[] {
  const healthy = account("acct-claude", "claude", 1, sessionAndWeekly("c", 82, 61), {
    nickname: "Work Claude",
    rank: 61,
  });
  const low = account("acct-codex", "codex", 2, sessionAndWeekly("x", 8, 35), {
    nickname: "Personal Codex",
    rank: 8,
  });
  const limited = {
    ...account("acct-cursor", "cursor", 3, sessionAndWeekly("u", 40, 40), {
      nickname: "Studio Cursor",
      rank: 40,
    }),
    fetch_state: "backoff" as const,
    next_attempt_at: "2026-10-01T12:05:00.000Z",
  };
  const offline = { ...openRouterAccount(), fetch_state: "offline" as const };
  const failed = {
    ...account("acct-zai", "zai", 5, sessionAndWeekly("z", 55, 55), {
      nickname: "Team Z.ai",
      rank: 55,
    }),
    fetch_state: "error" as const,
  };
  const reconnect = account("acct-kimi", "kimi", 6, [], {
    nickname: "Old Kimi",
    connectionState: "reauthentication_required",
    unrankedReason: "reconnect_required",
  });
  const disabled = account("acct-grok", "grok", 7, sessionAndWeekly("g", 90, 90), {
    nickname: "Parked Grok",
    monitoringEnabled: false,
    unrankedReason: "disabled",
  });
  return [healthy, low, limited, offline, failed, reconnect, disabled];
}

/** An account whose reading the provider could not give. */
export function unavailableAccount(): AccountSnapshot {
  return account(
    "acct-minimax",
    "minimax",
    8,
    [quotaWindow("m-week", "weekly", unavailable(), { label: "Weekly" })],
    { nickname: "Unreported" },
  );
}

/** The confirmed preferences a first run starts from, with the popover docked. */
export function defaultPreferences(overrides: Partial<Preferences> = {}): Preferences {
  return basePreferences({ overview_mode: "tray", theme: "dark", ...overrides });
}

const CONFIRMED_WINDOW: OverviewWindowState = {
  kind: "confirmed",
  value: { mode: "tray", always_on_top: false, visible: true, geometry_revision: 1 },
};

function capabilities(provider_id: ProviderId): ProviderCapabilities {
  return {
    provider_id,
    cardinality: "independent",
    supports_app_owned_authorization: true,
    supports_external_profile: true,
    reports_monthly_window: true,
    minimum_interval_seconds: 300,
  };
}

/** Every provider the wizard offers, compiled in. */
export function providers(): RegisteredProvider[] {
  return (
    [
      "codex",
      "claude",
      "open_code_go",
      "cursor",
      "openrouter",
      "zai",
      "minimax",
      "kimi",
      "grok",
      "muse_code",
      "ollama_cloud",
      "typesafe",
    ] as const
  ).map((id) => ({
    provider_id: id,
    capabilities: capabilities(id),
    compiled_in_this_build: true,
  }));
}

/** A scripted successful sign-in: verified identity, held for confirmation. */
export function verifiedConnection(): ConnectionScript {
  return {
    progress: [
      { kind: "started" },
      {
        kind: "awaiting_confirmation",
        context: {
          candidate: candidate(
            "codex",
            sessionAndWeekly("n", 90, 75),
            "new.person@example.test",
          ),
        },
      },
    ],
  };
}

/** Builds a complete host configuration for one window. */
export function scenario(
  window: WindowLabel,
  options: {
    readonly accounts?: readonly AccountSnapshot[];
    readonly preferences?: Preferences;
    readonly snapshot?: Partial<AppSnapshot>;
    readonly connection?: ConnectionScript | null;
    readonly update?: FakeConfig["update"];
    readonly launchAtLogin?: boolean;
    readonly refuse?: FakeConfig["refuse"];
  } = {},
): FakeConfig {
  const accounts = options.accounts ?? overviewAccounts();
  return {
    window,
    snapshot: { ...baseSnapshot(INSTANCE, 1, accounts), ...options.snapshot },
    preferences: options.preferences ?? defaultPreferences(),
    overviewWindow: CONFIRMED_WINDOW,
    providers: providers(),
    launchAtLogin: options.launchAtLogin ?? false,
    connection: options.connection ?? null,
    update: options.update ?? null,
    refuse: options.refuse ?? {},
  };
}
