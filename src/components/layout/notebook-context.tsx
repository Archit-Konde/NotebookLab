/*
 * Name: notebook-context.tsx
 * Purpose: Show which notebook the app is working in, at all times.
 * Description: Chat, the Studio, the Thinking Partner, the Canvas and search all
 *   act on the active notebook, so which one is active decides what every one of
 *   them reads and writes. It used to appear only in the status bar, in the
 *   smallest type on screen, among transient counters, and only once a notebook
 *   had been chosen: with none chosen the row was simply absent, which reads as
 *   "no context needed" rather than "nothing selected". It sits in the header
 *   now, next to the app's own name, because it is context rather than status,
 *   and it always says something.
 * Tech Stack: React, TypeScript, React Router
 * License: MIT
 * Authors: Amey Thakur (https://github.com/Amey-Thakur)
 *          Archit Konde (https://github.com/Archit-Konde)
 * Date: 2026-08-01
 */

import { Link } from "react-router";

import { ROUTES } from "@/lib/constants";
import { useNotebookStore } from "@/stores/notebook-store";
import { useNotebooks } from "@/features/notebooks/hooks/use-notebooks";

export function NotebookContext() {
  const activeNotebookId = useNotebookStore((s) => s.activeNotebookId);
  const { data: notebooks, isPending } = useNotebooks();
  const active = notebooks?.find((notebook) => notebook.id === activeNotebookId);

  /* Hidden only while the first load is genuinely in flight, so the launch does
     not flash "No notebook selected" before the list arrives. Deliberately not
     keyed on the data being absent: a failed load would then hide the control
     entirely, and a header that silently drops the one thing telling the user
     what they are working in is the problem this exists to fix. */
  if (isPending) return null;

  return (
    <Link
      to={ROUTES.NOTEBOOKS}
      data-tour="notebook-context"
      className="group hidden min-w-0 items-center gap-2 sm:flex"
      title={
        active
          ? `Working in ${active.name}. Chat, the Studio and the Canvas all read this notebook. Click to switch.`
          : "No notebook selected. Click to choose one."
      }
    >
      <span aria-hidden="true" className="text-text-4 select-none">
        /
      </span>
      {active ? (
        <span className="flex min-w-0 items-center gap-2">
          {/* The colour the user gave the notebook, which is how they tell them
              apart in the list; repeating it here ties the two together. */}
          <span
            aria-hidden="true"
            className="h-2 w-2 shrink-0 rounded-full"
            style={{ backgroundColor: active.color || "var(--color-accent)" }}
          />
          <span className="truncate font-display text-sm font-semibold text-text-2 transition-colors group-hover:text-text-1">
            {active.name}
          </span>
        </span>
      ) : (
        <span className="font-display text-sm text-text-4 transition-colors group-hover:text-text-2">
          No notebook selected
        </span>
      )}
    </Link>
  );
}
