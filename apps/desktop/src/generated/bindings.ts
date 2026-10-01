/**
 * HAND-MAINTAINED MIRROR — DO NOT EDIT BY HAND.
 *
 * This file is the checked-in TypeScript mirror of the Rust IPC contract. In a
 * working checkout it is machine output, regenerated with:
 *
 *     cargo xtask bindings
 *
 * That command cannot run in this environment (no `webkit2gtk`, no
 * `pkg-config`, so the desktop host does not build), so this copy was written by
 * hand once, from `crates/quota-contracts/src/` and
 * `crates/quota-domain/src/`. It is a mirror, not a second contract: when the
 * host's Specta registry exists, regenerate this file and delete the note.
 *
 * Two mirror rules that the exporter must also honour:
 *  - Identifier newtypes are branded, so passing a `ConnectionRef` where an
 *    `AccountRef` is required fails to compile (spec 8.2).
 *  - Every enum below is written exactly as its `serde` representation: unit
 *    variants are plain strings, `tag`/`content` unions are adjacent tagged
 *    objects, and `Option<T>` is `T | null`.
 *
 * This module is the only place in the renderer that may name a Tauri wire
 * string. Every other module calls the typed functions and wrappers below.
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/* ------------------------------------------------------------------ identity */

/** A monitored subscription account. */
export type AccountId = string & { readonly __entity: "AccountId" };
/** An authorization connection owned by, or referenced by, the application. */
export type ConnectionId = string & { readonly __entity: "ConnectionId" };
/** One attempt to establish a connection. */
export type ConnectionAttemptId = string & { readonly __entity: "ConnectionAttemptId" };
/** A shared allowance owner, or an opaque isolated pool. */
export type QuotaPoolId = string & { readonly __entity: "QuotaPoolId" };
/** A stable pool, scope, metric and period definition. */
export type QuotaWindowId = string & { readonly __entity: "QuotaWindowId" };
/** One running application instance. */
export type AppInstanceId = string & { readonly __entity: "AppInstanceId" };
/** A provider-verified principal. */
export type ProviderPrincipalId = string & { readonly __entity: "ProviderPrincipalId" };
/** A provider workspace or team within a principal. */
export type WorkspaceId = string & { readonly __entity: "WorkspaceId" };
/** A provider-reported plan or entitlement context. */
export type EntitlementId = string & { readonly __entity: "EntitlementId" };
/** A metered resource such as a model or product surface. */
export type ResourceId = string & { readonly __entity: "ResourceId" };
/** A provider-defined unit outside the closed vocabulary. */
export type UnitSymbol = string & { readonly __entity: "UnitSymbol" };

/** A version of a window or account definition. */
export type DefinitionVersion = number;
/** An ISO 4217-style currency code. */
export type CurrencyCode = string;
/** Decimal places a provider reported. */
export type DecimalPrecision = number;
/** An unrounded remaining percentage. Never rounded before comparison. */
export type Percent = number;
/** An RFC 3339 instant in UTC. */
export type DateTime = string;
/** A `chrono::Duration` on the wire. */
export interface Duration {
  readonly secs: number;
  readonly nanos: number;
}

/* -------------------------------------------------------------- closed sets */

/** The providers this build knows about. */
export type ProviderId = "codex" | "claude" | "clinepass" | "opencode_go" | "fixture";

/** How many independent accounts an adapter can monitor at once. */
export type AccountCardinality = "independent" | "workspace_scoped" | "single_profile";

/** Who owns the credential behind a connection. */
export type CredentialOwnership = "app_owned" | "external_client";

/** Where a connection stands. */
export type ConnectionState =
  | "never_connected"
  | "connecting"
  | "connected"
  | "reauthentication_required"
  | "unsupported"
  | "disconnected";

/** How the last read attempt went. Not a quota severity. */
export type FetchState = "idle" | "fetching" | "backoff" | "offline" | "error";

/** What happens when the included allowance runs out. */
export type ContinuationMode = "blocked_at_limit" | "paid_overage" | "unknown";

/** The colour scheme preference. */
export type Theme = "system" | "light" | "dark";

/** How much room a row takes. */
export type Density = "compact" | "comfortable";

/** How a remaining allowance is drawn. */
export type IndicatorStyle = "ring" | "bar";

/** Where the overview lives. */
export type OverviewMode = "floating" | "tray";

/** What happens at login. */
export type LaunchBehavior = "quiet_in_tray" | "restore_last_mode";

/** How account identities appear in the overview. */
export type PrivacyAliasMode = "off" | "stable_aliases";

/** The comparison column a window category projects into. */
export type WindowColumn = "session" | "weekly" | "monthly";

/** The period category a window belongs to. */
export type QuotaCategory = "session" | "weekly" | "monthly" | "daily" | "custom";

/** What a reported duration actually means. */
export type WindowSemantics =
  | "anchored_period"
  | "rolling_period"
  | "calendar_cycle"
  | "unknown";

/** What kind of change happens at a boundary. */
export type BoundaryKind =
  | "full_reset"
  | "next_replenishment"
  | "billing_boundary"
  | "unknown";

/** What the number represents in the plan. */
export type MetricRole = "included_allowance" | "extra_spend_cap" | "credit_balance";

/** How strongly the provider enforces a limit. */
export type Enforcement = "hard" | "soft" | "informational" | "unknown";

/** Where a reading came from. */
export type SourceKind =
  | "documented_api"
  | "documented_cli_protocol"
  | "observed_web_endpoint"
  | "local_capture"
  | "manual";

/** Whether every applicable field of a window was reported. */
export type Completeness = "complete" | "partial";

/** Which presentation section an account belongs to. */
export type OrderSection = "needs_checking" | "ranked" | "monitoring_off";

/** Why an account has no comparable numeric rank. */
export type UnrankedReason =
  | "stale"
  | "incomplete"
  | "reconnect_required"
  | "reset_pending"
  | "native_units_only"
  | "unlimited_only"
  | "disabled"
  | "monitoring_paused"
  | "no_included_allowance";

/** Which accounts a read or refresh covers. */
export type AccountSelection = { readonly kind: "all" } | {
  readonly kind: "listed";
  readonly account_refs: readonly AccountRef[];
};

/** Why a refresh was requested. */
export type RefreshReason =
  | "user_requested"
  | "scheduled"
  | "boundary_verification"
  | "overview_opened"
  | "resumed";

/* -------------------------------------------------------------------- quota */

/** A provider-reported percentage, with its original polarity. */
export interface PercentageMeasurement {
  readonly used_percent: Percent;
  readonly remaining_percent: Percent;
  readonly precision: DecimalPrecision;
}

/** The unit a counted allowance is expressed in. */
export type QuotaUnit =
  | { readonly kind: "requests" }
  | { readonly kind: "tokens" }
  | { readonly kind: "messages" }
  | { readonly kind: "credits" }
  | { readonly kind: "custom"; readonly symbol: UnitSymbol };

/** A counted allowance in a provider unit. */
export interface QuantityMeasurement {
  readonly unit: QuotaUnit;
  readonly precision: DecimalPrecision;
  readonly used: number | null;
  readonly remaining: number | null;
  readonly limit: number | null;
}

/** A monetary allowance, in integer minor units. */
export interface MoneyMeasurement {
  readonly currency: CurrencyCode;
  readonly scale: number;
  readonly used_minor_units: number | null;
  readonly remaining_minor_units: number | null;
  readonly limit_minor_units: number | null;
}

