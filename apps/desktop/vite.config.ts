import path from "node:path";
import process from "node:process";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";

const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(() => ({
  plugins: [react(), tailwindcss()],
  test: {
    environment: "jsdom",
    setupFiles: ["./src/test/setup.ts"],
    include: ["src/**/*.test.{ts,tsx}"],
    css: false,
    restoreMocks: true,
    // The suite runs many screens at once and the machine may be busy with other work.
    testTimeout: 20_000,
    // Each worker holds a whole browser-like environment. Fewer at a time keeps a busy machine out
    // of swap, which is slower than running fewer in parallel.
    maxWorkers: 3,
    // The language icons come from one very large file. Bundling it once keeps every test file from
    // reading it again.
    deps: { optimizer: { client: { enabled: true, include: ["react-icons/si"] } } },
  },
  resolve: {
    alias: { "@": path.resolve(import.meta.dirname, "./src") },
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
