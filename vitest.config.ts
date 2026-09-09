import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

// Test configuration, kept separate from U1's `vite.config.ts` so the app
// bundler config and the test runner config can evolve independently.
export default defineConfig({
  plugins: [react()],
  test: {
    globals: true,
    environment: "jsdom",
    setupFiles: ["./src/test-setup.ts"],
    include: ["src/**/*.{test,property.test}.{ts,tsx}"],
  },
});
