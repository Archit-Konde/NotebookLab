/*
 * Name: command-palette.tsx
 * Purpose: Ctrl+K command palette for jumping anywhere and acting fast.
 * Description: One input filters pages, notebooks, and actions; arrows move,
 *   Enter runs, Escape closes. Matches rank prefix hits first. Fully
 *   keyboard driven with dialog and listbox semantics for screen
 *   readers, and the backdrop click closes it for mouse users.
 * Tech Stack: React 19, React Router, TanStack Query, Tailwind CSS
 * License: MIT
 * Authors: Amey Thakur (https://github.com/Amey-Thakur)
 *          Archit Konde (https://github.com/Archit-Konde)
 * Date: 2026-07-12
 */

import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router";
import { useQueryClient } from "@tanstack/react-query";

import { tauriInvoke } from "@/services/tauri-client";
import { QUERY_KEYS, ROUTES } from "@/lib/constants";
import { useNotebookStore } from "@/stores/notebook-store";
import { useNotebooks } from "@/features/notebooks/hooks/use-notebooks";
import { useTheme } from "@/components/providers/theme-context";
import type { Note } from "@/types/models";


interface PaletteItem {
  id: string;
  label: string;
  hint: string;
  run: () => void;
}

interface CommandPaletteProps {
  open: boolean;
  onClose: () => void;
}


const PAGES: Array<{ label: string; route: string }> = [
  { label: "Home", route: ROUTES.HOME },
  { label: "Notebooks", route: ROUTES.NOTEBOOKS },
  { label: "Documents", route: ROUTES.DOCUMENTS },
  { label: "Search", route: ROUTES.SEARCH },
  { label: "Chat", route: ROUTES.CHAT },
  { label: "Think", route: ROUTES.THINKING_PARTNER },
  { label: "Studio", route: ROUTES.STUDIO },
  { label: "Canvas", route: ROUTES.CANVAS },
  { label: "Transform", route: ROUTES.TRANSFORMS },
  { label: "Audio Studio", route: ROUTES.PODCASTS },
  { label: "Prompt Studio", route: ROUTES.PROMPT_STUDIO },
  { label: "Connections", route: ROUTES.GRAPH },
  { label: "Models", route: ROUTES.MODELS },
  { label: "Settings", route: ROUTES.SETTINGS },
  { label: "Help", route: ROUTES.HELP },
  { label: "About", route: ROUTES.ABOUT },
];


export function CommandPalette({ open, onClose }: CommandPaletteProps) {
  /* Mounting fresh on every open gives clean state with no reset effects */
  if (!open) return null;
  return <PaletteDialog onClose={onClose} />;
}


