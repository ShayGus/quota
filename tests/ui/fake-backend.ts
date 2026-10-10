/**
 * The faked Tauri backend for the interface tests.
 *
 * This file is bundled into the page and runs there, in front of the real built
 * renderer. It stands in for the Rust host at the IPC boundary and nowhere
 * else: Tauri's own `mockIPC` supplies the `invoke` and event plumbing, and the
 * handler below answers each typed command the way the host does, including the
 * events the host publishes after a command. The renderer under test is the
 * production bundle, unchanged.
 *
 * The host's behaviour is reproduced only as far as the renderer can observe
 * it: a confirmed preference arrives as an event, a confirmed connection adds an
 * account to the snapshot, and a command the host refuses rejects with the
 * typed error. Every call is recorded, so a test can assert what the renderer
 * asked for. A command this file does not know rejects and is recorded as
 * unhandled, so a new command cannot slip through unnoticed.
 */
import { emit as emitEvent } from "@tauri-apps/api/event";
import { mockConvertFileSrc, mockIPC, mockWindows } from "@tauri-apps/api/mocks";

import type {
  AccountGroup,
  AccountSnapshot,
  AppSnapshot,
  GroupSnapshot,
  KeyGroupChoice,
  PeriodSpend,
  CommandError,
  ConnectionProgress,
  MonitoringState,
  OverviewWindowState,
  Preferences,
  RegisteredProvider,
  UpdatePrompt,
  VerifiedCandidate,
} from "../../src/generated/bindings";

/** The windows the application creates, by label. */
export type WindowLabel = "overview" | "settings" | "widget" | "update";

/** What a scripted connection attempt does after it starts. */
export interface ConnectionScript {
  /** The progress events the host publishes, in order, after `begin_connection`. */
  readonly progress: readonly ConnectionProgress[];
  /** When set, `begin_connection` itself is refused with this error. */
  readonly refuseWith?: CommandError;
  /** When set, `confirm_connection` is refused with this error. */
  readonly confirmRefusedWith?: CommandError;
}

/**
 * What the faked host does about an update: what the pop-up shows first, and
 * how the install ends once the test lets it end.
 */
export interface UpdateScript {
  /** What the pop-up shows when its window opens. */
  readonly prompt: UpdatePrompt;
  /** `restarts`: the install works and Quota relaunches. `fails`: it does not. */
  readonly installOutcome: "restarts" | "fails";
}

/** Everything a test decides about the faked host before the page loads. */
export interface FakeConfig {
  readonly window: WindowLabel;
  readonly snapshot: AppSnapshot;
  readonly preferences: Preferences;
  readonly overviewWindow: OverviewWindowState;
  readonly providers: readonly RegisteredProvider[];
  readonly launchAtLogin: boolean;
  readonly connection: ConnectionScript | null;
  /** The update pop-up's script, or `null` for a window that is not one. */
  readonly update: UpdateScript | null;
  /** Commands the host refuses, by wire name, with the typed error. */
  readonly refuse: Readonly<Record<string, CommandError>>;
}

/** One recorded IPC call. */
export interface RecordedCall {
  readonly command: string;
  readonly args: unknown;
}

/** The handle tests use from the page. */
export interface FakeHandle {
  readonly calls: RecordedCall[];
  readonly unhandled: string[];
  /** Publishes a host event to the renderer. */
  emit: (event: string, payload: unknown) => Promise<void>;
  /** The host's current view of the snapshot and preferences. */
  state: () => { snapshot: AppSnapshot; preferences: Preferences };
  /** Lets a started install end, the way the script says it ends. */
  finishUpdate: () => void;
}

declare global {
  interface Window {
    __quotaFake?: FakeHandle;
    __installQuotaFake?: (config: FakeConfig) => void;
  }
}

type Args = Record<string, unknown> | undefined;

/**
 * The groups the host would publish for these accounts: in account order,
 * the balance once from the member that read it last, the key spend added up
 * per period (`quota_domain::group::group_snapshots`).
 */
