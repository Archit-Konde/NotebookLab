/*
 * Name: format-contracts.test.ts
 * Purpose: Keep the format names the frontend sends and the backend accepts
 *   identical.
 * Description: A Studio or Audio Studio request carries its format as a plain
 *   string across the IPC boundary, and the backend answers an unknown one with
 *   "Unknown Studio format". Nothing on either side of that boundary is checked
 *   by a compiler: adding a format to the picker without adding its prompt, or
 *   renaming one on the Rust side, produces a button that fails only when
 *   pressed. Both lists are read from the source and compared in both
 *   directions, so a format offered but not buildable, and one buildable but
 *   never offered, are each a failure.
 * Tech Stack: Vitest
 * License: MIT
 * Authors: Amey Thakur (https://github.com/Amey-Thakur)
 *          Archit Konde (https://github.com/Archit-Konde)
 * Date: 2026-08-02
 */

import { readFileSync } from "node:fs";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

const root = join(__dirname, "..", "..");
const read = (p: string) => readFileSync(join(root, p), "utf-8");

/** The arms of the backend's `match format.as_str()`, i.e. what it can build. */
function backendFormats(file: string, matchOn: string): string[] {
  const source = read(file);
  const start = source.indexOf(matchOn);
  if (start === -1) throw new Error(`no '${matchOn}' in ${file}`);
  /* Stop at the arm that rejects everything else, which closes the list. */
  const end = source.indexOf("other =>", start);
  const body = source.slice(start, end === -1 ? start + 4000 : end);
  return [...new Set([...body.matchAll(/^\s*"([a-z_]+)" =>/gm)].map((m) => m[1]))].sort();
}

/** The ids the picker offers, read from its FORMATS list. */
function frontendFormats(file: string): string[] {
  const source = read(file);
  const start = source.indexOf("const FORMATS");
  if (start === -1) throw new Error(`no FORMATS list in ${file}`);
  const end = source.indexOf("];", start);
  const body = source.slice(start, end);
  return [...new Set([...body.matchAll(/id:\s*"([a-z_]+)"/g)].map((m) => m[1]))].sort();
}

describe("format contracts", () => {
  it("reads both sides rather than silently comparing nothing", () => {
    /* An extractor that finds nothing would make every comparison below pass. */
    expect(backendFormats("src-tauri/src/commands/studio_commands.rs", "match format.as_str()").length)
      .toBeGreaterThan(5);
    expect(frontendFormats("src/features/studio/pages/studio-page.tsx").length).toBeGreaterThan(5);
  });

  it("offers exactly the Studio formats the backend can build", () => {
    const backend = backendFormats("src-tauri/src/commands/studio_commands.rs", "match format.as_str()");
    const frontend = frontendFormats("src/features/studio/pages/studio-page.tsx");
    expect(frontend).toEqual(backend);
  });

  it("offers exactly the Audio Studio formats the backend can build", () => {
    const podcast = read("src-tauri/src/commands/podcast_commands.rs");
    const matchOn = podcast.includes("match style.as_str()")
      ? "match style.as_str()"
      : "match format.as_str()";
    const backend = backendFormats("src-tauri/src/commands/podcast_commands.rs", matchOn);
    const frontend = frontendFormats("src/features/podcasts/pages/podcast-page.tsx");
    expect(frontend).toEqual(backend);
  });
});
