/**
 * Theme resolution.
 *
 * The chosen theme is a confirmed preference. `system` follows the operating
 * system and must react when that changes, so a media-query listener is owned
 * by an Effect with a matching cleanup (spec 7.8.3).
 */
import { useEffect, useState } from "react";

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
  const [prefersDark, setPrefersDark] = useState(() => systemPrefersDark());
  useEffect(() => {
    if (chosen !== "system") {
      return;
    }
    const query = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = (event: MediaQueryListEvent): void => {
      setPrefersDark(event.matches);
    };
    query.addEventListener("change", onChange);
    return () => {
      query.removeEventListener("change", onChange);
    };
  }, [chosen]);
  const resolved = resolveTheme(chosen, prefersDark);
  useEffect(() => {
    applyTheme(resolved);
  }, [resolved]);
  return resolved;
}