export function groupsOf(accounts: readonly AccountSnapshot[]): GroupSnapshot[] {
  const groups: GroupSnapshot[] = [];
  for (const account of accounts) {
    if (account.group === null) {
      continue;
    }
    const existing = groups.find((group) => group.id === account.group?.id);
    if (existing !== undefined) {
      existing.account_ids.push(account.account_id);
      continue;
    }
    groups.push({
      id: account.group.id,
      provider_id: account.provider_id,
      name: account.group.name,
      account_ids: [account.account_id],
      balance: null,
      key_spend: null,
      spend_shown: account.group.spend_shown,
    });
  }
  for (const group of groups) {
    const members = accounts.filter((account) => account.group?.id === group.id);
    const withBalance = members
      .filter((account) => account.balance !== null)
      .sort((a, b) => (b.last_success_at ?? "").localeCompare(a.last_success_at ?? ""));
    group.balance = withBalance[0]?.balance ?? null;
    const spends = members.flatMap((account) =>
      account.balance?.key_spend == null ? [] : [account.balance.key_spend],
    );
    if (spends.length > 0) {
      const sum = (period: keyof PeriodSpend): number | null => {
        const values = spends.flatMap((spend) => {
          const value = spend[period];
          return value === null ? [] : [value];
        });
        return values.length === 0 ? null : values.reduce((a, b) => a + b, 0);
      };
      group.key_spend = {
        today_minor: sum("today_minor"),
        week_minor: sum("week_minor"),
        month_minor: sum("month_minor"),
      };
    }
  }
  return groups;
}

