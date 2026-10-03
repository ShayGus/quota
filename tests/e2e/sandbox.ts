/**
 * The throwaway world one real-app journey runs in.
 *
 * Every journey gets its own directory with its own HOME and XDG folders and its
 * own private D-Bus session bus, so the app can neither read nor write the real
 * user's data and a second journey cannot see the first one's single-instance
 * lock. The folder is deleted afterwards. Nothing here touches the real HOME.
 *
 * Linux only: it needs a display (the CI job runs under `xvfb-run`), `dbus-daemon`,
 * `busctl`, `tauri-driver` and `WebKitWebDriver`.
 */
import { spawn, execFile, type ChildProcess } from "node:child_process";
import {
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  openSync,
  readdirSync,
  readFileSync,
  readlinkSync,
  realpathSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import process from "node:process";
import { promisify } from "node:util";

import { startSession, waitFor, type Session } from "./webdriver";

const run = promisify(execFile);

/** Where logs and screenshots are kept for CI to upload. */
export const RESULTS_DIRECTORY = join(process.cwd(), "test-results", "e2e");

/** A running application, started through `tauri-driver`. */
export interface RunningApp {
  readonly session: Session;
  /** Ends the WebDriver session, which closes the application. */
  stop: () => Promise<void>;
}

/** The sandbox for one journey. */
export class Sandbox {
  public readonly root: string;
  public readonly home: string;
  public readonly config: string;
  public readonly data: string;
  public readonly cache: string;
  public readonly runtime: string;
  public readonly env: NodeJS.ProcessEnv;
  private bus: ChildProcess | null = null;
  private driver: ChildProcess | null = null;
  private readonly children: ChildProcess[] = [];

  public readonly name: string;

  private constructor(name: string, root: string) {
    this.name = name;
    this.root = root;
    this.home = join(root, "home");
    this.config = join(root, "config");
    this.data = join(root, "data");
    this.cache = join(root, "cache");
    this.runtime = join(root, "run");
    for (const directory of [
      this.home,
      this.config,
      this.data,
      this.cache,
      this.runtime,
      join(root, "logs"),
    ]) {
      mkdirSync(directory, { recursive: true, mode: 0o700 });
    }
    const display = process.env["DISPLAY"];
    if (display === undefined || display === "") {
      throw new Error(
        "no DISPLAY: run the real-app suite under a display, for example `xvfb-run -a bun run test:e2e`",
      );
    }
    this.env = {
      // Everything the app resolves a folder from is inside the sandbox.
      PATH: process.env["PATH"],
      LD_LIBRARY_PATH: process.env["LD_LIBRARY_PATH"],
      DISPLAY: display,
      HOME: this.home,
      XDG_CONFIG_HOME: this.config,
      XDG_DATA_HOME: this.data,
      XDG_CACHE_HOME: this.cache,
      XDG_RUNTIME_DIR: this.runtime,
      XDG_STATE_HOME: join(root, "state"),
      // The keyring and WebKit must not find the real session's services.
      DBUS_SESSION_BUS_ADDRESS: `unix:path=${join(root, "run", "bus")}`,
      NO_AT_BRIDGE: "1",
      WEBKIT_DISABLE_COMPOSITING_MODE: "1",
      RUST_BACKTRACE: "1",
    };
  }

  /** Creates the folders and starts the private session bus. */
  public static async create(name: string): Promise<Sandbox> {
    const root = mkdtempSync(join(tmpdir(), "quota-e2e-"));
    const sandbox = new Sandbox(name, root);
    await sandbox.startBus();
    return sandbox;
  }

  private async startBus(): Promise<void> {
    const bus = spawn(
      "dbus-daemon",
      [
        "--session",
        "--nofork",
        `--address=${this.env["DBUS_SESSION_BUS_ADDRESS"] ?? ""}`,
      ],
      { env: this.env, stdio: "ignore" },
    );
    this.bus = bus;
    await waitFor("the private session bus", () =>
      Promise.resolve(existsSync(join(this.runtime, "bus"))),
    );
  }

  /** Starts the application through tauri-driver and opens a WebDriver session. */
  public async launch(binary: string): Promise<RunningApp> {
    const port = await freePort();
    const logPath = join(this.root, "logs", `driver-${String(Date.now())}.log`);
    const log = openLog(logPath);
    const driver = spawn("tauri-driver", ["--port", String(port)], {
      env: this.env,
      stdio: ["ignore", log, log],
    });
    this.driver = driver;
    const base = `http://127.0.0.1:${String(port)}`;
    await waitFor("tauri-driver", async () => {
      const response = await fetch(`${base}/status`).catch(() => null);
      return response?.ok === true ? true : null;
    });
    const session = await startSession(base, realpathSync(binary));
    return {
      session,
      stop: async () => {
        await session.end().catch(() => undefined);
        driver.kill("SIGTERM");
        this.driver = null;
        await waitFor("the application to exit", () =>
          Promise.resolve(this.applicationProcesses(binary).length === 0),
        );
      },
    };
  }

  /** Starts a second copy of the application directly, as a person would. */
  public launchDirect(binary: string): ChildProcess {
    const child = spawn(realpathSync(binary), [], {
      env: this.env,
      stdio: "ignore",
    });
    this.children.push(child);
    return child;
  }

  /**
   * The ids of the processes of this application that belong to this sandbox.
   * A process belongs to it when its executable is the binary and its
   * environment carries this sandbox's HOME.
   */
  public applicationProcesses(binary: string): number[] {
    const target = realpathSync(binary);
    const marker = `HOME=${this.home}`;
    return readdirSync("/proc")
      .filter((entry) => /^\d+$/.test(entry))
      .filter((entry) => {
        try {
          if (readlinkSync(`/proc/${entry}/exe`) !== target) return false;
          return readFileSync(`/proc/${entry}/environ`, "utf8")
            .split("\0")
            .includes(marker);
        } catch {
          return false;
        }
      })
      .map(Number);
  }

  /** The ids of every process, whatever its program, that has this sandbox's HOME. */
  private sandboxPids(): number[] {
    const marker = `HOME=${this.home}`;
    const found: number[] = [];
    for (const entry of readdirSync("/proc").filter((e) => /^\d+$/.test(e))) {
      try {
        if (readFileSync(`/proc/${entry}/environ`, "utf8").split("\0").includes(marker)) {
          found.push(Number(entry));
        }
      } catch {
        // The process ended while it was being read.
      }
    }
    return found;
  }

  /** The program names of the sandbox's processes other than the driver and the bus. */
  public leftoverProcesses(): string[] {
    const own = new Set([this.bus?.pid, this.driver?.pid]);
    return this.sandboxPids()
      .filter((pid) => !own.has(pid))
      .map((pid) => {
        try {
          return readFileSync(`/proc/${String(pid)}/comm`, "utf8").trim();
        } catch {
          return "";
        }
      })
      .filter((name) => name !== "");
  }

  /** Runs `busctl --user` against the sandbox's private bus. */
  public async busctl(...args: string[]): Promise<string> {
    const { stdout } = await run("busctl", ["--user", ...args], { env: this.env });
    return stdout;
  }

  /** The well-known names on the sandbox's session bus. */
  public async busNames(): Promise<string[]> {
    const listing = await this.busctl("--json=short", "list");
    return (JSON.parse(listing) as { name: string }[]).map((entry) => entry.name);
  }

  /**
   * Chooses an item of the tray icon's menu, the way a click on it would.
   *
   * The Linux tray publishes its menu on the session bus through the
   * `com.canonical.dbusmenu` interface, so the menu can be driven without a
   * panel. Returns false when the tray has not published a menu.
   */
  public async chooseTrayItem(label: string): Promise<boolean> {
    const names = (await this.busNames()).filter((name) => name.startsWith(":"));
    for (const name of names) {
      const tree = await this.busctl("--json=short", "tree", name).catch(() => "");
      const path = /\/org\/ayatana\/NotificationItem\/[^"\s]*\/Menu/.exec(tree)?.[0];
      if (path === undefined) continue;
      const layout = await this.busctl(
        "--json=short",
        "call",
        name,
        path,
        "com.canonical.dbusmenu",
        "GetLayout",
        "iias",
        "0",
        "2",
        "0",
      );
      const id = findMenuItem(JSON.parse(layout), label);
      if (id === null) continue;
      await this.busctl(
        "call",
        name,
        path,
        "com.canonical.dbusmenu",
        "Event",
        "isvu",
        String(id),
        "clicked",
        "s",
        "",
        "0",
      );
      return true;
    }
    return false;
  }

  /** Saves a screenshot of the window the session is on. */
  public async screenshot(session: Session, name: string): Promise<void> {
    const directory = join(RESULTS_DIRECTORY, "screenshots");
    mkdirSync(directory, { recursive: true });
    const png = await session.screenshot();
    writeFileSync(join(directory, `${name}.png`), Buffer.from(png, "base64"));
  }

  /** Keeps the logs, then removes the sandbox and everything started in it. */
  public destroy(): void {
    // A journey that failed midway leaves the app running; nothing may outlive it.
    for (const pid of this.sandboxPids()) {
      try {
        process.kill(pid, "SIGKILL");
      } catch {
        // Already gone.
      }
    }
    for (const child of [this.driver, this.bus, ...this.children]) {
      child?.kill("SIGKILL");
    }
    const logs = join(this.root, "logs");
    const kept = join(RESULTS_DIRECTORY, "logs", this.name.replaceAll(/[^\w-]+/g, "_"));
    mkdirSync(kept, { recursive: true });
    cpSync(logs, kept, { recursive: true });
    rmSync(this.root, { recursive: true, force: true });
  }
}

/** Opens a log file for a child's output and returns its descriptor. */
function openLog(path: string): number {
  return openSync(path, "a");
}

/** A TCP port nothing is listening on. */
function freePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const server = createServer();
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => {
      const address = server.address();
      server.close(() => {
        if (address !== null && typeof address === "object") resolve(address.port);
        else reject(new Error("no port"));
      });
    });
  });
}

