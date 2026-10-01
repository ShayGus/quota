/**
 * The immutable renderer state.
 *
 * Every value on screen is read from this projection. The store replaces the
 * whole object on a change and reuses it otherwise, so a consumer can compare
 * with `Object.is` (spec 7.8.3).
 */
import type {
  AccountId,
  AppSnapshot,
  CommandError,
  ConnectionAttemptId,
  ConnectionProgress,
  MonitoringState,
  OverviewWindowState,
  PersistenceStatus,
  Preferences,
  TransportFailure,
} from "../../generated/bindings";

/** How far the renderer has got with the backend. */
export type LinkState = "connecting" | "live" | "reconciling" | "unavailable";

/** One reported failure. A transport failure is never a domain error. */
export type RendererFailure =
  | { readonly kind: "domain"; readonly error: CommandError }
  | { readonly kind: "transport"; readonly failure: TransportFailure };

/** The progress of one connection attempt, as last reported. */
export interface AttemptProgress {
  readonly attemptId: ConnectionAttemptId;
  readonly revision: number;
  readonly progress: ConnectionProgress;
}

/** Everything the renderer knows, at one instant. */
export interface RendererState {
  readonly link: LinkState;
  readonly failure: RendererFailure | null;
  readonly snapshot: AppSnapshot | null;
  /** The confirmed monitoring state, or `null` before the backend has answered. */
  readonly monitoring: MonitoringState | null;
  /** The confirmed storage availability, or `null` before the backend has answered. */
  readonly persistence: PersistenceStatus | null;
  readonly preferences: Preferences | null;
  readonly nativeWindow: OverviewWindowState | null;
  readonly attempts: readonly AttemptProgress[];
  /** The account order currently displayed. Presentation only. */
  readonly appliedOrder: readonly AccountId[];
  /** A newer order waiting for a safe idle point, or `null`. */
  readonly pendingOrder: readonly AccountId[] | null;
}

/** The state before the first snapshot arrives. It is not an empty account list. */
export const initialRendererState: RendererState = {
  link: "connecting",
  failure: null,
  snapshot: null,
  monitoring: null,
  persistence: null,
  preferences: null,
  nativeWindow: null,
  attempts: [],
  appliedOrder: [],
  pendingOrder: null,
};
