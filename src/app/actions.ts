/**
 * The typed commands the renderer may issue, wrapped as owned operations.
 *
 * Every action here delegates to `reportAsync`, so the promise has an owner and
 * a failure path. Nothing in the renderer calls a generated binding directly
 * outside this module and the subscription lifecycle.
 */
import {
  beginConnection,
  cancelConnection,
  clearLocalHistory,
  disconnectAccount,
  exportSanitizedDiagnostics,
  fitOverviewToAccounts,
  openProviderUsagePage,
  refreshAccounts,
  renameAccount,
  resetOverviewPosition,
  setAccountEnabled,
  setMonitoringState,
  setOverviewAlwaysOnTop,
  setOverviewMode,
  updatePreferences,
  type AccountId,
  type AttemptRef,
  type BeginConnectionRequest,
  type OverviewMode,
  type Preferences,
  type RefreshReason,
} from "../generated/bindings";
import { reportAsync } from "../shared/ipc/report";
import { acceptAttempt, getRendererState } from "../shared/state/store";

/** The renderer's command surface. Every function awaits its own failure path. */
export const actions = {
  /** Requests an explicit read. The backend coalesces repeated requests. */
  async refresh(reason: RefreshReason): Promise<void> {
    await reportAsync(refreshAccounts({ selection: { kind: "all" }, reason }));
  },
  /** Starts or stops scheduled reads. */
  async setMonitoring(paused: boolean): Promise<void> {
    await reportAsync(setMonitoringState({ paused }));
  },
  /** Enables or disables one account. */
  async setAccountEnabled(accountId: AccountId, enabled: boolean): Promise<void> {
    await reportAsync(setAccountEnabled({ account_ref: { id: accountId }, enabled }));
  },
  /** Renames one account. The nickname is presentation only. */
  async renameAccount(accountId: AccountId, nickname: string): Promise<void> {
    await reportAsync(renameAccount({ account_ref: { id: accountId }, nickname }));
  },
  /** Disconnects one account, leaving its same-provider siblings alone. */
  async disconnectAccount(accountId: AccountId): Promise<void> {
    await reportAsync(disconnectAccount({ account_ref: { id: accountId } }));
  },
  /** Begins a connection attempt and records the backend-issued identity. */
  async beginConnection(request: BeginConnectionRequest): Promise<AttemptRef | null> {
    const accepted = await reportAsync(beginConnection(request));
    if (accepted === null) {
      return null;
    }
    acceptAttempt({
      attemptId: accepted.attempt_id,
      revision: 0,
      progress: { kind: "started" },
    });
    return accepted.attempt_ref;
  },
  /** Cancels one live attempt. Cancellation is a deliberate result, not a failure. */
  async cancelConnection(attempt: AttemptRef): Promise<void> {
    await reportAsync(cancelConnection({ attempt_ref: attempt }));
  },
  /** Moves the overview between floating and tray mode. */
  async setOverviewMode(mode: OverviewMode): Promise<void> {
    const result = await reportAsync(setOverviewMode(mode));
    if (result !== null && result.kind === "refused") {
      // The platform refused. The previous state stands, and the reason is typed.
      await this.setAlwaysOnTop(getRendererState().preferences?.always_on_top ?? false);
    }
  },
  /** Sets the independent always-on-top preference. It changes nothing else. */
  async setAlwaysOnTop(alwaysOnTop: boolean): Promise<void> {
    await reportAsync(setOverviewAlwaysOnTop({ always_on_top: alwaysOnTop }));
  },
  /** Widens the overview to fit every account within the work area. */
  async fitToAccounts(): Promise<void> {
    await reportAsync(fitOverviewToAccounts());
  },
  /** Returns the overview to a visible work area. */
  async resetPosition(): Promise<void> {
    await reportAsync(resetOverviewPosition());
  },
  /** Saves the whole preference object. The confirmed object arrives by event. */
  async savePreferences(next: Preferences): Promise<void> {
    await reportAsync(updatePreferences({ preferences: next }));
  },
  /** Opens one provider's usage page in the external browser. */
  async openUsagePage(accountId: AccountId): Promise<void> {
    const provider = getRendererState().snapshot?.accounts.find(
      (account) => account.account_id === accountId,
    )?.provider_id;
    if (provider === undefined) {
      return;
    }
    await reportAsync(openProviderUsagePage({ provider_id: provider }));
  },
  /** Drops retained local history for one account. */
  async clearHistory(accountId: AccountId): Promise<void> {
    await reportAsync(clearLocalHistory({ account_ref: { id: accountId } }));
  },
  /** Writes a sanitized diagnostic export to a host-resolved destination. */
  async exportDiagnostics(destination: string): Promise<void> {
    await reportAsync(exportSanitizedDiagnostics({ destination }));
  },
};
