// Compares the running app with the approved wireframe, case by case.
//
// Usage (repository root, app running under `bun run inspect`):
//   QUOTA_WIREFRAME=/path/to/quota-wireframe.html bun tools/ui-parity/compare.mjs [case...]
//
// Each case writes tools/ui-parity/results/<case>.json with both measurements
// and the differences, and prints the differences. Text is never recorded.
import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

import { callTool, evaluateInApp } from "./app.mjs";
import { CASES, START } from "./cases.mjs";
import { evaluateInWireframe } from "./wireframe.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const measure = readFileSync(join(here, "measure.js"), "utf-8");
const results = join(here, "results");
mkdirSync(results, { recursive: true });

/** The script that sets the selectors and scope, then measures. */
function measurement(testCase) {
  return `globalThis.__paritySelectors = ${JSON.stringify(testCase.selectors)};
globalThis.__parityScope = ${JSON.stringify(testCase.scope ?? "")};
${measure}`;
}

/**
 * Whether two values match: numbers, and pixel lengths inside strings, within
 * one pixel, so sub-pixel layout noise is not reported as a design difference.
 */
function matches(expected, actual) {
  if (typeof expected === "number") return Math.abs(expected - actual) < 1;
  if (expected === actual) return true;
  const pattern = /-?\d+(?:\.\d+)?px/g;
  const left = String(expected).split(pattern);
  const right = String(actual).split(pattern);
  const leftNumbers = String(expected).match(pattern) ?? [];
  const rightNumbers = String(actual).match(pattern) ?? [];
  return (
    left.join("|") === right.join("|") &&
    leftNumbers.length === rightNumbers.length &&
    leftNumbers.every(
      (value, index) => Math.abs(parseFloat(value) - parseFloat(rightNumbers[index])) < 1,
    )
  );
}

/**
 * The differences between two measurements, ignoring the given properties
 * everywhere and the `ignoreAt` properties on their selector.
 */
function differences(selectors, wireframe, app, ignore, ignoreAt = {}) {
  const found = [];
  for (const selector of selectors) {
    const expected = wireframe[selector];
    const actual = app[selector];
    if (!expected || !actual) {
      found.push({ selector, missing: !expected ? "wireframe" : "app" });
      continue;
    }
    for (const key of Object.keys(expected)) {
      if (ignore.includes(key) || ignoreAt[selector]?.includes(key)) continue;
      if (!matches(expected[key], actual[key]))
        found.push({
          selector,
          property: key,
          wireframe: expected[key],
          app: actual[key],
        });
    }
  }
  return found;
}

/** Runs one app step: JavaScript in the overview, an inspection tool, or a wait. */
async function runStep(step) {
  if (typeof step === "string") {
    await evaluateInApp("overview", step);
  } else if ("tool" in step) {
    await callTool(step.tool, step.args);
  } else {
    await new Promise((resolve) => setTimeout(resolve, step.wait));
  }
}

const head = execFileSync("git", ["rev-parse", "--short", "HEAD"], {
  encoding: "utf-8",
}).trim();
const wanted = process.argv.slice(2);
let failures = 0;
for (const step of START) await runStep(step);
for (const testCase of CASES.filter(
  (c) => wanted.length === 0 || wanted.includes(c.name),
)) {
  for (const step of testCase.app.setup) await runStep(step);
  await new Promise((resolve) => setTimeout(resolve, 400));
  const app = await evaluateInApp("overview", measurement(testCase));
  for (const step of testCase.app.teardown) await runStep(step);
  const wireframe = await evaluateInWireframe(testCase.wireframe, measurement(testCase));
  const found = differences(
    testCase.selectors,
    wireframe,
    app,
    testCase.ignore,
    testCase.ignoreAt,
  );
  failures += found.length;
  writeFileSync(
    join(results, `${testCase.name}.json`),
    `${JSON.stringify({ case: testCase.name, head, ignored: testCase.ignore, ignoredAt: testCase.ignoreAt ?? {}, differences: found, wireframe, app }, null, 2)}\n`,
  );
  console.log(`${testCase.name}: ${String(found.length)} difference(s)`);
  for (const difference of found) console.log(`  ${JSON.stringify(difference)}`);
}
process.exitCode = failures === 0 ? 0 : 1;
