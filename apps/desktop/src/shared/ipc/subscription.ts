/**
 * The lifecycle of one webview's subscription to the backend.
 *
 * The snapshot store is filled by:
 *  1. registering every typed event listener;
 *  2. then requesting the snapshot once (spec 7.9).
 *
 * If the component unmounts while registration is still pending, the listeners
 * that do arrive are unsubscribed immediately, so Strict Mode's mount/unmount
 * pair leaves exactly one active subscription (AC-85).
 */
import { attachEventHandlers, getSnapshot } from "../../generated/bindings";
import { reportTransportFailure } from "./report";
import {
  acceptAttempt,
  acceptMonitoring,
  acceptPersistence,
  acceptPreferences,
  acceptSnapshot,
  acceptNativeWindow,
  setFailure,
  setLink,
} from "../state/store";

/** Starts the subscription. Resolves with the function that stops it. */
export async function startSnapshotSubscription(): Promise<() => void> {
  let stopped = false;
  const unlisten = await attachEventHandlers({
    onSnapshotUpdated: (payload) => {
      acceptSnapshot(payload.snapshot);
      setFailure(null);
    },
    onConnectionProgressChanged: (payload) => {
      acceptAttempt({
        attemptId: payload.attempt_id,
        revision: payload.attempt_revision,
        progress: payload.progress,
      });
    },
    onPreferencesChanged: (payload) => {
      acceptPreferences(payload.preferences);
    },
    onMonitoringStateChanged: (payload) => {
      acceptMonitoring(payload.monitoring_state);
    },
    onOverviewWindowStateChanged: (payload) => {
      acceptNativeWindow(payload.state);
    },
    onPersistenceStatusChanged: (payload) => {
      acceptPersistence(payload.status);
    },
  });
  if (stopped) {
    for (const detach of unlisten) {
      detach();
    }
    return () => undefined;
  }
  return () => {
    stopped = true;
    for (const detach of unlisten) {
      detach();
    }
  };
}

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
  const result = await getSnapshot();
  if ("transportError" in result) {
    reportTransportFailure(result.transportError);
    return;
  }
  if ("error" in result) {
    setFailure({ kind: "domain", error: result.error });
    setLink("unavailable");
    return;
  }
  acceptSnapshot(result.ok.snapshot);
  setFailure(null);
}
