// Runs the checks CI runs, in CI's order, and stops at the first failure.
//
// The pre-push hook (`.githooks/pre-push`) runs it, so a push that would fail
// CI's Rust, Windows adapter, Frontend or Interface jobs stops here instead.
// `bun run check` runs it by hand. The real-app suites are left to CI: they
// need the debug applications built and a WebDriver for the system's webview
// (CONTRIBUTING, section 10).
import { spawnSync } from "node:child_process";

const RUST = "src-tauri";

const STEPS = [
  ["Rust format", RUST, "cargo", ["fmt", "--all", "--", "--check"]],
  [
    "Clippy",
    RUST,
    "cargo",
    ["clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings"],
  ],
  ["Rust tests", RUST, "cargo", ["test", "--workspace", "--locked"]],
  [
    "Rust documentation",
    RUST,
    "cargo",
    ["doc", "--workspace", "--no-deps", "--locked"],
    { RUSTDOCFLAGS: "-D warnings" },
  ],
  ["Architecture gate", RUST, "cargo", ["xtask", "check-architecture"]],
  ["Release gate", RUST, "cargo", ["xtask", "check-release"]],
  ["Generated binding drift", RUST, "cargo", ["xtask", "bindings", "--check"]],
  ["Type check", ".", "bun", ["run", "typecheck"]],
  ["Lint", ".", "bun", ["run", "lint"]],
  ["Format check", ".", "bun", ["run", "format:check"]],
  ["Frontend tests", ".", "bun", ["run", "test"]],
  ["Renderer build without inspection", ".", "bun", ["run", "check:release:renderer"]],
  ["Interface tests", ".", "bun", ["run", "test:ui"]],
];

const started = Date.now();
for (const [name, cwd, command, args, env] of STEPS) {
  const at = Date.now();
  console.log(`\n▶ ${name}: ${command} ${args.join(" ")}`);
  const result = spawnSync(command, args, {
    cwd,
    stdio: "inherit",
    env: { ...process.env, ...env },
    // `cargo` and `bun` are `.exe` files on Windows, so no shell is needed.
  });
  if (result.status !== 0) {
    console.error(
      `\n✖ ${name} failed${result.error ? `: ${result.error.message}` : ""}.`,
    );
    console.error(
      "Fix it and push again, or skip these checks once with `git push --no-verify`.",
    );
    process.exit(result.status ?? 1);
  }
  console.log(`✔ ${name} (${((Date.now() - at) / 1000).toFixed(1)} s)`);
}
console.log(`\nEvery check passed in ${((Date.now() - started) / 1000).toFixed(0)} s.`);