/** Why an allowance has no usable number. */
export type UnavailableReason =
  | "not_reported"
  | "unsupported"
  | "invalid_response"
  | "not_applicable";

/** A normalised reading of one allowance. */
export type Measurement =
  | { readonly kind: "percentage"; readonly value: PercentageMeasurement }
  | { readonly kind: "quantity"; readonly value: QuantityMeasurement }
  | { readonly kind: "money"; readonly value: MoneyMeasurement }
  | { readonly kind: "unlimited" }
  | { readonly kind: "not_entitled" }
  | { readonly kind: "unavailable"; readonly value: UnavailableReason };

/** A structured decoding or normalisation finding. */
export type QuotaIssue =
  | { readonly code: "non_finite_value"; readonly context: { readonly field: string } }
  | { readonly code: "non_positive_denominator" }
  | {
      readonly code: "contradictory_counts";
      readonly context: { readonly remaining: number; readonly limit: number };
    }
  | { readonly code: "negative_usage"; readonly context: { readonly value: number } }
  | {
      readonly code: "unsupported_schema_version";
      readonly context: { readonly version: number };
    }
  | {
      readonly code: "scope_mismatch";
      readonly context: { readonly expected: string; readonly actual: string };
    };

/** A metered resource plus the label shown beside it. */
export interface QuotaScope {
  readonly resource: ResourceId;
  readonly label: string;
}

/** A reported time at which the window's allowance changes. */
export interface Boundary {
  readonly at: DateTime;
  readonly kind: BoundaryKind;
}

/** One allowance, for one account, over one period. */
export interface QuotaWindow {
  readonly id: QuotaWindowId;
  readonly provider_bucket_id: string | null;
  readonly pool_id: QuotaPoolId;
  readonly scope: QuotaScope;
  readonly category: QuotaCategory;
  readonly semantics: WindowSemantics;
  readonly duration: Duration | null;
  readonly metric_role: MetricRole;
  readonly enforcement: Enforcement;
  readonly measurement: Measurement;
  readonly period_started_at: DateTime | null;
  readonly boundary: Boundary | null;
  readonly observed_at: DateTime | null;
  readonly received_at: DateTime;
  readonly valid_until: DateTime | null;
  readonly source: SourceKind;
  readonly completeness: Completeness;
  readonly definition_version: DefinitionVersion;
  readonly issues: readonly QuotaIssue[];
}

/* ---------------------------------------------------------------- accounts */

/** A provider-verified identity, shown for confirmation. */
export interface VerifiedIdentity {
  readonly principal_label: string;
  readonly workspace_label: string | null;
  readonly plan_label: string | null;
  readonly source: SourceKind;
}

/** A non-secret summary of one authorization connection. */
export interface ConnectionSummary {
  readonly id: ConnectionId;
  readonly provider_id: ProviderId;
  readonly credential_ownership: CredentialOwnership;
  readonly generation: number;
  readonly profile_label: string | null;
  readonly cardinality: AccountCardinality;
  readonly state: ConnectionState;
  readonly principal_id: ProviderPrincipalId | null;
  readonly workspace_id: WorkspaceId | null;
  readonly entitlement_id: EntitlementId | null;
}

/** A comparable rank plus the window that produced it. */
export interface RankedOrder {
  readonly remaining_percent: Percent;
  readonly controlling_window_id: QuotaWindowId;
  readonly scope_label: string;
  readonly rule_version: number;
}

/** An explicit absence of rank. */
export interface UnrankedOrder {
  readonly reason: UnrankedReason;
  readonly rule_version: number;
}

/** The outcome of ranking one account. */
export type AccountOrder =
  | { readonly kind: "ranked"; readonly value: RankedOrder }
  | { readonly kind: "unranked"; readonly value: UnrankedOrder };

/** One monitored account as the renderer sees it. */
export interface AccountSnapshot {
  readonly account_id: AccountId;
  readonly connection_id: ConnectionId;
  readonly connection_generation: number;
  readonly provider_id: ProviderId;
  readonly nickname: string;
  readonly identity: VerifiedIdentity | null;
  readonly connection_ordinal: number;
  readonly monitoring_enabled: boolean;
  readonly connection_state: ConnectionState;
  readonly fetch_state: FetchState;
  readonly last_attempt_at: DateTime | null;
  readonly last_success_at: DateTime | null;
  readonly next_attempt_at: DateTime | null;
  readonly windows: readonly QuotaWindow[];
  readonly expected_but_missing_window_ids: readonly QuotaWindowId[];
  readonly order: AccountOrder;
}

/** One account's position in the canonical order. */
export interface OrderEntry {
  readonly account_id: AccountId;
  readonly section: OrderSection;
  readonly order: AccountOrder;
  readonly connection_ordinal: number;
}

/** Whether the supervisor is scheduling reads. */
export type MonitoringState =
  | { readonly kind: "running" }
  | { readonly kind: "paused" }
  | { readonly kind: "recovery_required"; readonly reason: string };

/** Whether durable storage is usable. */
export type PersistenceStatus =
  | { readonly kind: "available" }
  | { readonly kind: "degraded"; readonly detail: string }
  | { readonly kind: "recovery_required"; readonly detail: string };

/** The complete application state at one revision. */
export interface AppSnapshot {
  readonly schema_version: number;
  readonly app_instance_id: AppInstanceId;
  readonly revision: number;
  readonly generated_at: DateTime;
  readonly monitoring_state: MonitoringState;
  readonly persistence_status: PersistenceStatus;
  readonly connections: readonly ConnectionSummary[];
  readonly accounts: readonly AccountSnapshot[];
  readonly order: readonly OrderEntry[];
}

/* ------------------------------------------------------------- preferences */

/** Default alert thresholds, expressed as remaining percentage. */
export interface NotificationThresholds {
  readonly low_percent: number;
  readonly critical_percent: number;
  readonly hysteresis_percent: number;
}

/** When notifications may be shown. */
export type QuietHours =
  | { readonly kind: "never" }
  | { readonly kind: "daily_utc"; readonly from_minute: number; readonly to_minute: number };

/** What the user wants to be told. */
export interface NotificationPolicy {
  readonly enabled: boolean;
  readonly thresholds: NotificationThresholds;
  readonly quiet_hours: QuietHours;
  readonly recovery_enabled: boolean;
}

/** How much local detail is retained and exported. */
export interface PrivacyPolicy {
  readonly alias_mode: PrivacyAliasMode;
  readonly retain_history: boolean;
  readonly export_identities: boolean;
}

/** A typed polling policy attached to one provider. */
export interface ProviderPollingPolicy {
  readonly provider_id: ProviderId;
  readonly strategy: unknown;
  readonly request_timeout_seconds: number;
  readonly helper_timeout_seconds: number;
  readonly backoff_minutes: readonly number[];
  readonly max_concurrent_remote_reads: number;
  readonly version: number;
}

/** The confirmed preference aggregate. */
export interface Preferences {
  readonly schema_version: number;
  readonly revision: number;
  readonly theme: Theme;
  readonly density: Density;
  readonly indicator_style: IndicatorStyle;
  readonly overview_mode: OverviewMode;
  readonly always_on_top: boolean;
  readonly launch_behavior: LaunchBehavior;
  readonly reduce_motion: boolean;
  readonly notifications: NotificationPolicy;
  readonly privacy: PrivacyPolicy;
  readonly polling: readonly ProviderPollingPolicy[];
}

