/**
 * The snapshot subscription lifecycle.
 *
 * One subscription per webview. The Effect registers the listeners and requests
 * the snapshot once; its cleanup stops both. Registration is asynchronous, so a
 * cleanup that runs before registration settles unsubscribes the listeners the
 * moment they arrive (spec 7.8.3, AC-85).
 *
 * The read follows the registration rather than racing it. The host answers the
 * first read with the preferences and window state as events, so a read that ran
 * first would deliver them to listeners that did not exist yet and Settings
 * would wait for them for ever.
 */
import { useEffect } from "react";

import { reconcileSnapshot, startSnapshotSubscription } from "../shared/ipc/subscription";

/**
 * Owns the webview's subscription for the lifetime of the component.
 *
 * Under Strict Mode the Effect runs, cleans up, and runs again. The store is
 * module state, not component state, so the second run does not duplicate
 * anything: the first subscription is stopped before the second starts.
 */
export function useSnapshotSubscription(): void {
  useEffect(() => {
    let stopped = false;
    let stop: (() => void) | null = null;

    const start = async (): Promise<void> => {
      const detach = await startSnapshotSubscription();
      if (stopped) {
        detach();
        return;
      }
      stop = detach;
      await reconcileSnapshot();
    };

    start().catch(() => {
      // A registration failure leaves the renderer without events. The read is
      // still attempted, because it recovers the accounts even though the
      // supplemental preferences and window state arrived as events.
      if (!stopped) {
        reconcileSnapshot().catch(() => {
          // `reconcileSnapshot` records its own failure in the store.
        });
      }
    });

    return () => {
      stopped = true;
      if (stop !== null) {
        stop();
      }
    };
  }, []);
}
