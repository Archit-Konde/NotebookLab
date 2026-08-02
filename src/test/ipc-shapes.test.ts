/*
 * Name: ipc-shapes.test.ts
 * Purpose: Keep the fields TypeScript reads off a command's result matching the
 *   ones Rust actually sends.
 * Description: A command's return value crosses as JSON and is read through a
 *   hand-written TypeScript interface. Nothing checks the two against each
 *   other: rename a field in Rust and the TypeScript still compiles, still
 *   lints, and reads `undefined` at runtime, which shows up as a blank label or
 *   a progress bar stuck at NaN rather than as an error.
 *
 *   Only structs under commands/ and database/models/ are compared. Those are
 *   the ones that cross the boundary, and scoping matters: there are two
 *   different `ChatResponse` structs in Rust, and the provider-internal one has
 *   nothing to do with the interface of the same name.
 * Tech Stack: Vitest
 * License: MIT
 * Authors: Amey Thakur (https://github.com/Amey-Thakur)
 *          Archit Konde (https://github.com/Archit-Konde)
 * Date: 2026-08-02
 */

import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

const root = join(__dirname, "..", "..");
const read = (p: string) => readFileSync(join(root, p), "utf-8");

function walk(dir: string, match: RegExp): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(join(root, dir), { withFileTypes: true })) {
    const rel = `${dir}/${entry.name}`;
    if (entry.isDirectory()) out.push(...walk(rel, match));
    else if (match.test(entry.name)) out.push(rel);
  }
  return out;
}

/** Serializable structs that cross to the frontend, by name. */
function rustShapes(): Map<string, { fields: Set<string>; renamed: boolean }> {
  const shapes = new Map<string, { fields: Set<string>; renamed: boolean }>();
  const files = [
    ...walk("src-tauri/src/commands", /\.rs$/),
    ...walk("src-tauri/src/database/models", /\.rs$/),
  ];
  for (const file of files) {
    const source = read(file);
    const pattern =
      /#\[derive\(([^)]*)\)\][^\n]*\n(?:#\[serde\(([^)]*)\)\]\s*)?pub struct (\w+)\s*\{([^}]*)\}/g;
    for (const m of source.matchAll(pattern)) {
      const [, derives, serdeAttr, name, body] = m;
      if (!derives.includes("Serialize")) continue;
      const fields = new Set<string>();
      let skipNext = false;
      for (const raw of body.split("\n")) {
        const line = raw.trim();
        if (line.startsWith("#[serde(skip")) skipNext = true;
        if (!line || line.startsWith("//") || line.startsWith("*") || line.startsWith("#[")) continue;
        const fm = line.match(/^pub (\w+):/);
        if (fm) {
          if (skipNext) skipNext = false;
          else fields.add(fm[1]);
        }
      }
      if (fields.size) {
        shapes.set(name, { fields, renamed: (serdeAttr ?? "").includes("rename") });
      }
    }
  }
  return shapes;
}

/** TypeScript interfaces, by name, with the fields they declare. */
function tsShapes(): Map<string, Set<string>> {
  const shapes = new Map<string, Set<string>>();
  for (const file of walk("src", /\.tsx?$/)) {
    if (file.includes(".test.")) continue;
    for (const m of read(file).matchAll(/(?:export )?interface (\w+)\s*\{([^}]*)\}/g)) {
      const fields = new Set([...m[2].matchAll(/^\s*(\w+)\??\s*:/gm)].map((f) => f[1]));
      if (!fields.size) continue;
      const existing = shapes.get(m[1]);
      if (existing) for (const f of fields) existing.add(f);
      else shapes.set(m[1], fields);
    }
  }
  return shapes;
}

/** A Rust enum's wire values, applying its serde rename_all rule. */
function wireValues(file: string, name: string): string[] {
  const source = read(file);
  const at = source.indexOf("pub enum " + name);
  if (at === -1) return [];
  /* The rename_all attribute sits just above the enum. */
  const before = source.slice(Math.max(0, at - 200), at);
  const rule = before.match(/rename_all = "([a-z_]+)"/)?.[1] ?? "";
  const open = source.indexOf("{", at);
  const close = source.indexOf("}", open);
  const body = source.slice(open + 1, close);
  return [...body.matchAll(/^\s*([A-Z]\w*),/gm)].map((v) => {
    const variant = v[1];
    if (rule === "lowercase") return variant.toLowerCase();
    if (rule === "snake_case") return variant.replace(/(?<!^)(?=[A-Z])/g, "_").toLowerCase();
    return variant;
  });
}

/** The string members of a TypeScript union. */
function unionMembers(file: string, pattern: RegExp): string[] {
  const m = read(file).match(pattern);
  return m ? [...m[1].matchAll(/"([a-z_]+)"/g)].map((x) => x[1]) : [];
}

describe("IPC result shapes", () => {
  const rust = rustShapes();
  const ts = tsShapes();
  const shared = [...rust.keys()].filter((name) => ts.has(name) && !rust.get(name)!.renamed);

  it("reads both sides rather than comparing nothing", () => {
    expect(rust.size).toBeGreaterThan(10);
    expect(ts.size).toBeGreaterThan(20);
    expect(shared.length).toBeGreaterThan(8);
  });

  it("spells every enum the same on both sides of the boundary", () => {
    /* These decide what the interface shows. A document is only offered as a
       source once its status reads "processed", and the progress bar only
       clears once a job reads "done", so a variant renamed on one side leaves
       documents permanently unavailable or generations permanently running,
       with no error anywhere to say why. serde's rename_all decides the wire
       spelling, and nothing checks it against the union that reads it. */
    const cases: [string, string, string, RegExp][] = [
      [
        "src-tauri/src/database/models/document.rs",
        "DocumentStatus",
        "src/types/models.ts",
        /status:\s*("pending"[^;]*);/,
      ],
      [
        "src-tauri/src/services/sidecar_service.rs",
        "SidecarState",
        "src/types/models.ts",
        /state:\s*("stopped"[^;]*);/,
      ],
      [
        "src-tauri/src/services/job_service.rs",
        "JobStatus",
        "src/stores/job-store.ts",
        /type JobStatus\s*=\s*([^;]+);/,
      ],
    ];

    const wrong: string[] = [];
    for (const [rustFile, enumName, tsFile, pattern] of cases) {
      const rust = wireValues(rustFile, enumName).sort();
      const ts = unionMembers(tsFile, pattern).sort();
      expect(rust.length, `${enumName} was not found in ${rustFile}`).toBeGreaterThan(2);
      expect(ts.length, `the union for ${enumName} was not found in ${tsFile}`).toBeGreaterThan(2);
      if (rust.join(",") !== ts.join(",")) {
        wrong.push(`${enumName}: Rust sends [${rust}], TypeScript expects [${ts}]`);
      }
    }
    expect(wrong).toEqual([]);
  });

  it("never reads a field Rust does not send", () => {
    /* The dangerous direction. A field TypeScript declares but Rust omits is
       `undefined` at runtime, and undefined renders as a blank rather than an
       error. Rust sending more than TypeScript reads is harmless, so it is not
       treated as a failure. */
    const wrong: string[] = [];
    for (const name of shared) {
      const sent = rust.get(name)!.fields;
      const expected = ts.get(name)!;
      const missing = [...expected].filter((f) => !sent.has(f));
      if (missing.length) {
        wrong.push(`${name}: TypeScript reads ${missing.join(", ")}, which Rust does not send`);
      }
    }
    expect(wrong).toEqual([]);
  });
});
