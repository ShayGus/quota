/**
 * The lifecycle of one webview's subscription to the backend.
 *
 * The snapshot store is filled by:
 *  1. registering every typed event listener through the generated bindings;
 *  2. then requesting the snapshot once (spec 7.9).
 *
 * If the component unmounts while registration is still pending, the listeners
 * that do arrive are unsubscribed immediately, so Strict Mode's mount/unmount
 * pair leaves exactly one active subscription (AC-85).
 */
import { commands, events } from "../../generated/bindings";
import { launch, reportCommandError, reportTransportFailure } from "./report";
import {
  acceptAttempt,
  acceptMonitoring,
  acceptNativeWindow,
  acceptPersistence,
  acceptPreferences,
  acceptSnapshot,
  setFailure,
  setLink,
} from "../state/store";

/**
 * Starts the subscription and resolves with the function that stops it.
 *
 * Registration is asynchronous, so a caller that unmounts before this resolves
 * must call the returned function as soon as it arrives. The caller owns that
 * contract; this function cannot observe its own cancellation.
 */
export async function startSnapshotSubscription(): Promise<() => void> {
  const detaches = await Promise.all([
    events.snapshotUpdated.listen((event) => {
      const payload = event.payload;
      acceptSnapshot(payload.snapshot);
      setFailure(null);
    }),
    events.connectionProgressChanged.listen((event) => {
      const payload = event.payload;
      acceptAttempt({
        attemptId: payload.attempt_id,
        revision: payload.attempt_revision,
        progress: payload.progress,
      });
      if (payload.progress.kind === "verified") launch(reconcileSnapshot());
    }),
    events.preferencesChanged.listen((event) => {
      const payload = event.payload;
      acceptPreferences(payload.preferences);
    }),
    events.monitoringStateChanged.listen((event) => {
      const payload = event.payload;
      acceptMonitoring(payload.monitoring_state);
    }),
    events.overviewWindowStateChanged.listen((event) => {
      const payload = event.payload;
      acceptNativeWindow(payload.state);
    }),
    events.persistenceStatusChanged.listen((event) => {
      const payload = event.payload;
      acceptPersistence(payload.status);
    }),
  ]);

  let stopped = false;
  return () => {
    if (stopped) {
      return;
    }
    stopped = true;
    for (const detach of detaches) {
      detach();
    }
  };
}

// The host answers the first read with the preferences and window state as
// events, so a read that arrives before bootstrap finishes loses them. These
// bound how long the renderer waits for a backend that is still starting.
const STARTUP_READ_ATTEMPTS = 40;
const STARTUP_READ_BACKOFF_MS = 250;

/**
 * Performs the one-time snapshot read.
 *
 * This is a reconciliation read, not a polling loop: the renderer never
 * schedules provider work (spec 7.8, spec 7.9).
 *
 * A failure here does not clear accounts and does not restart anything. The
 * previous projection stays on screen, and the link state says what happened.
 */
export async function reconcileSnapshot(): Promise<void> {
  setLink("reconciling");
  for (let attempt = 0; attempt < STARTUP_READ_ATTEMPTS; attempt += 1) {
    try {
      const result = await commands.getSnapshot();
      if (result.status === "error") {
        // The host answers the first read with the preferences and window state
        // as events, so a read that arrives before bootstrap finishes loses
        // them. Retrying is how that read is recovered.
        if (
          result.error.kind === "initialization_pending" &&
          attempt + 1 < STARTUP_READ_ATTEMPTS
        ) {
          const { promise, resolve } = Promise.withResolvers<undefined>();
          setTimeout(resolve, STARTUP_READ_BACKOFF_MS);
          await promise;
          continue;
        }
        reportCommandError(result.error);
        setLink("unavailable");
        return;
      }
      acceptSnapshot(result.data.snapshot);
      setFailure(null);
      return;
    } catch (error) {
      reportTransportFailure({
        code: "unavailable",
        detail: error instanceof Error ? error.name : "command rejected",
      });
      return;
    }
  }
}
