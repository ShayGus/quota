// Builds the two debug applications the real-app suite drives, and copies them
// to `src-tauri/target/e2e/`.
//
// Both are debug builds (so they run under the Quota Dev identity) with the
// renderer embedded.
import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";

const OUT = "src-tauri/target/e2e";
const BUILT = "src-tauri/target/debug/quota";
const LAUNCHER = "src-tauri/target/debug/examples/quota_e2e";

function run(command, args) {
  const result = spawnSync(command, args, { stdio: "inherit" });
  if (result.status !== 0) process.exit(result.status ?? 1);
}

mkdirSync(OUT, { recursive: true });

// The ordinary debug build, exactly as `bun tauri build --debug` makes it.
run("bun", ["tauri", "build", "--debug", "--no-bundle"]);
copyFileSync(BUILT, `${OUT}/quota`);

// The test launcher: the same app with `sample-data` (ten seeded fixture
// accounts) behind `src-tauri/examples/quota_e2e.rs`, which can point the
// provider transport at a local fake server. The renderer built above is
// embedded through `custom-protocol`.
run("cargo", [
  "build",
  "--manifest-path",
  "src-tauri/Cargo.toml",
  "--example",
  "quota_e2e",
  "--features",
  "sample-data,custom-protocol",
]);
copyFileSync(LAUNCHER, `${OUT}/quota-sample`);
