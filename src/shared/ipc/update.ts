/**
 * The update pop-up's connection to the host.
 *
 * The host owns the update flow; this window only learns what to show and says
 * which button was pressed. It uses the generated command and event wrappers, so
 * a wire mismatch cannot compile, and it holds no updater permission.
 */
import {
  commands,
  events,
  type ReleaseNotes,
  type UpdatePrompt,
  type UpdateResponse,
} from "../../generated/bindings";
import { reportSettled } from "./report";

/** What the host is showing in the pop-up now, or `null` when nothing. */
export function readUpdatePrompt(): Promise<UpdatePrompt | null> {
  return commands.getUpdatePrompt();
}

/** The release notes built into this version, or `null` when it has none. */
export function readReleaseNotes(): Promise<ReleaseNotes | null> {
  return commands.getReleaseNotes();
}

/** Listens for the host changing what the pop-up shows. */
export function listenForUpdatePrompt(
  onChange: (prompt: UpdatePrompt) => void,
): Promise<() => void> {
  return events.updatePromptChanged.listen((event) => {
    onChange(event.payload.prompt);
  });
}

/** Tells the host which button the person pressed. */
export function respondToUpdate(response: UpdateResponse): Promise<boolean> {
  return reportSettled(commands.respondToUpdatePrompt(response));
}
