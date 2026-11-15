import { fileURLToPath, URL } from "node:url";

import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// The Tauri dev URL is fixed so `tauri.conf.json` can point at it.
export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      "@akooti/shared": fileURLToPath(
        new URL("../../packages/shared/src/index.ts", import.meta.url),
      ),
    },
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
    target: "chrome110",
  },
});
