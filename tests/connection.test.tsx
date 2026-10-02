import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { act } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => undefined),
}));

import { actions as hostActions } from "../src/app/actions";
import { Settings, type SettingsActions } from "../src/features/settings/Settings";
import type {
  AttemptRef,
  ConnectionProgress,
  VerifiedCandidate,
} from "../src/generated/bindings";
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
    confirmConnection: vi.fn(() => Promise.resolve(true)),
    reconnectAccount: vi.fn(() => Promise.resolve(undefined)),
    clearHistory: vi.fn(),
    exportDiagnostics: vi.fn(),
  };
}

function Harness({ actions }: { readonly actions: SettingsActions }): React.ReactElement {
  const state = useRendererState();
  return <Settings state={state} actions={actions} />;
}

const REVIEW = "Is this the right account?";

/** The progress an attempt reports once it has verified one identity. */
function awaitingConfirmation(
  overrides: Partial<VerifiedCandidate> = {},
): ConnectionProgress {
  return {
    kind: "awaiting_confirmation",
    context: {
      candidate: {
        provider_id: "claude",
        nickname: "Work",
        identity: {
          principal_label: "b@example.test",
          workspace_label: "Home",
          plan_label: null,
          source: "documented_api",
        },
        windows: [quotaWindow("quota", "session", percent(72))],
        ...overrides,
      },
    },
  };
}

beforeEach(() => {
  window.location.hash = "#/settings/connect";
  acceptPreferences(preferences());
  acceptSnapshot(snapshot("instance-1", 1, [account("a", "claude", 1, [])]));
});

