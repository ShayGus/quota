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
import type { AccountSnapshot } from "../src/generated/bindings";
import {
  acceptAttempt,
  acceptPreferences,
  acceptSnapshot,
  getRendererState,
} from "../src/shared/state/store";
import { useRendererState } from "../src/shared/state/useRendererState";
import {
  account,
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
    reconnectAccount: vi.fn(() => Promise.resolve(undefined)),
    clearHistory: vi.fn(),
    exportDiagnostics: vi.fn(),
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
  readonly onDone: () => void;
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

/** A newly saved account of one provider. */
function added(provider: AccountSnapshot["provider_id"]): AccountSnapshot {
  return account("b", provider, 2, [quotaWindow("quota", "session", percent(72))]);
}

/** Reports one attempt as verified. */
function verify(attemptId: string, revision = 2): void {
  act(() => {
    acceptAttempt({
      attemptId,
      revision,
      progress: { kind: "verified", context: { state: "connected" } },
    });
  });
}

/** Publishes the snapshot that carries the new account. */
function save(provider: AccountSnapshot["provider_id"], revision = 2): void {
  act(() => {
    acceptSnapshot(snapshot("instance-1", revision, [existing, added(provider)]));
  });
}

const VERIFY = { name: "Is this the right account?" };

beforeEach(() => {
  window.location.hash = "";
  acceptPreferences(preferences());
  acceptSnapshot(snapshot("instance-1", 1, [existing]));
});

describe("Provider → Connect → Verify", () => {
  it("waits for its own verification and the new account, then confirms and names it", async () => {
    const actions = settingsActions();
    const onDone = vi.fn();
    render(<Wizard actions={actions} onDone={onDone} />);
    expect(screen.getByRole("heading", { name: "Add a subscription" })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: /^Claude/ }));
    expect(screen.getByRole("heading", { name: "Connect Claude" })).toBeTruthy();
    await act(() => fireEvent.click(screen.getByRole("button", { name: "Connect" })));
    expect(actions.beginConnection).toHaveBeenCalledWith({
      provider_id: "claude",
      nickname: "Personal",
      profile_label: null,
    });

    verify("another-attempt");
    expect(screen.queryByRole("heading", VERIFY)).toBeNull();

    // Verified, but the account has not arrived. The account that was already
    // there is never taken for it.
    verify("attempt-1");
    expect(screen.queryByRole("heading", VERIFY)).toBeNull();
    expect(screen.getByRole("status").textContent).toContain(
      "Waiting for the new account",
    );
    expect(screen.getByRole("button", { name: "Verifying…" })).toHaveProperty(
      "disabled",
      true,
    );

    save("claude");
    expect(screen.getByRole("heading", VERIFY)).toBeTruthy();
    const facts = document.querySelector(".detail-list")?.textContent ?? "";
    expect(facts).toContain("b@example.test");
    expect(facts).toContain("Home");
    expect(facts).toContain("1 window available");
    expect(facts).not.toContain("a@example.test");

    const add = screen.getByRole("button", { name: "Add account" });
    expect(add).toHaveProperty("disabled", true);
    fireEvent.change(screen.getByLabelText("Account nickname"), {
      target: { value: "Work" },
    });
    fireEvent.click(screen.getByRole("checkbox"));
    expect(add).toHaveProperty("disabled", false);
    fireEvent.click(add);
    expect(actions.renameAccount).toHaveBeenCalledWith("b", "Work");
    expect(actions.disconnectAccount).not.toHaveBeenCalled();
    expect(onDone).toHaveBeenCalledTimes(1);
  });

  it("does not rename an account whose nickname was kept", async () => {
    const actions = settingsActions();
    const onDone = vi.fn();
    render(<Wizard actions={actions} onDone={onDone} />);
    fireEvent.click(screen.getByRole("button", { name: /^Codex/ }));
    await act(() => fireEvent.click(screen.getByRole("button", { name: "Connect" })));
    verify("attempt-1");
    act(() => {
      acceptSnapshot(
        snapshot("instance-1", 2, [
          existing,
          account("b", "codex", 2, [], { nickname: "Personal" }),
        ]),
      );
    });
    fireEvent.click(screen.getByRole("checkbox"));
    fireEvent.click(screen.getByRole("button", { name: "Add account" }));
    expect(actions.renameAccount).not.toHaveBeenCalled();
    expect(onDone).toHaveBeenCalledTimes(1);
  });

  it("removes an unconfirmed account on Back and on Cancel", async () => {
    const actions = settingsActions();
    const onDone = vi.fn();
    render(<Wizard actions={actions} onDone={onDone} />);
    fireEvent.click(screen.getByRole("button", { name: /^Claude/ }));
    await act(() => fireEvent.click(screen.getByRole("button", { name: "Connect" })));
    verify("attempt-1");
    save("claude");
    expect(screen.getByRole("heading", VERIFY)).toBeTruthy();

    fireEvent.click(screen.getByRole("button", { name: "Back" }));
    expect(actions.disconnectAccount).toHaveBeenCalledWith("b");
    expect(screen.getByRole("heading", { name: "Connect Claude" })).toBeTruthy();
    expect(onDone).not.toHaveBeenCalled();

    vi.mocked(actions.beginConnection).mockResolvedValueOnce({ id: "attempt-2" });
    await act(() => fireEvent.click(screen.getByRole("button", { name: "Connect" })));
    verify("attempt-2");
    act(() => {
      acceptSnapshot(
        snapshot("instance-1", 3, [existing, account("c", "claude", 3, [])]),
      );
    });
    expect(screen.getByRole("heading", VERIFY)).toBeTruthy();
    await act(() => fireEvent.click(screen.getByRole("button", { name: "Cancel" })));
    expect(actions.disconnectAccount).toHaveBeenLastCalledWith("c");
    expect(actions.cancelConnection).not.toHaveBeenCalled();
    expect(onDone).toHaveBeenCalledTimes(1);
  });

  it("hides the verified identity behind its alias", async () => {
    const actions = settingsActions();
    render(<Wizard actions={actions} onDone={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: /^Codex/ }));
    await act(() => fireEvent.click(screen.getByRole("button", { name: "Connect" })));
    verify("attempt-1");
    save("codex");
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
    expect(facts).toContain("Account 2");
    expect(facts).toContain("Workspace hidden");
    expect(facts).not.toContain("b@example.test");
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
      fireEvent.click(screen.getByRole("button", { name: new RegExp(`^${provider}`) }));
      await act(() => fireEvent.click(screen.getByRole("button", { name: "Connect" })));
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
      expect(recovery).not.toContain("Reconnect that account");
      expect(screen.getByRole("button", { name: "Connect" })).toHaveProperty(
        "disabled",
        false,
      );
      expect(getRendererState().snapshot?.accounts).toHaveLength(0);
      vi.mocked(actions.beginConnection).mockResolvedValueOnce({ id: "attempt-2" });
      await act(() => fireEvent.click(screen.getByRole("button", { name: "Connect" })));
      expect(screen.queryByRole("alert")).toBeNull();
      expect(actions.beginConnection).toHaveBeenCalledTimes(2);
      verify("attempt-2");
      act(() => {
        acceptSnapshot(snapshot("instance-1", 3, [added(id)]));
      });
      expect(screen.getByRole("heading", VERIFY)).toBeTruthy();
    },
  );

  it("shows connection failures and cancels a running attempt by its identity", async () => {
    const actions = settingsActions();
    const onDone = vi.fn();
    render(<Wizard actions={actions} onDone={onDone} />);
    fireEvent.click(screen.getByRole("button", { name: /^Codex/ }));
    await act(() => fireEvent.click(screen.getByRole("button", { name: "Connect" })));
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
    expect(screen.getByRole("alert").textContent).toContain("key store is unavailable");
    vi.mocked(actions.beginConnection).mockResolvedValueOnce({ id: "attempt-2" });
    await act(() => fireEvent.click(screen.getByRole("button", { name: "Connect" })));
    await act(() => fireEvent.click(screen.getByRole("button", { name: "Cancel" })));
    expect(actions.cancelConnection).toHaveBeenCalledWith({ id: "attempt-2" });
    expect(actions.disconnectAccount).not.toHaveBeenCalled();
    expect(onDone).toHaveBeenCalledTimes(1);
  });

  it("preserves verification received before the begin command returns", async () => {
    invoke.mockImplementation(() => {
      acceptAttempt({
        attemptId: "attempt-1",
        revision: 2,
        progress: { kind: "verified", context: { state: "connected" } },
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
    fireEvent.click(screen.getByRole("button", { name: /^Claude/ }));
    await act(() => fireEvent.click(screen.getByRole("button", { name: "Connect" })));
    save("claude");
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
    fireEvent.click(screen.getByRole("button", { name: /^OpenCode Go/ }));
    await act(() => fireEvent.click(screen.getByRole("button", { name: "Connect" })));
    expect(screen.getByRole("alert").textContent).toContain("connection was refused");
    expect(screen.queryByRole("heading", VERIFY)).toBeNull();
  });
});

describe("the settings connection route", () => {
  it("starts a fresh wizard when the host reopens the connection route", async () => {
    window.location.hash = "#/settings/connect/request-1";
    const actions = settingsActions();
    const { container } = render(<SettingsHarness actions={actions} />);
    fireEvent.click(screen.getByRole("button", { name: /^Claude/ }));
    await act(() => fireEvent.click(screen.getByRole("button", { name: "Connect" })));
    verify("attempt-1");
    save("claude");
    expect(screen.getByRole("heading", VERIFY)).toBeTruthy();
    container.hidden = true;
    act(() => {
      window.location.hash = "#/settings/connect/request-2";
      fireEvent(window, new HashChangeEvent("hashchange"));
    });
    container.hidden = false;
    expect(screen.getByRole("heading", { name: "Add a subscription" })).toBeTruthy();
    expect(screen.queryByRole("heading", VERIFY)).toBeNull();
  });

  it("returns to Accounts when the wizard finishes there", async () => {
    window.location.hash = "#/settings/connect";
    const actions = settingsActions();
    render(<SettingsHarness actions={actions} />);
    fireEvent.click(screen.getByRole("button", { name: /^Claude/ }));
    await act(() => fireEvent.click(screen.getByRole("button", { name: "Connect" })));
    verify("attempt-1");
    save("claude");
    fireEvent.click(screen.getByRole("checkbox"));
    act(() => {
      fireEvent.click(screen.getByRole("button", { name: "Add account" }));
    });
    expect(window.location.hash).toBe("#/settings/accounts");
  });
});
