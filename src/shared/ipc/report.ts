/**
 * Reporting for asynchronous work.
 *
 * Every promise the renderer starts has an owner: either it is awaited by a
 * typed handler, or it is handed to `launch` or `reportAsync`, which report its
 * failure through the store. `void promise` is never used as error handling
 * (spec 7.8.2).
 *
 * The command results and the error union are the generated ones. Only the
 * transport model and the retry rule are the renderer's own: they describe how
 * this process talks to the backend, not what the contract says.
 */
import type { CommandError } from "../../generated/bindings";
import { setFailure, setLink } from "../state/store";

/**
 * A failure to reach the backend at all.
 *
 * This is not a domain error, so it carries no `CommandError`: nothing the
 * backend said produced it.
 */
export type TransportFailure = {
  /** Which transport step failed. */
  readonly code: "unavailable" | "malformed_response" | "unknown_error";
  /** A non-identifying detail, safe to show. */
  readonly detail: string;
};

/** The result shape the generated commands return. */
export type CommandResult<T> =
  | { readonly status: "ok"; readonly data: T }
  | { readonly status: "error"; readonly error: CommandError };

/** The words for one typed command failure. The variant is the contract. */
export function describeCommandError(error: CommandError): string {
  switch (error.kind) {
    case "initialization_pending":
      return "Quota is still starting up.";
    case "validation_failed":
      return `That setting is not valid: ${error.context.reason}`;
    case "account_not_found":
      return "That account is no longer known to Quota.";
    case "unsupported_provider":
      return "This build of Quota does not support that provider.";
    case "unsupported_method":
      return "This provider does not offer that connection method.";
    case "reconnect_required":
      return "Reconnect that account before Quota can read it.";
    case "permission_denied":
      return "This window is not allowed to do that.";
    case "secure_store_unavailable":
      return "The operating system key store is unavailable.";
    case "revision_conflict":
      return "The settings changed elsewhere. Reload and try again.";
    case "persistence_unavailable":
      return "Quota cannot reach its local storage.";
    case "native_operation_unsupported":
      return "This system does not support that window operation.";
    case "native_operation_failed":
      return error.context.operation.endsWith("launch_at_login")
        ? "Windows did not change the login item. Launch at login is unchanged."
        : "The system refused that window operation.";
    case "cancelled":
      return "That was cancelled.";
    case "internal":
      return "Quota hit an internal problem.";
  }
}

/** The words for one transport failure. It is not a domain error. */
export function describeTransportFailure(failure: TransportFailure): string {
  switch (failure.code) {
    case "unavailable":
      return "Quota is not reachable.";
    case "malformed_response":
      return "Quota returned a response this version cannot read.";
    case "unknown_error":
      return "Quota is not reachable.";
  }
}

/**
 * Whether retrying the same call could plausibly succeed unchanged.
 *
 * Two variants are transient by contract: the backend has not finished booting,
 * and a durable owner could not be reached. Everything else needs the user to
 * change something first.
 */
export function isRetryable(error: CommandError): boolean {
  return (
    error.kind === "initialization_pending" || error.kind === "persistence_unavailable"
  );
}

/** Records a failure in the store, and reports whether the call may be retried. */
export function reportFailure(error: CommandError): boolean {
  setFailure({ kind: "domain", error });
  return isRetryable(error);
}

/** Records a command failure under its own name. */
export function reportCommandError(error: CommandError): void {
  reportFailure(error);
}

/** Records a transport failure. Transport trouble also marks the link degraded. */
export function reportTransportFailure(failure: TransportFailure): void {
  setFailure({ kind: "transport", failure });
  setLink("unavailable");
}

/**
 * Owns one operation started from a synchronous UI handler.
 *
 * A click handler cannot await, and `void promise` is not error handling
 * (spec 7.8.2). This attaches the failure path explicitly, so the promise
 * always has an owner.
 */
export function launch(operation: Promise<unknown>): void {
  operation.catch((reason: unknown) => {
    reportTransportFailure({
      code: "unknown_error",
      detail: reason instanceof Error ? reason.name : "command rejected",
    });
  });
}

/**
 * Owns one asynchronous operation.
 *
 * The promise is awaited inside this function, so a rejection is always
 * handled. A caller that needs the value uses the awaited form instead.
 */
export async function reportAsync<T>(
  operation: Promise<CommandResult<T>>,
): Promise<T | null> {
  try {
    const result = await operation;
    if (result.status === "error") {
      reportFailure(result.error);
      return null;
    }
    return result.data;
  } catch (reason: unknown) {
    reportTransportFailure({
      code: "unknown_error",
      detail: reason instanceof Error ? reason.name : "command rejected",
    });
    return null;
  }
}
