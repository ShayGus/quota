/**
 * The displayed-countdown clock.
 *
 * This is the only renderer timer. It refreshes labels that show how long is
 * left; it does not refill a quota, change freshness, change rank, or schedule
 * any request (spec 7.8). `Date.now()` is never called during render, because a
 * render must be pure.
 */
import { useEffect, useState } from "react";

/** How often a visible countdown label is recomputed. */
export const CLOCK_TICK_MS = 30_000;

/** The current instant, refreshed on a timer while the component is mounted. */
export function useNow(): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = window.setInterval(() => {
      setNow(Date.now());
    }, CLOCK_TICK_MS);
    return () => {
      window.clearInterval(timer);
    };
  }, []);
  return now;
}
