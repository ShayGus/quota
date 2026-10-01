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
import "./shared/ui/shell.css";
import "./shared/ui/components.css";
import "./shared/ui/cells.css";
import "./shared/ui/meters.css";
import "./shared/ui/overview.css";
import "./shared/ui/surfaces.css";
import "./shared/ui/settings.css";

const container = document.getElementById("root");
if (container === null) {
  throw new Error("the renderer root element is missing from index.html");
}

createRoot(container).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
