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
import type {
  AttemptRef,
  ProviderId,
  VerifiedCandidate,
} from "../src/generated/bindings";
import {
  acceptAttempt,
  acceptPreferences,
  acceptSnapshot,
  getRendererState,
  setFailure,
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
    setAppView: vi.fn(),
    setOverviewMode: vi.fn(),
    setAccountEnabled: vi.fn(),
    renameAccount: vi.fn(),
    setKeyLimitShown: vi.fn(),
    setGroupSpendShown: vi.fn(),
    setGroupKeyShown: vi.fn(),
    createAccountGroup: vi.fn(),
    setAccountGroup: vi.fn(),
    renameAccountGroup: vi.fn(),
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
    openBugReportIssue: vi.fn(() => Promise.resolve()),
    copyBugReportPrompt: vi.fn(() => Promise.resolve(true)),
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

const VERIFY = { name: "Add this account?" };

/** The Verify step's primary action, which names the provider. */
const ADD = { name: /^Add .+ account$/ };

beforeEach(() => {
  window.location.hash = "";
  invoke.mockReset();
  setFailure(null);
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
    const facts = document.querySelector(".verified-account")?.textContent ?? "";
    expect(facts).toContain("new@example.test");
    // The reading being approved, not just a count of windows.
    expect(facts).toContain("5-hour · Subscription");
    expect(facts).toContain("72% remaining");
    // Nothing is saved yet: the account list is unchanged.
    expect(getRendererState().snapshot?.accounts).toHaveLength(1);
    // One decision, no checkbox: the button names what it adds, and only an
    // empty nickname holds it back.
    const add = screen.getByRole("button", ADD);
    expect(add).toHaveProperty("disabled", false);
    fireEvent.change(screen.getByLabelText("Account nickname"), {
      target: { value: "  " },
    });
    expect(add).toHaveProperty("disabled", true);
    fireEvent.change(screen.getByLabelText("Account nickname"), {
      target: { value: "  Work  " },
    });
    await act(() => fireEvent.click(add));
    expect(actions.confirmConnection).toHaveBeenCalledWith(
      { id: "attempt-1" },
      "Work",
      null,
    );
    expect(onDone).toHaveBeenCalledWith(true);
    expect(actions.cancelConnection).not.toHaveBeenCalled();
    expect(actions.disconnectAccount).not.toHaveBeenCalled();
    expect(actions.renameAccount).not.toHaveBeenCalled();
  });

  it("leaves once the host reports the account saved, even if the reply was lost", async () => {
    const actions = {
      ...settingsActions(),
      confirmConnection: vi.fn(() => Promise.resolve(false)),
    };
    const onDone = vi.fn();
    render(<Wizard actions={actions} onDone={onDone} />);
    await connect("Claude");
    hold("attempt-1", "claude");
    await act(() => fireEvent.click(screen.getByRole("button", ADD)));
    expect(onDone).not.toHaveBeenCalled();
    act(() => {
      acceptAttempt({
        attemptId: "attempt-1",
        revision: 3,
        progress: { kind: "verified", context: { state: "connected" } },
      });
    });
    expect(onDone).toHaveBeenCalledExactlyOnceWith(true);
    expect(actions.cancelConnection).not.toHaveBeenCalled();
  });

  it("finishes once when both the reply and the Verified event arrive", async () => {
    const actions = settingsActions();
    const onDone = vi.fn();
    render(<Wizard actions={actions} onDone={onDone} />);
    await connect("Claude");
    hold("attempt-1", "claude");
    await act(() => fireEvent.click(screen.getByRole("button", ADD)));
    act(() => {
      acceptAttempt({
        attemptId: "attempt-1",
        revision: 3,
        progress: { kind: "verified", context: { state: "connected" } },
      });
    });
    expect(onDone).toHaveBeenCalledExactlyOnceWith(true);
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
    await act(() => fireEvent.click(screen.getByRole("button", ADD)));
    expect(actions.confirmConnection).toHaveBeenCalledTimes(1);
    expect(onDone).not.toHaveBeenCalled();
    expect(screen.getByRole("heading", VERIFY)).toBeTruthy();
  });

  it("discards the candidate on Not this account and on Cancel, deleting nothing", async () => {
    const actions = settingsActions();
    const onDone = vi.fn();
    render(<Wizard actions={actions} onDone={onDone} />);
    await connect("Codex");
    hold("attempt-1", "codex");
    await act(() =>
      fireEvent.click(screen.getByRole("button", { name: "Not this account" })),
    );
    expect(actions.cancelConnection).toHaveBeenCalledWith({ id: "attempt-1" });
    expect(screen.getByRole("heading", { name: "Connect Codex" })).toBeTruthy();
    // It says how to put the right account into Codex before trying again.
    expect(screen.getByRole("status").textContent).toContain("run codex login");
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

  it("lets a confirmation already in flight finish when the wizard goes away", async () => {
    const confirmed = Promise.withResolvers<boolean>();
    const actions = {
      ...settingsActions(),
      confirmConnection: vi.fn(() => confirmed.promise),
    };
    const view = render(<Wizard actions={actions} onDone={vi.fn()} />);
    await connect("Codex");
    hold("attempt-1", "codex");
    fireEvent.click(screen.getByRole("button", ADD));
    view.unmount();
    await act(async () => {
      confirmed.resolve(true);
      await confirmed.promise;
    });
    expect(actions.confirmConnection).toHaveBeenCalledTimes(1);
    expect(actions.cancelConnection).not.toHaveBeenCalled();
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
    const facts = document.querySelector(".verified-account")?.textContent ?? "";
    expect(facts).toContain("Account 0");
    expect(facts).toContain("Workspace hidden");
    expect(facts).not.toContain("new@example.test");
    expect(facts).not.toContain("Home");
  });

  it("masks the nickname being typed while account labels are hidden, and follows the setting", async () => {
    const actions = settingsActions();
    render(<Wizard actions={actions} onDone={vi.fn()} />);
    await connect("Codex");
    hold("attempt-1", "codex");
    const field = screen.getByLabelText<HTMLInputElement>("Account nickname");
    expect(field.classList.contains("masked")).toBe(false);
    const aliases = (mode: "stable_aliases" | "off"): void => {
      act(() => {
        acceptPreferences(
          preferences({
            privacy: {
              alias_mode: mode,
              retain_history: false,
              export_identities: false,
            },
          }),
        );
      });
    };
    // Turned on while Verify is open.
    aliases("stable_aliases");
    expect(field.classList.contains("masked")).toBe(true);
    expect(field.getAttribute("placeholder")).toBeNull();
    expect(screen.getByText("Hidden while Hide account names is on.")).toBeTruthy();
    // Typing still works, and the typed nickname is what is saved.
    fireEvent.change(field, { target: { value: "Secret" } });
    await act(() => fireEvent.click(screen.getByRole("button", ADD)));
    expect(actions.confirmConnection).toHaveBeenCalledWith(
      { id: "attempt-1" },
      "Secret",
      null,
    );
    aliases("off");
  });

  it.each([
    ["Codex", "codex", "codex login"],
    ["Claude", "claude", "Run claude in a terminal"],
    ["OpenCode Go", "open_code_go", "opencode auth login"],
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

  it("signs OpenRouter in with a pasted key, sent once and then forgotten", async () => {
    const actions = settingsActions();
    render(<Wizard actions={actions} onDone={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: /^OpenRouter/ }));
    const connectButton = screen.getByRole("button", { name: "Connect" });
    const key = screen.getByLabelText("API key");
    expect(key).toHaveProperty("type", "password");
    // Nothing to send yet.
    expect(connectButton).toHaveProperty("disabled", true);
    fireEvent.change(key, { target: { value: "  sk-or-v1-abc  " } });
    expect(connectButton).toHaveProperty("disabled", false);
    await act(() => fireEvent.click(connectButton));
    expect(actions.beginConnection).toHaveBeenCalledExactlyOnceWith(
      expect.objectContaining({ provider_id: "openrouter", credential: "sk-or-v1-abc" }),
    );
    // The host has the key now, so the field no longer holds it.
    expect(screen.getByLabelText("API key")).toHaveProperty("value", "");
  });

  it("lets Kimi connect through its CLI's sign-in when no key is pasted", async () => {
    const actions = settingsActions();
    render(<Wizard actions={actions} onDone={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: /^Kimi/ }));
    expect(
      screen.getByText(/leave it empty to use the sign-in the Kimi CLI/),
    ).toBeTruthy();
    const connectButton = screen.getByRole("button", { name: "Connect" });
    expect(connectButton).toHaveProperty("disabled", false);
    await act(() => fireEvent.click(connectButton));
    expect(actions.beginConnection).toHaveBeenCalledWith(
      expect.objectContaining({ provider_id: "kimi", credential: null }),
    );
  });

  it("signs OpenCode Go in with a pasted key, sent once and then forgotten", async () => {
    const actions = settingsActions();
    render(<Wizard actions={actions} onDone={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: /^OpenCode Go/ }));
    const connectButton = screen.getByRole("button", { name: "Connect" });
    const key = screen.getByLabelText("API key");
    expect(key).toHaveProperty("type", "password");
    expect(connectButton).toHaveProperty("disabled", false);
    fireEvent.change(key, { target: { value: "  oc-go-abc  " } });
    await act(() => fireEvent.click(connectButton));
    expect(actions.beginConnection).toHaveBeenCalledExactlyOnceWith(
      expect.objectContaining({
        provider_id: "open_code_go",
        credential: "oc-go-abc",
      }),
    );
    // The host has the key now, so the field no longer holds it.
    expect(screen.getByLabelText("API key")).toHaveProperty("value", "");
  });

  it("lets OpenCode Go connect through the OpenCode CLI when no key is pasted", async () => {
    const actions = settingsActions();
    render(<Wizard actions={actions} onDone={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: /^OpenCode Go/ }));
    expect(
      screen.getByText(/leave it empty to use the sign-in the OpenCode CLI/),
    ).toBeTruthy();
    const connectButton = screen.getByRole("button", { name: "Connect" });
    expect(connectButton).toHaveProperty("disabled", false);
    await act(() => fireEvent.click(connectButton));
    expect(actions.beginConnection).toHaveBeenCalledWith(
      expect.objectContaining({ provider_id: "open_code_go", credential: null }),
    );
  });

  it("signs Grok in through the browser and shows the code to enter", async () => {
    const actions = settingsActions();
    render(<Wizard actions={actions} onDone={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: /^Grok/ }));
    await act(() =>
      fireEvent.click(screen.getByRole("button", { name: "Sign in with browser" })),
    );
    expect(actions.beginConnection).toHaveBeenCalledWith(
      expect.objectContaining({
        provider_id: "grok",
        browser_sign_in: true,
        credential: null,
      }),
    );
    act(() => {
      acceptAttempt({
        attemptId: "attempt-1",
        revision: 2,
        progress: {
          kind: "awaiting_user",
          context: {
            sign_in: {
              user_code: "WDJB-MJHT",
              verification_uri: "https://accounts.x.ai/device",
            },
          },
        },
      });
    });
    expect(screen.getByText("WDJB-MJHT")).toBeTruthy();
    expect(screen.getByText(/https:\/\/accounts\.x\.ai\/device/)).toBeTruthy();
    expect(
      screen.getByRole("button", { name: "Waiting for the browser…" }),
    ).toHaveProperty("disabled", true);
  });

  it.each([
    ["Grok", "grok", "the system browser refused the page"],
    ["Muse Code", "muse_code", "the browser did not answer within 10 seconds"],
  ] as const)(
    "keeps %s approval active after browser launch trouble",
    async (label, provider, detail) => {
      const actions = settingsActions();
      const onDone = vi.fn();
      render(<Wizard actions={actions} onDone={onDone} />);
      fireEvent.click(screen.getByRole("button", { name: new RegExp(`^${label}`) }));
      await act(() =>
        fireEvent.click(screen.getByRole("button", { name: "Sign in with browser" })),
      );
      const reason = `${detail}. Open https://auth.example.test/device and enter the code TEST-CODE. The log is at /logs/quota.log`;
      act(() => {
        acceptAttempt({
          attemptId: "attempt-1",
          revision: 3,
          progress: {
            kind: "awaiting_user",
            context: {
              sign_in: {
                user_code: "TEST-CODE",
                verification_uri: "https://auth.example.test/device",
                launch_error: {
                  kind: "native_operation_failed",
                  context: { operation: "browser_launch", reason },
                },
              },
            },
          },
        });
      });
      expect(screen.getByRole("alert").textContent).toBe(reason);
      expect(screen.getByText("TEST-CODE")).toBeTruthy();
      expect(
        screen.getByRole("button", { name: "Waiting for the browser…" }),
      ).toHaveProperty("disabled", true);
      expect(actions.cancelConnection).not.toHaveBeenCalled();
      expect(onDone).not.toHaveBeenCalled();
      hold("attempt-1", provider, 4);
      expect(screen.getByRole("heading", VERIFY)).toBeTruthy();
      await act(() => fireEvent.click(screen.getByRole("button", ADD)));
      expect(actions.confirmConnection).toHaveBeenCalledOnce();
      expect(onDone).toHaveBeenCalledExactlyOnceWith(true);
    },
  );

  it.each([
    "authorization request timeout",
    "poll timeout",
    "sign-in denial",
    "sign-in expiration",
    "discovery timeout",
    "verification timeout",
  ])("shows the host reason and log for a terminal %s", async (failure) => {
    const actions = settingsActions();
    render(<Wizard actions={actions} onDone={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: /^Muse Code/ }));
    await act(() =>
      fireEvent.click(screen.getByRole("button", { name: "Sign in with browser" })),
    );
    const reason = `${failure}. The log is at C:\\Quota\\logs\\quota.log`;
    act(() => {
      acceptAttempt({
        attemptId: "attempt-1",
        revision: 3,
        progress: {
          kind: "failed",
          context: { error: { kind: "provider_refused", context: { reason } } },
        },
      });
    });
    expect(screen.getByRole("alert").textContent).toBe(reason);
    expect(screen.getByRole("button", { name: "Sign in with browser" })).toHaveProperty(
      "disabled",
      false,
    );
    expect(actions.cancelConnection).not.toHaveBeenCalled();
  });

  it("can use the Grok CLI's sign-in instead of the browser", async () => {
    const actions = settingsActions();
    render(<Wizard actions={actions} onDone={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: /^Grok/ }));
    await act(() =>
      fireEvent.click(screen.getByRole("button", { name: "Use the Grok CLI sign-in" })),
    );
    expect(actions.beginConnection).toHaveBeenCalledWith(
      expect.objectContaining({ provider_id: "grok", browser_sign_in: false }),
    );
  });

  it("reads Cursor through the Cursor app's own sign-in, with no key", async () => {
    const actions = settingsActions();
    render(<Wizard actions={actions} onDone={vi.fn()} />);
    await connect("Cursor");
    expect(screen.queryByLabelText("API key")).toBeNull();
    expect(actions.beginConnection).toHaveBeenCalledWith(
      expect.objectContaining({
        provider_id: "cursor",
        credential: null,
        browser_sign_in: false,
      }),
    );
  });

  it("lets Ollama Cloud use Ollama's own sign-in, or a pasted key", async () => {
    const actions = settingsActions();
    render(<Wizard actions={actions} onDone={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: /^Ollama Cloud/ }));
    expect(
      screen.getByText(/leave it empty to use the sign-in the Ollama app keeps/),
    ).toBeTruthy();
    await act(() => fireEvent.click(screen.getByRole("button", { name: "Connect" })));
    expect(actions.beginConnection).toHaveBeenCalledWith(
      expect.objectContaining({ provider_id: "ollama_cloud", credential: null }),
    );
  });

  it("needs a key for Z.ai, which has no tool of its own", () => {
    render(<Wizard actions={settingsActions()} onDone={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: /^Z\.ai/ }));
    expect(screen.getByRole("button", { name: "Connect" })).toHaveProperty(
      "disabled",
      true,
    );
    expect(screen.queryByText(/leave it empty/)).toBeNull();
  });

  it("never sends a key for a provider read through its own sign-in", async () => {
    const actions = settingsActions();
    render(<Wizard actions={actions} onDone={vi.fn()} />);
    await connect("Codex");
    expect(screen.queryByLabelText("API key")).toBeNull();
    expect(actions.beginConnection).toHaveBeenCalledWith(
      expect.objectContaining({ provider_id: "codex", credential: null }),
    );
  });

  it("says how to fix a key OpenRouter refused", async () => {
    const actions = settingsActions();
    render(<Wizard actions={actions} onDone={vi.fn()} />);
    fireEvent.click(screen.getByRole("button", { name: /^OpenRouter/ }));
    fireEvent.change(screen.getByLabelText("API key"), {
      target: { value: "sk-or-v1-x" },
    });
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
    expect(screen.getByRole("alert").textContent).toContain("did not accept this key");
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
    await act(() => fireEvent.click(screen.getByRole("button", ADD)));
    expect(window.location.hash).toMatch(/^#\/settings\/accounts\//);
  });

  it("cancels a late begin reply after a fresh route replaces the wizard", async () => {
    window.location.hash = "#/settings/connect/request-1";
    const accepted = Promise.withResolvers<AttemptRef | null>();
    const actions = {
      ...settingsActions(),
      beginConnection: vi.fn(() => accepted.promise),
    };
    render(<SettingsHarness actions={actions} />);
    await connect("Claude");
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
    window.location.hash = "#/settings/connect/request-1";
    const confirmed = Promise.withResolvers<boolean>();
    const actions = {
      ...settingsActions(),
      confirmConnection: vi.fn(() => confirmed.promise),
    };
    render(<SettingsHarness actions={actions} />);
    await connect("Claude");
    hold("attempt-1", "claude");
    fireEvent.click(screen.getByRole("button", ADD));
    act(() => {
      window.location.hash = "#/settings/connect/request-2";
      fireEvent(window, new HashChangeEvent("hashchange"));
    });
    await act(async () => {
      confirmed.resolve(true);
      await confirmed.promise;
    });
    expect(window.location.hash).toBe("#/settings/connect/request-2");
    expect(screen.getByRole("heading", { name: "Add a subscription" })).toBeTruthy();
  });
});

