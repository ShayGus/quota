// Builds the two debug applications the real-app suite drives, and copies them
// to `src-tauri/target/e2e/` so that the second build does not overwrite the
// first.
//
// Both are debug builds (so they run under the Quota Dev identity), built with
// `--no-bundle` and the renderer embedded. The sample build adds the
// `sample-data` feature, which seeds ten fixture accounts.
import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";

const OUT = "src-tauri/target/e2e";
const BUILT = "src-tauri/target/debug/quota";

mkdirSync(OUT, { recursive: true });
for (const [name, extra] of [
  ["quota", []],
  ["quota-sample", ["--features", "sample-data"]],
]) {
  const result = spawnSync(
    "bun",
    [
      "tauri",
      "build",
      "--debug",
      "--no-bundle",
      ...(extra.length > 0 ? ["--", ...extra] : []),
    ],
    { stdio: "inherit" },
  );
  if (result.status !== 0) process.exit(result.status ?? 1);
  copyFileSync(BUILT, `${OUT}/${name}`);
}
