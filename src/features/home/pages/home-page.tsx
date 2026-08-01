/*
 * Name: home-page.tsx
 * Purpose: The app's home: a calm landing that orients you and gets you moving.
 * Description: The first screen after launch. It greets you by time of day,
 *   offers the four things people reach for most (write, import, ask, search),
 *   brings back the notes you were last in, and previews your notebooks. It
 *   reads real data, degrades to a warm empty state on a fresh install, and is
 *   fully keyboard operable. Spacious and quiet by design, so the work is the
 *   loudest thing on the page.
 * Tech Stack: React 19, TanStack Query, React Router, Tailwind CSS
 * License: MIT
 * Authors: Amey Thakur (https://github.com/Amey-Thakur)
 *          Archit Konde (https://github.com/Archit-Konde)
 * Date: 2026-07-13
 */

import { useMemo } from "react";
import { useNavigate, Link } from "react-router";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { tauriInvoke } from "@/services/tauri-client";
import { QUERY_KEYS, ROUTES } from "@/lib/constants";
import { BrandMark } from "@/components/shared/brand-mark";
import { useNotebookStore } from "@/stores/notebook-store";
import { useUserStore } from "@/stores/user-store";
import { useNotebooks } from "@/features/notebooks/hooks/use-notebooks";
import type { Note, RecentNote, RecentDocument } from "@/types/models";

function greeting(): string {
  const hour = new Date().getHours();
  if (hour < 12) return "Good morning";
  if (hour < 18) return "Good afternoon";
  return "Good evening";
}

type RecentItem = {
  kind: "note" | "document";
  id: string;
  title: string;
  notebookId: string;
  notebookName: string;
  when: string;
  fileType?: string;
};

function relativeTime(iso: string): string {
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return "";
  const min = Math.round((Date.now() - then) / 60000);
  if (min < 1) return "just now";
  if (min < 60) return `${min}m ago`;
  const hr = Math.round(min / 60);
  if (hr < 24) return `${hr}h ago`;
  const day = Math.round(hr / 24);
  if (day === 1) return "yesterday";
  if (day < 7) return `${day}d ago`;
  return new Date(iso).toLocaleDateString();
}

function fileTypeLabel(fileType: string): string {
  const t = fileType.toLowerCase();
  if (t === "pdf") return "PDF";
  if (t === "docx" || t === "doc") return "Word";
  if (["png", "jpg", "jpeg", "tiff", "tif", "webp", "bmp"].includes(t)) return "Image";
  if (t === "md" || t === "markdown") return "Markdown";
  return "Text";
}