function PaletteDialog({ onClose }: { onClose: () => void }) {
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState(0);
  const listRef = useRef<HTMLUListElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const openerRef = useRef<HTMLElement | null>(null);
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { data: notebooks } = useNotebooks();
  const { resolvedTheme, setTheme } = useTheme();
  const activeNotebookId = useNotebookStore((s) => s.activeNotebookId);
  const setActiveNotebook = useNotebookStore((s) => s.setActiveNotebook);

  const items = useMemo<PaletteItem[]>(() => {
    const go = (route: string) => () => {
      navigate(route);
      onClose();
    };

    const actions: PaletteItem[] = [
      {
        id: "action-new-note",
        label: "New note",
        hint: "action",
        run: () => {
          onClose();
          if (!activeNotebookId) {
            navigate(ROUTES.NOTEBOOKS);
            return;
          }
          tauriInvoke<Note>("create_note", { input: { notebook_id: activeNotebookId } })
            .then((note) => {
              queryClient.invalidateQueries({ queryKey: [QUERY_KEYS.NOTES, activeNotebookId] });
              navigate(`/editor/${note.id}`);
            })
            .catch(() => navigate(ROUTES.NOTEBOOKS));
        },
      },
      {
        id: "action-theme",
        label: resolvedTheme === "dark" ? "Switch to light theme" : "Switch to dark theme",
        hint: "action",
        run: () => {
          setTheme(resolvedTheme === "dark" ? "light" : "dark");
          onClose();
        },
      },
    ];

    const pageItems: PaletteItem[] = PAGES.map((p) => ({
      id: `page-${p.route}`,
      label: p.label,
      hint: "page",
      run: go(p.route),
    }));

    const notebookItems: PaletteItem[] = (notebooks ?? []).map((nb) => ({
      id: `notebook-${nb.id}`,
      label: nb.name,
      hint: "notebook",
      run: () => {
        setActiveNotebook(nb.id);
        navigate(`/notebooks/${nb.id}`);
        onClose();
      },
    }));

    return [...actions, ...pageItems, ...notebookItems];
  }, [notebooks, resolvedTheme, activeNotebookId, navigate, onClose, queryClient, setActiveNotebook, setTheme]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return items;
    const starts = items.filter((i) => i.label.toLowerCase().startsWith(q));
    const contains = items.filter(
      (i) => !i.label.toLowerCase().startsWith(q) && i.label.toLowerCase().includes(q),
    );
    return [...starts, ...contains];
  }, [items, query]);

  /* Typing changes the result list, so selection snaps back to the top.
     Render-phase adjustment avoids an extra effect pass. */
  const [prevQuery, setPrevQuery] = useState(query);
  if (prevQuery !== query) {
    setPrevQuery(query);
    setSelected(0);
  }

  /* Keep the selected option visible while arrowing through a long list */
  useEffect(() => {
    listRef.current
      ?.querySelector('[aria-selected="true"]')
      ?.scrollIntoView({ block: "nearest" });
  }, [selected]);

  /* Escape always closes, even if focus has drifted off the input. */
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  /* Hand focus back to whatever opened the palette when it closes, so keyboard
     users are not dropped at the top of the page. Capture the opener in a
     layout effect BEFORE moving focus into the input, then focus the input
     ourselves; a passive effect would run after the input's autofocus and
     capture the input itself, so the input carries no autoFocus. */
  useLayoutEffect(() => {
    openerRef.current = document.activeElement as HTMLElement | null;
    inputRef.current?.focus();
    return () => openerRef.current?.focus?.();
  }, []);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelected((s) => Math.min(s + 1, filtered.length - 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelected((s) => Math.max(s - 1, 0));
    } else if (e.key === "Enter") {
      e.preventDefault();
      filtered[selected]?.run();
    } else if (e.key === "Escape") {
      e.preventDefault();
      onClose();
    } else if (e.key === "Tab") {
      /* The input is the only focusable control; keep Tab from escaping the
         open dialog to the page behind the scrim. */
      e.preventDefault();
    }
  };

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center pt-[18vh] bg-black/40"
      onClick={onClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Command palette"
        className="w-full max-w-lg mx-4 bg-surface border border-border shadow-xl"
        onClick={(e) => e.stopPropagation()}
        onKeyDown={handleKeyDown}
      >
        <input
          ref={inputRef}
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Jump to a page or notebook, or run an action..."
          aria-label="Search commands"
          role="combobox"
          aria-expanded="true"
          aria-controls="palette-listbox"
          aria-activedescendant={filtered.length > 0 ? `palette-option-${selected}` : undefined}
          className="w-full px-4 py-3 text-sm bg-surface text-text-1 border-b border-border
                     placeholder:text-text-4 outline-none"
        />
        <ul id="palette-listbox" ref={listRef} role="listbox" aria-label="Commands" className="max-h-72 overflow-y-auto py-1">
          {filtered.length === 0 && (
            <li className="px-4 py-3 text-sm text-text-4">Nothing matches. Try fewer letters.</li>
          )}
          {filtered.map((item, index) => (
            <li
              key={item.id}
              id={`palette-option-${index}`}
              role="option"
              aria-selected={index === selected}
              className={`flex items-center justify-between px-4 py-2 text-sm cursor-pointer ${
                index === selected ? "bg-surface-2 text-text-1" : "text-text-2"
              }`}
              onMouseEnter={() => setSelected(index)}
              onClick={() => item.run()}
            >
              <span className="truncate">{item.label}</span>
              <span className="ml-3 shrink-0 text-2xs font-mono text-text-4">{item.hint}</span>
            </li>
          ))}
        </ul>
        <div className="flex items-center gap-3 px-4 py-2 border-t border-border text-2xs font-mono text-text-4">
          <span>Up and Down to move</span>
          <span>Enter to run</span>
          <span>Esc to close</span>
        </div>
      </div>
    </div>
  );
}
