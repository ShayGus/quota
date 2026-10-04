/**
 * A fake provider on a loopback port.
 *
 * It answers the Codex usage endpoint with the sanitized payload the provider
 * crate's own mapping tests use, and records every request, so a journey can
 * show that the application really asked it, with the credential from the
 * sandbox. The application reaches it through the test launcher
 * (`src-tauri/examples/quota_e2e.rs`), which retargets the provider transport;
 * a shipped build cannot be pointed anywhere.
 */
import { readFileSync } from "node:fs";
import { createServer, type Server } from "node:http";
import { join } from "node:path";
import process from "node:process";

const PAYLOAD = join(
  process.cwd(),
  "src-tauri/crates/quota-providers/tests/fixtures/codex_success.json",
);

/** One request the fake provider received. */
export interface ProviderRequest {
  readonly path: string;
  readonly authorization: string | undefined;
}

/** A running fake provider. */
export interface FakeProvider {
  /** The origin to give the launcher, such as `http://127.0.0.1:41234`. */
  readonly base: string;
  readonly requests: ProviderRequest[];
  close: () => Promise<void>;
}

/** Starts the fake provider on a free loopback port. */
export async function startFakeProvider(): Promise<FakeProvider> {
  // The crate's fixture carries fixed reset times, which are in the past by now,
  // and a window whose reset is overdue is shown as "verifying". The numbers are
  // the fixture's; only the reset times move to the future.
  const payload = JSON.parse(readFileSync(PAYLOAD, "utf8")) as {
    rate_limit: {
      primary_window: { reset_at: number };
      secondary_window: { resetsAt: number };
    };
  };
  const now = Math.floor(Date.now() / 1000);
  payload.rate_limit.primary_window.reset_at = now + 3 * 3600;
  payload.rate_limit.secondary_window.resetsAt = now + 3 * 86_400;
  const body = JSON.stringify(payload);
  const requests: ProviderRequest[] = [];
  const server: Server = createServer((request, response) => {
    requests.push({
      path: request.url ?? "",
      authorization: request.headers.authorization,
    });
    if (request.url?.startsWith("/backend-api/wham/usage") === true) {
      response.writeHead(200, { "content-type": "application/json" });
      response.end(body);
    } else {
      response.writeHead(404);
      response.end();
    }
  });
  await new Promise<void>((resolve) => {
    server.listen(0, "127.0.0.1", resolve);
  });
  const address = server.address();
  if (address === null || typeof address === "string")
    throw new Error("no fake provider port");
  return {
    base: `http://127.0.0.1:${String(address.port)}`,
    requests,
    close: () =>
      new Promise<void>((resolve) => {
        server.close(() => {
          resolve();
        });
      }),
  };
}