/* ----------------------------------------------------------------- errors */

/** The typed error union every command may return. */
export type CommandError =
  | { readonly kind: "initialization_pending" }
  | {
      readonly kind: "validation_failed";
      readonly context: { readonly field: string; readonly reason: string };
    }
  | { readonly kind: "account_not_found" }
  | { readonly kind: "unsupported_provider"; readonly context: { readonly provider_id: string } }
  | { readonly kind: "unsupported_method"; readonly context: { readonly requested: string } }
  | { readonly kind: "reconnect_required" }
  | { readonly kind: "permission_denied"; readonly context: { readonly window_label: string } }
  | { readonly kind: "secure_store_unavailable" }
  | {
      readonly kind: "revision_conflict";
      readonly context: { readonly expected: number; readonly actual: number };
    }
  | { readonly kind: "persistence_unavailable"; readonly context: { readonly owner: string } }
  | {
      readonly kind: "native_operation_unsupported";
      readonly context: { readonly operation: string };
    }
  | {
      readonly kind: "native_operation_failed";
      readonly context: { readonly operation: string; readonly reason: string };
    }
  | { readonly kind: "cancelled" }
  | { readonly kind: "internal"; readonly context: { readonly code: string } };

/** A transport failure: the call never reached a validated backend handler. */
export interface TransportFailure {
  /** A stable, non-identifying code for the failure class. */
  readonly code: "unavailable" | "malformed_response" | "unknown_error";
  /** Sanitized text. Never a token, a path, or a provider payload. */
  readonly detail: string;
}

/** The result of one command call. */
export type Invocation<T> =
  | { readonly ok: T }
  | { readonly error: CommandError }
  | { readonly transportError: TransportFailure };

/* -------------------------------------------------------- commands and refs */

/** A monitored account, tagged so it cannot be confused with a connection. */
export interface AccountRef {
  readonly id: AccountId;
}
/** An authorization connection, tagged so it cannot be confused with an account. */
export interface ConnectionRef {
  readonly id: ConnectionId;
}
/** One connection attempt, tagged so it cannot be confused with a connection. */
export interface AttemptRef {
  readonly id: ConnectionAttemptId;
}

/** The snapshot plus the revision the renderer should reconcile against. */
export interface SnapshotResponse {
  readonly snapshot: AppSnapshot;
}

/** Arguments for starting an authorized connection attempt. */
export interface BeginConnectionRequest {
  readonly provider_id: ProviderId;
  readonly profile_label: string | null;
  readonly nickname: string;
}

/** The accepted outcome of a connection attempt. */
export interface ConnectionAttemptAccepted {
  readonly attempt_ref: AttemptRef;
  readonly attempt_id: ConnectionAttemptId;
}

/** Arguments for enabling or disabling one account. */
export interface SetAccountEnabledRequest {
  readonly account_ref: AccountRef;
  readonly enabled: boolean;
  readonly expected_revision: number;
}

/** Arguments for renaming one account. */
export interface RenameAccountRequest {
  readonly account_ref: AccountRef;
  readonly nickname: string;
  readonly expected_revision: number;
}

/** Arguments for disconnecting one account. */
export interface DisconnectAccountRequest {
  readonly account_ref: AccountRef;
  readonly expected_revision: number;
}

/** Arguments for cancelling a live connection attempt. */
export interface CancelConnectionRequest {
  readonly attempt_ref: AttemptRef;
}

/** Arguments for starting or stopping scheduled reads. */
export interface SetMonitoringStateRequest {
  readonly running: boolean;
  readonly expected_revision: number;
}

/** Arguments for an explicit refresh. */
export interface RefreshAccountsRequest {
  readonly selection: AccountSelection;
  readonly reason: RefreshReason;
}

/** A window mode change requested by the renderer. */
export interface WindowModeChangeRequest {
  readonly mode: OverviewMode;
  readonly expected_revision: number;
}

/** A confirmed mode change, or the state that was actually reached. */
export type WindowModeChange =
  | { readonly kind: "applied"; readonly value: OverviewMode }
  | {
      readonly kind: "refused";
      readonly value: { readonly reason: string; readonly current: OverviewMode };
    };

/** Arguments for the always-on-top preference. */
export interface SetOverviewAlwaysOnTopRequest {
  readonly always_on_top: boolean;
  readonly expected_revision: number;
}

/** Arguments for a preferences change. */
export interface UpdatePreferencesRequest {
  readonly preferences: Preferences;
  readonly expected_revision: number;
}

/** Arguments for opening one provider's usage page in the external browser. */
export interface OpenProviderUsagePageRequest {
  readonly account_ref: AccountRef;
}

/** Arguments for dropping retained local history. */
export interface ClearLocalHistoryRequest {
  readonly account_ref: AccountRef | null;
  readonly expected_revision: number;
}

/** The label and size of a completed sanitized diagnostic export. */
export interface DiagnosticExport {
  readonly destination_label: string;
  readonly byte_count: number;
}

/** What an adapter declares it can do, before any account exists. */
export interface ProviderCapabilities {
  readonly provider_id: ProviderId;
  readonly cardinality: AccountCardinality;
  readonly supports_app_owned_authorization: boolean;
  readonly supports_external_profile: boolean;
  readonly reports_monthly_window: boolean;
  readonly minimum_interval_seconds: number;
}

/** One compiled adapter and what it declares. */
export interface RegisteredProvider {
  readonly provider_id: ProviderId;
  readonly capabilities: ProviderCapabilities;
  readonly compiled_in_this_build: boolean;
}

/* --------------------------------------------------------------- events */

/** The primary quota and account update channel. */
export interface SnapshotUpdated {
  readonly app_instance_id: AppInstanceId;
  readonly revision: number;
  readonly schema_version: number;
  readonly snapshot: AppSnapshot;
}

/** Where an authorized connection attempt stands. */
export type ConnectionProgress =
  | { readonly kind: "started" }
  | { readonly kind: "awaiting_user" }
  | { readonly kind: "verified"; readonly context: { readonly state: ConnectionState } }
  | { readonly kind: "failed"; readonly context: { readonly error: CommandError } }
  | { readonly kind: "cancelled" };

/** Progress of one connection attempt. */
export interface ConnectionProgressChanged {
  readonly app_instance_id: AppInstanceId;
  readonly attempt_id: ConnectionAttemptId;
  readonly attempt_revision: number;
  readonly progress: ConnectionProgress;
}

/** Emitted after a preference save succeeds, never before. */
export interface PreferencesChanged {
  readonly app_instance_id: AppInstanceId;
  readonly preference_revision: number;
  readonly preferences: Preferences;
}

/** Whether the supervisor is scheduling reads. */
export interface MonitoringStateChanged {
  readonly app_instance_id: AppInstanceId;
  readonly monitoring_revision: number;
  readonly monitoring_state: MonitoringState;
}

/** Confirmed native overview window state, or a typed native failure. */
export type OverviewWindowState =
  | {
      readonly kind: "confirmed";
      readonly value: {
        readonly mode: OverviewMode;
        readonly always_on_top: boolean;
        readonly visible: boolean;
        readonly geometry_revision: number;
      };
    }
  | { readonly kind: "failed"; readonly value: { readonly error: CommandError } };

/** A change to the native overview window. */
export interface OverviewWindowStateChanged {
  readonly app_instance_id: AppInstanceId;
  readonly state: OverviewWindowState;
}

