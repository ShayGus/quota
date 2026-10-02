// Evaluates JavaScript in the running app's overview webview through the
// development inspection server (`bun run inspect`; see
// docs/inspecting-the-app.md).
//
// TAURI_MCP_SERVER may name a server entry point; on Windows it must be a copy
// patched for the plugin's pipe name, as the inspection guide describes, with
// TAURI_MCP_PIPE naming the pipe. TAURI_MCP_AUTH_TOKEN, or the token file
// beside the socket, authenticates.
import { spawn } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const SERVER =
  process.env.TAURI_MCP_SERVER ?? "node_modules/tauri-plugin-mcp-server/build/index.js";

/** The authentication token, from the environment or the file beside the socket. */
function token() {
  if (process.env.TAURI_MCP_AUTH_TOKEN) return process.env.TAURI_MCP_AUTH_TOKEN;
  const file = join(tmpdir(), "tauri-mcp.sock.token");
  return existsSync(file) ? readFileSync(file, "utf-8").trim() : undefined;
}

/** Calls one inspection tool and answers its text content. */
export async function callTool(name, args) {
  const server = spawn("bun", [SERVER], {
    stdio: ["pipe", "pipe", "ignore"],
    env: { ...process.env, TAURI_MCP_AUTH_TOKEN: token() },
  });
  let buffer = "";
  let next = 0;
  const waiting = new Map();
  server.stdout.on("data", (chunk) => {
    buffer += chunk;
    let end;
    while ((end = buffer.indexOf("\n")) >= 0) {
      const line = buffer.slice(0, end).trim();
      buffer = buffer.slice(end + 1);
      if (!line) continue;
      try {
        const message = JSON.parse(line);
        waiting.get(message.id)?.(message);
        waiting.delete(message.id);
      } catch {
        // Not a protocol line.
      }
    }
  });
  const request = (method, params) =>
    new Promise((resolve) => {
      next += 1;
      waiting.set(next, resolve);
      server.stdin.write(
        `${JSON.stringify({ jsonrpc: "2.0", id: next, method, params })}\n`,
      );
    });
  try {
    await request("initialize", {
      protocolVersion: "2024-11-05",
      capabilities: {},
      clientInfo: { name: "quota-ui-parity", version: "1" },
    });
    server.stdin.write(
      `${JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized" })}\n`,
    );
    const result = await request("tools/call", { name, arguments: args });
    if (result.error) throw new Error(JSON.stringify(result.error));
    return (result.result?.content ?? [])
      .filter((part) => part.type === "text")
      .map((part) => part.text)
      .join("\n");
  } finally {
    server.kill();
  }
}

/** Evaluates `code` in one window and answers its JSON value. */
export async function evaluateInApp(windowLabel, code) {
  const text = await callTool("execute_js", { window_label: windowLabel, code });
  const start = text.indexOf("{");
  const end = text.lastIndexOf("}");
  if (start < 0) return JSON.parse(text.split("\n")[0]);
  return JSON.parse(text.slice(start, end + 1));
}
