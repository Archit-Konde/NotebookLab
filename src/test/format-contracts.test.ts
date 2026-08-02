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

import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

import { formatError } from "@/lib/format-error";

const root = join(__dirname, "..", "..");
const read = (p: string) => readFileSync(join(root, p), "utf-8");

/** Every event name the Rust side emits, resolving `const` names to their value. */
function emittedEvents(): string[] {
  const files = rustFiles();
  const consts = new Map<string, string>();
  for (const file of files) {
    for (const m of read(file).matchAll(/pub const (\w+): &str = "([a-z0-9-]+)";/g)) {
      consts.set(m[1], m[2]);
    }
  }
  const names = new Set<string>();
  for (const file of files) {
    const code = read(file);
    for (const m of code.matchAll(/\.emit\(\s*"([a-z0-9-]+)"/g)) names.add(m[1]);
    for (const m of code.matchAll(/\.emit\(\s*([A-Z_]+)\s*,/g)) {
      const value = consts.get(m[1]);
      if (value) names.add(value);
    }
  }
  return [...names].sort();
}

/** Every event name the frontend listens for, resolving imported constants. */
function listenedEvents(): string[] {
  const files = userFacingFiles();
  const consts = new Map<string, string>();
  for (const file of files) {
    for (const m of read(file).matchAll(/export const (\w+) = "([a-z0-9-]+)";/g)) {
      consts.set(m[1], m[2]);
    }
  }
  const names = new Set<string>();
  for (const file of files) {
    const code = read(file);
    for (const m of code.matchAll(/listen(?:<[^>]*>)?\(\s*"([a-z0-9-]+)"/g)) names.add(m[1]);
    for (const m of code.matchAll(/listen(?:<[^>]*>)?\(\s*([A-Z_]+)\s*,/g)) {
      const value = consts.get(m[1]);
      if (value) names.add(value);
    }
  }
  return [...names].sort();
}

function rustFiles(): string[] {
  const out: string[] = [];
  const walk = (dir: string) => {
    for (const entry of readdirSync(join(root, dir), { withFileTypes: true })) {
      const rel = `${dir}/${entry.name}`;
      if (entry.isDirectory()) walk(rel);
      else if (entry.name.endsWith(".rs")) out.push(rel);
    }
  };
  walk("src-tauri/src");
  return out;
}

function userFacingFiles(): string[] {
  const out: string[] = [];
  const walk = (dir: string) => {
    for (const entry of readdirSync(join(root, dir), { withFileTypes: true })) {
      const rel = `${dir}/${entry.name}`;
      if (entry.isDirectory()) walk(rel);
      else if (/\.tsx?$/.test(entry.name)) out.push(rel);
    }
  };
  walk("src");
  return out;
}


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

  it("shows the friendly hint for the first error a new user meets", () => {
    /* With no model connected, every AI feature fails, and what the user reads
       is decided by whether formatError recognises the backend's wording. The
       automatic-selection path used to answer "No providers registered. Set up a
       model first.", which matched no hint, so that user got a bare sentence
       while everyone else got somewhere to go. Both paths share one constant
       now, and this checks a hint still matches it. */
    const router = read("src-tauri/src/providers/router.rs");
    const message = router.match(/pub const NO_MODEL_CONNECTED: &str = "([^"]+)"/)?.[1];
    expect(message, "NO_MODEL_CONNECTED went missing or changed shape").toBeTruthy();
    expect(message).not.toMatch(/\s\s/);

    /* Every no-provider failure must go through that one constant. Comments are
       stripped first: the constant's own note quotes the wording it replaced,
       and explaining a retired message is not the same as sending it. */
    const code = router.replace(/\/\/\/.*$/gm, "").replace(/\/\*[\s\S]*?\*\//g, "");
    expect(code).not.toContain("No providers registered");

    expect(formatError(new Error(message!))).not.toBe(message);
  });

  it("listens for exactly the events the backend emits", () => {
    /* An event name is a string agreed by two languages and checked by neither.
       Renaming one side leaves the other listening for something that never
       arrives: no error, no failed request, just a progress bar that never
       moves or a download that never reports finishing. */
    const emitted = emittedEvents();
    const listened = listenedEvents();
    expect(emitted.length).toBeGreaterThan(5);
    expect(listened.length).toBeGreaterThan(5);
    expect(emitted.filter((e) => !listened.includes(e))).toEqual([]);
    expect(listened.filter((e) => !emitted.includes(e))).toEqual([]);
  });

  it("only reads theme colours the stylesheet actually defines", () => {
    /* The canvas, the notes graph and the idea space are drawn on a 2D context,
       so they read their colours from CSS custom properties at runtime. Every
       read has a fallback, which means a name no stylesheet defines does not
       fail: the drawing quietly uses a hardcoded colour and stops following the
       theme. That has happened here once already, to a colour meant for
       evidence in the idea space, which fell back to a green that ignored light
       mode entirely. */
    const css = read("src/styles/globals.css");
    const defined = new Set([...css.matchAll(/(--[a-z0-9-]+)\s*:/g)].map((m) => m[1]));
    expect(defined.size).toBeGreaterThan(10);

    const drawings = [
      "src/features/thinking-partner/components/idea-space-view.tsx",
      "src/features/graph/components/graph-3d.tsx",
      "src/features/canvas/pages/canvas-page.tsx",
    ];
    const missing: string[] = [];
    for (const file of drawings) {
      const names = [...read(file).matchAll(/"(--color-[a-z0-9-]+)"/g)].map((m) => m[1]);
      expect(names.length, `${file} reads no theme colours; has it changed shape?`).toBeGreaterThan(0);
      for (const name of new Set(names)) {
        if (!defined.has(name)) missing.push(`${file} reads ${name}, which no stylesheet defines`);
      }
    }
    expect(missing).toEqual([]);
  });

  it("offers exactly the Studio formats the backend can build", () => {
    const backend = backendFormats("src-tauri/src/commands/studio_commands.rs", "match format.as_str()");
    const frontend = frontendFormats("src/features/studio/pages/studio-page.tsx");
    expect(frontend).toEqual(backend);
  });

  it("sends transform names in the exact spelling serde expects", () => {
    /* TransformType crosses as a string and is turned back into an enum by
       serde, so the spelling is decided by its rename_all attribute rather than
       by anything either side can see. lowercase makes ExtractKeyPoints into
       "extractkeypoints"; snake_case would make it "extract_key_points" and
       every transform would fail to deserialize. */
    const service = read("src-tauri/src/services/transform_service.rs");
    const rename = service.match(/#\[serde\(rename_all = "([a-z_]+)"\)\]\s*pub enum TransformType/);
    expect(rename?.[1], "TransformType lost its rename_all attribute").toBe("lowercase");

    const body = service.slice(service.indexOf("pub enum TransformType"));
    const variants = [...body.slice(0, body.indexOf("}")).matchAll(/^\s{4}([A-Z]\w+),/gm)].map((m) => m[1]);
    expect(variants.length).toBeGreaterThan(2);
    const expected = variants.map((v) => v.toLowerCase()).sort();

    const page = read("src/features/content-transformations/pages/transforms-page.tsx");
    const union = page.match(/type TransformType\s*=\s*([^;]+);/);
    const sent = [...(union?.[1] ?? "").matchAll(/"([a-z_]+)"/g)].map((m) => m[1]).sort();

    expect(sent).toEqual(expected);
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