/** Durable storage availability, with no file contents or credential references. */
export interface PersistenceStatusChanged {
  readonly app_instance_id: AppInstanceId;
  readonly status: PersistenceStatus;
}

/* ------------------------------------------------------ payload validation */

/**
 * Field readers for untrusted decoded payloads.
 *
 * A payload is untrusted input until it has been narrowed (spec 8.4, spec
 * 12.1), so nothing here asserts a whole shape: each reader answers what
 * actually arrived, and each closed-set reader yields `null` for a variant this
 * build does not know. The caller then rejects the payload instead of guessing.
 */

/** Reads a string field, or the empty string when it is absent or not text. */
function text(value: unknown): string {
  return typeof value === "string" ? value : "";
}

/** Reads a finite number field, or zero. */
function count(value: unknown): number {
  return typeof value === "number" && Number.isFinite(value) ? value : 0;
}

/** Reads a boolean field, or `false`. */
function flag(value: unknown): boolean {
  return value === true;
}

/** Reads a nested object, or an empty one. */
function nested(value: unknown): Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : {};
}

/** Reads an array, or an empty one. */
function list(value: unknown): readonly unknown[] {
  return Array.isArray(value) ? value : [];
}

/** Reads a nullable string. Absence stays absent. */
function optionalText(value: unknown): string | null {
  return typeof value === "string" ? value : null;
}

/** Reads a nullable instant. Absence stays absent. */
function optionalTextOrNull(value: unknown): DateTime | null {
  return typeof value === "string" && value.length > 0 ? value : null;
}

/** Reads one variant of a closed string vocabulary. */
function oneOf<T extends string>(value: unknown, allowed: readonly T[]): T | null {
  if (typeof value !== "string") {
    return null;
  }
  return allowed.find((candidate) => candidate === value) ?? null;
}

const PROVIDERS = [
  "codex",
  "claude",
  "clinepass",
  "opencode_go",
  "fixture",
] as const satisfies readonly ProviderId[];
const CONNECTION_STATES = [
  "never_connected",
  "connecting",
  "connected",
  "reauthentication_required",
  "unsupported",
  "disconnected",
] as const satisfies readonly ConnectionState[];
const FETCH_STATES = ["idle", "fetching", "backoff", "offline", "error"] as const satisfies
  readonly FetchState[];
const QUOTA_CATEGORIES = ["session", "weekly", "monthly", "daily", "custom"] as const satisfies
  readonly QuotaCategory[];
const WINDOW_SEMANTICS = [
  "anchored_period",
  "rolling_period",
  "calendar_cycle",
  "unknown",
] as const satisfies readonly WindowSemantics[];
const BOUNDARY_KINDS = [
  "full_reset",
  "next_replenishment",
  "billing_boundary",
  "unknown",
] as const satisfies readonly BoundaryKind[];
const METRIC_ROLES = [
  "included_allowance",
  "extra_spend_cap",
  "credit_balance",
] as const satisfies readonly MetricRole[];
const ENFORCEMENTS = ["hard", "soft", "informational", "unknown"] as const satisfies
  readonly Enforcement[];
const SOURCE_KINDS = [
  "documented_api",
  "documented_cli_protocol",
  "observed_web_endpoint",
  "local_capture",
  "manual",
] as const satisfies readonly SourceKind[];
const COMPLETENESSES = ["complete", "partial"] as const satisfies readonly Completeness[];
const UNAVAILABLE_REASONS = [
  "not_reported",
  "unsupported",
  "invalid_response",
  "not_applicable",
] as const satisfies readonly UnavailableReason[];
const ORDER_SECTIONS = ["needs_checking", "ranked", "monitoring_off"] as const satisfies
  readonly OrderSection[];
const UNRANKED_REASONS = [
  "stale",
  "incomplete",
  "reconnect_required",
  "reset_pending",
  "native_units_only",
  "unlimited_only",
  "disabled",
  "monitoring_paused",
  "no_included_allowance",
] as const satisfies readonly UnrankedReason[];
const THEMES = ["system", "light", "dark"] as const satisfies readonly Theme[];
const DENSITIES = ["compact", "comfortable"] as const satisfies readonly Density[];
const INDICATOR_STYLES = ["ring", "bar"] as const satisfies readonly IndicatorStyle[];
const OVERVIEW_MODES = ["floating", "tray"] as const satisfies readonly OverviewMode[];
const LAUNCH_BEHAVIORS = ["quiet_in_tray", "restore_last_mode"] as const satisfies readonly
  LaunchBehavior[];
const PRIVACY_ALIAS_MODES = ["off", "stable_aliases"] as const satisfies readonly
  PrivacyAliasMode[];
const CARDINALITIES = ["independent", "workspace_scoped", "single_profile"] as const satisfies
  readonly AccountCardinality[];
const CREDENTIAL_OWNERSHIPS = ["app_owned", "external_client"] as const satisfies readonly
  CredentialOwnership[];

/** Reads a quota scope. The resource and label must both survive. */
function parseScope(value: unknown): QuotaScope | null {
  const source = nested(value);
  const resource = text(source["resource"]);
  if (resource.length === 0) {
    return null;
  }
  return { resource: resource as ResourceId, label: text(source["label"]) };
}

/** Reads one validation finding, or `null` for an unknown code. */
function parseIssue(value: unknown): QuotaIssue | null {
  const source = nested(value);
  const code = text(source["code"]);
  const context = nested(source["context"]);
  switch (code) {
    case "non_finite_value":
      return { code: "non_finite_value", context: { field: text(context["field"]) } };
    case "non_positive_denominator":
      return { code: "non_positive_denominator" };
    case "contradictory_counts":
      return {
        code: "contradictory_counts",
        context: {
          remaining: count(context["remaining"]),
          limit: count(context["limit"]),
        },
      };
    case "negative_usage":
      return { code: "negative_usage", context: { value: count(context["value"]) } };
    case "unsupported_schema_version":
      return {
        code: "unsupported_schema_version",
        context: { version: count(context["version"]) },
      };
    case "scope_mismatch":
      return {
        code: "scope_mismatch",
        context: { expected: text(context["expected"]), actual: text(context["actual"]) },
      };
    default:
      return null;
  }
}

/** Reads a counted-allowance unit. */
function parseUnit(value: unknown): QuotaUnit | null {
  const source = nested(value);
  switch (text(source["kind"])) {
    case "requests":
      return { kind: "requests" };
    case "tokens":
      return { kind: "tokens" };
    case "messages":
      return { kind: "messages" };
    case "credits":
      return { kind: "credits" };
    case "custom":
      return {
        kind: "custom",
        symbol: text(source["symbol"]) as UnitSymbol,
      };
    default:
      return null;
  }
}

