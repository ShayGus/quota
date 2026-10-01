/**
 * The renderer entry point.
 *
 * Strict Mode is on in development, so the subscription lifecycle is exercised
 * by the mount/unmount/mount cycle (spec 7.8).
 */
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "./app/App";
import "./shared/ui/tokens.css";
import "./shared/ui/shell.css";
import "./shared/ui/components.css";

const container = document.getElementById("root");
if (container === null) {
  throw new Error("the renderer root element is missing from index.html");
}

createRoot(container).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
