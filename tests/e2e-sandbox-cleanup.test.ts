import type * as ChildProcess from "node:child_process";
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import type * as Os from "node:os";
import { basename, join } from "node:path";
import process from "node:process";

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { Sandbox } from "./e2e/sandbox";
import type * as WebDriver from "./e2e/webdriver";

const host = vi.hoisted(() => ({
  temporaryDirectory: "",
  run: vi.fn(),
  spawn: vi.fn(),
  spawnSync: vi.fn(),
  execFileSync: vi.fn(),
  startSession: vi.fn(),
}));

vi.mock("node:os", async (importOriginal) => {
  const original = await importOriginal<typeof Os>();
  const overrides = { tmpdir: () => host.temporaryDirectory };
  return { ...original, ...overrides, default: { ...original, ...overrides } };
});
vi.mock("node:child_process", async (importOriginal) => {
  const original = await importOriginal<typeof ChildProcess>();
  const { promisify } = await import("node:util");
  const overrides = {
    execFile: Object.assign(vi.fn(), { [promisify.custom]: host.run }),
    spawn: host.spawn,
    spawnSync: host.spawnSync,
    execFileSync: host.execFileSync,
  };
  return { ...original, ...overrides, default: { ...original, ...overrides } };
});
vi.mock("./e2e/webdriver", async (importOriginal) => ({
  ...(await importOriginal<typeof WebDriver>()),
  startSession: host.startSession,
}));

interface WindowsProcess {
  readonly parent: number;
  readonly image: string;
}

const processes = new Map<number, WindowsProcess>();
let nextPid = 100;
let driverPid = 0;
let binary = "";
let world: Sandbox | null = null;

beforeEach(() => {
  vi.spyOn(process, "platform", "get").mockReturnValue("win32");
  mkdirSync("test-results", { recursive: true });
  host.temporaryDirectory = mkdtempSync(join(process.cwd(), "test-results", "cleanup-"));
  binary = join(host.temporaryDirectory, "quota.exe");
  writeFileSync(binary, "test executable");
  processes.clear();

  host.run.mockImplementation(
    (command: string, args: string[], options?: { env: NodeJS.ProcessEnv }) => {
      if (command === "powershell.exe") {
        return Promise.resolve({
          stdout: `${options?.env["APPDATA"] ?? ""}\r\n${options?.env["LOCALAPPDATA"] ?? ""}\r\n`,
        });
      }
      if (command === "reg.exe" && args[0] === "query") {
        return Promise.resolve({ stdout: `${args[3] ?? ""} REG_SZ original-folder` });
      }
      return Promise.resolve({ stdout: "" });
    },
  );
  host.spawn.mockImplementation(() => {
    driverPid = nextPid++;
    processes.set(driverPid, { parent: 0, image: "tauri-driver.exe" });
    return { pid: driverPid };
  });
  host.execFileSync.mockImplementation((_command: string, args: string[]) => {
    const image = args[1]?.replace("IMAGENAME eq ", "");
    return [...processes]
      .filter(([, entry]) => entry.image === image)
      .map(([pid, entry]) => `"${entry.image}","${pid}"`)
      .join("\r\n");
  });
  host.spawnSync.mockImplementation((_command: string, args: string[]) => {
    const pid = Number(args[1]);
    // Windows taskkill /T can reach descendants only while the parent exists.
    const killTree = (target: number): void => {
      if (!processes.has(target)) return;
      for (const [child, entry] of processes) {
        if (entry.parent === target) killTree(child);
      }
      processes.delete(target);
    };
    killTree(pid);
    return { status: 0 };
  });
  host.startSession.mockImplementation((_base: string, application: string) => {
    const appPid = nextPid++;
    processes.set(appPid, { parent: driverPid, image: basename(application) });
    processes.set(nextPid++, { parent: appPid, image: "msedgewebview2.exe" });
    return Promise.resolve({
      // WebDriver closes the host; WebView2's asynchronous exit has not finished.
      end: () => {
        processes.delete(appPid);
        return Promise.resolve();
      },
    });
  });
  vi.stubGlobal("fetch", () => Promise.resolve({ ok: true }));
});

afterEach(async () => {
  processes.clear();
  await world?.destroy();
  world = null;
  rmSync(host.temporaryDirectory, { recursive: true, force: true });
});

describe("Windows sandbox process cleanup", () => {
  it("stops WebView2 with its host before deleting the sandbox", async () => {
    world = await Sandbox.create("windows-stop");
    const app = await world.launch(binary);
    await app.stop();
    expect([...processes.values()]).toEqual([]);
    const root = world.root;
    await world.destroy();
    world = null;
    expect(existsSync(root)).toBe(false);
  });

  it("can restart in the same sandbox without leaving old WebView2 processes", async () => {
    world = await Sandbox.create("windows-restart");
    const first = await world.launch(binary);
    await first.stop();
    const second = await world.launch(binary);
    expect(world.applicationProcesses(binary)).toHaveLength(1);
    expect(
      [...processes.values()].filter((entry) => entry.image === "msedgewebview2.exe"),
    ).toHaveLength(1);
    await second.stop();
    expect([...processes.values()]).toEqual([]);
  });

  it("destroys the process tree when a journey fails before stopping the app", async () => {
    world = await Sandbox.create("windows-failed-journey");
    await world.launch(binary);
    const root = world.root;
    await world.destroy();
    world = null;
    expect([...processes.values()]).toEqual([]);
    expect(existsSync(root)).toBe(false);
  });
});