/** Reads a normalised measurement. An unknown variant is rejected, never zeroed. */
function parseMeasurement(value: unknown): Measurement | null {
  const source = nested(value);
  const kind = text(source["kind"]);
  const body = nested(source["value"]);
  switch (kind) {
    case "percentage":
      return {
        kind: "percentage",
        value: {
          used_percent: count(body["used_percent"]),
          remaining_percent: count(body["remaining_percent"]),
          precision: count(body["precision"]),
        },
      };
    case "quantity": {
      const unit = parseUnit(body["unit"]);
      if (unit === null) {
        return null;
      }
      return {
        kind: "quantity",
        value: {
          unit,
          precision: count(body["precision"]),
          used: typeof body["used"] === "number" ? body["used"] : null,
          remaining: typeof body["remaining"] === "number" ? body["remaining"] : null,
          limit: typeof body["limit"] === "number" ? body["limit"] : null,
        },
      };
    }
    case "money":
      return {
        kind: "money",
        value: {
          currency: text(body["currency"]),
          scale: count(body["scale"]),
          used_minor_units:
            typeof body["used_minor_units"] === "number" ? body["used_minor_units"] : null,
          remaining_minor_units:
            typeof body["remaining_minor_units"] === "number"
              ? body["remaining_minor_units"]
              : null,
          limit_minor_units:
            typeof body["limit_minor_units"] === "number" ? body["limit_minor_units"] : null,
        },
      };
    case "unlimited":
      return { kind: "unlimited" };
    case "not_entitled":
      return { kind: "not_entitled" };
    case "unavailable": {
      const reason = oneOf(source["value"], UNAVAILABLE_REASONS);
      return reason === null ? null : { kind: "unavailable", value: reason };
    }
    default:
      return null;
  }
}

/** Reads one quota window. A window without an identity or a reading is rejected. */
function parseWindow(value: unknown): QuotaWindow | null {
  const source = nested(value);
  const id = text(source["id"]);
  const category = oneOf(source["category"], QUOTA_CATEGORIES);
  const metricRole = oneOf(source["metric_role"], METRIC_ROLES);
  const semantics = oneOf(source["semantics"], WINDOW_SEMANTICS);
  const enforcement = oneOf(source["enforcement"], ENFORCEMENTS);
  const sourceKind = oneOf(source["source"], SOURCE_KINDS);
  const completeness = oneOf(source["completeness"], COMPLETENESSES);
  const measurement = parseMeasurement(source["measurement"]);
  const scope = parseScope(source["scope"]);
  if (
    id.length === 0 ||
    category === null ||
    metricRole === null ||
    semantics === null ||
    enforcement === null ||
    sourceKind === null ||
    completeness === null ||
    measurement === null ||
    scope === null
  ) {
    return null;
  }
  const rawBoundary = nested(source["boundary"]);
  const boundaryKind = oneOf(rawBoundary["kind"], BOUNDARY_KINDS);
  const boundaryAt = text(rawBoundary["at"]);
  const issues: QuotaIssue[] = [];
  for (const entry of list(source["issues"])) {
    const issue = parseIssue(entry);
    if (issue === null) {
      return null;
    }
    issues.push(issue);
  }
  return {
    id: id as QuotaWindowId,
    provider_bucket_id: optionalText(source["provider_bucket_id"]),
    pool_id: text(source["pool_id"]) as QuotaPoolId,
    scope,
    category,
    semantics,
    duration: null,
    metric_role: metricRole,
    enforcement,
    measurement,
    period_started_at: optionalTextOrNull(source["period_started_at"]),
    boundary:
      boundaryKind === null || boundaryAt.length === 0
        ? null
        : { at: boundaryAt, kind: boundaryKind },
    observed_at: optionalTextOrNull(source["observed_at"]),
    received_at: text(source["received_at"]),
    valid_until: optionalTextOrNull(source["valid_until"]),
    source: sourceKind,
    completeness,
    definition_version: count(source["definition_version"]),
    issues,
  };
}

/** Reads an account rank, or `null` when the variant is unknown. */
function parseOrder(value: unknown): AccountOrder | null {
  const source = nested(value);
  const body = nested(source["value"]);
  switch (text(source["kind"])) {
    case "ranked":
      return {
        kind: "ranked",
        value: {
          remaining_percent: count(body["remaining_percent"]),
          controlling_window_id: text(body["controlling_window_id"]) as QuotaWindowId,
          scope_label: text(body["scope_label"]),
          rule_version: count(body["rule_version"]),
        },
      };
    case "unranked": {
      const reason = oneOf(body["reason"], UNRANKED_REASONS);
      return reason === null
        ? null
        : {
            kind: "unranked",
            value: { reason, rule_version: count(body["rule_version"]) },
          };
    }
    default:
      return null;
  }
}

/** Reads one monitored account. */
function parseAccount(value: unknown): AccountSnapshot | null {
  const source = nested(value);
  const accountId = text(source["account_id"]);
  const provider = oneOf(source["provider_id"], PROVIDERS);
  const connectionState = oneOf(source["connection_state"], CONNECTION_STATES);
  const fetchState = oneOf(source["fetch_state"], FETCH_STATES);
  const order = parseOrder(source["order"]);
  if (
    accountId.length === 0 ||
    provider === null ||
    connectionState === null ||
    fetchState === null ||
    order === null
  ) {
    return null;
  }
  const windows: QuotaWindow[] = [];
  for (const entry of list(source["windows"])) {
    const window = parseWindow(entry);
    if (window === null) {
      return null;
    }
    windows.push(window);
  }
  const rawIdentity = nested(source["identity"]);
  const identitySource = oneOf(rawIdentity["source"], SOURCE_KINDS);
  const principalLabel = text(rawIdentity["principal_label"]);
  return {
    account_id: accountId as AccountId,
    connection_id: text(source["connection_id"]) as ConnectionId,
    connection_generation: count(source["connection_generation"]),
    provider_id: provider,
    nickname: text(source["nickname"]),
    identity:
      identitySource === null || principalLabel.length === 0
        ? null
        : {
            principal_label: principalLabel,
            workspace_label: optionalText(rawIdentity["workspace_label"]),
            plan_label: optionalText(rawIdentity["plan_label"]),
            source: identitySource,
          },
    connection_ordinal: count(source["connection_ordinal"]),
    monitoring_enabled: flag(source["monitoring_enabled"]),
    connection_state: connectionState,
    fetch_state: fetchState,
    last_attempt_at: optionalTextOrNull(source["last_attempt_at"]),
    last_success_at: optionalTextOrNull(source["last_success_at"]),
    next_attempt_at: optionalTextOrNull(source["next_attempt_at"]),
    windows,
    expected_but_missing_window_ids: list(source["expected_but_missing_window_ids"]).map(
      (entry) => text(entry) as QuotaWindowId,
    ),
    order,
  };
}

/** Reads one connection summary. */
function parseConnection(value: unknown): ConnectionSummary | null {
  const source = nested(value);
  const provider = oneOf(source["provider_id"], PROVIDERS);
  const state = oneOf(source["state"], CONNECTION_STATES);
  const cardinality = oneOf(source["cardinality"], CARDINALITIES);
  const ownership = oneOf(source["credential_ownership"], CREDENTIAL_OWNERSHIPS);
  if (provider === null || state === null || cardinality === null || ownership === null) {
    return null;
  }
  return {
    id: text(source["id"]) as ConnectionId,
    provider_id: provider,
    credential_ownership: ownership,
    generation: count(source["generation"]),
    profile_label: optionalText(source["profile_label"]),
    cardinality,
    state,
    principal_id: optionalText(source["principal_id"]) as ProviderPrincipalId | null,
    workspace_id: optionalText(source["workspace_id"]) as WorkspaceId | null,
    entitlement_id: optionalText(source["entitlement_id"]) as EntitlementId | null,
  };
}

/** Reads one canonical-order entry. */
function parseOrderEntry(value: unknown): OrderEntry | null {
  const source = nested(value);
  const accountId = text(source["account_id"]);
  const section = oneOf(source["section"], ORDER_SECTIONS);
  const order = parseOrder(source["order"]);
  if (accountId.length === 0 || section === null || order === null) {
    return null;
  }
  return {
    account_id: accountId as AccountId,
    section,
    order,
    connection_ordinal: count(source["connection_ordinal"]),
  };
}

