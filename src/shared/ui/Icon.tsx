/**
 * The line-icon set.
 *
 * Paths are copied from the approved wireframe so the rendered geometry matches
 * it exactly. Decorative icons are hidden from assistive technology; the
 * meaning is always carried by adjacent text or an accessible name.
 */
import type { JSX } from "react";

/** One icon's name. */
export type IconName =
  | "arrow-left"
  | "arrow-right"
  | "bell"
  | "check"
  | "chevron-down"
  | "chevron-right"
  | "chevron-up"
  | "clock"
  | "close"
  | "donut"
  | "download"
  | "external"
  | "layers"
  | "list"
  | "link"
  | "moon"
  | "mouse"
  | "palette"
  | "pause"
  | "pin"
  | "play"
  | "plus"
  | "power"
  | "refresh"
  | "search"
  | "settings"
  | "shield"
  | "sun"
  | "terminal"
  | "user"
  | "warning";

/** The path data for every icon, on a 24-unit view box. */
const PATHS: Record<IconName, JSX.Element> = {
  "arrow-left": <path d="M20 12H5m6-6-6 6 6 6" />,
  "arrow-right": <path d="M4 12h15m-6-6 6 6-6 6" />,
  bell: <path d="M18 8a6 6 0 0 0-12 0c0 7-3 7-3 9h18c0-2-3-2-3-9ZM10 21h4" />,
  check: <path d="m5 12 4 4L19 6" />,
  "chevron-down": <path d="m6 9 6 6 6-6" />,
  "chevron-right": <path d="m9 5 7 7-7 7" />,
  "chevron-up": <path d="m6 15 6-6 6 6" />,
  clock: (
    <>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 7v5l3 2" />
    </>
  ),
  close: <path d="m6 6 12 12M6 18 18 6" />,
  donut: (
    <>
      <circle cx="12" cy="12" r="9" />
      <circle cx="12" cy="12" r="4" />
      <path d="M12 3v5" />
    </>
  ),
  download: <path d="M12 3v12m-5-5 5 5 5-5M4 17v4h16v-4" />,
  external: (
    <>
      <path d="M14 3h7v7m0-7-11 11" />
      <path d="M10 3H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2v-5" />
    </>
  ),
  layers: <path d="m12 3 10 5-10 5L2 8l10-5Zm-10 9 10 5 10-5M2 16l10 5 10-5" />,
  link: <path d="m10 8 3-3a5 5 0 0 1 7 7l-3 3M14 16l-3 3a5 5 0 0 1-7-7l3-3M8 16l8-8" />,
  moon: <path d="M21 13a9 9 0 0 1-10-10 9 9 0 1 0 10 10Z" />,
  mouse: (
    <>
      <rect x="6" y="2" width="12" height="20" rx="6" />
      <path d="M12 3v5" />
    </>
  ),
  palette: (
    <>
      <path d="M12 3a9 9 0 1 0 0 18c2 0 3-1 2-3s0-3 2-3h2c5 0 4-12-6-12Z" />
      <path d="M7 9h.1M10 6h.1M15 6h.1M18 10h.1" />
    </>
  ),
  pause: <path d="M8 5v14M16 5v14" />,
  pin: <path d="M9 3h6l-1 6 4 4v2H6v-2l4-4-1-6ZM12 15v6" />,
  play: <path d="m7 4 14 8-14 8V4Z" />,
  plus: <path d="M12 5v14M5 12h14" />,
  power: <path d="M12 2v10M5 5a9 9 0 1 0 14 0" />,
  refresh: (
    <>
      <path d="M20 7v5h-5M4 17v-5h5" />
      <path d="M6.1 7a7 7 0 0 1 11.6-2L20 8M4 16l2.3 3A7 7 0 0 0 18 17" />
    </>
  ),
  list: <path d="M8 5h12M8 12h12M8 19h12M3 5h.1M3 12h.1M3 19h.1" />,
  search: (
    <>
      <circle cx="10" cy="10" r="6" />
      <path d="m15 15 6 6" />
    </>
  ),
  settings: (
    <>
      <path d="m9 4 1-2h4l1 2 2 1 2-.2 2 3.4-1.3 1.7v2.2L21 14l-2 3.4-2-.2-2 1-1 2h-4l-1-2-2-1-2 .2L3 14l1.3-1.9V9.9L3 8.2l2-3.4L7 5l2-1Z" />
      <circle cx="12" cy="11" r="3" />
    </>
  ),
  shield: (
    <>
      <path d="m12 3-8 3v6c0 5 8 9 8 9s8-4 8-9V6l-8-3Z" />
      <path d="m8 12 3 3 5-6" />
    </>
  ),
  sun: (
    <>
      <circle cx="12" cy="12" r="4" />
      <path d="M12 2v2m0 16v2M2 12h2m16 0h2M5 5l1.4 1.4m11.2 11.2L19 19M5 19l1.4-1.4M17.6 6.4 19 5" />
    </>
  ),
  terminal: (
    <>
      <rect x="3" y="4" width="18" height="16" rx="2" />
      <path d="m6 8 4 4-4 4m7 0h5" />
    </>
  ),
  user: (
    <>
      <circle cx="12" cy="7" r="4" />
      <path d="M4 21v-2a8 8 0 0 1 16 0v2" />
    </>
  ),
  warning: (
    <>
      <path d="m12 3 10 18H2L12 3Z" />
      <path d="M12 9v5m0 3v.1" />
    </>
  ),
};

/**
 * Renders one icon.
 *
 * Its size comes from the stylesheet, which sizes each icon by where it sits,
 * exactly as the wireframe does.
 */
export function Icon({ name }: { name: IconName }): JSX.Element {
  return (
    <svg className="icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
      {PATHS[name]}
    </svg>
  );
}

/** The Quota mark, drawn from the wireframe's geometry. */
export function Logo(): JSX.Element {
  return (
    <svg
      className="logo"
      viewBox="0 0 32 32"
      fill="none"
      aria-hidden="true"
      focusable="false"
    >
      <circle
        cx="15"
        cy="15"
        r="10"
        stroke="currentColor"
        strokeWidth="3.6"
        opacity=".2"
      />
      <path
        d="M15 5a10 10 0 1 1-9.5 6.9"
        stroke="currentColor"
        strokeWidth="3.6"
        strokeLinecap="round"
      />
      <path
        d="m21 22 5 5"
        stroke="currentColor"
        strokeWidth="3.6"
        strokeLinecap="round"
      />
    </svg>
  );
}
