/*
 * Name: ipc-arguments.test.ts
 * Purpose: Keep the argument names the frontend sends matching the ones each
 *   Tauri command declares.
 * Description: Command arguments cross as a JSON object and are turned back
 *   into function parameters by serde, matched on name. Nothing on either side
 *   is checked by a compiler: rename a parameter in Rust, or mistype a key in
 *   TypeScript, and the command fails at the boundary with a deserialization
 *   error the moment a user presses the button, having compiled and linted
 *   cleanly. Every call site is read and compared against the command it names.
 *
 *   Optional parameters are allowed to be absent, which is what `Option<T>`
 *   means to serde. Everything else must be supplied, and nothing may be sent
 *   that the command does not declare.
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

/** Parameters Tauri injects itself; the caller never supplies them. */
const INJECTED = new Set(["app", "window", "state", "sidecar", "webview", "app_handle"]);
const INJECTED_TYPES = ["AppHandle", "State<", "Window", "WebviewWindow"];

interface CommandParams {
  required: Set<string>;
  optional: Set<string>;
}

function walk(dir: string, match: RegExp): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(join(root, dir), { withFileTypes: true })) {
    const rel = `${dir}/${entry.name}`;
    if (entry.isDirectory()) out.push(...walk(rel, match));
    else if (match.test(entry.name)) out.push(rel);
  }
  return out;
}

/** Every #[tauri::command] and the arguments it expects from the caller. */
function commands(): Map<string, CommandParams> {
  const found = new Map<string, CommandParams>();
  for (const file of walk("src-tauri/src", /\.rs$/)) {
    const source = read(file);
    const pattern =
      /#\[tauri::command[^\]]*\]\s*(?:pub\s+)?(?:async\s+)?fn\s+(\w+)\s*\(([\s\S]*?)\)/g;
    for (const m of source.matchAll(pattern)) {
      const required = new Set<string>();
      const optional = new Set<string>();
      for (const raw of m[2].split(",")) {
        const part = raw.trim();
        if (!part.includes(":")) continue;
        const name = part.slice(0, part.indexOf(":")).trim();
        const type = part.slice(part.indexOf(":") + 1);
        if (INJECTED.has(name)) continue;
        if (INJECTED_TYPES.some((t) => type.includes(t))) continue;
        /* Option<T> is absent-tolerant to serde, so the caller may omit it. */
        if (type.includes("Option<")) optional.add(name);
        else required.add(name);
      }
      found.set(m[1], { required, optional });
    }
  }
  return found;
}

/** Split an object literal body on commas that are not nested. */
function splitTopLevel(body: string): string[] {
  const parts: string[] = [];
  let depth = 0;
  let current = "";
  for (const ch of body) {
    if ("([{".includes(ch)) depth++;
    else if (")]}".includes(ch)) depth--;
    if (ch === "," && depth === 0) {
      parts.push(current);
      current = "";
    } else current += ch;
  }
  if (current.trim()) parts.push(current);
  return parts;
}

