/**
 * The throwaway world one real-app journey runs in.
 *
 * Linux isolates HOME, XDG folders, and a private D-Bus session. Windows
 * redirects the user's roaming and local app folders and sets USERPROFILE for
 * profile files. The app's OS credential store is not isolated.
 */
import {
  spawn,
  spawnSync,
  execFile,
  execFileSync,
  type ChildProcess,
} from "node:child_process";
import {
  cpSync,
  linkSync,
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
import { basename, dirname, extname, join, resolve } from "node:path";
import process from "node:process";
import { promisify } from "node:util";

import { startSession, waitFor, type Session } from "./webdriver";

const run = promisify(execFile);
const SHELL_FOLDERS_KEY =
  "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\User Shell Folders";
const WINDOWS_ENVIRONMENT: Record<string, true> = {
  PATH: true,
  SYSTEMROOT: true,
  WINDIR: true,
  PATHEXT: true,
  COMSPEC: true,
  PROGRAMDATA: true,
  PROGRAMFILES: true,
  "PROGRAMFILES(X86)": true,
  COMMONPROGRAMFILES: true,
  "COMMONPROGRAMFILES(X86)": true,
  PROCESSOR_ARCHITECTURE: true,
  NUMBER_OF_PROCESSORS: true,
  OS: true,
};

interface WindowsShellFolder {
  readonly type: string;
  readonly value: string;
}

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
  private readonly stagedBinaries = new Map<string, string>();
  private readonly windowsShellFolders = new Map<string, WindowsShellFolder>();

  public readonly name: string;

  private constructor(name: string, root: string) {
    this.name = name;
    this.root = root;
    this.home = join(root, "home");
    this.config = join(root, "config");
    this.data = process.platform === "win32" ? this.config : join(root, "data");
    this.cache = join(root, "cache");
    this.runtime = join(root, "run");
    for (const directory of [
      this.home,
      this.config,
      ...(process.platform === "win32" ? [] : [this.data]),
      this.cache,
      this.runtime,
      join(root, "logs"),
    ]) {
      mkdirSync(directory, { recursive: true, mode: 0o700 });
    }

    if (process.platform === "win32") {
      const inherited = Object.fromEntries(
        Object.entries(process.env).filter(
          ([name]) => WINDOWS_ENVIRONMENT[name.toUpperCase()] === true,
        ),
      );
      this.env = {
        ...inherited,
        HOME: this.home,
        USERPROFILE: this.home,
        APPDATA: this.config,
        LOCALAPPDATA: this.cache,
        TEMP: this.runtime,
        TMP: this.runtime,
        RUST_BACKTRACE: "1",
      };
    } else {
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
        // `xvfb-run` protects its display with an authority file; without it GTK
        // is refused and the app cannot open a window.
        XAUTHORITY: process.env["XAUTHORITY"],
        // No GPU under a virtual display: draw in software.
        LIBGL_ALWAYS_SOFTWARE: "1",
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
  }

  /** Creates an isolated sandbox for the current platform. */
  public static async create(name: string): Promise<Sandbox> {
    const root = mkdtempSync(join(tmpdir(), "quota-e2e-"));
    const sandbox = new Sandbox(name, root);
    try {
      if (process.platform === "win32") await sandbox.redirectWindowsFolders();
      else await sandbox.startBus();
      return sandbox;
    } catch (error) {
      await sandbox.destroy();
      throw error;
    }
  }

  private async redirectWindowsFolders(): Promise<void> {
    for (const name of ["AppData", "Local AppData"]) {
      const { stdout } = await run("reg.exe", ["query", SHELL_FOLDERS_KEY, "/v", name]);
      const row = stdout
        .split(/\r?\n/)
        .map((line) => line.trim())
        .find((line) => line.startsWith(`${name} `));
      const value = row?.match(/^(.+?)\s+(REG_[A-Z_]+)\s+(.+)$/);
      if (value?.[1] !== name || value[2] === undefined || value[3] === undefined) {
        throw new Error(`could not read the Windows ${name} folder setting`);
      }
      this.windowsShellFolders.set(name, { type: value[2], value: value[3] });
    }

    try {
      for (const [name, path] of [
        ["AppData", this.config],
        ["Local AppData", this.cache],
      ] as const) {
        await run("reg.exe", [
          "add",
          SHELL_FOLDERS_KEY,
          "/v",
          name,
          "/t",
          this.windowsShellFolders.get(name)?.type ?? "REG_SZ",
          "/d",
          path,
          "/f",
        ]);
      }

      const { stdout } = await run(
        "powershell.exe",
        [
          "-NoProfile",
          "-NonInteractive",
          "-Command",
          "[Environment]::GetFolderPath([Environment+SpecialFolder]::ApplicationData); [Environment]::GetFolderPath([Environment+SpecialFolder]::LocalApplicationData)",
        ],
        { env: this.env },
      );
      const [roaming, local] = stdout.trim().split(/\r?\n/);
      if (
        roaming === undefined ||
        local === undefined ||
        resolve(roaming).toLowerCase() !== resolve(this.config).toLowerCase() ||
        resolve(local).toLowerCase() !== resolve(this.cache).toLowerCase()
      ) {
        throw new Error("Windows did not apply the sandbox's app-folder paths");
      }
    } catch (error) {
      await this.restoreWindowsFolders();
      throw error;
    }
  }

  private async restoreWindowsFolders(): Promise<void> {
    for (const [name, folder] of this.windowsShellFolders) {
      await run("reg.exe", [
        "add",
        SHELL_FOLDERS_KEY,
        "/v",
        name,
        "/t",
        folder.type,
        "/d",
        folder.value,
        "/f",
      ]);
    }
    this.windowsShellFolders.clear();
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
  public async launch(
    binary: string,
    extraEnvironment: NodeJS.ProcessEnv = {},
  ): Promise<RunningApp> {
    const application = this.stagedBinary(binary);
    const port = await freePort();
    const logPath = join(this.root, "logs", `driver-${String(Date.now())}.log`);
    const log = openLog(logPath);
    const driver = spawn("tauri-driver", ["--port", String(port)], {
      env: { ...this.env, ...extraEnvironment },
      stdio: ["ignore", log, log],
    });
    this.driver = driver;
    const base = `http://127.0.0.1:${String(port)}`;
    await waitFor("tauri-driver", async () => {
      const response = await fetch(`${base}/status`).catch(() => null);
      return response?.ok === true ? true : null;
    });
    const session = await startSession(base, application);
    return {
      session,
      stop: async () => {
        await session.end().catch(() => undefined);
        await waitFor("the application to exit", () =>
          Promise.resolve(this.applicationProcesses(binary).length === 0),
        );
        if (process.platform === "win32") {
          if (driver.pid !== undefined) terminateProcessTree(driver.pid);
        } else {
          driver.kill("SIGTERM");
        }
        this.driver = null;
      },
    };
  }

  /**
   * Gives the sandbox's user a Codex sign-in, as the Codex CLI would have
   * written it. The token is synthetic and only the fake provider sees it.
   */
  public writeCodexSignIn(token: string): void {
    const directory = join(this.home, ".codex");
    mkdirSync(directory, { recursive: true });
    writeFileSync(
      join(directory, "auth.json"),
      JSON.stringify({ tokens: { access_token: token, account_id: "fixture-account" } }),
    );
  }

  private stagedBinary(binary: string): string {
    const source = realpathSync(binary);
    if (process.platform !== "win32") return source;
    const existing = this.stagedBinaries.get(source);
    if (existing !== undefined) return existing;
    const suffix = extname(source);
    const staged = join(
      dirname(source),
      `${basename(source, suffix)}-${basename(this.root)}${suffix}`,
    );
    linkSync(source, staged);
    this.stagedBinaries.set(source, staged);
    return staged;
  }

  /** Starts a second copy of the application directly, as a person would. */
  public launchDirect(binary: string): ChildProcess {
    const child = spawn(this.stagedBinary(binary), [], {
      env: this.env,
      stdio: "ignore",
    });
    this.children.push(child);
    return child;
  }

  /** The pids of this sandbox's application process. */
  public applicationProcesses(binary: string): number[] {
    const target = this.stagedBinary(binary);
    if (process.platform === "win32") {
      const image = basename(target);
      const output = execFileSync(
        "tasklist.exe",
        ["/FI", `IMAGENAME eq ${image}`, "/FO", "CSV", "/NH"],
        { encoding: "utf8" },
      );
      return output.split(/\r?\n/).flatMap((line) => {
        const match = /^"([^"]+)","(\d+)"/.exec(line);
        return match?.[1]?.toLowerCase() === image.toLowerCase() && match[2] !== undefined
          ? [Number(match[2])]
          : [];
      });
    }
    const targetPath = realpathSync(target);
    const marker = `HOME=${this.home}`;
    return readdirSync("/proc")
      .filter((entry) => /^\d+$/.test(entry))
      .filter((entry) => {
        try {
          if (readlinkSync(`/proc/${entry}/exe`) !== targetPath) return false;
          return readFileSync(`/proc/${entry}/environ`, "utf8")
            .split("\0")
            .includes(marker);
        } catch {
          return false;
        }
      })
      .map(Number);
  }

  public applicationVisibleWindowCount(binary: string): number {
    if (process.platform !== "win32") {
      throw new Error("application window visibility is only available on Windows");
    }
    const [pid] = this.applicationProcesses(binary);
    if (pid === undefined) return 0;
    const type = [
      "using System;",
      "using System.Runtime.InteropServices;",
      "public static class NativeWindow {",
      "public delegate bool EnumWindowsProc(IntPtr window, IntPtr data);",
      '[DllImport("user32.dll")] public static extern bool EnumWindows(EnumWindowsProc callback, IntPtr data);',
      '[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);',
      '[DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr window);',
      "public static int CountVisible(uint targetProcessId) {",
      "var visible = 0;",
      "EnumWindows((window, data) => { uint owner; GetWindowThreadProcessId(window, out owner); if (owner == targetProcessId && IsWindowVisible(window)) visible++; return true; }, IntPtr.Zero);",
      "return visible;",
      "}",
      "}",
    ].join(" ");
    const script = `Add-Type -TypeDefinition '${type}'; [NativeWindow]::CountVisible([uint32]${String(pid)})`;
    const output = execFileSync(
      "powershell.exe",
      ["-NoProfile", "-NonInteractive", "-Command", script],
      { encoding: "utf8", windowsHide: true },
    );
    return Number(output.trim());
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

  /** Keeps logs, stops child processes, restores Windows paths, then deletes the sandbox. */
  public async destroy(): Promise<void> {
    try {
      if (process.platform === "win32") {
        for (const binary of this.stagedBinaries.keys()) {
          for (const pid of this.applicationProcesses(binary)) terminateProcessTree(pid);
        }
        for (const child of [this.driver, this.bus, ...this.children]) {
          if (child?.pid !== undefined) terminateProcessTree(child.pid);
        }
      } else {
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
      }
      const logs = join(this.root, "logs");
      const kept = join(RESULTS_DIRECTORY, "logs", this.name.replaceAll(/[^\w-]+/g, "_"));
      mkdirSync(kept, { recursive: true });
      cpSync(logs, kept, { recursive: true });
    } finally {
      try {
        for (const binary of this.stagedBinaries.values()) {
          rmSync(binary, { force: true });
        }
      } finally {
        try {
          await this.restoreWindowsFolders();
        } finally {
          rmSync(this.root, { recursive: true, force: true });
        }
      }
    }
  }
}

function terminateProcessTree(pid: number): void {
  const result = spawnSync("taskkill.exe", ["/PID", String(pid), "/T", "/F"], {
    stdio: "ignore",
    windowsHide: true,
  });
  if (result.error !== undefined) throw result.error;
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
  const selector = {
    overview: ".popover",
    settings: ".settings-window",
    widget: ".widget",
  }[role];
  return waitFor(`the ${role} window`, async () => {
    for (const handle of await session.handles()) {
      await session.switchTo(handle);
      // A visible WebView can still be blank or have its URL before React has
      // mounted. Select the rendered surface, so callers can use its controls.
      const found = await session.evaluate<boolean>(
        "return document.querySelector(arguments[0]) !== null",
        [selector],
      );
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
