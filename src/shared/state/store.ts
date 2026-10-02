/**
 * The snapshot store.
 *
 * One cached immutable projection per webview. The store keeps one object and
 * replaces it only when a real revision changes something, so
 * `useSyncExternalStore` sees a stable reference and does not re-render on an
 * unchanged read (spec 7.8.3).
 *
 * The store is a plain module, not a React value, so it is also usable from
 * tests without a renderer.
 */
import type {
  ConnectionAttemptId,
  AccountId,
  AppSnapshot,
  MonitoringState,
  OverviewWindowState,
  PersistenceStatus,
  Preferences,
} from "../../generated/bindings";
import { canonicalOrder } from "./order";
import {
  initialRendererState,
  type AttemptProgress,
  type LinkState,
  type RendererFailure,
  type RendererState,
} from "./types";

/** Anything that can be compared by identity for a change. */
type Listener = () => void;

let state: RendererState = initialRendererState;
const listeners = new Set<Listener>();

/** The current immutable projection. The same object until something changes. */
export function getRendererState(): RendererState {
  return state;
}

/** Subscribes to changes. The returned function is a no-op after the first call. */
export function subscribe(listener: Listener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** Applies a change, then notifies only when the object really changed. */
function commit(next: RendererState): void {
  if (Object.is(next, state)) {
    return;
  }
  state = next;
  for (const listener of [...listeners]) {
    listener();
  }
}

/** Records how far the renderer has got with the backend. */
export function setLink(link: LinkState): void {
  commit({ ...state, link });
}

/** Records the latest failure, or clears it. */
export function setFailure(failure: RendererFailure | null): void {
  commit({ ...state, failure });
}

/**
 * Accepts a snapshot, applying the revision rules of spec 7.9.
 *
 * A snapshot from a different application instance replaces the current one,
 * because a restart makes the old revisions incomparable. Within one instance,
 * only a strictly newer revision is accepted, so an older command response can
 * never roll back values or order (AC-83). A newer revision computes the new
 * presentation order and stages it as pending; the order is applied at a safe
 * idle point by `applyPendingOrder`.
 */
export function acceptSnapshot(snapshot: AppSnapshot): void {
  const current = state.snapshot;
  if (
    current !== null &&
    current.app_instance_id === snapshot.app_instance_id &&
    snapshot.revision <= current.revision
  ) {
    return;
  }
  const staged = canonicalOrder(snapshot.accounts);
  commit({
    ...state,
    link: "live",
    snapshot,
    monitoring: snapshot.monitoring_state,
    persistence: snapshot.persistence_status,
    pendingOrder: staged,
  });
}

/** Replaces the confirmed preferences. The aggregate arrives only after a save. */
export function acceptPreferences(preferences: Preferences): void {
  commit({ ...state, preferences, link: "live" });
}

/** Replaces the confirmed monitoring state. */
export function acceptMonitoring(monitoring: MonitoringState): void {
  commit({ ...state, monitoring });
}

/** Replaces the confirmed storage availability. */
export function acceptPersistence(persistence: PersistenceStatus): void {
  commit({ ...state, persistence });
}

/** Replaces the confirmed native window state. */
export function acceptNativeWindow(nativeWindow: OverviewWindowState): void {
  commit({ ...state, nativeWindow });
}

/**
 * Records the latest progress of one connection attempt.
 *
 * Only a strictly newer revision is accepted within one attempt. The command
 * response and the progress event for the same attempt race: the backend can
 * report a terminal failure before the reply that started it arrives. Comparing
 * revisions before filtering the old entry keeps that failure on screen instead
 * of letting a later, older `Started` event replace it.
 */
export function acceptAttempt(attempt: AttemptProgress): void {
  const current = state.attempts.find((entry) => entry.attemptId === attempt.attemptId);
  if (current !== undefined && current.revision >= attempt.revision) {
    return;
  }
  const others = state.attempts.filter((entry) => entry.attemptId !== attempt.attemptId);
  commit({ ...state, attempts: [...others, attempt] });
}

/**
 * Drops a finished attempt once its result is on screen.
 *
 * Without this the store would keep one entry per connection for the life of
 * the process. Only a terminal progress value may be dropped, so an attempt
 * that is still running can never be forgotten.
 */
export function clearAttempt(attemptId: ConnectionAttemptId): void {
  const current = state.attempts.find((entry) => entry.attemptId === attemptId);
  if (current === undefined || current.progress.kind === "started") {
    return;
  }
  commit({
    ...state,
    attempts: state.attempts.filter((entry) => entry.attemptId !== attemptId),
  });
}

/**
 * Applies the staged presentation order.
 *
 * Called when the overview is first opened, from the “Update order” control, and
 * from the deferred reorder timer once the list is idle (spec 4.3). Row identity
 * and focus are unaffected, because the order is keyed by account ID.
 */
export function applyPendingOrder(): void {
  const order = state.pendingOrder;
  if (order === null) {
    return;
  }
  commit({ ...state, appliedOrder: order, pendingOrder: null });
}

/**
 * The current order for one snapshot, for a caller that has none in the store.
 *
 * Used by the error-recovery path, which rebuilds presentation order without
 * clearing accounts or restarting polling.
 */
export function orderFor(snapshot: AppSnapshot): readonly AccountId[] {
  return canonicalOrder(snapshot.accounts);
}

/** Clears every cached value. Used only when the backend instance is replaced. */
export function resetRendererState(): void {
  commit(initialRendererState);
}

/** The number of live subscribers. Used by the Strict Mode subscription test. */
export function subscriberCount(): number {
  return listeners.size;
}

/** The confirmed native state, when the mode and topmost values are known. */
export function currentNativeWindow(): OverviewWindowState | null {
  return state.nativeWindow;
}
