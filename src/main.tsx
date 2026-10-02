/**
 * The renderer entry point.
 *
 * Strict Mode is on in development, so the subscription lifecycle is exercised
 * by the mount/unmount/mount cycle (spec 7.8).
 */
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "./app/App";
// Stylesheets are split by surface, so one rule set stays readable on its own.
import "./shared/ui/tokens.css";
import "./shared/ui/components.css";
import "./shared/ui/overview.css";
import "./shared/ui/surfaces.css";
import "./shared/ui/settings.css";

// Development-only overview listeners. `manage_ipc` records only calls an
// agent issues through the plugin's tools; ordinary frontend invokes are not
// intercepted. Vite removes this guarded import from a production build.
if (import.meta.env.DEV) {
  import("./shared/ipc/inspection")
    .then(async ({ canInspectCurrentWindow }) => {
      if (await canInspectCurrentWindow()) {
        const { setupPluginListeners } = await import("tauri-plugin-mcp");
        await setupPluginListeners();
      }
    })
    .catch((error: unknown) => {
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
