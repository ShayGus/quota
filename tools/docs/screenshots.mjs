/**
 * Copies the screenshots the user guide shows from the interface-test run.
 *
 * `bun run test:ui` renders the real built renderer against the faked host in
 * `tests/ui/fake-backend.ts`, whose accounts are fictional, and writes every
 * state to `test-results/screenshots/`. This copies the ones the docs use to
 * `docs/images/`, so the pictures can be refreshed after an interface change
 * with `bun run test:ui && bun run docs:screenshots`.
 */
import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import process from "node:process";

const SOURCE = join(process.cwd(), "test-results", "screenshots");
const TARGET = join(process.cwd(), "docs", "images");

/** The published name, then the screenshot it is copied from. */
const IMAGES = [
  ["first-launch.png", "popover-first-launch.png"],
  ["overview-rings.png", "popover-seven-accounts.png"],
  ["overview-bars-light.png", "popover-440-bar-light.png"],
  ["account-detail.png", "popover-account-detail.png"],
  ["attention-filter.png", "popover-attention-filter.png"],
  ["widget-rings.png", "widget-rings.png"],
  ["widget-cards.png", "widget-bars.png"],
  ["wizard-provider.png", "wizard-1-provider.png"],
  ["wizard-connect.png", "wizard-2-connect.png"],
  ["wizard-browser-code.png", "wizard-browser-code.png"],
  ["wizard-api-key.png", "wizard-opencode-key-light.png"],
  ["wizard-verify.png", "wizard-3-verify.png"],
  ["settings-general.png", "settings-general.png"],
  ["settings-accounts.png", "settings-accounts.png"],
  ["settings-appearance.png", "settings-appearance.png"],
  ["settings-notifications.png", "settings-notifications.png"],
  ["settings-privacy.png", "settings-privacy.png"],
  ["settings-diagnostics.png", "settings-diagnostics.png"],
  ["update-offer.png", join("update-popup", "offer-light.png")],
];

mkdirSync(TARGET, { recursive: true });
const missing = IMAGES.filter(([, from]) => !existsSync(join(SOURCE, from)));
if (missing.length > 0) {
  console.error(
    `Run bun run test:ui first; missing: ${missing.map(([, from]) => from).join(", ")}`,
  );
  process.exit(1);
}
for (const [to, from] of IMAGES) {
  copyFileSync(join(SOURCE, from), join(TARGET, to));
}
console.log(`Copied ${IMAGES.length} screenshots to docs/images.`);
