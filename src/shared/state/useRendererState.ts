/**
 * The single React entry point into the store.
 *
 * `useSyncExternalStore` is the only way state enters React (spec 7.8.3). The
 * subscribe function is stable, so subscribing does not re-run on every render.
 */
import { useSyncExternalStore } from "react";

import { getRendererState, subscribe } from "./store";
import type { RendererState } from "./types";

/** The current renderer state. The same object when nothing changed. */
export function useRendererState(): RendererState {
  return useSyncExternalStore(subscribe, getRendererState, getRendererState);
}
