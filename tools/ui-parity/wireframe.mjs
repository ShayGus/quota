// Drives the wireframe page in headless Chrome over the DevTools protocol.
//
// QUOTA_WIREFRAME must name the approved `quota-wireframe.html` (it is kept
// outside this repository). CHROME may name the browser executable.
import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const CHROME_CANDIDATES = [
  process.env.CHROME,
  "C:/Program Files/Google/Chrome/Application/chrome.exe",
  "/usr/bin/google-chrome",
  "/usr/bin/chromium",
  "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
].filter(Boolean);

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/** The wireframe's file URL with its own query options (theme, layout, canvas). */
function wireframeUrl(query) {
  const file = process.env.QUOTA_WIREFRAME;
  if (!file || !existsSync(file)) {
    throw new Error("set QUOTA_WIREFRAME to the approved quota-wireframe.html");
  }
  return `${pathToFileURL(file).href}?canvas=1&${query}`;
}

/**
 * Loads the wireframe, performs the given clicks in order, and evaluates
 * `expression`, returning its JSON value.
 *
 * `clicks` are CSS selectors, optionally `selector~text` to pick the element
 * containing that text, dispatched as the wireframe's own click handlers expect.
 */
export async function evaluateInWireframe(
  { query, clicks = [], scale = 1.25 },
  expression,
) {
  const chrome = CHROME_CANDIDATES.find((candidate) => existsSync(candidate));
  if (!chrome) throw new Error("set CHROME to a Chrome or Chromium executable");
  const port = 9300 + Math.floor(Math.random() * 500);
  const profile = mkdtempSync(join(tmpdir(), "quota-parity-"));
  const browser = spawn(chrome, [
    "--headless=new",
    "--disable-gpu",
    "--hide-scrollbars",
    `--remote-debugging-port=${String(port)}`,
    `--force-device-scale-factor=${String(scale)}`,
    "--window-size=1440,1000",
    `--user-data-dir=${profile}`,
    "about:blank",
  ]);
  try {
    let targets;
    for (let attempt = 0; attempt < 75 && !targets; attempt++) {
      try {
        targets = await (await fetch(`http://127.0.0.1:${String(port)}/json`)).json();
      } catch {
        await sleep(200);
      }
    }
    const page = targets.find((target) => target.type === "page");
    const socket = new WebSocket(page.webSocketDebuggerUrl);
    await new Promise((resolve) => socket.addEventListener("open", resolve));
    let next = 0;
    const waiting = new Map();
    socket.addEventListener("message", (event) => {
      const message = JSON.parse(event.data);
      waiting.get(message.id)?.(message);
      waiting.delete(message.id);
    });
    const send = (method, params = {}) =>
      new Promise((resolve) => {
        next += 1;
        waiting.set(next, resolve);
        socket.send(JSON.stringify({ id: next, method, params }));
      });
    const evaluate = async (code) =>
      (await send("Runtime.evaluate", { expression: code, returnByValue: true })).result
        ?.result?.value;
    await send("Page.enable");
    await send("Page.navigate", { url: wireframeUrl(query) });
    await sleep(1500);
    for (const click of clicks) {
      const [css, text] = click.split("~");
      await evaluate(`(() => {
        const nodes = [...document.querySelectorAll(${JSON.stringify(css)})];
        const text = ${JSON.stringify(text ?? null)};
        const node = text === null ? nodes[0] : nodes.find((n) => n.textContent.includes(text));
        node?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
      })()`);
      await sleep(300);
    }
    const value = await evaluate(expression);
    socket.close();
    return value;
  } finally {
    // Chrome holds its profile until it exits, so wait before removing it.
    const exited = new Promise((resolve) => browser.once("exit", resolve));
    browser.kill();
    await Promise.race([exited, sleep(5000)]);
    rmSync(profile, { recursive: true, force: true, maxRetries: 5, retryDelay: 300 });
  }
}
