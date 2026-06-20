import { sveltekit } from "@sveltejs/kit/vite";
import { defineConfig } from "vite";

// Tauri serves the dev server on a fixed port and embeds the static build in release.
export default defineConfig({
  plugins: [sveltekit()],
  // Tauri expects a fixed port and clear errors rather than auto-incrementing.
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
  },
});
