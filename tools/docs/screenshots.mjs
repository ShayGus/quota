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

const SOURCE = join(process.cwd(), "test-results", "screenshots", "docs");
const TARGET = join(process.cwd(), "docs", "images");

/**
 * The published name, then the screenshot it is copied from. Every one is a
 * capture under `docs/`; most are `screenshotFull`, made as tall as their
 * content so nothing is cut off, and the Report a bug menus are taken at the
 * window's own size, which already shows all of them.
 */
const IMAGES = [
  ["overview-rings.png", "hero-ring-dark.png"],
  ["overview-bars-light.png", "hero-bar-light.png"],
  ["overview-all.png", "overview-all.png"],
  ["first-launch.png", "first-launch.png"],
  ["account-detail.png", "account-detail.png"],
  ["attention-filter.png", "attention-filter.png"],
  ["widget-rings.png", "widget-rings.png"],
  ["widget-cards.png", "widget-cards.png"],
  ["wizard-provider.png", "wizard-provider.png"],
  ["wizard-connect.png", "wizard-connect.png"],
  ["wizard-browser-code.png", "wizard-browser-code.png"],
  ["wizard-api-key.png", "wizard-api-key-light.png"],
  ["wizard-verify.png", "wizard-verify.png"],
  ["settings-general.png", "settings-general.png"],
  ["settings-accounts.png", "settings-accounts.png"],
  ["settings-appearance.png", "settings-appearance.png"],
  ["settings-notifications.png", "settings-notifications.png"],
  ["settings-privacy.png", "settings-privacy.png"],
  ["settings-diagnostics.png", "settings-diagnostics.png"],
  ["update-offer.png", "update-offer.png"],
  ["report-bug-menu.png", "report-bug-menu.png"],
  ["widget-report-bug.png", "widget-report-bug.png"],
  ["openrouter-card.png", "openrouter-card.png"],
  ["openrouter-detail.png", "openrouter-detail.png"],
  ["settings-key-limit.png", "settings-accounts-openrouter.png"],
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