/** Reads whether reads are being scheduled. */
function parseMonitoringState(value: unknown): MonitoringState | null {
  const source = nested(value);
  switch (text(source["kind"])) {
    case "running":
      return { kind: "running" };
    case "paused":
      return { kind: "paused" };
    case "recovery_required":
      return { kind: "recovery_required", reason: text(source["reason"]) };
    default:
      return null;
  }
}

/** Reads durable-storage availability. */
function parsePersistenceStatus(value: unknown): PersistenceStatus | null {
  const source = nested(value);
  switch (text(source["kind"])) {
    case "available":
      return { kind: "available" };
    case "degraded":
      return { kind: "degraded", detail: text(source["detail"]) };
    case "recovery_required":
      return { kind: "recovery_required", detail: text(source["detail"]) };
    default:
      return null;
  }
}

/** Reads the complete snapshot. Any unrecognised nested value rejects the payload. */
export function parseSnapshot(value: unknown): AppSnapshot | null {
  const source = nested(value);
  const instance = text(source["app_instance_id"]);
  const monitoring = parseMonitoringState(source["monitoring_state"]);
  const persistence = parsePersistenceStatus(source["persistence_status"]);
  if (instance.length === 0 || monitoring === null || persistence === null) {
    return null;
  }
  const accounts: AccountSnapshot[] = [];
  for (const entry of list(source["accounts"])) {
    const account = parseAccount(entry);
    if (account === null) {
      return null;
    }
    accounts.push(account);
  }
  const connections: ConnectionSummary[] = [];
  for (const entry of list(source["connections"])) {
    const connection = parseConnection(entry);
    if (connection === null) {
      return null;
    }
    connections.push(connection);
  }
  const order: OrderEntry[] = [];
  for (const entry of list(source["order"])) {
    const position = parseOrderEntry(entry);
    if (position === null) {
      return null;
    }
    order.push(position);
  }
  return {
    schema_version: count(source["schema_version"]),
    app_instance_id: instance as AppInstanceId,
    revision: count(source["revision"]),
    generated_at: text(source["generated_at"]),
    monitoring_state: monitoring,
    persistence_status: persistence,
    connections,
    accounts,
    order,
  };
}

/** Reads the confirmed preference aggregate. */
export function parsePreferences(value: unknown): Preferences | null {
  const source = nested(value);
  const theme = oneOf(source["theme"], THEMES);
  const density = oneOf(source["density"], DENSITIES);
  const indicator = oneOf(source["indicator_style"], INDICATOR_STYLES);
  const mode = oneOf(source["overview_mode"], OVERVIEW_MODES);
  const launch = oneOf(source["launch_behavior"], LAUNCH_BEHAVIORS);
  const aliasMode = oneOf(source["privacy"] === undefined ? undefined : nested(source["privacy"])["alias_mode"], PRIVACY_ALIAS_MODES);
  if (
    theme === null ||
    density === null ||
    indicator === null ||
    mode === null ||
    launch === null ||
    aliasMode === null
  ) {
    return null;
  }
  const notifications = nested(source["notifications"]);
  const thresholds = nested(notifications["thresholds"]);
  const quiet = nested(notifications["quiet_hours"]);
  const quietKind = text(quiet["kind"]);
  const privacy = nested(source["privacy"]);
  return {
    schema_version: count(source["schema_version"]),
    revision: count(source["revision"]),
    theme,
    density,
    indicator_style: indicator,
    overview_mode: mode,
    always_on_top: flag(source["always_on_top"]),
    launch_behavior: launch,
    reduce_motion: flag(source["reduce_motion"]),
    notifications: {
      enabled: flag(notifications["enabled"]),
      thresholds: {
        low_percent: count(thresholds["low_percent"]),
        critical_percent: count(thresholds["critical_percent"]),
        hysteresis_percent: count(thresholds["hysteresis_percent"]),
      },
      quiet_hours:
        quietKind === "daily_utc"
          ? {
              kind: "daily_utc",
              from_minute: count(quiet["from_minute"]),
              to_minute: count(quiet["to_minute"]),
            }
          : { kind: "never" },
      recovery_enabled: flag(notifications["recovery_enabled"]),
    },
    privacy: {
      alias_mode: aliasMode,
      retain_history: flag(privacy["retain_history"]),
      export_identities: flag(privacy["export_identities"]),
    },
    polling: list(source["polling"]).map((entry) => {
      const policy = nested(entry);
      return {
        provider_id: oneOf(policy["provider_id"], PROVIDERS) ?? "fixture",
        strategy: policy["strategy"],
        request_timeout_seconds: count(policy["request_timeout_seconds"]),
        helper_timeout_seconds: count(policy["helper_timeout_seconds"]),
        backoff_minutes: list(policy["backoff_minutes"]).map(count),
        max_concurrent_remote_reads: count(policy["max_concurrent_remote_reads"]),
        version: count(policy["version"]),
      };
    }),
  };
}

/** Reads one connection attempt's progress. */
function parseProgress(value: unknown): ConnectionProgress | null {
  const source = nested(value);
  const context = nested(source["context"]);
  switch (text(source["kind"])) {
    case "started":
      return { kind: "started" };
    case "awaiting_user":
      return { kind: "awaiting_user" };
    case "verified": {
      const state = oneOf(context["state"], CONNECTION_STATES);
      return state === null ? null : { kind: "verified", context: { state } };
    }
    case "failed": {
      const error = parseCommandError(context["error"]);
      return error === null ? null : { kind: "failed", context: { error } };
    }
    case "cancelled":
      return { kind: "cancelled" };
    default:
      return null;
  }
}

/** Reads confirmed native window state. */
function parseWindowState(value: unknown): OverviewWindowState | null {
  const source = nested(value);
  const body = nested(source["value"]);
  switch (text(source["kind"])) {
    case "confirmed": {
      const mode = oneOf(body["mode"], OVERVIEW_MODES);
      return mode === null
        ? null
        : {
            kind: "confirmed",
            value: {
              mode,
              always_on_top: flag(body["always_on_top"]),
              visible: flag(body["visible"]),
              geometry_revision: count(body["geometry_revision"]),
            },
          };
    }
    case "failed": {
      const error = parseCommandError(body["error"]);
      return error === null ? null : { kind: "failed", value: { error } };
    }
    default:
      return null;
  }
}

/**
 * Validates one decoded `CommandError` payload.
 *
 * Generated compile-time types do not validate a runtime payload (spec 8.4), so
 * the boundary narrows what actually arrived. An unrecognised shape is reported
 * as `null`, and the caller treats it as a transport failure rather than
 * inventing a domain error.
 */
