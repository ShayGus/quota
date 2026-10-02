/**
 * Theme and motion resolution.
 *
 * Each is a confirmed preference. `system` follows the operating system and must
 * react when that changes, so the media query is read through a subscription
 * rather than captured once: a cached value left Follow system showing the wrong
 * scheme after the system changed while the listener was detached.
 */
import { useEffect, useSyncExternalStore } from "react";

import type { Theme } from "../generated/bindings";
import type { RendererState } from "../shared/state/types";

/** The colour scheme actually painted. */
export type ResolvedTheme = "light" | "dark";

/** Whether the operating system currently asks for a dark colour scheme. */
export function systemPrefersDark(): boolean {
  return window.matchMedia("(prefers-color-scheme: dark)").matches;
}

/** The scheme a chosen theme resolves to right now. */
export function resolveTheme(chosen: Theme, prefersDark: boolean): ResolvedTheme {
  if (chosen === "light" || chosen === "dark") {
    return chosen;
  }
  return prefersDark ? "dark" : "light";
}

/** The theme the current state resolves to, before the media query is applied. */
export function chosenTheme(state: RendererState): Theme {
  return state.preferences?.theme ?? "system";
}

/** Applies a resolved theme to the document element. */
export function applyTheme(theme: ResolvedTheme): void {
  document.documentElement.dataset["theme"] = theme;
}

/** Reads the theme the current state resolves to, following the system setting. */
export function useTheme(state: RendererState): ResolvedTheme {
  const chosen = chosenTheme(state);
  const prefersDark = useSyncExternalStore(subscribeSystemTheme, systemPrefersDark);
  const resolved = resolveTheme(chosen, prefersDark);
  // Both are confirmed preferences, so both are applied at the document rather
  // than left to the operating system.
  const reduceMotion = state.preferences?.reduce_motion ?? false;
  useEffect(() => {
    applyTheme(resolved);
    document.documentElement.dataset["reduceMotion"] = String(reduceMotion);
  }, [resolved, reduceMotion]);
  return resolved;
}

function subscribeSystemTheme(onChange: () => void): () => void {
  const query = window.matchMedia("(prefers-color-scheme: dark)");
  query.addEventListener("change", onChange);
  return () => {
    query.removeEventListener("change", onChange);
  };
}
