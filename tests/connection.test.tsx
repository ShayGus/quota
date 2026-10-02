import { fireEvent, render, screen } from "@testing-library/react";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => undefined),
}));

import { actions as hostActions } from "../src/app/actions";
import { ConnectionWizard } from "../src/features/settings/ConnectionWizard";
import { Settings, type SettingsActions } from "../src/features/settings/Settings";
import type { ProviderId, VerifiedCandidate } from "../src/generated/bindings";
import {
  acceptAttempt,
  acceptPreferences,
  acceptSnapshot,
  getRendererState,
} from "../src/shared/state/store";
import { useRendererState } from "../src/shared/state/useRendererState";
import {
  account,
  candidate,
  percent,
  preferences,
  snapshot,
  window as quotaWindow,
} from "./fixtures";

function settingsActions(): SettingsActions {
  return {
    savePreferences: vi.fn(),
    setMonitoring: vi.fn(),
    savePollingPreferences: vi.fn(),
    setAlwaysOnTop: vi.fn(),
    setOverviewMode: vi.fn(),
    fitToAccounts: vi.fn(),
    resetPosition: vi.fn(),
    setAccountEnabled: vi.fn(),
    renameAccount: vi.fn(),
    disconnectAccount: vi.fn(),
    openUsagePage: vi.fn(),
    beginConnection: vi.fn(() => Promise.resolve({ id: "attempt-1" })),
    cancelConnection: vi.fn(() => Promise.resolve(undefined)),
    confirmConnection: vi.fn(() => Promise.resolve(true)),
    reconnectAccount: vi.fn(() => Promise.resolve(undefined)),
    clearHistory: vi.fn(),
    exportDiagnostics: vi.fn(() =>
      Promise.resolve("/data/diagnostics/quota-diagnostics-settings.json"),
    ),
    showAddAccount: vi.fn(),
    showOverview: vi.fn(),
    showAccountDetail: vi.fn(),
    launchAtLogin: vi.fn(() => Promise.resolve(false)),
    setLaunchAtLogin: vi.fn((launch: boolean) => Promise.resolve(launch)),
  };
}

/** The wizard as the popover shows it. */
function Wizard({
  actions,
  onDone,
}: {
  readonly actions: SettingsActions;
  readonly onDone: (added: boolean) => void;
}): React.ReactElement {
  const state = useRendererState();
  return <ConnectionWizard state={state} actions={actions} onDone={onDone} />;
}

/** The settings window, which still renders the wizard on its connection route. */
function SettingsHarness({
  actions,
}: {
  readonly actions: SettingsActions;
}): React.ReactElement {
  const state = useRendererState();
  return <Settings state={state} actions={actions} />;
}

/** The account that was there before every test. */
const existing = account("a", "claude", 1, []);

/** Reports that the host verified one attempt and holds its candidate. */
function hold(attemptId: string, provider: ProviderId, revision = 2): VerifiedCandidate {
  const held = candidate(provider, [quotaWindow("quota", "session", percent(72))]);
  act(() => {
    acceptAttempt({
      attemptId,
      revision,
      progress: { kind: "awaiting_confirmation", context: { candidate: held } },
    });
  });
  return held;
}

/** Picks a provider and presses Connect. */
async function connect(provider: string): Promise<void> {
  fireEvent.click(screen.getByRole("button", { name: new RegExp(`^${provider}`) }));
  await act(() => fireEvent.click(screen.getByRole("button", { name: "Connect" })));
}

const VERIFY = { name: "Is this the right account?" };

beforeEach(() => {
  window.location.hash = "";
  invoke.mockReset();
  acceptPreferences(preferences());
  acceptSnapshot(snapshot("instance-1", 1, [existing]));
});

