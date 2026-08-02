/*
 * Name: tauri-client.ts
 * Purpose: Typed wrapper around Tauri's invoke() for frontend-to-backend IPC.
 * Description: Centralizes all Tauri invoke calls with error handling. Every
 *   feature's api/ folder calls through this layer rather than
 *   importing @tauri-apps/api directly. This enables mocking in
 *   tests and consistent error handling.
 * Tech Stack: Tauri v2, TypeScript
 * License: MIT
 * Authors: Amey Thakur (https://github.com/Amey-Thakur)
 *          Archit Konde (https://github.com/Archit-Konde)
 * Date: 2026-07-12
 */

import { invoke } from "@tauri-apps/api/core";


/**
 * Whether the app is running inside Tauri rather than a plain browser.
 *
 * Anything reaching for the event bridge or the webview has to ask first:
 * `listen` and `getCurrentWebview` read globals Tauri injects, and without them
 * they throw or reject rather than returning nothing. An unhandled rejection is
 * the quiet outcome; `getCurrentWebview` took a whole page down.
 *
 * The packaged app always has them, so this only matters when the frontend is
 * served in a browser, which is how the layout is checked during development.
 */
export function hasTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}


/**
 * Type-safe Tauri command invocation with standardized error handling.
 * Wraps @tauri-apps/api/core invoke() to catch Rust-side errors and
 * convert them into structured frontend errors.
 */
export async function tauriInvoke<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    /* Tauri serializes Rust errors as strings */
    const message = typeof error === "string" ? error : String(error);
    throw new TauriError(command, message);
  }
}


export class TauriError extends Error {
  public readonly command: string;

  constructor(command: string, message: string) {
    super(`[${command}] ${message}`);
    this.name = "TauriError";
    this.command = command;
  }
}