/** The keys an object literal sets, ignoring the values they are set to. */
function keysOf(body: string): Set<string> {
  const keys = new Set<string>();
  for (const raw of splitTopLevel(body)) {
    const part = raw.trim();
    if (!part || part.startsWith("...")) continue;
    const key = (part.includes(":") ? part.slice(0, part.indexOf(":")) : part).trim().replace(/["']/g, "");
    if (/^[A-Za-z_$][\w$]*$/.test(key)) keys.add(key);
  }
  return keys;
}

describe("IPC arguments", () => {
  const declared = commands();

  /* Three shapes reach a command. A direct invoke names it inline. The long
     generations go through useJobRun, which is handed the command once and the
     arguments later, so the variable holding it must be followed. And the
     Thinking Partner picks between two of those runs with a ternary, then
     passes one of two argument objects the same way, so a variable can stand
     for either command and a call can carry either object.

     Missing the second shape left every generation command unchecked, which a
     deliberate rename proved by passing; missing the third left the idea space
     and the Socratic questions unchecked on top of that. */
  interface Site {
    file: string;
    command: string;
    sent: Set<string>;
  }

  /** Every `{ ... }` object at the top level of a call's arguments. */
  function objectsIn(argumentText: string): string[] {
    const found: string[] = [];
    let depth = 0;
    let start = -1;
    for (let i = 0; i < argumentText.length; i++) {
      const ch = argumentText[i];
      if (ch === "{") {
        if (depth === 0) start = i + 1;
        depth++;
      } else if (ch === "}") {
        depth--;
        if (depth === 0 && start >= 0) {
          found.push(argumentText.slice(start, i));
          start = -1;
        }
      }
    }
    return found;
  }

  /** The text between the parentheses of `name.start( ... )`. */
  function startArguments(source: string, variable: string): string[] {
    const calls: string[] = [];
    const needle = variable + ".start(";
    let at = source.indexOf(needle);
    while (at !== -1) {
      let depth = 0;
      let i = at + needle.length - 1;
      const from = i + 1;
      for (; i < source.length; i++) {
        if (source[i] === "(") depth++;
        else if (source[i] === ")") {
          depth--;
          if (depth === 0) break;
        }
      }
      calls.push(source.slice(from, i));
      at = source.indexOf(needle, i);
    }
    return calls;
  }

  const callSites: Site[] = [];
  const jobSites: { file: string; commands: string[]; sent: Set<string> }[] = [];

  for (const file of walk("src", /\.tsx?$/).filter((f) => !f.includes(".test."))) {
    const source = read(file);
    for (const m of source.matchAll(
      /tauriInvoke(?:<[^>]*>)?\(\s*"(\w+)"\s*,\s*\{([\s\S]*?)\}\s*\)/g,
    )) {
      callSites.push({ file, command: m[1], sent: keysOf(m[2]) });
    }

    const byVariable = new Map<string, string[]>();
    for (const m of source.matchAll(/const\s+(\w+)\s*=\s*useJobRun\(\s*"(\w+)"/g)) {
      byVariable.set(m[1], [m[2]]);
    }
    /* `const run = mode === "ideas" ? ideas : socratic` stands for both. */
    for (const m of source.matchAll(/const\s+(\w+)\s*=\s*[^;]*?\?\s*(\w+)\s*:\s*(\w+);/g)) {
      const left = byVariable.get(m[2]);
      const right = byVariable.get(m[3]);
      if (left && right) byVariable.set(m[1], [...left, ...right]);
    }

    for (const [variable, commands] of byVariable) {
      for (const argumentText of startArguments(source, variable)) {
        for (const body of objectsIn(argumentText)) {
          jobSites.push({ file, commands: commands.filter((c) => declared.has(c)), sent: keysOf(body) });
        }
      }
    }
  }

  const directSites = callSites.filter((c) => declared.has(c.command));

  it("reads both sides rather than comparing nothing", () => {
    expect(declared.size).toBeGreaterThan(40);
    expect(directSites.length).toBeGreaterThan(20);
    /* Every shape must be represented, or a whole family goes unchecked. */
    const covered = new Set(jobSites.flatMap((s) => s.commands));
    for (const command of [
      "send_chat_message",
      "generate_studio",
      "generate_podcast",
      "transform_document",
      "generate_idea_space",
      "generate_socratic_questions",
    ]) {
      expect(covered.has(command), `no call site found for ${command}`).toBe(true);
    }
  });

  it("sends no argument the command does not declare", () => {
    /* A key the command has no parameter for is dropped at best and refused at
       worst, and either way the button does nothing the user can explain. */
    const wrong: string[] = [];
    for (const { file, command, sent } of directSites) {
      const params = declared.get(command)!;
      const unknown = [...sent].filter((k) => !params.required.has(k) && !params.optional.has(k));
      if (unknown.length) wrong.push(`${command} in ${file} sends ${unknown.join(", ")}`);
    }
    /* A ternary run may be either command, so the object has to satisfy one. */
    for (const { file, commands, sent } of jobSites) {
      const fits = commands.some((command) => {
        const params = declared.get(command)!;
        return [...sent].every((k) => params.required.has(k) || params.optional.has(k));
      });
      if (!fits) wrong.push(`${commands.join("/")} in ${file} sends ${[...sent].sort().join(", ")}`);
    }
    expect(wrong).toEqual([]);
  });

  it("supplies every argument the command requires", () => {
    const missing: string[] = [];
    for (const { file, command, sent } of directSites) {
      const params = declared.get(command)!;
      const absent = [...params.required].filter((k) => !sent.has(k));
      if (absent.length) missing.push(`${command} in ${file} omits ${absent.join(", ")}`);
    }
    for (const { file, commands, sent } of jobSites) {
      const fits = commands.some((command) =>
        [...declared.get(command)!.required].every((k) => sent.has(k)),
      );
      if (!fits) missing.push(`${commands.join("/")} in ${file} omits a required argument`);
    }
    expect(missing).toEqual([]);
  });
});
