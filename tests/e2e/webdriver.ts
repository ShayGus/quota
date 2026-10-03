/**
 * A minimal W3C WebDriver client.
 *
 * The real-app suite needs about a dozen protocol calls: start a session, list
 * and switch windows, find and click elements, run a script, take a screenshot.
 * WebDriver is plain JSON over HTTP, so these few functions replace a
 * test-runner framework and its dependency tree. `tauri-driver` speaks the same
 * protocol, so anything here also works against any other WebDriver server.
 */

/** A WebDriver error answer. */
export class WebDriverError extends Error {
  public readonly code: string;

  public constructor(code: string, message: string) {
    super(`${code}: ${message}`);
    this.name = "WebDriverError";
    this.code = code;
  }
}

interface Reply {
  readonly value: unknown;
}

const ELEMENT_KEY = "element-6066-11e4-a52e-4f735466cecf";

/** A located element. */
export class Element {
  private readonly session: Session;
  private readonly id: string;

  public constructor(session: Session, id: string) {
    this.session = session;
    this.id = id;
  }

  public click(): Promise<void> {
    return this.session.post(`/element/${this.id}/click`, {}).then(() => undefined);
  }

  public async text(): Promise<string> {
    return (await this.session.get(`/element/${this.id}/text`)) as string;
  }

  public async attribute(name: string): Promise<string | null> {
    return (await this.session.get(`/element/${this.id}/attribute/${name}`)) as
      string | null;
  }

  public type(text: string): Promise<void> {
    return this.session.post(`/element/${this.id}/value`, { text }).then(() => undefined);
  }
}

/** One WebDriver session: one running application. */
export class Session {
  public readonly id: string;
  private readonly base: string;

  public constructor(base: string, id: string) {
    this.base = base;
    this.id = id;
  }

  public async get(path: string): Promise<unknown> {
    return request("GET", `${this.base}/session/${this.id}${path}`);
  }

  public async post(path: string, body: unknown): Promise<unknown> {
    return request("POST", `${this.base}/session/${this.id}${path}`, body);
  }

  /** The handles of every window the application has, hidden ones included. */
  public async handles(): Promise<string[]> {
    return (await this.get("/window/handles")) as string[];
  }

  public async switchTo(handle: string): Promise<void> {
    await this.post("/window", { handle });
  }

  /** Runs a synchronous script in the current window. */
  public async evaluate<T>(script: string, args: unknown[] = []): Promise<T> {
    return (await this.post("/execute/sync", { script, args })) as T;
  }

  /** Finds the first element by CSS selector or XPath. */
  public async find(selector: string, using: "css selector" | "xpath" = "css selector") {
    const value = (await this.post("/element", { using, value: selector })) as Record<
      string,
      string
    >;
    const id = value[ELEMENT_KEY];
    if (id === undefined) throw new WebDriverError("no such element", selector);
    return new Element(this, id);
  }

  /** Finds a button, link or other element by its exact visible text. */
  public findByText(tag: string, text: string): Promise<Element> {
    return this.find(`//${tag}[normalize-space(.)=${JSON.stringify(text)}]`, "xpath");
  }

  /** A PNG screenshot of the current window, base64-encoded. */
  public async screenshot(): Promise<string> {
    return (await this.get("/screenshot")) as string;
  }

  public async end(): Promise<void> {
    await request("DELETE", `${this.base}/session/${this.id}`);
  }
}

async function request(method: string, url: string, body?: unknown): Promise<unknown> {
  const response = await fetch(url, {
    method,
    headers: { "content-type": "application/json" },
    ...(body === undefined ? {} : { body: JSON.stringify(body) }),
  });
  const reply = (await response.json()) as Reply;
  if (!response.ok) {
    const failure = reply.value as { error?: string; message?: string };
    throw new WebDriverError(failure.error ?? "unknown", failure.message ?? "");
  }
  return reply.value;
}

/** Starts a session for a Tauri application through a running `tauri-driver`. */
export async function startSession(base: string, application: string): Promise<Session> {
  const value = (await request("POST", `${base}/session`, {
    capabilities: {
      alwaysMatch: {
        browserName: "wry",
        "tauri:options": { application },
      },
    },
  })) as { sessionId: string };
  return new Session(base, value.sessionId);
}

/** Polls until `check` returns a truthy value, or fails with `what`. */
export async function waitFor<T>(
  what: string,
  check: () => Promise<T | false | null | undefined>,
  timeoutMs = 20_000,
): Promise<T> {
  const deadline = Date.now() + timeoutMs;
  let lastError: unknown;
  for (;;) {
    try {
      const result = await check();
      if (result !== false && result !== null && result !== undefined) return result;
    } catch (error) {
      lastError = error;
    }
    if (Date.now() > deadline) {
      throw new Error(
        `timed out waiting for ${what}${lastError instanceof Error ? `: ${lastError.message}` : ""}`,
      );
    }
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
}