/** Installs the faked host into the current page. */
export function installFakeBackend(config: FakeConfig): void {
  let snapshot: AppSnapshot = structuredClone(config.snapshot);
  snapshot = { ...snapshot, groups: groupsOf(snapshot.accounts) };
  let preferences: Preferences = structuredClone(config.preferences);
  let launchAtLogin = config.launchAtLogin;
  let monitoringRevision = 1;
  let attemptCounter = 0;
  let candidate: VerifiedCandidate | null = null;
  let updatePrompt: UpdatePrompt | null = config.update?.prompt ?? null;
  const calls: RecordedCall[] = [];
  const unhandled: string[] = [];
  const instance = snapshot.app_instance_id;

  mockWindows(config.window);
  mockConvertFileSrc("linux");

  const emit = (event: string, payload: unknown): Promise<void> =>
    emitEvent(event, payload);

  /** Sends an event the renderer is not waiting on; a failure is recorded. */
  const fire = (event: string, payload: unknown): void => {
    emit(event, payload).catch((error: unknown) => {
      unhandled.push(`emit ${event}: ${String(error)}`);
    });
  };

  /** Publishes after the current command has answered, as the host's events do. */
  const publishLater = (event: string, payload: unknown): void => {
    setTimeout(() => {
      fire(event, payload);
    }, 0);
  };

  const publishPreferences = (): void => {
    publishLater("preferences-changed", {
      app_instance_id: instance,
      preference_revision: preferences.revision,
      preferences,
    });
  };

  const publishSnapshot = (): void => {
    snapshot = {
      ...snapshot,
      revision: snapshot.revision + 1,
      groups: groupsOf(snapshot.accounts),
    };
    publishLater("snapshot-updated", {
      app_instance_id: instance,
      revision: snapshot.revision,
      schema_version: snapshot.schema_version,
      snapshot,
    });
  };

  const savePreferences = (next: Preferences): Preferences => {
    preferences = { ...next, revision: preferences.revision + 1 };
    publishPreferences();
    return preferences;
  };

  const progress = (attemptId: string, revision: number, state: ConnectionProgress) => {
    publishLater("connection-progress-changed", {
      app_instance_id: instance,
      attempt_id: attemptId,
      attempt_revision: revision,
      progress: state,
    });
  };

  const updateAccount = (
    id: string,
    change: (account: AccountSnapshot) => AccountSnapshot,
  ): void => {
    snapshot = {
      ...snapshot,
      accounts: snapshot.accounts.map((entry) =>
        entry.account_id === id ? change(entry) : entry,
      ),
    };
    publishSnapshot();
  };

  const accountId = (args: Args): string => {
    const request = (args?.request ?? args) as Record<string, unknown> | undefined;
    const ref = (request?.account_ref ?? request?.accountRef) as
      { id: string } | undefined;
    return ref?.id ?? "";
  };

  // The three host commands a settings write may also reach through a plugin.
  const handle = (command: string, args: Args): unknown => {
    calls.push({ command, args });
    const refusal = config.refuse[command];
    if (refusal !== undefined) {
      // Tauri rejects with the serialized error value, which is not an Error.
      // eslint-disable-next-line @typescript-eslint/only-throw-error
      throw refusal;
    }
    switch (command) {
      case "get_snapshot":
        // The host answers the first read with the preferences and window state
        // as events, after the listeners are registered.
        publishPreferences();
        publishLater("overview-window-state-changed", {
          app_instance_id: instance,
          state: config.overviewWindow,
        });
        publishLater("monitoring-state-changed", {
          app_instance_id: instance,
          monitoring_revision: monitoringRevision,
          monitoring_state: snapshot.monitoring_state,
        });
        publishLater("persistence-status-changed", {
          app_instance_id: instance,
          status: snapshot.persistence_status,
        });
        return { snapshot };
      case "list_provider_capabilities":
        return config.providers;
      case "refresh_accounts":
        return [];
      case "set_monitoring_state": {
        const paused = args?.paused === true;
        const state: MonitoringState = paused ? { kind: "paused" } : { kind: "running" };
        monitoringRevision += 1;
        snapshot = { ...snapshot, monitoring_state: state };
        publishLater("monitoring-state-changed", {
          app_instance_id: instance,
          monitoring_revision: monitoringRevision,
          monitoring_state: state,
        });
        publishSnapshot();
        return state;
      }
      case "update_preferences":
        return savePreferences(args?.preferences as Preferences);
      case "set_indicator_style":
        return savePreferences({
          ...preferences,
          indicator_style: args?.style as Preferences["indicator_style"],
        });
      case "set_app_view":
        return savePreferences({
          ...preferences,
          view: args?.view as Preferences["view"],
        });
      case "set_polling_preferences":
        return args?.policy;
      case "set_overview_mode": {
        const mode = args?.mode as Preferences["overview_mode"];
        savePreferences({ ...preferences, overview_mode: mode });
        return { kind: "applied", value: mode };
      }
      case "set_overview_always_on_top":
        savePreferences({ ...preferences, always_on_top: args?.alwaysOnTop === true });
        return config.overviewWindow;
      case "fit_overview_height":
        return config.overviewWindow;
      case "fit_widget": {
        const asked = typeof args?.contentHeight === "number" ? args.contentHeight : 0;
        const direction = args?.direction === "up" ? "up" : "down";
        return {
          height: asked,
          direction,
          room_above: 800,
          room_below: 800,
        };
      }
      case "set_account_enabled": {
        const enabled = (args?.request as { enabled: boolean }).enabled;
        updateAccount(accountId(args), (entry) => ({
          ...entry,
          monitoring_enabled: enabled,
        }));
        return null;
      }
      case "rename_account":
        updateAccount((args?.accountRef as { id: string }).id, (entry) => ({
          ...entry,
          nickname: args?.nickname as string,
        }));
        return null;
      case "set_key_limit_shown":
        updateAccount((args?.accountRef as { id: string }).id, (entry) => ({
          ...entry,
          show_key_limit: args?.shown as boolean,
        }));
        return null;
      case "create_account_group": {
        const id = `group-${String(snapshot.revision)}`;
        const name = (args?.name as string).trim();
        const members = (args?.accountRefs as { id: string }[]).map((ref) => ref.id);
        snapshot = {
          ...snapshot,
          accounts: snapshot.accounts.map((entry) =>
            members.includes(entry.account_id)
              ? { ...entry, group: { id, name, spend_shown: true, key_shown: true } }
              : entry,
          ),
        };
        publishSnapshot();
        return id;
      }
      case "set_account_group": {
        const groupId = args?.groupId as string | null;
        const group =
          groupId === null
            ? null
            : (snapshot.accounts
                .map((entry) => entry.group)
                .find((candidate) => candidate?.id === groupId) ?? null);
        updateAccount((args?.accountRef as { id: string }).id, (entry) => ({
          ...entry,
          group: group === null ? null : { ...group, key_shown: true },
        }));
        return null;
      }
      case "rename_account_group": {
        const groupId = args?.groupId as string;
        const name = (args?.name as string).trim();
        snapshot = {
          ...snapshot,
          accounts: snapshot.accounts.map((entry) =>
            entry.group?.id === groupId
              ? { ...entry, group: { ...entry.group, name } }
              : entry,
          ),
        };
        publishSnapshot();
        return null;
      }
      case "set_group_spend_shown": {
        const groupId = args?.groupId as string;
        const shown = args?.shown as boolean;
        snapshot = {
          ...snapshot,
          accounts: snapshot.accounts.map((entry) =>
            entry.group?.id === groupId
              ? { ...entry, group: { ...entry.group, spend_shown: shown } }
              : entry,
          ),
        };
        publishSnapshot();
        return null;
      }
      case "set_group_key_shown":
        updateAccount((args?.accountRef as { id: string }).id, (entry) => ({
          ...entry,
          group:
            entry.group === null
              ? null
              : { ...entry.group, key_shown: args?.shown as boolean },
        }));
        return null;
      case "disconnect_account": {
        const id = (args?.accountRef as { id: string }).id;
        snapshot = {
          ...snapshot,
          accounts: snapshot.accounts.filter((entry) => entry.account_id !== id),
        };
        publishSnapshot();
        return null;
      }
      case "reconnect_account":
        return 2;
      case "clear_local_history":
      case "open_settings_window":
      case "open_provider_usage_page":
      case "open_bug_report_issue":
      case "copy_bug_report_prompt":
        return null;
      case "export_sanitized_diagnostics":
        return "/tmp/quota-diagnostics.json";
      case "begin_connection": {
        const script = config.connection;
        if (script === null) {
          unhandled.push(`${command} (no connection script)`);
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw { kind: "internal", context: { code: "unscripted" } };
        }
        if (script.refuseWith !== undefined) {
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw script.refuseWith;
        }
        attemptCounter += 1;
        const attemptId = `attempt-${String(attemptCounter)}`;
        script.progress.forEach((state, index) => {
          if (state.kind === "awaiting_confirmation") {
            candidate = state.context.candidate;
          }
          progress(attemptId, index + 1, state);
        });
        return { attempt_ref: { id: attemptId }, attempt_id: attemptId };
      }
      case "cancel_connection":
        candidate = null;
        return null;
      case "confirm_connection": {
        const script = config.connection;
        if (script?.confirmRefusedWith !== undefined) {
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw script.confirmRefusedWith;
        }
        const held = candidate;
        const attemptId = (args?.attemptRef as { id: string }).id;
        if (held !== null) {
          snapshot = {
            ...snapshot,
            accounts: [
              ...snapshot.accounts,
              {
                ...confirmedAccount(
                  held,
                  args?.nickname as string,
                  snapshot.accounts.length,
                ),
                group: joinedGroup(
                  snapshot.accounts,
                  args?.group as KeyGroupChoice | null,
                  snapshot.revision,
                ),
              },
            ],
          };
          publishSnapshot();
        }
        candidate = null;
        progress(attemptId, 100, {
          kind: "verified",
          context: { state: "never_connected" },
        });
        return null;
      }
      case "get_update_prompt":
        return updatePrompt;
      case "respond_to_update_prompt": {
        const response = args?.response as string;
        const shown = updatePrompt?.kind;
        const fits =
          (shown === "offer" && (response === "install" || response === "decline")) ||
          (shown === "failed" && response === "dismiss");
        if (!fits || updatePrompt === null) {
          // eslint-disable-next-line @typescript-eslint/only-throw-error
          throw {
            kind: "validation_failed",
            context: { field: "response", reason: "that answer does not belong here" },
          };
        }
        if (response === "install" && updatePrompt.kind === "offer") {
          updatePrompt = {
            kind: "installing",
            context: { version: updatePrompt.context.version },
          };
          publishLater("update-prompt-changed", { prompt: updatePrompt });
        } else {
          // The host closes the window; the page is left as it was.
          calls.push({ command: "(host) close the pop-up", args: null });
          updatePrompt = null;
        }
        return null;
      }
      case "plugin:autostart|is_enabled":
        return launchAtLogin;
      case "plugin:autostart|enable":
        launchAtLogin = true;
        return null;
      case "plugin:autostart|disable":
        launchAtLogin = false;
        return null;
      default:
        // Window plumbing (`plugin:window|close`, `show`, `set_focus`) has no
        // effect in a page; the call is recorded above and answered with unit.
        if (command.startsWith("plugin:window|")) return null;
        if (command === "plugin:event|emit_to") {
          fire(args?.event as string, args?.payload);
          return null;
        }
        unhandled.push(command);
        throw new Error(`the faked host has no answer for ${command}`);
    }
  };

  mockIPC((command, args) => handle(command, args as Args), { shouldMockEvents: true });

  const finishUpdate = (): void => {
    if (updatePrompt?.kind !== "installing") return;
    if (config.update?.installOutcome === "fails") {
      updatePrompt = { kind: "failed" };
      publishLater("update-prompt-changed", { prompt: updatePrompt });
    } else {
      // The host relaunches Quota; the window goes with the process.
      calls.push({ command: "(host) relaunch", args: null });
      updatePrompt = null;
    }
  };

  window.__quotaFake = {
    calls,
    unhandled,
    emit,
    state: () => ({ snapshot, preferences }),
    finishUpdate,
  };
}

