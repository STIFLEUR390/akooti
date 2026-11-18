import js from "@eslint/js";
import tseslint from "typescript-eslint";
import globals from "globals";

export default tseslint.config(
  {
    ignores: [
      "**/node_modules/**",
      "**/dist/**",
      "**/target/**",
      "**/.output/**",
      "**/.wxt/**",
      "**/src-tauri/**",
      "**/coverage/**",
      "**/.agents/**",
      "**/.github/skills/**",
    ],
  },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ["**/*.{ts,tsx,js,mjs}"],
    languageOptions: {
      ecmaVersion: 2022,
      sourceType: "module",
      globals: {
        ...globals.browser,
        ...globals.es2022,
        defineBackground: "readonly",
        defineContentScript: "readonly",
        browser: "readonly",
      },
    },
    rules: {
      "@typescript-eslint/consistent-type-imports": "error",
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_" },
      ],
    },
  },
  {
    files: ["**/*.config.{js,ts,mjs}", "eslint.config.js"],
    languageOptions: { globals: { ...globals.node } },
  },
  {
    // E2E runner: Bun + WebSocket, no framework globals.
    files: ["scripts/**/*.mjs"],
    languageOptions: { globals: { ...globals.bun, ...globals.node, Bun: "readonly" } },
  },
);
