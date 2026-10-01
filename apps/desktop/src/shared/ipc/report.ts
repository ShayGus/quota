/**
 * Reporting for asynchronous work.
 *
 * Every promise the renderer starts has an owner: either it is awaited by a
 * typed handler, or it is handed to `reportAsync`, which reports its failure
 * through the store. `void promise` is never used as error handling
 * (spec 7.8.2).
 */
import {
  isRetryable,
  type CommandError,
  type Invocation,
  type TransportFailure,
} from "../../generated/bindings";
import { setFailure, setLink } from "../state/store";

/** The words for one typed command failure. The variant is the contract. */
export function describeCommandError(error: CommandError): string {
  switch (error.kind) {
    case "initialization_pending":
      return "Quota is still starting.";
    case "validation_failed":
      return `Quota rejected ${error.context.field}: ${error.context.reason}`;
    case "account_not_found":
      return "That account is no longer monitored.";
    case "unsupported_provider":
      return "This build does not support that provider.";
    case "unsupported_method":
      return "This account does not support that connection method.";
    case "reconnect_required":
      return "This account needs to be connected again.";
    case "permission_denied":
      return "This window may not perform that action.";
    case "secure_store_unavailable":
      return "The operating system credential store is locked or unavailable.";
    case "revision_conflict":
      return "The value changed before this save reached the backend.";
    case "persistence_unavailable":
      return `Local storage (${error.context.owner}) is unavailable.`;
    case "native_operation_unsupported":
      return "This system does not support that window operation.";
    case "native_operation_failed":
      return `The window operation failed: ${error.context.reason}`;
    case "cancelled":
      return "Cancelled.";
    case "internal":
      return `Quota reported an internal failure (${error.context.code}).`;
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

/** Records a failure in the store, and reports whether the call may be retried. */
export function reportFailure(error: CommandError): boolean {
  setFailure({ kind: "domain", error });
  return isRetryable(error);
}

/** Records a transport failure. Transport trouble also marks the link degraded. */
export function reportTransportFailure(failure: TransportFailure): void {
  setFailure({ kind: "transport", failure });
  setLink("unavailable");
}

/**
 * Owns one asynchronous operation.
 *
 * The promise is awaited inside this function, so a rejection is always
 * handled. A caller that needs the value uses the awaited form instead.
 */
export async function reportAsync<T>(operation: Promise<Invocation<T>>): Promise<T | null> {
  const result = await operation;
  if ("transportError" in result) {
    reportTransportFailure(result.transportError);
    return null;
  }
  if ("error" in result) {
    reportFailure(result.error);
    return null;
  }
  return result.ok;
}