/** The account a confirmed candidate becomes, as the host saves it. */
/** The group a confirmed key joins, as the host resolves the wizard's choice. */
function joinedGroup(
  accounts: readonly AccountSnapshot[],
  choice: KeyGroupChoice | null,
  revision: number,
): AccountGroup | null {
  if (choice === null) {
    return null;
  }
  if (choice.kind === "new") {
    return {
      id: `group-${String(revision)}`,
      name: choice.name.trim(),
      spend_shown: true,
      key_shown: true,
    };
  }
  const existing = accounts
    .map((account) => account.group)
    .find((group) => group?.id === choice.group_id);
  return existing == null ? null : { ...existing, key_shown: true };
}

function confirmedAccount(
  held: VerifiedCandidate,
  nickname: string,
  ordinal: number,
): AccountSnapshot {
  const id = `saved-${String(ordinal + 1)}`;
  return {
    account_id: id,
    connection_id: `${id}-connection`,
    connection_generation: 1,
    provider_id: held.provider_id,
    nickname,
    identity: {
      principal_label: held.identity.principal_label,
      workspace_label: held.identity.workspace_label,
      plan_label: held.identity.plan_label,
      source: held.identity.source,
    },
    connection_ordinal: ordinal + 1,
    monitoring_enabled: true,
    connection_state: "connected",
    fetch_state: "idle",
    last_attempt_at: "2026-10-01T12:00:00.000Z",
    last_success_at: "2026-10-01T12:00:00.000Z",
    next_attempt_at: null,
    windows: held.windows,
    expected_but_missing_window_ids: [],
    balance: null,
    show_key_limit: false,
    group: null,
    order: {
      kind: "ranked",
      value: {
        remaining_percent: 100,
        controlling_window_id: held.windows[0]?.id ?? "w",
        scope_label: held.windows[0]?.scope.label ?? "Subscription",
        rule_version: 1,
      },
    },
  };
}

window.__installQuotaFake = installFakeBackend;
