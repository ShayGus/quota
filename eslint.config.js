// ESLint flat configuration (spec 7.8.2).
//
// Type-aware linting over the whole package through typescript-eslint's
// `strictTypeChecked` profile, plus the current `eslint-plugin-react-hooks`
// `recommended-latest` set, which carries the React Compiler diagnostics.
// Formatting belongs to Prettier alone; no formatting rule is defined here.
//
// Type-aware rules need a file that a TypeScript project actually includes, so
// the typed preset is applied to the source, config, and test files that the
// three tsconfig projects cover, and only the JavaScript config file is left
// untyped.
import js from "@eslint/js";
import reactHooks from "eslint-plugin-react-hooks";
import globals from "globals";
import tseslint from "typescript-eslint";

const TYPED_FILES = ["src/**/*.{ts,tsx}", "tests/**/*.{ts,tsx}", "*.config.ts"];

export default tseslint.config(
  {
    ignores: ["dist/**", "coverage/**", "node_modules/**"],
  },
  js.configs.recommended,
  {
    files: TYPED_FILES,
    extends: [...tseslint.configs.strictTypeChecked],
    languageOptions: {
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
    rules: {
      // A dependency-array warning is a defect here, not a suggestion. The
      // compiler diagnostics below come from the plugin's own recommended set.
      "@typescript-eslint/consistent-type-imports": [
        "error",
        { prefer: "type-imports", fixStyle: "separate-type-imports" },
      ],
      "@typescript-eslint/no-floating-promises": [
        "error",
        { ignoreVoid: false, checkThenables: true },
      ],
      // The JSX void-return check is deliberately left ON: a promise must not be
      // passed where React expects a synchronous handler.
      "@typescript-eslint/no-misused-promises": [
        "error",
        { checksVoidReturn: { attributes: true } },
      ],
      "@typescript-eslint/switch-exhaustiveness-check": "error",
      "@typescript-eslint/no-non-null-assertion": "error",
      "@typescript-eslint/restrict-template-expressions": [
        "error",
        { allowNumber: true },
      ],
    },
  },
  {
    files: TYPED_FILES,
    plugins: { "react-hooks": reactHooks },
    rules: {
      ...reactHooks.configs["recommended-latest"].rules,
      "react-hooks/exhaustive-deps": "error",
    },
  },
  {
    files: ["src/**/*.{ts,tsx}"],
    languageOptions: { globals: globals.browser },
  },
  {
    files: ["tests/**/*.{ts,tsx}"],
    languageOptions: { globals: { ...globals.browser, ...globals.node } },
  },
  {
    files: ["*.config.{ts,js}", "eslint.config.js"],
    languageOptions: { globals: globals.node },
  },
);