export function parseCommandError(payload: unknown): CommandError | null {
  const source = nested(payload);
  const kind = text(source["kind"]);
  const context = nested(source["context"]);
  switch (kind) {
    case "initialization_pending":
      return { kind: "initialization_pending" };
    case "validation_failed":
      return {
        kind: "validation_failed",
        context: { field: text(context["field"]), reason: text(context["reason"]) },
      };
    case "account_not_found":
      return { kind: "account_not_found" };
    case "unsupported_provider":
      return {
        kind: "unsupported_provider",
        context: { provider_id: text(context["provider_id"]) },
      };
    case "unsupported_method":
      return {
        kind: "unsupported_method",
        context: { requested: text(context["requested"]) },
      };
    case "reconnect_required":
      return { kind: "reconnect_required" };
    case "permission_denied":
      return {
        kind: "permission_denied",
        context: { window_label: text(context["window_label"]) },
      };
    case "secure_store_unavailable":
      return { kind: "secure_store_unavailable" };
    case "revision_conflict":
      return {
        kind: "revision_conflict",
        context: { expected: count(context["expected"]), actual: count(context["actual"]) },
      };
    case "persistence_unavailable":
      return {
        kind: "persistence_unavailable",
        context: { owner: text(context["owner"]) },
      };
    case "native_operation_unsupported":
      return {
        kind: "native_operation_unsupported",
        context: { operation: text(context["operation"]) },
      };
    case "native_operation_failed":
      return {
        kind: "native_operation_failed",
        context: {
          operation: text(context["operation"]),
          reason: text(context["reason"]),
        },
      };
    case "cancelled":
      return { kind: "cancelled" };
    case "internal":
      return { kind: "internal", context: { code: text(context["code"]) } };
    default:
      return null;
  }
}

/** Whether retrying the same call could plausibly succeed unchanged. */
export function isRetryable(error: CommandError): boolean {
  return error.kind === "initialization_pending" || error.kind === "persistence_unavailable";
}

/* ------------------------------------------------------------------- calls */

/** A reader that validates one decoded command result. */
type ResultReader<T> = (value: unknown) => T | null;

/**
 * Runs one command and separates a domain error from a transport failure.
 *
 * A decoded success payload is validated before it is returned; a payload this
 * build cannot interpret is a malformed-response transport failure, never a
 * half-filled domain value.
 */
async function call<T>(
  command: string,
  args: Record<string, unknown>,
  read: ResultReader<T>,
): Promise<Invocation<T>> {
  try {
    const decoded = await invoke<unknown>(command, args);
    const result = read(decoded);
    if (result === null) {
      return { transportError: { code: "malformed_response", detail: command } };
    }
    return { ok: result };
  } catch (reason: unknown) {
    const error = parseCommandError(reason);
    if (error === null) {
      return {
        transportError: {
          code: "unknown_error",
          detail: reason instanceof Error ? reason.name : "command rejected",
        },
      };
    }
    return { error };
  }
}

/** Runs a command whose success payload carries no value. */
async function callVoid(
  command: string,
  args: Record<string, unknown>,
): Promise<Invocation<null>> {
  try {
    await invoke<unknown>(command, args);
    return { ok: null };
  } catch (reason: unknown) {
    const error = parseCommandError(reason);
    if (error === null) {
      return {
        transportError: {
          code: "unknown_error",
          detail: reason instanceof Error ? reason.name : "command rejected",
        },
      };
    }
    return { error };
  }
}

/** Reads the snapshot response, rejecting a snapshot this build cannot interpret. */
function readSnapshotResponse(value: unknown): SnapshotResponse | null {
  const snapshot = parseSnapshot(nested(value)["snapshot"]);
  return snapshot === null ? null : { snapshot };
}

/** Reads the accepted connection attempt. */
function readAttemptAccepted(value: unknown): ConnectionAttemptAccepted | null {
  const source = nested(value);
  const attemptId = text(source["attempt_id"]);
  return attemptId.length === 0
    ? null
    : {
        attempt_ref: { id: attemptId as ConnectionAttemptId },
        attempt_id: attemptId as ConnectionAttemptId,
      };
}

/** Reads one window-mode change. */
function readWindowModeChange(value: unknown): WindowModeChange | null {
  const source = nested(value);
  const body = nested(source["value"]);
  switch (text(source["kind"])) {
    case "applied": {
      const mode = oneOf(body, OVERVIEW_MODES);
      return mode === null ? null : { kind: "applied", value: mode };
    }
    case "refused": {
      const current = oneOf(body["current"], OVERVIEW_MODES);
      return current === null
        ? null
        : {
            kind: "refused",
            value: { reason: text(body["reason"]), current },
          };
    }
    default:
      return null;
  }
}

/** Reads one compiled adapter and its declared capabilities. */
function readRegisteredProvider(value: unknown): RegisteredProvider | null {
  const source = nested(value);
  const provider = oneOf(source["provider_id"], PROVIDERS);
  const capabilities = nested(source["capabilities"]);
  const cardinality = oneOf(capabilities["cardinality"], CARDINALITIES);
  if (provider === null || cardinality === null) {
    return null;
  }
  return {
    provider_id: provider,
    capabilities: {
      provider_id: provider,
      cardinality,
      supports_app_owned_authorization: flag(
        capabilities["supports_app_owned_authorization"],
      ),
      supports_external_profile: flag(capabilities["supports_external_profile"]),
      reports_monthly_window: flag(capabilities["reports_monthly_window"]),
      minimum_interval_seconds: count(capabilities["minimum_interval_seconds"]),
    },
    compiled_in_this_build: flag(source["compiled_in_this_build"]),
  };
}

/** Reads the diagnostic-export summary. */
function readDiagnosticExport(value: unknown): DiagnosticExport | null {
  const source = nested(value);
  const label = text(source["destination_label"]);
  return label.length === 0
    ? null
    : { destination_label: label, byte_count: count(source["byte_count"]) };
}

/** Reads the current snapshot and the revision to reconcile against. */
export function getSnapshot(): Promise<Invocation<SnapshotResponse>> {
  return call("get_snapshot", {}, readSnapshotResponse);
}

/** Requests a read of the selected accounts. */
export function refreshAccounts(request: RefreshAccountsRequest): Promise<Invocation<null>> {
  return callVoid("refresh_accounts", { request });
}

/** Starts or stops scheduled reads. */
export function setMonitoringState(
  request: SetMonitoringStateRequest,
): Promise<Invocation<MonitoringState>> {
  return call("set_monitoring_state", { request }, parseMonitoringState);
}

/** Enables or disables one account. */
export function setAccountEnabled(request: SetAccountEnabledRequest): Promise<Invocation<null>> {
  return callVoid("set_account_enabled", { request });
}

/** Renames one account. Presentation only. */
export function renameAccount(request: RenameAccountRequest): Promise<Invocation<null>> {
  return callVoid("rename_account", { request });
}

/** Disconnects one account. Siblings from the same provider are untouched. */
export function disconnectAccount(request: DisconnectAccountRequest): Promise<Invocation<null>> {
  return callVoid("disconnect_account", { request });
}

/** Begins an authorized connection attempt. */
export function beginConnection(
  request: BeginConnectionRequest,
): Promise<Invocation<ConnectionAttemptAccepted>> {
  return call("begin_connection", { request }, readAttemptAccepted);
}

/** Cancels a live connection attempt. */
export function cancelConnection(request: CancelConnectionRequest): Promise<Invocation<null>> {
  return callVoid("cancel_connection", { request });
}

/** Lists the compiled adapters and what each declares. */
export function listProviderCapabilities(): Promise<Invocation<readonly RegisteredProvider[]>> {
  return call(
    "list_provider_capabilities",
    {},
    (value): readonly RegisteredProvider[] | null => {
      const providers: RegisteredProvider[] = [];
      for (const entry of list(value)) {
        const provider = readRegisteredProvider(entry);
        if (provider === null) {
          return null;
        }
        providers.push(provider);
      }
      return providers;
    },
  );
}

