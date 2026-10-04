/**
 * What the update pop-up is showing.
 *
 * The window is created when an update is found, and the host may already have
 * said what to show before this window could listen. So it listens first, then
 * asks once; an answer to that question never replaces something newer that
 * arrived as an event in the meantime.
 */
import { useEffect, useState } from "react";

import type { UpdatePrompt } from "../../generated/bindings";
import { launch } from "../../shared/ipc/report";
import { listenForUpdatePrompt, readUpdatePrompt } from "../../shared/ipc/update";

/** The current prompt, or `null` until the host has said. */
export function useUpdatePrompt(): UpdatePrompt | null {
  const [prompt, setPrompt] = useState<UpdatePrompt | null>(null);

  useEffect(() => {
    const life = { stopped: false, heardEvent: false };
    // Read through functions: the Effect's cleanup changes them behind an await.
    const isStopped = (): boolean => life.stopped;
    const heardEvent = (): boolean => life.heardEvent;
    let stop: (() => void) | null = null;

    const start = async (): Promise<void> => {
      const detach = await listenForUpdatePrompt((next) => {
        life.heardEvent = true;
        setPrompt(next);
      });
      if (isStopped()) {
        detach();
        return;
      }
      stop = detach;
      const current = await readUpdatePrompt();
      if (!isStopped() && !heardEvent() && current !== null) {
        setPrompt(current);
      }
    };
    launch(start());

    return () => {
      life.stopped = true;
      stop?.();
    };
  }, []);

  return prompt;
}