describe("the connection commands", () => {
  it("reconciles a saved confirmation without waiting for its Verified event", async () => {
    const saved = snapshot("instance-1", 2, [
      existing,
      account("saved", "claude", 2, []),
    ]);
    invoke.mockImplementation((command: string) =>
      Promise.resolve(command === "get_snapshot" ? { snapshot: saved } : null),
    );
    expect(await hostActions.confirmConnection({ id: "attempt-1" }, "Work")).toBe(true);
    expect(invoke).toHaveBeenCalledWith("confirm_connection", {
      attemptRef: { id: "attempt-1" },
      nickname: "Work",
      group: null,
    });
    expect(invoke).toHaveBeenCalledWith("get_snapshot");
    expect(getRendererState().snapshot).toEqual(saved);
  });

  it("does not reconcile a confirmation the host refused before saving", async () => {
    invoke.mockRejectedValue({
      kind: "persistence_unavailable",
      context: { owner: "sqlite" },
    });
    expect(await hostActions.confirmConnection({ id: "attempt-1" }, "Work")).toBe(false);
    expect(invoke).toHaveBeenCalledExactlyOnceWith("confirm_connection", {
      attemptRef: { id: "attempt-1" },
      nickname: "Work",
      group: null,
    });
  });

  it("keeps a confirmation successful when reconciliation cannot reach the host", async () => {
    invoke.mockImplementation((command: string) =>
      command === "get_snapshot"
        ? Promise.reject(new Error("snapshot unavailable"))
        : Promise.resolve(null),
    );
    expect(await hostActions.confirmConnection({ id: "attempt-1" }, "Work")).toBe(true);
    expect(getRendererState().failure).toMatchObject({ kind: "transport" });
  });

  it("treats cancelling an attempt that already finished as harmless", async () => {
    invoke.mockRejectedValue({ kind: "cancelled" });
    await hostActions.cancelConnection({ id: "finished-attempt" });
    expect(invoke).toHaveBeenCalledWith("cancel_connection", {
      attemptRef: { id: "finished-attempt" },
    });
    expect(getRendererState().failure).toBeNull();
  });

  it("reports other cancellation failures", async () => {
    invoke.mockRejectedValue({
      kind: "permission_denied",
      context: { window_label: "settings" },
    });
    await hostActions.cancelConnection({ id: "attempt-1" });
    expect(getRendererState().failure).toEqual({
      kind: "domain",
      error: { kind: "permission_denied", context: { window_label: "settings" } },
    });
  });
});
