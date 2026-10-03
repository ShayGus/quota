/**
 * The typed commands the renderer may issue, wrapped as owned operations.
 *
 * Every action here owns its promise and failure path. Nothing in the renderer
 * calls a generated binding directly outside this module and the subscription
 * lifecycle.
 */
import {
  commands,
  type AccountId,
  type AppView,
  type AttemptRef,
  type BeginConnectionRequest,
  type ConnectionAttemptAccepted,
  type ConnectionAttemptId,
  type OverviewMode,
  type Preferences,
  type ProviderPollingPolicy,
  type RefreshReason,
  type IndicatorStyle,
  type SettingsDestination,
} from "../generated/bindings";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import { reportAsync, reportSettled } from "../shared/ipc/report";
import { reconcileSnapshot } from "../shared/ipc/subscription";
import {
  acceptAttempt,
  clearAttempt,
  getRendererState,
  setFailure,
} from "../shared/state/store";

/** Records a login-item failure, so the window can say so. */
function reportLaunchAtLogin(operation: string): null {
  setFailure({
    kind: "domain",
    error: {
      kind: "native_operation_failed",
      context: { operation, reason: "the operating system refused the login item" },
    },
  });
  return null;
}

/** The renderer's command surface. Every function awaits its own failure path. */
export const actions = {
  /** Requests an explicit read. The backend coalesces repeated requests. */
  async refresh(reason: RefreshReason): Promise<void> {
    await reportAsync(commands.refreshAccounts("all", reason));
  },
  /** Starts or stops scheduled reads. */
  async setMonitoring(paused: boolean): Promise<void> {
    await reportAsync(commands.setMonitoringState(paused));
  },
  /** Enables or disables one account. */
  async setAccountEnabled(accountId: AccountId, enabled: boolean): Promise<void> {
    await reportAsync(
      commands.setAccountEnabled({
        account_ref: { id: accountId },
        enabled,
        expected_revision: 0,
      }),
    );
  },
  /** Renames one account. The nickname is presentation only. */
  async renameAccount(accountId: AccountId, nickname: string): Promise<void> {
    await reportAsync(commands.renameAccount({ id: accountId }, nickname));
  },
  /** Disconnects one account, leaving its same-provider siblings alone. */
  async disconnectAccount(accountId: AccountId): Promise<void> {
    await reportAsync(commands.disconnectAccount({ id: accountId }));
  },
  /** Begins a connection attempt and records the backend-issued identity. */
  async beginConnection(
    request: BeginConnectionRequest,
  ): Promise<ConnectionAttemptAccepted | null> {
    const accepted = await reportAsync(commands.beginConnection(request));
    if (accepted === null) {
      return null;
    }
    acceptAttempt({
      attemptId: accepted.attempt_id,
      revision: 0,
      progress: { kind: "started" },
    });
    return accepted;
  },
  /**
   * Forgets a finished connection attempt once its result is on screen.
   *
   * Only a terminal progress value is dropped, so an attempt that is still
   * running is never forgotten while it is still working.
   */
  clearConnectionAttempt(attemptId: ConnectionAttemptId): void {
    clearAttempt(attemptId);
  },
  /**
   * Re-verifies one account under a new connection generation.
   *
   * This is the real reconnect: the backend bumps the generation and queues a
   * fresh verified read, so it is not the same as opening the account's detail
   * view.
   */
  async reconnectAccount(accountId: AccountId): Promise<void> {
    await reportAsync(commands.reconnectAccount({ id: accountId }));
  },
  /** Cancels a live attempt or discards a verified candidate. Neither is a failure. */
  async cancelConnection(attempt: AttemptRef): Promise<void> {
    await reportAsync(
      commands
        .cancelConnection(attempt)
        .then((result) =>
          result.status === "error" && result.error.kind === "cancelled"
            ? { status: "ok" as const, data: null }
            : result,
        ),
    );
  },
  /**
   * Confirms one candidate and reports whether the command acknowledged success.
   *
   * A false result can mean a lost reply after a durable save. Verified progress
   * independently closes the wizard, and the subscription reconciles the
   * snapshot. A successful reply reconciles here even if Verified was lost;
   * reconciliation failure does not turn that acknowledgement into a failed save.
   */
  async confirmConnection(attempt: AttemptRef, nickname: string): Promise<boolean> {
    const saved = await reportSettled(commands.confirmConnection(attempt, nickname));
    if (saved) await reconcileSnapshot();
    return saved;
  },
  /** Moves the overview between floating and tray mode. */
  async setOverviewMode(mode: OverviewMode): Promise<void> {
    const result = await reportAsync(commands.setOverviewMode(mode));
    if (result !== null && result.kind === "refused") {
      // The platform refused. The previous state stands, and the reason is typed.
      await this.setAlwaysOnTop(getRendererState().preferences?.always_on_top ?? false);
    }
  },
  /** Sets the independent always-on-top preference. It changes nothing else. */
  async setAlwaysOnTop(alwaysOnTop: boolean): Promise<void> {
    await reportAsync(commands.setOverviewAlwaysOnTop(alwaysOnTop));
  },
  /**
   * Switches between the full window and the mini widget. The host shows the
   * chosen view before it puts the other away, then saves the choice; the
   * tray's check mark follows.
   */
  async setAppView(view: AppView): Promise<void> {
    await reportAsync(commands.setAppView(view));
  },
  /**
   * Asks the host to fit the popover's height to its content. The host decides
   * the height and position inside the work area; a refusal is reported.
   */
  async fitOverviewHeight(contentHeight: number): Promise<void> {
    await reportAsync(commands.fitOverviewHeight(Math.max(0, Math.round(contentHeight))));
  },
  async savePollingPreferences(policy: ProviderPollingPolicy): Promise<void> {
    await reportAsync(commands.setPollingPreferences(policy.provider_id, policy));
  },
  /** Saves the whole preference object. The confirmed object arrives by event. */
  async savePreferences(next: Preferences): Promise<void> {
    await reportAsync(commands.updatePreferences(next));
  },
  /** Shows the settings window, which is the only window that renders settings. */
  async openSettings(destination: SettingsDestination = "general"): Promise<void> {
    await reportAsync(commands.openSettingsWindow(destination));
  },
  async setIndicatorStyle(style: IndicatorStyle): Promise<void> {
    await reportAsync(commands.setIndicatorStyle(style));
  },
  async closeWindow(): Promise<void> {
    await getCurrentWindow().close();
  },
  /**
   * Whether Quota is registered to start at login, as the system reports it,
   * or `null` when the system could not be asked.
   */
  async launchAtLogin(): Promise<boolean | null> {
    try {
      return await isEnabled();
    } catch {
      return null;
    }
  },
  /**
   * Registers or removes the login item, then reports the state the system
   * confirms, so the switch never shows a registration that did not happen.
   */
  async setLaunchAtLogin(launch: boolean): Promise<boolean | null> {
    try {
      if (launch) {
        await enable();
      } else {
        await disable();
      }
      return await isEnabled();
    } catch {
      return reportLaunchAtLogin(
        launch ? "enable_launch_at_login" : "disable_launch_at_login",
      );
    }
  },
  /** Opens one provider's usage page in the external browser. */
  async openUsagePage(accountId: AccountId): Promise<void> {
    const provider = getRendererState().snapshot?.accounts.find(
      (account) => account.account_id === accountId,
    )?.provider_id;
    if (provider === undefined) {
      return;
    }
    await reportAsync(commands.openProviderUsagePage(provider));
  },
  /** Drops retained local history for one account. */
  async clearHistory(accountId: AccountId): Promise<void> {
    await reportAsync(commands.clearLocalHistory({ id: accountId }));
  },
  /** Writes a sanitized diagnostic export to a host-resolved destination. */
  async exportDiagnostics(label: string): Promise<string | null> {
    return reportAsync(commands.exportSanitizedDiagnostics(label));
  },
};
