/**
 * Provider names as a person reads them.
 *
 * The wire contract names a provider by its identifier, which is what the rest
 * of the program passes around. This module turns one of those identifiers
 * into the word on screen, so the overview, the details surface, and the
 * accounts panel all say the same thing about the same provider.
 */
import type { ProviderId } from "../../generated/bindings";

/** The display name of every provider this build knows. */
const LABELS: Record<ProviderId, string> = {
  codex: "Codex",
  claude: "Claude",
  open_code_go: "OpenCode Go",
  openrouter: "OpenRouter",
  zai: "Z.ai",
  minimax: "MiniMax",
  kimi: "Kimi",
  fixture: "Fixture",
};

/** The name to show for one provider. */
export function providerLabel(providerId: ProviderId): string {
  return LABELS[providerId];
}
