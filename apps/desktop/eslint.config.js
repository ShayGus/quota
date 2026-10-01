// ESLint flat configuration (spec 7.8.2).
//
// Type-aware linting over the whole package through typescript-eslint's
// `strictTypeChecked` profile, plus the current `eslint-plugin-react-hooks`
// `recommended-latest` set, which carries the React Compiler diagnostics.
// Formatting belongs to Prettier alone; no formatting rule is defined here.
import js from "@eslint/js";
import reactHooks from "eslint-plugin-react-hooks";
import globals from "globals";
import tseslint from "typescript-eslint";

export default tseslint.config(
  {
    ignores: ["dist/**", "coverage/**", "node_modules/**"],
  },
  js.configs.recommended,
  tseslint.configs.strictTypeChecked,
  {
    files: ["**/*.{ts,tsx}"],
    languageOptions: {
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
  },
  {
    files: ["**/*.{ts,tsx}"],
    plugins: { "react-hooks": reactHooks },
    rules: {
      ...reactHooks.configs["recommended-latest"].rules,
      // A dependency array warning is a defect here, not a suggestion.
      "react-hooks/exhaustive-deps": "error",
      // Generated bindings are machine output, but they stay typechecked.
      // No rule is disabled for them; they are plain typed code.
      "@typescript-eslint/consistent-type-imports": [
        "error",
        { prefer: "type-imports", fixStyle: "separate-type-imports" },
      ],
      "@typescript-eslint/no-floating-promises": [
        "error",
        { ignoreVoid: false, checkThenables: true },
      ],
      "@typescript-eslint/no-misused-promises": ["error"],
      "@typescript-eslint/switch-exhaustiveness-check": "error",
      "@typescript-eslint/no-non-null-assertion": "error",
      "@typescript-eslint/restrict-template-expressions": [
        "error",
        { allowNumber: true },
      ],
    },
  },
  {
    files: ["**/*.config.{ts,js}", "eslint.config.js"],
    languageOptions: { globals: globals.node },
  },
  {
    files: ["tests/**/*.{ts,tsx}"],
    languageOptions: { globals: globals.browser },
  },
  {
    // The one audited native-integration boundary. It is the only module allowed
    // to name a Tauri wire string; every other module calls its typed wrappers.
    files: ["src/generated/bindings.ts"],
    rules: {
      "@typescript-eslint/no-unsafe-assignment": "error",
      "@typescript-eslint/no-unsafe-return": "error",
    },
  },
);
