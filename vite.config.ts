// Vite configuration for the Quota renderer.
//
// The server block is the Tauri template's, unchanged: fixed port 1420, strict
// port so a conflict fails loudly, and HMR on 1421 when a dev host is set. The
// React Compiler runs through its documented plugin-react 6.x route, the
// `compiler` option, which uses the Rust port `oxc-transform-react` (spec
// 7.8.4). The previous inline `react({ babel })` form was removed in
// plugin-react 6.0.0, and the Babel preset route was measured and rejected
// because @rolldown/plugin-babel 0.2.4 ships declarations that contradict both
// supported @babel/core lines under `skipLibCheck: false`.
import react from "@vitejs/plugin-react";
import { existsSync, readFileSync } from "node:fs";
import { dirname, isAbsolute, join } from "node:path";
import process from "node:process";
import { defineConfig } from "vite";
import type { Plugin } from "vite";

const host = process.env.TAURI_DEV_HOST;

function inspectionModule(id: string): boolean {
  let directory = dirname(id.split("?")[0] ?? id);
  if (!isAbsolute(directory)) return false;
  for (;;) {
    const manifest = join(directory, "package.json");
    if (existsSync(manifest)) {
      const metadata: unknown = JSON.parse(readFileSync(manifest, "utf8"));
      return (
        typeof metadata === "object" &&
        metadata !== null &&
        "name" in metadata &&
        metadata.name === "tauri-plugin-mcp"
      );
    }
    const parent = dirname(directory);
    if (parent === directory) return false;
    directory = parent;
  }
}

export function inspectionReleaseCheck(): Plugin {
  return {
    name: "quota-release-inspection",
    apply: "build",
    generateBundle(_options, bundle) {
      for (const output of Object.values(bundle)) {
        if (output.type !== "chunk") continue;
        if (
          [...output.imports, ...output.dynamicImports].some(
            (id) => id === "tauri-plugin-mcp" || id.startsWith("tauri-plugin-mcp/"),
          ) ||
          Object.entries(output.modules).some(
            ([id, module]) => module.renderedLength > 0 && inspectionModule(id),
          )
        ) {
          this.error("production renderer includes tauri-plugin-mcp");
        }
      }
    },
  };
}

export default defineConfig(() => ({
  plugins: [react({ compiler: { logDiagnostics: true } }), inspectionReleaseCheck()],
  build: {
    // Vite transpiles; `bun run typecheck` is the separate type gate (spec 7.8.1).
    outDir: "dist",
    emptyOutDir: true,
    sourcemap: true,
  },
  // prevent Vite from obscuring rust errors
  clearScreen: false,
  // tauri expects a fixed port, fail if that port is not available
  server: {
    strictPort: true,
    port: 1420,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : false,
    watch: {
      // Ignore the Rust side: `src-tauri` and the workspace-root Cargo `target`
      // directory. Cargo holds build artifacts (e.g. proc-macro DLLs) locked on
      // Windows, which makes chokidar's fs.watch throw EBUSY and kill the dev server.
      ignored: ["**/src-tauri/**", "**/target/**"],
    },
  },
}));