export function HomePage() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const activeNotebookId = useNotebookStore((s) => s.activeNotebookId);
  const setActiveNotebook = useNotebookStore((s) => s.setActiveNotebook);
  const displayName = useUserStore((s) => s.displayName).trim();

  const { data: notebooks } = useNotebooks();
  const { data: recentNotes } = useQuery({
    queryKey: [QUERY_KEYS.NOTES, "recent", 6],
    queryFn: () => tauriInvoke<RecentNote[]>("list_recent_notes", { limit: 6 }),
  });
  const { data: recentDocuments } = useQuery({
    queryKey: [QUERY_KEYS.DOCUMENTS, "recent"],
    queryFn: () => tauriInvoke<RecentDocument[]>("list_recent_documents", { limit: 6 }),
  });

  const newNote = () => {
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
  };

  /* Each hint states something the label does not: where the note lands, which
     files can be dropped, what an answer is built from, what is searched. The
     first read "Start writing", which is only "New note" again, and the second
     named three formats when the app also reads Word documents and images. */
  const actions = [
    { label: "New note", hint: "In the open notebook", onClick: newNote, icon: <IconPencil /> },
    { label: "Import a document", hint: "PDF, Word, text, Markdown, images", onClick: () => navigate(ROUTES.DOCUMENTS), icon: <IconImport /> },
    { label: "Ask a question", hint: "Answered from your sources", onClick: () => navigate(ROUTES.CHAT), icon: <IconChat /> },
    { label: "Search everything", hint: "Notes and documents", onClick: () => navigate(ROUTES.SEARCH), icon: <IconSearch /> },
  ];

  const openNotebook = (id: string) => {
    setActiveNotebook(id);
    navigate(`/notebooks/${id}`);
  };

  /* Unify recent notes and documents into one "jump back in" list, newest
     first. A note opens in the editor; a document opens its notebook, where it
     can be read, searched, and chatted with. */
  const recentItems = useMemo<RecentItem[]>(() => {
    const notes: RecentItem[] = (recentNotes ?? []).map((n) => ({
      kind: "note",
      id: n.id,
      title: n.title || "Untitled note",
      notebookId: n.notebook_id,
      notebookName: n.notebook_name,
      when: n.updated_at,
    }));
    const docs: RecentItem[] = (recentDocuments ?? []).map((d) => ({
      kind: "document",
      id: d.id,
      title: d.title,
      notebookId: d.notebook_id,
      notebookName: d.notebook_name,
      when: d.created_at,
      fileType: d.file_type,
    }));
    return [...notes, ...docs].sort((a, b) => (a.when < b.when ? 1 : -1)).slice(0, 6);
  }, [recentNotes, recentDocuments]);

  const openItem = (item: RecentItem) => {
    setActiveNotebook(item.notebookId);
    if (item.kind === "note") {
      navigate(`/editor/${item.id}`);
    } else {
      navigate(`/notebooks/${item.notebookId}`);
    }
  };

  const hasNotebooks = (notebooks?.length ?? 0) > 0;
  const hasRecents = recentItems.length > 0;
  const isEmpty = notebooks !== undefined && !hasNotebooks && !hasRecents;

  return (
    <div className="px-8 py-14 max-w-4xl mx-auto">
      {/* Greeting.

          The mark and the wordmark used to be repeated here as an eyebrow, an
          inch below the header that already carries both permanently, and the
          line about staying on your machine was repeated again by the empty
          state directly beneath it. Neither told a returning user anything they
          did not already know, so the greeting stands on its own. */}
      <header className="mb-14">
        <h1 className="font-display text-4xl font-bold text-text-1 tracking-tight">
          {displayName ? `${greeting()}, ${displayName}` : greeting()}
        </h1>
      </header>

      {/* Quick actions */}
      <section className="mb-14" aria-label="Quick actions">
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-3">
          {actions.map((action) => (
            <button
              key={action.label}
              type="button"
              onClick={action.onClick}
              className="group text-left p-5 border border-border bg-surface hover:border-accent-dim
                         focus-visible:border-accent outline-none transition-colors"
            >
              <span className="flex h-9 w-9 items-center justify-center border border-border text-accent
                               group-hover:border-accent-dim transition-colors">
                {action.icon}
              </span>
              <p className="mt-4 text-sm font-semibold text-text-1">{action.label}</p>
              <p className="mt-1 text-xs text-text-3">{action.hint}</p>
            </button>
          ))}
        </div>
      </section>

      {/* Empty state for a fresh install */}
      {isEmpty && (
        <section className="border border-border bg-surface px-8 py-16 text-center">
          <div className="flex justify-center mb-5">
            <BrandMark className="h-12 w-12" />
          </div>
          <h2 className="font-display text-xl font-bold text-text-1 mb-2">Start with one document</h2>
          <p className="font-body text-sm text-text-2 max-w-md mx-auto mb-7 leading-relaxed">
            Create a notebook, drop in a PDF or a note, and ask the AI what it says. Nothing you
            add ever leaves this machine.
          </p>
          <div className="flex flex-wrap items-center justify-center gap-3">
            <Link
              to={ROUTES.NOTEBOOKS}
              className="px-4 py-2 text-sm font-mono bg-primary text-on-primary hover:bg-primary-hover transition-colors"
            >
              Create a notebook
            </Link>
            <Link
              to={ROUTES.DOCUMENTS}
              className="px-4 py-2 text-sm font-mono border border-border text-text-3
                         hover:text-text-1 hover:border-border-hover transition-colors"
            >
              Import a document
            </Link>
          </div>
        </section>
      )}

      {/* Jump back in: recent notes and documents, newest first */}
      {hasRecents && (
        <section className="mb-14">
          <h2 className="text-xs font-mono tracking-widest uppercase text-text-4 mb-4">
            Jump back in
          </h2>
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
            {recentItems.map((item) => (
              <button
                key={`${item.kind}-${item.id}`}
                type="button"
                onClick={() => openItem(item)}
                className="flex items-start gap-3 text-left p-4 border border-border bg-surface
                           hover:border-accent-dim focus-visible:border-accent outline-none transition-colors"
              >
                <span className="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center border border-border text-accent">
                  {item.kind === "note" ? <IconNote /> : <IconDoc />}
                </span>
                <span className="min-w-0">
                  <span className="block text-sm font-medium text-text-1 truncate">{item.title}</span>
                  <span className="mt-1 block text-2xs font-mono text-text-4 truncate">
                    {item.kind === "document" ? `${fileTypeLabel(item.fileType ?? "")} · ` : ""}
                    {item.notebookName} &middot; {relativeTime(item.when)}
                  </span>
                </span>
              </button>
            ))}
          </div>
        </section>
      )}

      {/* Notebooks preview */}
      {hasNotebooks && (
        <section>
          <div className="flex items-center justify-between mb-4">
            <h2 className="text-xs font-mono tracking-widest uppercase text-text-4">Your notebooks</h2>
            <Link to={ROUTES.NOTEBOOKS} className="text-xs font-mono text-text-4 hover:text-text-2 transition-colors">
              View all
            </Link>
          </div>
          <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
            {notebooks!.slice(0, 6).map((nb) => (
              <div
                key={nb.id}
                role="button"
                tabIndex={0}
                aria-label={`Open notebook ${nb.name}`}
                onClick={() => openNotebook(nb.id)}
                onKeyDown={(event) => {
                  if (event.key === "Enter" || event.key === " ") {
                    event.preventDefault();
                    openNotebook(nb.id);
                  }
                }}
                className="border border-border bg-surface p-5 cursor-pointer hover:border-accent-dim
                           focus-visible:border-accent outline-none transition-colors"
              >
                <div className="flex items-center gap-2 mb-3">
                  <span className="w-2.5 h-2.5 rounded-full shrink-0" style={{ backgroundColor: nb.color }} />
                </div>
                <h3 className="text-sm font-semibold text-text-1 mb-1 truncate">{nb.name}</h3>
                {nb.description && <p className="text-xs text-text-3 line-clamp-2">{nb.description}</p>}
              </div>
            ))}
          </div>
        </section>
      )}
    </div>
  );
}

/* Line icons, drawn in the accent color by their parent */
function IconPencil() {
  return (
    <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M12 20h9" />
      <path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4z" />
    </svg>
  );
}
function IconImport() {
  return (
    <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M12 3v12" />
      <path d="M8 11l4 4 4-4" />
      <path d="M4 17v2a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-2" />
    </svg>
  );
}
function IconChat() {
  return (
    <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M21 12a8 8 0 0 1-11.5 7.2L4 20l.8-5.5A8 8 0 1 1 21 12z" />
    </svg>
  );
}
function IconSearch() {
  return (
    <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <circle cx="11" cy="11" r="7" />
      <path d="M21 21l-4.3-4.3" />
    </svg>
  );
}
function IconNote() {
  return (
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M6 3h9l3 3v15H6z" />
      <path d="M9 8h6M9 12h6M9 16h4" />
    </svg>
  );
}
function IconDoc() {
  return (
    <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M7 2h7l4 4v16H7z" />
      <path d="M14 2v4h4" />
      <path d="M10 13h5M10 17h3" />
    </svg>
  );
}
