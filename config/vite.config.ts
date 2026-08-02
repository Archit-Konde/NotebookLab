/*
 * Name: vite.config.ts
 * Purpose: Single build configuration for bundling, styling, and tests.
 * Description: Lives in config/ to keep the repository root minimal; npm
 *   scripts pass it explicitly with --config. The Vite root is src/, where
 *   index.html lives. Path alias @ maps to src/ for clean imports. Tauri
 *   requires clearScreen false and a fixed dev port by convention. PostCSS
 *   runs Tailwind and Autoprefixer inline, and the vitest section drives
 *   the unit tests, so the frontend needs exactly one config file.
 * Tech Stack: Vite, React, Tailwind CSS, Vitest, Tauri v2
 * License: MIT
 * Authors: Amey Thakur (https://github.com/Amey-Thakur)
 *          Archit Konde (https://github.com/Archit-Konde)
 * Date: 2026-07-12
 */

/// <reference types="vitest/config" />

import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "path";
import tailwindcss from "@tailwindcss/vite";

/* import.meta.dirname rather than __dirname: Vite is moving to a native config
   loader in which __dirname is not defined, and it already warns on every run.
   Available since Node 20.11, and the project builds on 22. */
const repoRoot = path.resolve(import.meta.dirname, "..");

export default defineConfig({
  root: path.join(repoRoot, "src"),

  plugins: [react(), tailwindcss()],

  resolve: {
    alias: {
      "@": path.join(repoRoot, "src"),
    },
  },

  /* Tauri dev server configuration */
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: "localhost",
  },

  build: {
    /* Tauri uses Chromium, target modern JS */
    target: "esnext",
    /* Vite root is src/, so the bundle goes up one level */
    outDir: path.join(repoRoot, "dist"),
    emptyOutDir: true,
    chunkSizeWarningLimit: 1000,
  },

  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: [path.join(repoRoot, "src", "test", "setup.ts")],
    include: ["**/*.test.{ts,tsx}"],
  },
});