describe("Provider → Connect → Verify", () => {
  it("shows only its own candidate, then confirms it under the chosen nickname", async () => {
    const actions = settingsActions();
    const onDone = vi.fn();
    render(<Wizard actions={actions} onDone={onDone} />);
    await connect("Codex");
    hold("someone-else", "codex");
    expect(screen.queryByRole("heading", VERIFY)).toBeNull();
    hold("attempt-1", "codex");
    expect(screen.getByRole("heading", VERIFY)).toBeTruthy();
    const facts = document.querySelector(".detail-list")?.textContent ?? "";
    expect(facts).toContain("new@example.test");
    expect(facts).toContain("1 window available");
    // Nothing is saved yet: the account list is unchanged.
    expect(getRendererState().snapshot?.accounts).toHaveLength(1);
    const add = screen.getByRole("button", { name: "Add account" });
    expect(add).toHaveProperty("disabled", true);
    fireEvent.change(screen.getByLabelText("Account nickname"), {
      target: { value: "  Work  " },
    });
    fireEvent.click(screen.getByRole("checkbox"));
    await act(() => fireEvent.click(add));
    expect(actions.confirmConnection).toHaveBeenCalledWith({ id: "attempt-1" }, "Work");
    expect(onDone).toHaveBeenCalledWith(true);
    expect(actions.cancelConnection).not.toHaveBeenCalled();
    expect(actions.disconnectAccount).not.toHaveBeenCalled();
    expect(actions.renameAccount).not.toHaveBeenCalled();
  });

  it("keeps the candidate on screen when the host refuses to save it", async () => {
    const actions = {
      ...settingsActions(),
      confirmConnection: vi.fn(() => Promise.resolve(false)),
    };
    const onDone = vi.fn();
    render(<Wizard actions={actions} onDone={onDone} />);
    await connect("Claude");
    hold("attempt-1", "claude");
    fireEvent.click(screen.getByRole("checkbox"));
    await act(() => fireEvent.click(screen.getByRole("button", { name: "Add account" })));
    expect(actions.confirmConnection).toHaveBeenCalledTimes(1);
    expect(onDone).not.toHaveBeenCalled();
    expect(screen.getByRole("heading", VERIFY)).toBeTruthy();
  });

  it("discards the candidate on Back and on Cancel, deleting nothing", async () => {
    const actions = settingsActions();
    const onDone = vi.fn();
    render(<Wizard actions={actions} onDone={onDone} />);
    await connect("Codex");
    hold("attempt-1", "codex");
    await act(() => fireEvent.click(screen.getByRole("button", { name: "Back" })));
    expect(actions.cancelConnection).toHaveBeenCalledWith({ id: "attempt-1" });
    expect(screen.getByRole("heading", { name: "Connect Codex" })).toBeTruthy();
    vi.mocked(actions.beginConnection).mockResolvedValueOnce({ id: "attempt-2" });
    await act(() => fireEvent.click(screen.getByRole("button", { name: "Connect" })));
    hold("attempt-2", "codex");
    await act(() => fireEvent.click(screen.getByRole("button", { name: "Cancel" })));
    expect(actions.cancelConnection).toHaveBeenLastCalledWith({ id: "attempt-2" });
    expect(onDone).toHaveBeenCalledWith(false);
    expect(actions.disconnectAccount).not.toHaveBeenCalled();
    expect(actions.confirmConnection).not.toHaveBeenCalled();
  });

  it("discards a held candidate when the wizard is dismissed any other way", async () => {
    const actions = settingsActions();
    const view = render(<Wizard actions={actions} onDone={vi.fn()} />);
    await connect("Codex");
    hold("attempt-1", "codex");
    // Escape, a fresh Add account, or navigating away all unmount the wizard.
    view.unmount();
    expect(actions.cancelConnection).toHaveBeenCalledWith({ id: "attempt-1" });
    expect(actions.disconnectAccount).not.toHaveBeenCalled();
  });

  it("cancels a running attempt when the wizard is dismissed mid-connection", async () => {
    const actions = settingsActions();
    const view = render(<Wizard actions={actions} onDone={vi.fn()} />);
    await connect("Claude");
    expect(screen.getByRole("button", { name: "Verifying…" })).toBeTruthy();
    view.unmount();
    expect(actions.cancelConnection).toHaveBeenCalledWith({ id: "attempt-1" });
  });

  it("does not cancel an attempt that already finished", async () => {
    const actions = settingsActions();
    const view = render(<Wizard actions={actions} onDone={vi.fn()} />);
    await connect("Codex");
    act(() => {
      acceptAttempt({
        attemptId: "attempt-1",
        revision: 2,
        progress: {
          kind: "failed",
          context: { error: { kind: "secure_store_unavailable" } },
        },
      });
    });
    view.unmount();
    expect(actions.cancelConnection).not.toHaveBeenCalled();
  });

  it("hides the pending identity behind an alias", async () => {
    const actions = settingsActions();
    render(<Wizard actions={actions} onDone={vi.fn()} />);
    await connect("Codex");
    hold("attempt-1", "codex");
    act(() => {
      acceptPreferences(
        preferences({
          privacy: {
            alias_mode: "stable_aliases",
            retain_history: false,
            export_identities: false,
          },
        }),
      );
    });
    const facts = document.querySelector(".detail-list")?.textContent ?? "";
    expect(facts).toContain("Account 0");
    expect(facts).toContain("Workspace hidden");
    expect(facts).not.toContain("new@example.test");
    expect(facts).not.toContain("Home");
  });

  it.each([
    ["Codex", "codex", "codex login"],
    ["Claude", "claude", "Run claude in a terminal"],
    ["OpenCode Go", "open_code_go", "Sign in with OpenCode"],
  ] as const)(
    "guides a first %s connection through its own sign-in tool",
    async (provider, id, tool) => {
      acceptSnapshot(snapshot("instance-1", 2, []));
      const actions = settingsActions();
      render(<Wizard actions={actions} onDone={vi.fn()} />);
      await connect(provider);
      act(() => {
        acceptAttempt({
          attemptId: "attempt-1",
          revision: 2,
          progress: {
            kind: "failed",
            context: { error: { kind: "reconnect_required" } },
          },
        });
      });
      const recovery = screen.getByRole("alert").textContent;
      expect(recovery).toContain(tool);
      expect(recovery).toContain("press Connect again");
      expect(screen.getByRole("button", { name: "Connect" })).toHaveProperty(
        "disabled",
        false,
      );
      vi.mocked(actions.beginConnection).mockResolvedValueOnce({ id: "attempt-2" });
      await act(() => fireEvent.click(screen.getByRole("button", { name: "Connect" })));
      expect(screen.queryByRole("alert")).toBeNull();
      hold("attempt-2", id);
      expect(screen.getByRole("heading", VERIFY)).toBeTruthy();
    },
  );

  it("preserves a candidate received before the begin command returns", async () => {
    const held = candidate("claude");
    invoke.mockImplementation(() => {
      acceptAttempt({
        attemptId: "attempt-1",
        revision: 2,
        progress: { kind: "awaiting_confirmation", context: { candidate: held } },
      });
      return Promise.resolve({
        attempt_ref: { id: "attempt-1" },
        attempt_id: "attempt-1",
      });
    });
    const actions: SettingsActions = {
      ...settingsActions(),
      beginConnection: async (request) => {
        const accepted = await hostActions.beginConnection(request);
        return accepted === null ? null : { id: accepted.attempt_id };
      },
    };
    render(<Wizard actions={actions} onDone={vi.fn()} />);
    await connect("Claude");
    expect(screen.getByRole("heading", VERIFY)).toBeTruthy();
    act(() => {
      acceptAttempt({
        attemptId: "attempt-1",
        revision: 1,
        progress: { kind: "started" },
      });
    });
    expect(screen.getByRole("heading", VERIFY)).toBeTruthy();
    expect(getRendererState().attempts[0]?.revision).toBe(2);
  });

  it("keeps a refused connection on Connect with a useful message", async () => {
    const actions = {
      ...settingsActions(),
      beginConnection: vi.fn(() => Promise.resolve(null)),
    };
    render(<Wizard actions={actions} onDone={vi.fn()} />);
    await connect("OpenCode Go");
    expect(screen.getByRole("alert").textContent).toContain("connection was refused");
    expect(screen.queryByRole("heading", VERIFY)).toBeNull();
  });
});

describe("the settings connection route", () => {
  it("starts a fresh wizard, discarding the old candidate, when the route is reopened", async () => {
    window.location.hash = "#/settings/connect/request-1";
    const actions = settingsActions();
    render(<SettingsHarness actions={actions} />);
    await connect("Claude");
    hold("attempt-1", "claude");
    expect(screen.getByRole("heading", VERIFY)).toBeTruthy();
    act(() => {
      window.location.hash = "#/settings/connect/request-2";
      fireEvent(window, new HashChangeEvent("hashchange"));
    });
    expect(screen.getByRole("heading", { name: "Add a subscription" })).toBeTruthy();
    expect(actions.cancelConnection).toHaveBeenCalledWith({ id: "attempt-1" });
  });

  it("returns to Accounts when the wizard finishes there", async () => {
    window.location.hash = "#/settings/connect/request-1";
    const actions = settingsActions();
    render(<SettingsHarness actions={actions} />);
    await connect("Claude");
    hold("attempt-1", "claude");
    fireEvent.click(screen.getByRole("checkbox"));
    await act(() => fireEvent.click(screen.getByRole("button", { name: "Add account" })));
    expect(window.location.hash).toMatch(/^#\/settings\/accounts\//);
  });
});
