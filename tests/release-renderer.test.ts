import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { build } from "vite";
import { inspectionReleaseCheck } from "../vite.config";

const roots: string[] = [];

beforeEach(() => {
  // Vite derives import.meta.env.DEV from NODE_ENV, which Vitest sets to test.
  vi.stubEnv("NODE_ENV", "production");
});

afterEach(async () => {
  await Promise.all(
    roots.splice(0).map((root) => rm(root, { recursive: true, force: true })),
  );
});

async function renderer(main: string, alias = "tauri-plugin-mcp"): Promise<string> {
  const root = await mkdtemp(join(tmpdir(), "quota-renderer-"));
  roots.push(root);
  const plugin = join(root, "node_modules", alias);
  await mkdir(plugin, { recursive: true });
  await writeFile(
    join(root, "index.html"),
    '<script type="module" src="/main.js"></script>',
  );
  await writeFile(join(root, "main.js"), main);
  await writeFile(
    join(root, "package.json"),
    JSON.stringify({ private: true, type: "module" }),
  );
  await writeFile(
    join(plugin, "package.json"),
    JSON.stringify({ name: "tauri-plugin-mcp", type: "module", exports: "./index.js" }),
  );
  await writeFile(join(plugin, "index.js"), "globalThis.inspectionLoaded = true;");
  return root;
}

function release(root: string, external: string[] = []) {
  return build({
    root,
    configFile: false,
    logLevel: "silent",
    plugins: [inspectionReleaseCheck()],
    build: { write: false, rolldownOptions: { external } },
  });
}

test("release bundles reject static, dynamic, and renamed inspection modules", async () => {
  for (const [main, alias] of [
    ["import 'tauri-plugin-mcp';", "tauri-plugin-mcp"],
    ["import('tauri-plugin-mcp');", "tauri-plugin-mcp"],
    ["import 'inspection-alias';", "inspection-alias"],
  ] as const) {
    const root = await renderer(main, alias);
    await expect(release(root)).rejects.toThrow(
      "production renderer includes tauri-plugin-mcp",
    );
  }
});

test("release bundles reject inspection left as an external import", async () => {
  const root = await renderer("import('tauri-plugin-mcp');");
  await expect(release(root, ["tauri-plugin-mcp"])).rejects.toThrow(
    "production renderer includes tauri-plugin-mcp",
  );
});

test("release bundles allow a guest removed by the development guard", async () => {
  const root = await renderer(
    "if (import.meta.env.DEV) { import('tauri-plugin-mcp'); } document.title = 'Quota';",
  );
  await expect(release(root)).resolves.toBeDefined();
});
