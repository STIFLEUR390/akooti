import { defineConfig } from "wxt";

export default defineConfig({
  manifest: {
    name: "Akooti",
    description: "Pont entre le coffre Akooti et le navigateur : remplissage aveugle et passkeys.",
    permissions: ["nativeMessaging", "storage", "activeTab", "scripting"],
    minimum_chrome_version: "116",
  },
});
