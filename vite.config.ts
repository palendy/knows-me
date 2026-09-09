import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri expects a fixed dev port and serves the built assets from `dist/`.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    // Tauri compiles into these dirs; watching them races the linker and
    // crashes Vite's file watcher with EBUSY on Windows. Ignore all Rust build
    // output so `tauri dev` is stable.
    watch: {
      ignored: ["**/target/**", "**/desktop/target/**", "**/src-tauri/target/**"],
    },
  },
  build: {
    outDir: "dist",
    target: "es2020",
  },
});
