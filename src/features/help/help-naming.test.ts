/*
 * Name: help-naming.test.ts
 * Purpose: Keep user-facing copy calling each feature what the sidebar calls it.
 * Description: The Help page told readers to open the "Thinking Partner" while
 *   the sidebar said "Think", so the page people reach when they cannot find
 *   something named a menu item that does not exist. The same drift had already
 *   happened once in the first-run sample notes. Renaming a destination is easy;
 *   remembering every place that names it is not, so the sidebar is read as the
 *   source of truth and the copy is checked against it.
 * Tech Stack: Vitest
 * License: MIT
 * Authors: Amey Thakur (https://github.com/Amey-Thakur)
 *          Archit Konde (https://github.com/Archit-Konde)
 * Date: 2026-08-02
 */

import { readFileSync } from "node:fs";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

const root = join(__dirname, "..", "..", "..");
const read = (p: string) => readFileSync(join(root, p), "utf-8");

const sidebar = read("src/components/layout/app-sidebar.tsx");
const help = read("src/features/help/pages/help-page.tsx");

/** Every label the sidebar renders, which is what the user actually sees. */
function sidebarLabels(): string[] {
  return [...sidebar.matchAll(/label: "([^"]+)"/g)].map((m) => m[1]);
}

/** The heading of each Help section. */
function helpTitles(): string[] {
  return [...help.matchAll(/^\s{4}title: "([^"]+)",$/gm)].map((m) => m[1]);
}

describe("help page naming", () => {
  it("reads the sidebar labels it is checked against", () => {
    const labels = sidebarLabels();
    expect(labels).toContain("Think");
    expect(labels).toContain("Studio");
    expect(labels.length).toBeGreaterThan(10);
  });

  it("finds the Help section titles", () => {
    expect(helpTitles().length).toBeGreaterThan(10);
  });

  it("never calls a feature by a name the sidebar does not use", () => {
    /* Names this app has used before and no longer does. A reader sent to any of
       them looks for a menu item that is not there. */
    const retired = ["Thinking Partner", "Booklet", "Field Guide", "Podcasts page"];
    const labels = sidebarLabels();
    for (const name of retired) {
      if (labels.includes(name)) continue;
      expect(help, `Help still calls something "${name}"`).not.toContain(name);
    }
  });

  it("titles a section after a sidebar destination exactly, when it names one", () => {
    /* A title that is only a destination name has to match the sidebar. Titles
       that are sentences ("Chat with your sources") are headings rather than
       labels and are left alone, so long as the destination appears in them. */
    const labels = new Set(sidebarLabels());
    const wrong: string[] = [];
    for (const title of helpTitles()) {
      const stripped = title.replace(/^The /, "");
      if (labels.has(stripped) && !labels.has(title)) {
        wrong.push(`"${title}" should be "${stripped}"`);
      }
    }
    expect(wrong).toEqual([]);
  });

  it("points at each of the app's tools somewhere", () => {
    /* A tool the Help page never mentions is one the user has to find alone. */
    for (const tool of ["Chat", "Think", "Studio", "Canvas", "Transform", "Audio Studio", "Prompt Studio", "Models", "Search"]) {
      expect(help, `Help never mentions ${tool}`).toContain(tool);
    }
  });
});
