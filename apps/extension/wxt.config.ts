import { defineConfig } from "wxt";

/**
 * One source, five targets.
 *
 * `-b chrome` (default) and `-b firefox` select the target; WXT derives the
 * manifest version and injects the polyfill. The Native Messaging host is
 * registered against this build by `akooti native-register`, which reads the
 * agent id below for Firefox and the id shown by the browser for Chromium.
 */
export default defineConfig({
  modules: ["@wxt-dev/module-react"],
  manifest: ({ browser }) => ({
    name: "Akooti",
    description:
      "Pont entre le coffre Akooti et le navigateur : remplissage aveugle, copie de secrets et passkeys.",
    // `nativeMessaging` is the only way to reach the daemon: no HTTP, no port.
    permissions: ["nativeMessaging", "storage", "scripting", "activeTab", "clipboardWrite"],
    // The main-world WebAuthn shim is injected as a web-accessible resource.
    web_accessible_resources: [
      {
        resources: ["webauthn-main.js"],
        matches: ["http://*/*", "https://*/*"],
      },
    ],
    ...(browser === "firefox"
      ? {
          // Firefox has no `minimum_chrome_version`, and its manifest carries
          // the add-on id the native host manifest must allow.
          browser_specific_settings: {
            gecko: {
              // Must match `native_register::FIREFOX_EXTENSION_ID`.
              id: "extension@akooti.local",
              strict_min_version: "115.0",
            },
          },
        }
      : { minimum_chrome_version: "116" }),
  }),
});