describe("Provider → Connect → Verify", () => {
  it("waits for its own verification event and saves only when Add account is pressed", async () => {
    const actions = settingsActions();
    render(<Harness actions={actions} />);
    fireEvent.click(screen.getByRole("button", { name: /^Claude/ }));
    expect(screen.getByRole("heading", { name: "Connect Claude" })).toBeTruthy();
    fireEvent.change(screen.getByLabelText("Account nickname"), {
      target: { value: "Work" },
    });
    await act(() => fireEvent.click(screen.getByRole("button", { name: "Connect" })));
    expect(actions.beginConnection).toHaveBeenCalledWith({
      provider_id: "claude",
      nickname: "Work",
      profile_label: null,
    });
    expect(screen.queryByRole("heading", { name: REVIEW })).toBeNull();
    act(() => {
      acceptAttempt({
        attemptId: "another-attempt",
        revision: 2,
        progress: awaitingConfirmation(),
      });
    });
    expect(screen.queryByRole("heading", { name: REVIEW })).toBeNull();
    act(() => {
      acceptAttempt({
        attemptId: "attempt-1",
        revision: 2,
        progress: awaitingConfirmation(),
      });
    });
    expect(screen.getByRole("heading", { name: REVIEW })).toBeTruthy();
    expect(
      screen.getByText(/Confirm the identity before adding this subscription/),
    ).toBeTruthy();
    expect(screen.getByText("b@example.test")).toBeTruthy();
    expect(screen.getByText("Home")).toBeTruthy();
    expect(screen.getByText(/72% remaining \(just verified\)/)).toBeTruthy();
    expect(actions.confirmConnection).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Add account" }));
    await waitFor(() => {
      expect(actions.confirmConnection).toHaveBeenCalledWith({ id: "attempt-1" });
    });
    await waitFor(() => {
      expect(screen.getByRole("article", { name: "Manage a" })).toBeTruthy();
    });
  });

  it("stays on the review step when saving the candidate is refused", async () => {
    const actions = {
      ...settingsActions(),
      confirmConnection: vi.fn(() => Promise.resolve(false)),
    };
    render(<Harness actions={actions} />);
    fireEvent.click(screen.getByRole("button", { name: /^Claude/ }));
    await act(() => fireEvent.click(screen.getByRole("button", { name: "Connect" })));
    act(() => {
      acceptAttempt({
        attemptId: "attempt-1",
        revision: 2,
        progress: awaitingConfirmation(),
      });
    });
    fireEvent.click(screen.getByRole("button", { name: "Add account" }));
    await waitFor(() => {
      expect(actions.confirmConnection).toHaveBeenCalledTimes(1);
    });
    expect(screen.getByRole("heading", { name: REVIEW })).toBeTruthy();
  });

  it("discards a verified candidate on Cancel without confirming it", async () => {
    const actions = settingsActions();
    render(<Harness actions={actions} />);
    fireEvent.click(screen.getByRole("button", { name: /^Claude/ }));
    await act(() => fireEvent.click(screen.getByRole("button", { name: "Connect" })));
    act(() => {
      acceptAttempt({
        attemptId: "attempt-1",
        revision: 2,
        progress: awaitingConfirmation(),
      });
    });
    expect(screen.getByRole("heading", { name: REVIEW })).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => {
      expect(actions.cancelConnection).toHaveBeenCalledWith({ id: "attempt-1" });
    });
    expect(actions.confirmConnection).not.toHaveBeenCalled();
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Add account" })).toBeTruthy();
    });
  });

  it.each([true, false])(
    "hides identity and nickname when confirmation returns %s with aliases on",
    async (saved) => {
      const confirmation = Promise.withResolvers<boolean>();
      const actions = {
        ...settingsActions(),
        confirmConnection: vi.fn(() => confirmation.promise),
      };
      const { container } = render(<Harness actions={actions} />);
      fireEvent.click(screen.getByRole("button", { name: /^Claude/ }));
      fireEvent.change(screen.getByLabelText("Account nickname"), {
        target: { value: "Captain's private nickname" },
      });
      await act(() => fireEvent.click(screen.getByRole("button", { name: "Connect" })));
      act(() => {
        acceptAttempt({
          attemptId: "attempt-1",
          revision: 2,
          progress: awaitingConfirmation({ nickname: "Captain's private nickname" }),
        });
      });
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
      expect(screen.queryByText("b@example.test")).toBeNull();
      expect(screen.queryByText("Home")).toBeNull();
      expect(screen.getByText("Account 0")).toBeTruthy();
      expect(screen.getByText("Workspace hidden")).toBeTruthy();
      expect(container.textContent).not.toContain("Captain's private nickname");
      expect(screen.queryByLabelText("Account nickname")).toBeNull();
      fireEvent.click(screen.getByRole("button", { name: "Add account" }));
      act(() => {
        acceptAttempt({
          attemptId: "attempt-1",
          revision: 3,
          progress: { kind: "verified", context: { state: "connected" } },
        });
      });
      expect(screen.queryByRole("heading", { name: "Account added" })).toBeNull();
      expect(screen.queryByLabelText("Account nickname")).toBeNull();
      expect(container.textContent).not.toContain("Captain's private nickname");
      await act(async () => {
        confirmation.resolve(saved);
        await confirmation.promise;
      });
      if (saved) {
        await waitFor(() => {
          expect(screen.getByRole("article", { name: "Manage Account 1" })).toBeTruthy();
        });
      } else {
        expect(screen.queryByRole("heading", { name: "Account added" })).toBeNull();
        expect(screen.queryByLabelText("Account nickname")).toBeNull();
        expect(container.textContent).not.toContain("Captain's private nickname");
      }
    },
  );

  it("starts a fresh wizard when Add reopens settings on the same connection section", async () => {
    window.location.hash = "#/settings/connect/request-1";
    const actions = settingsActions();
    const { container } = render(<Harness actions={actions} />);
    fireEvent.click(screen.getByRole("button", { name: /^Claude/ }));
    fireEvent.change(screen.getByLabelText("Account nickname"), {
      target: { value: "Work" },
    });
    await act(() => fireEvent.click(screen.getByRole("button", { name: "Connect" })));
    act(() => {
      acceptAttempt({
        attemptId: "attempt-1",
        revision: 2,
        progress: awaitingConfirmation(),
      });
    });
    expect(screen.getByRole("heading", { name: REVIEW })).toBeTruthy();
    container.hidden = true;
    act(() => {
      window.location.hash = "#/settings/connect/request-2";
      fireEvent(window, new HashChangeEvent("hashchange"));
    });
    container.hidden = false;
    expect(actions.cancelConnection).toHaveBeenCalledExactlyOnceWith({
      id: "attempt-1",
    });
    expect(screen.getByRole("heading", { name: "Add a subscription" })).toBeTruthy();
    expect(screen.queryByRole("heading", { name: REVIEW })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: /^Codex/ }));
    expect(screen.getByLabelText("Account nickname")).toHaveProperty("value", "Personal");
    vi.mocked(actions.beginConnection).mockResolvedValueOnce({ id: "attempt-2" });
    await act(() => fireEvent.click(screen.getByRole("button", { name: "Connect" })));
    act(() => {
      acceptAttempt({
        attemptId: "attempt-1",
        revision: 3,
        progress: awaitingConfirmation(),
      });
    });
    expect(screen.queryByRole("heading", { name: REVIEW })).toBeNull();
    expect(screen.getByRole("heading", { name: "Connect Codex" })).toBeTruthy();
  });

  it.each(["started", "awaiting_confirmation"] as const)(
    "cancels a %s attempt when the wizard unmounts",
    async (kind) => {
      const actions = settingsActions();
      const settings = render(<Harness actions={actions} />);
      fireEvent.click(screen.getByRole("button", { name: /^Claude/ }));
      await act(() => fireEvent.click(screen.getByRole("button", { name: "Connect" })));
      act(() => {
        acceptAttempt({
          attemptId: "attempt-1",
          revision: 2,
          progress: kind === "started" ? { kind } : awaitingConfirmation(),
        });
      });
      settings.unmount();
      expect(actions.cancelConnection).toHaveBeenCalledExactlyOnceWith({
        id: "attempt-1",
      });
      expect(actions.confirmConnection).not.toHaveBeenCalled();
    },
  );

  it("cancels a late begin reply after a fresh route replaces the wizard", async () => {
    const accepted = Promise.withResolvers<AttemptRef | null>();
    const actions = {
      ...settingsActions(),
      beginConnection: vi.fn(() => accepted.promise),
    };
    render(<Harness actions={actions} />);
    fireEvent.click(screen.getByRole("button", { name: /^Claude/ }));
    fireEvent.click(screen.getByRole("button", { name: "Connect" }));
    act(() => {
      window.location.hash = "#/settings/connect/request-2";
      fireEvent(window, new HashChangeEvent("hashchange"));
    });
    expect(actions.cancelConnection).not.toHaveBeenCalled();
    await act(async () => {
      accepted.resolve({ id: "late-attempt" });
      await accepted.promise;
    });
    expect(actions.cancelConnection).toHaveBeenCalledExactlyOnceWith({
      id: "late-attempt",
    });
    expect(screen.getByRole("heading", { name: "Add a subscription" })).toBeTruthy();
    expect(actions.confirmConnection).not.toHaveBeenCalled();
  });

  it("keeps the fresh route when an abandoned confirmation finishes", async () => {
    const confirmed = Promise.withResolvers<boolean>();
    const actions = {
      ...settingsActions(),
      confirmConnection: vi.fn(() => confirmed.promise),
    };
    render(<Harness actions={actions} />);
    fireEvent.click(screen.getByRole("button", { name: /^Claude/ }));
    await act(() => fireEvent.click(screen.getByRole("button", { name: "Connect" })));
    act(() => {
      acceptAttempt({
        attemptId: "attempt-1",
        revision: 2,
        progress: awaitingConfirmation(),
      });
    });
    fireEvent.click(screen.getByRole("button", { name: "Add account" }));
    act(() => {
      window.location.hash = "#/settings/connect/request-2";
      fireEvent(window, new HashChangeEvent("hashchange"));
    });
    await act(async () => {
      confirmed.resolve(true);
      await confirmed.promise;
    });
    expect(screen.getByRole("heading", { name: "Add a subscription" })).toBeTruthy();
  });

  it("treats cleanup of an already finished attempt as harmless", async () => {
    invoke.mockRejectedValue({ kind: "cancelled" });
    await hostActions.cancelConnection({ id: "finished-attempt" });
    expect(invoke).toHaveBeenCalledWith("cancel_connection", {
      attemptRef: { id: "finished-attempt" },
    });
    expect(getRendererState().failure).toBeNull();
  });

  it("reports cancellation failures other than an already finished attempt", async () => {
    invoke.mockRejectedValue({
      kind: "permission_denied",
      context: {
        window_label: "settings",
      },
    });
    await hostActions.cancelConnection({ id: "attempt-1" });
    expect(getRendererState().failure).toEqual({
      kind: "domain",
      error: { kind: "permission_denied", context: { window_label: "settings" } },
    });
  });

  it.each([
    ["Codex", "codex login"],
    ["Claude", "Run claude in a terminal"],
    ["OpenCode Go", "Sign in with OpenCode"],
  ])(
    "guides a first %s connection through its own sign-in tool",
    async (provider, tool) => {
      acceptSnapshot(snapshot("instance-1", 2, []));
      const actions = settingsActions();
      render(<Harness actions={actions} />);
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
      act(() => {
        acceptAttempt({
          attemptId: "attempt-2",
          revision: 2,
          progress: awaitingConfirmation(),
        });
      });
      expect(screen.getByRole("heading", { name: REVIEW })).toBeTruthy();
    },
  );

  it("shows connection failures and cancels a running attempt by its identity", async () => {
    const actions = settingsActions();
    render(<Harness actions={actions} />);
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
  });

  it("preserves verification received before the begin command returns", async () => {
    invoke.mockImplementation(() => {
      acceptAttempt({
        attemptId: "attempt-1",
        revision: 2,
        progress: awaitingConfirmation(),
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
    render(<Harness actions={actions} />);
    fireEvent.click(screen.getByRole("button", { name: /^Claude/ }));
    await act(() => fireEvent.click(screen.getByRole("button", { name: "Connect" })));
    expect(screen.getByRole("heading", { name: REVIEW })).toBeTruthy();
    act(() => {
      acceptAttempt({
        attemptId: "attempt-1",
        revision: 1,
        progress: { kind: "started" },
      });
    });
    expect(screen.getByRole("heading", { name: REVIEW })).toBeTruthy();
    expect(getRendererState().attempts[0]?.revision).toBe(2);
  });

  it("keeps a refused connection on Connect with a useful message", async () => {
    const actions = {
      ...settingsActions(),
      beginConnection: vi.fn(() => Promise.resolve(null)),
    };
    render(<Harness actions={actions} />);
    fireEvent.click(screen.getByRole("button", { name: /^OpenCode Go/ }));
    await act(() => fireEvent.click(screen.getByRole("button", { name: "Connect" })));
    expect(screen.getByRole("alert").textContent).toContain("connection was refused");
    expect(screen.queryByRole("heading", { name: REVIEW })).toBeNull();
  });
});

describe("manage accounts navigation", () => {
  it.each([
    ["General", "general"],
    ["Appearance", "appearance"],
    ["Notifications", "notifications"],
    ["Privacy", "privacy"],
    ["Diagnostics", "diagnostics"],
  ])("keeps the plain hash when requesting %s", async (label, tab) => {
    window.history.replaceState(null, "", "#/settings/accounts");
    render(<Harness actions={settingsActions()} />);
    fireEvent.click(screen.getByRole("button", { name: label }));
    await waitFor(() => {
      expect(window.location.hash).toBe(`#/settings/${tab}`);
      expect(
        screen.getByRole("button", { name: label }).getAttribute("aria-current"),
      ).toBe("page");
    });
    fireEvent.click(screen.getByRole("button", { name: label }));
    expect(window.location.hash).toBe(`#/settings/${tab}`);
  });

  it("shows the management list again when Accounts is requested after a detail view", () => {
    const actions = settingsActions();
    render(<Harness actions={actions} />);
    act(() => {
      window.location.hash = "#/settings/accounts/1";
      fireEvent(window, new HashChangeEvent("hashchange"));
    });
    fireEvent.click(screen.getByRole("button", { name: "Details" }));
    expect(screen.getByRole("button", { name: "Manage accounts" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Add account" })).toBeNull();

    act(() => {
      window.location.hash = "#/settings/accounts/2";
      fireEvent(window, new HashChangeEvent("hashchange"));
    });
    expect(screen.getByRole("button", { name: "Add account" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Manage accounts" })).toBeNull();
  });

  it("reopens the management list from a route that never changed", async () => {
    const actions = settingsActions();
    const settings = render(<Harness actions={actions} />);
    act(() => {
      window.location.hash = "#/settings/accounts/1";
      fireEvent(window, new HashChangeEvent("hashchange"));
    });
    fireEvent.click(screen.getByRole("button", { name: "Details" }));
    expect(screen.queryByRole("button", { name: "Add account" })).toBeNull();

    // The Accounts section is requested again from inside the settings window,
    // which the nav does by asking for a new route rather than the old hash.
    fireEvent.click(screen.getByRole("button", { name: "Accounts" }));
    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Add account" })).toBeTruthy();
    });
    expect(window.location.hash).toMatch(/^#\/settings\/accounts\/\d+$/);
    settings.unmount();
  });
});
