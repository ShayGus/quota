import console from "node:console";
import { existsSync, readFileSync } from "node:fs";
import { dirname, isAbsolute, join, resolve } from "node:path";
import process from "node:process";
import { build } from "vite";

const inspectionPackage = "tauri-plugin-mcp";

function inspectionSpecifier(source) {
  return source === inspectionPackage || source.startsWith(`${inspectionPackage}/`);
}

function inspectionModule(id) {
  const path = id.split("?")[0];
  if (!isAbsolute(path)) return false;
  for (let directory = dirname(path); ; directory = dirname(directory)) {
    const manifest = join(directory, "package.json");
    if (existsSync(manifest)) {
      return JSON.parse(readFileSync(manifest, "utf8")).name === inspectionPackage;
    }
    if (directory === dirname(directory)) return false;
  }
}

try {
  await build({
    root: resolve(process.argv[2] ?? process.cwd()),
    mode: "production",
    logLevel: "silent",
    build: { write: false },
    plugins: [
      {
        name: "quota-release-inspection",
        generateBundle(_options, bundle) {
          for (const output of Object.values(bundle)) {
            if (output.type !== "chunk") continue;
            if (
              [...output.imports, ...output.dynamicImports].some(inspectionSpecifier) ||
              Object.entries(output.modules).some(
                ([id, module]) => module.renderedLength > 0 && inspectionModule(id),
              )
            ) {
              this.error("production renderer includes tauri-plugin-mcp");
            }
          }
        },
      },
    ],
  });
} catch (error) {
  console.error(error);
  process.exitCode = 1;
}
