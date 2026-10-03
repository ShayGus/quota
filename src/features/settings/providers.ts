/**
 * What the add-account wizard says about each provider it offers.
 *
 * Codex, Claude and OpenCode Go are read through the sign-in their own tools
 * already keep on this computer. A provider with no such tool is signed in by
 * Quota itself with an API key, which the host keeps in the system credential
 * store and never in Quota's own files.
 */

/** The providers the wizard offers, in the order it lists them. */
export const PROVIDERS = ["codex", "claude", "open_code_go", "openrouter"] as const;

export type OfferedProvider = (typeof PROVIDERS)[number];

/** How a person signs a provider in. */
export type SignIn =
  | { readonly kind: "local" }
  | {
      readonly kind: "api_key";
      /** Where the person creates a key, as they would type it. */
      readonly keyPage: string;
      /** The form the provider's keys take. */
      readonly placeholder: string;
    };

export const SIGN_IN: Record<OfferedProvider, SignIn> = {
  codex: { kind: "local" },
  claude: { kind: "local" },
  open_code_go: { kind: "local" },
  openrouter: {
    kind: "api_key",
    keyPage: "openrouter.ai/settings/keys",
    placeholder: "sk-or-v1-…",
  },
};

/** What each provider reports, as the picker lists it. */
export const PROVIDER_WINDOWS: Record<OfferedProvider, string> = {
  codex: "5-hour · Weekly",
  claude: "5-hour · Weekly · Model-specific",
  open_code_go: "5-hour · Weekly · Monthly",
  openrouter: "Credit balance · API key limit",
};

/** What to do when the provider refuses the sign-in. */
export const AUTHENTICATION_RECOVERY: Record<OfferedProvider, string> = {
  codex:
    "Codex sign-in is required. Open a terminal, run codex login, then press Connect again.",
  claude:
    "Claude Code sign-in is required. Run claude in a terminal, sign in, then press Connect again.",
  open_code_go:
    "OpenCode Go sign-in is required. Sign in with OpenCode, or set OPENCODE_API_KEY, then press Connect again.",
  openrouter:
    "OpenRouter did not accept this key. Copy it again from openrouter.ai/settings/keys, paste it, then press Connect.",
};

/** How to add a different account of each provider. */
export const SWITCH_ACCOUNT: Record<OfferedProvider, string> = {
  codex:
    "To add a different Codex account, run codex login with it, then press Connect again.",
  claude:
    "To add a different Claude account, sign in to it in Claude Code (/login), then press Connect again.",
  open_code_go:
    "To add a different OpenCode Go account, sign in to it with OpenCode, then press Connect again.",
  openrouter:
    "To add a different OpenRouter account, paste an API key from it, then press Connect again.",
};

/** The system credential store's own name, as this computer calls it. */
export function credentialStoreName(): string {
  const agent = typeof navigator === "undefined" ? "" : navigator.userAgent;
  if (agent.includes("Windows")) return "Windows Credential Manager";
  if (agent.includes("Mac")) return "the macOS Keychain";
  return "your system keyring";
}
