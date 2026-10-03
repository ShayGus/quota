/**
 * What the add-account wizard says about each provider it offers.
 *
 * Codex, Claude and OpenCode Go are read through the sign-in their own tools
 * already keep on this computer. A provider with no such tool is signed in by
 * Quota itself with an API key, which the host keeps in the system credential
 * store and never in Quota's own files.
 */

/** The providers the wizard offers, in the order it lists them. */
export const PROVIDERS = [
  "codex",
  "claude",
  "open_code_go",
  "openrouter",
  "zai",
  "minimax",
  "kimi",
] as const;

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
      /**
       * The provider's own command-line tool, when its sign-in can stand in for
       * a key: the key is then optional.
       */
      readonly cli?: { readonly name: string; readonly signIn: string };
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
  zai: {
    kind: "api_key",
    keyPage: "z.ai/manage-apikey/apikey-list",
    placeholder: "Your Z.ai API key",
  },
  minimax: {
    kind: "api_key",
    keyPage: "platform.minimax.io",
    placeholder: "sk-cp-…",
  },
  kimi: {
    kind: "api_key",
    keyPage: "kimi.com/code/console",
    placeholder: "Your Kimi Code API key",
    cli: { name: "Kimi CLI", signIn: "kimi, then /login" },
  },
};

/** What each provider reports, as the picker lists it. */
export const PROVIDER_WINDOWS: Record<OfferedProvider, string> = {
  codex: "5-hour · Weekly",
  claude: "5-hour · Weekly · Model-specific",
  open_code_go: "5-hour · Weekly · Monthly",
  openrouter: "Credit balance · API key limit",
  zai: "5-hour · Weekly · Web tools",
  minimax: "5-hour · Weekly",
  kimi: "5-hour · Weekly · Monthly",
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
  zai: "Z.ai did not accept this key. Copy it again from z.ai/manage-apikey/apikey-list, paste it, then press Connect.",
  minimax:
    "MiniMax did not accept this key. A Coding Plan key starts with sk-cp-: copy it from platform.minimax.io, paste it, then press Connect.",
  kimi: "Kimi needs a sign-in. Paste a key from kimi.com/code/console, or sign in with the Kimi CLI (kimi, then /login), then press Connect again.",
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
  zai: "To add a different Z.ai account, paste an API key from it, then press Connect again.",
  minimax:
    "To add a different MiniMax account, paste an API key from it, then press Connect again.",
  kimi: "To add a different Kimi account, paste an API key from it, or sign in to it with the Kimi CLI, then press Connect again.",
};

/** The system credential store's own name, as this computer calls it. */
export function credentialStoreName(): string {
  const agent = typeof navigator === "undefined" ? "" : navigator.userAgent;
  if (agent.includes("Windows")) return "Windows Credential Manager";
  if (agent.includes("Mac")) return "the macOS Keychain";
  return "your system keyring";
}
