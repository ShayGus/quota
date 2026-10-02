/**
 * The provider mark.
 *
 * The approved wireframe carries no provider artwork: it draws each provider as
 * a short glyph inside the account's square tile, so the same tile works in the
 * popover card, on the detail surface, and in settings without bundling anything.
 */
import type { JSX } from "react";

import type { ProviderId } from "../../generated/bindings";

/** The glyph each provider is drawn as. */
const MARKS: Record<ProviderId, string> = {
  codex: ">_",
  claude: "✳",
  open_code_go: "GO",
  fixture: "FX",
};

/** One account's provider tile. */
export function ProviderMark({
  providerId,
}: {
  readonly providerId: ProviderId;
}): JSX.Element {
  return (
    <span className={`provider-icon ${providerId}`} aria-hidden="true">
      {MARKS[providerId]}
    </span>
  );
}