/** Moves the overview between floating and tray mode. */
export function setOverviewMode(
  request: WindowModeChangeRequest,
): Promise<Invocation<WindowModeChange>> {
  return call("set_overview_mode", { request }, readWindowModeChange);
}

/** Sets the independent always-on-top preference. */
export function setOverviewAlwaysOnTop(
  request: SetOverviewAlwaysOnTopRequest,
): Promise<Invocation<OverviewWindowState>> {
  return call("set_overview_always_on_top", { request }, parseWindowState);
}

/** Widens the overview to fit every account within the work area. */
export function fitOverviewToAccounts(): Promise<Invocation<OverviewWindowState>> {
  return call("fit_overview_to_accounts", {}, parseWindowState);
}

/** Returns the overview to a visible work area. */
export function resetOverviewPosition(): Promise<Invocation<OverviewWindowState>> {
  return call("reset_overview_position", {}, parseWindowState);
}

/** Saves preferences. Emits `PreferencesChanged` only after the save succeeds. */
export function updatePreferences(
  request: UpdatePreferencesRequest,
): Promise<Invocation<Preferences>> {
  return call("update_preferences", { request }, parsePreferences);
}

/** Opens one provider's usage page in the external browser. */
export function openProviderUsagePage(
  request: OpenProviderUsagePageRequest,
): Promise<Invocation<null>> {
  return callVoid("open_provider_usage_page", { request });
}

/** Drops retained local history for one account, or for every account. */
export function clearLocalHistory(
  request: ClearLocalHistoryRequest,
): Promise<Invocation<null>> {
  return callVoid("clear_local_history", { request });
}

/** Writes a sanitized diagnostic export. */
export function exportSanitizedDiagnostics(): Promise<Invocation<DiagnosticExport>> {
  return call("export_sanitized_diagnostics", {}, readDiagnosticExport);
}

/* --------------------------------------------------------------- listeners */

/** The renderer-side handlers for the six typed backend events. */
export interface EventHandlers {
  readonly onSnapshotUpdated?: (payload: SnapshotUpdated) => void;
  readonly onConnectionProgressChanged?: (payload: ConnectionProgressChanged) => void;
  readonly onPreferencesChanged?: (payload: PreferencesChanged) => void;
  readonly onMonitoringStateChanged?: (payload: MonitoringStateChanged) => void;
  readonly onOverviewWindowStateChanged?: (payload: OverviewWindowStateChanged) => void;
  readonly onPersistenceStatusChanged?: (payload: PersistenceStatusChanged) => void;
}

/** Reads a `SnapshotUpdated` payload, or rejects one this build cannot interpret. */
function parseSnapshotUpdated(value: unknown): SnapshotUpdated | null {
  const source = nested(value);
  const instance = text(source["app_instance_id"]);
  const snapshot = parseSnapshot(source["snapshot"]);
  return instance.length === 0 || snapshot === null
    ? null
    : {
        app_instance_id: instance as AppInstanceId,
        revision: count(source["revision"]),
        schema_version: count(source["schema_version"]),
        snapshot,
      };
}

/** Reads a `ConnectionProgressChanged` payload. */
function parseConnectionProgressChanged(
  value: unknown,
): ConnectionProgressChanged | null {
  const source = nested(value);
  const attempt = text(source["attempt_id"]);
  const progress = parseProgress(source["progress"]);
  return attempt.length === 0 || progress === null
    ? null
    : {
        app_instance_id: text(source["app_instance_id"]) as AppInstanceId,
        attempt_id: attempt as ConnectionAttemptId,
        attempt_revision: count(source["attempt_revision"]),
        progress,
      };
}

/** Reads a `PreferencesChanged` payload. */
function parsePreferencesChanged(value: unknown): PreferencesChanged | null {
  const source = nested(value);
  const preferences = parsePreferences(source["preferences"]);
  return preferences === null
    ? null
    : {
        app_instance_id: text(source["app_instance_id"]) as AppInstanceId,
        preference_revision: count(source["preference_revision"]),
        preferences,
      };
}

/** Reads a `MonitoringStateChanged` payload. */
function parseMonitoringStateChanged(value: unknown): MonitoringStateChanged | null {
  const source = nested(value);
  const state = parseMonitoringState(source["monitoring_state"]);
  return state === null
    ? null
    : {
        app_instance_id: text(source["app_instance_id"]) as AppInstanceId,
        monitoring_revision: count(source["monitoring_revision"]),
        monitoring_state: state,
      };
}

/** Reads an `OverviewWindowStateChanged` payload. */
function parseOverviewWindowStateChanged(
  value: unknown,
): OverviewWindowStateChanged | null {
  const source = nested(value);
  const state = parseWindowState(source["state"]);
  return state === null
    ? null
    : {
        app_instance_id: text(source["app_instance_id"]) as AppInstanceId,
        state,
      };
}

/** Reads a `PersistenceStatusChanged` payload. */
function parsePersistenceStatusChanged(value: unknown): PersistenceStatusChanged | null {
  const source = nested(value);
  const status = parsePersistenceStatus(source["status"]);
  return status === null
    ? null
    : {
        app_instance_id: text(source["app_instance_id"]) as AppInstanceId,
        status,
      };
}

/**
 * Registers every requested listener, then returns its unlisten functions.
 *
 * Listener registration is asynchronous. A caller that unmounts before this
 * promise settles must unsubscribe the functions it receives, so this function
 * returns them all and never registers on its own after a rejection.
 */
export async function attachEventHandlers(
  handlers: EventHandlers,
): Promise<readonly UnlistenFn[]> {
  const attached: UnlistenFn[] = [];
  const snapshot = handlers.onSnapshotUpdated;
  if (snapshot !== undefined) {
    attached.push(
      await listen<unknown>("SnapshotUpdated", (message) => {
        const payload = parseSnapshotUpdated(message.payload);
        if (payload !== null) {
          snapshot(payload);
        }
      }),
    );
  }
  const progress = handlers.onConnectionProgressChanged;
  if (progress !== undefined) {
    attached.push(
      await listen<unknown>("ConnectionProgressChanged", (message) => {
        const payload = parseConnectionProgressChanged(message.payload);
        if (payload !== null) {
          progress(payload);
        }
      }),
    );
  }
  const preferences = handlers.onPreferencesChanged;
  if (preferences !== undefined) {
    attached.push(
      await listen<unknown>("PreferencesChanged", (message) => {
        const payload = parsePreferencesChanged(message.payload);
        if (payload !== null) {
          preferences(payload);
        }
      }),
    );
  }
  const monitoring = handlers.onMonitoringStateChanged;
  if (monitoring !== undefined) {
    attached.push(
      await listen<unknown>("MonitoringStateChanged", (message) => {
        const payload = parseMonitoringStateChanged(message.payload);
        if (payload !== null) {
          monitoring(payload);
        }
      }),
    );
  }
  const windowState = handlers.onOverviewWindowStateChanged;
  if (windowState !== undefined) {
    attached.push(
      await listen<unknown>("OverviewWindowStateChanged", (message) => {
        const payload = parseOverviewWindowStateChanged(message.payload);
        if (payload !== null) {
          windowState(payload);
        }
      }),
    );
  }
  const persistence = handlers.onPersistenceStatusChanged;
  if (persistence !== undefined) {
    attached.push(
      await listen<unknown>("PersistenceStatusChanged", (message) => {
        const payload = parsePersistenceStatusChanged(message.payload);
        if (payload !== null) {
          persistence(payload);
        }
      }),
    );
  }
  return attached;
}
