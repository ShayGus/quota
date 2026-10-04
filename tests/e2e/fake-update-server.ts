/**
 * A fake update server on a loopback port.
 *
 * It serves an update list that offers a far newer version, and it records every
 * request. A build that must never check for updates is pointed at it through the
 * test launcher (`src-tauri/examples/quota_e2e.rs`), and the journey asserts that
 * the list of requests stays empty. If the guard in the application stopped
 * working, the same list would be offered and this server would see the request.
 */
import { createServer, type Server } from "node:http";

/** A running fake update server. */
export interface FakeUpdateServer {
  /** The address of the update list, as the updater would be configured with. */
  readonly endpoint: string;
  /** Every request received, as `METHOD path`. */
  readonly requests: string[];
  close: () => Promise<void>;
}

/** Starts the fake update server on a free loopback port. */
export async function startFakeUpdateServer(): Promise<FakeUpdateServer> {
  const requests: string[] = [];
  const server: Server = createServer((request, response) => {
    requests.push(`${request.method ?? "?"} ${request.url ?? ""}`);
    const offer = {
      version: "99.0.0",
      platforms: {
        "linux-x86_64": {
          url: "http://127.0.0.1:9/never-downloaded",
          signature: "not-a-real-signature",
        },
      },
    };
    response.writeHead(200, { "content-type": "application/json" });
    response.end(JSON.stringify(offer));
  });
  await new Promise<void>((resolve) => {
    server.listen(0, "127.0.0.1", resolve);
  });
  const address = server.address();
  if (address === null || typeof address === "string")
    throw new Error("no fake update server port");
  return {
    endpoint: `http://127.0.0.1:${String(address.port)}/latest.json`,
    requests,
    close: () =>
      new Promise<void>((resolve) => {
        server.close(() => {
          resolve();
        });
      }),
  };
}
