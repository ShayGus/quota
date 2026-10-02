/**
 * The renderer entry point.
 *
 * Strict Mode is on in development, so the subscription lifecycle is exercised
 * by the mount/unmount/mount cycle (spec 7.8).
 */
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "./app/App";
// Development-only. `setupPluginListeners` forwards console.* calls and
// observed IPC traffic into the inspection plugin's ring buffers, which is
// what lets an agent read them. It is imported statically because Vite removes
// the branch at build time, and the release bundle must not contain it; see
// `docs/inspecting-the-app.md`.
import { setupPluginListeners } from "tauri-plugin-mcp";
// Stylesheets are split by surface, so one rule set stays readable on its own.
import "./shared/ui/tokens.css";
import "./shared/ui/shell.css";
import "./shared/ui/components.css";
import "./shared/ui/cells.css";
import "./shared/ui/meters.css";
import "./shared/ui/overview.css";
import "./shared/ui/surfaces.css";
import "./shared/ui/settings.css";

// `import.meta.env.DEV` is replaced by Vite with a literal at build time, so
// the release bundle contains neither the call nor the plugin's code.
if (import.meta.env.DEV) {
  setupPluginListeners().catch((error: unknown) => {
    console.error("agent inspection listeners failed", error);
  });
}

const container = document.getElementById("root");
if (container === null) {
  throw new Error("the renderer root element is missing from index.html");
}

createRoot(container).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
