/**
 * Fixtures for the renderer tests.
 *
 * Each fixture is built from the generated contract types, so a change to the
 * contract breaks these fixtures at compile time rather than at run time.
 */
import type {
  AccountSnapshot,
  AppSnapshot,
  ConnectionProgress,
  Measurement,
  MonitoringState,
  PersistenceStatus,
  Preferences,
  ProviderId,
  QuotaWindow,
  UnrankedReason,
  VerifiedCandidate,
} from "../src/generated/bindings";

/** A timestamp far enough in the future that fixtures never expire. */
export const NOW = Date.parse("2026-10-01T12:00:00.000Z");

/** Builds one window with an explicit measurement. */
export function window(
  id: string,
  category: QuotaWindow["category"],
  measurement: Measurement,
  options: {
    readonly label?: string;
    readonly boundaryAt?: string | null;
    readonly role?: QuotaWindow["metric_role"];
  } = {},
): QuotaWindow {
  return {
    id: id,
    provider_bucket_id: null,
    pool_id: "pool-1",
    scope: {
      resource: "resource",
      label: options.label ?? "Subscription",
    },
    category,
    semantics: "anchored_period",
    duration: null,
    metric_role: options.role ?? "included_allowance",
    enforcement: "hard",
    measurement,
    period_started_at: null,
    boundary:
      options.boundaryAt === null
        ? null
        : {
            at: options.boundaryAt ?? "2026-10-01T14:00:00.000Z",
            kind: "full_reset",
          },
    observed_at: "2026-10-01T11:59:00.000Z",
    received_at: "2026-10-01T11:59:00.000Z",
    valid_until: null,
    source: "documented_api",
    completeness: "complete",
    definition_version: 1,
    issues: [],
  };
}

/** A percentage reading from a remaining value. */
export function percent(remaining: number): Measurement {
  return {
    kind: "percentage",
    value: {
      used_percent: 100 - remaining,
      remaining_percent: remaining,
      precision: 0,
    },
  };
}

/** A reading the source could not provide. */
export function unavailable(): Measurement {
  return { kind: "unavailable", value: "not_reported" };
}

/** Builds one account. */
export function account(
  id: string,
  provider: ProviderId,
  ordinal: number,
  windows: readonly QuotaWindow[],
  options: {
    readonly nickname?: string;
    readonly monitoringEnabled?: boolean;
    readonly connectionState?: AccountSnapshot["connection_state"];
    readonly rank?: number | null;
    readonly unrankedReason?: UnrankedReason;
  } = {},
): AccountSnapshot {
  const rank = options.rank === undefined ? null : options.rank;
  return {
    account_id: id,
    connection_id: `${id}-connection`,
    connection_generation: 1,
    provider_id: provider,
    nickname: options.nickname ?? id,
    identity: {
      principal_label: `${id}@example.test`,
      workspace_label: "Home",
      plan_label: null,
      source: "documented_api",
    },
    connection_ordinal: ordinal,
    monitoring_enabled: options.monitoringEnabled ?? true,
    connection_state: options.connectionState ?? "connected",
    fetch_state: "idle",
    last_attempt_at: "2026-10-01T11:59:00.000Z",
    last_success_at: "2026-10-01T11:59:00.000Z",
    next_attempt_at: null,
    windows: [...windows],
    expected_but_missing_window_ids: [],
    order:
      rank === null
        ? {
            kind: "unranked",
            value: {
              reason: options.unrankedReason ?? "incomplete",
              rule_version: 1,
            },
          }
        : {
            kind: "ranked",
            value: {
              remaining_percent: rank,
              controlling_window_id: windows[0]?.id ?? "w",
              scope_label: windows[0]?.scope.label ?? "Subscription",
              rule_version: 1,
            },
          },
  };
}

/** Wraps accounts in a snapshot. */
export function snapshot(
  instance: string,
  revision: number,
  accounts: readonly AccountSnapshot[],
  monitoring: MonitoringState = { kind: "running" },
  persistence: PersistenceStatus = { kind: "available" },
): AppSnapshot {
  return {
    schema_version: 1,
    app_instance_id: instance,
    revision,
    generated_at: "2026-10-01T12:00:00.000Z",
    monitoring_state: monitoring,
    persistence_status: persistence,
    connections: [],
    accounts: [...accounts],
    order: [],
  };
}

/** Confirmed preferences, with every value overridable. */
export function preferences(overrides: Partial<Preferences> = {}): Preferences {
  return {
    schema_version: 1,
    revision: 7,
    theme: "dark",
    density: "compact",
    indicator_style: "ring",
    overview_mode: "floating",
    always_on_top: false,
    launch_behavior: "quiet_in_tray",
    reduce_motion: false,
    notifications: {
      enabled: false,
      thresholds: { low_percent: 20, critical_percent: 10, hysteresis_percent: 3 },
      quiet_hours: { kind: "never" },
      recovery_enabled: false,
    },
    privacy: { alias_mode: "off", retain_history: false, export_identities: false },
    polling: [],
    ...overrides,
  };
}

/** A verified identity held for confirmation, not yet saved. */
export function candidate(
  provider: ProviderId,
  windows: readonly QuotaWindow[] = [],
  principal = "new@example.test",
): VerifiedCandidate {
  return {
    provider_id: provider,
    nickname: "Personal",
    identity: {
      principal_label: principal,
      workspace_label: "Home",
      plan_label: null,
      source: "documented_api",
    },
    windows: [...windows],
  };
}

/** Attempt progress holding one candidate for confirmation. */
export function awaitingConfirmation(
  attemptId: string,
  held: VerifiedCandidate,
  revision = 2,
): { attemptId: string; revision: number; progress: ConnectionProgress } {
  return {
    attemptId,
    revision,
    progress: { kind: "awaiting_confirmation", context: { candidate: held } },
  };
}