/** The dbusmenu id of the item with this label, or null. */
function findMenuItem(layout: unknown, label: string): number | null {
  const walk = (node: unknown): number | null => {
    if (!Array.isArray(node)) return null;
    // A layout node is [id, properties, children].
    const [id, properties, children] = node as [
      number,
      Record<string, { data: unknown }>,
      unknown[],
    ];
    if (properties["label"]?.data === label) return id;
    for (const child of children) {
      const entry = (child as { data: unknown }).data;
      const found = walk(entry);
      if (found !== null) return found;
    }
    return null;
  };
  return walk((layout as { data: unknown[] }).data[1]);
}

/** The window of the application the session can see by its role. */
export async function findWindow(
  session: Session,
  role: "overview" | "settings" | "widget",
): Promise<string> {
  return waitFor(`the ${role} window`, async () => {
    for (const handle of await session.handles()) {
      await session.switchTo(handle);
      const href = await session.evaluate<string>("return window.location.href");
      const hash = href.split("#")[1] ?? "";
      const found =
        role === "settings"
          ? hash.startsWith("/settings")
          : role === "widget"
            ? hash.startsWith("/widget")
            : !hash.startsWith("/settings") && !hash.startsWith("/widget");
      if (found) return handle;
    }
    return null;
  });
}

/** Waits until the window the session is on is on screen. */
export function waitUntilVisible(session: Session, what: string): Promise<true> {
  return waitFor(`${what} to be visible`, async () => {
    const state = await session.evaluate<string>("return document.visibilityState");
    return state === "visible" ? (true as const) : null;
  });
}
