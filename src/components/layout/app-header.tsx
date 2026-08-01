/*
 * Name: app-header.tsx
 * Purpose: Top application bar with hamburger toggle, brand, model switcher,
 *   command palette trigger, and theme switch.
 * Description: Hamburger button visible only on mobile (<768px). The model
 *   switcher shows the active AI model and swaps it in two clicks. The
 *   "Jump to..." button opens the command palette, the same action bound to
 *   Ctrl+K globally in AppShell, so the visible kbd hint is real.
 * Tech Stack: React 19, Tailwind CSS, Tauri v2
 * License: MIT
 * Authors: Amey Thakur (https://github.com/Amey-Thakur)
 *          Archit Konde (https://github.com/Archit-Konde)
 * Date: 2026-07-12
 */

import { ThemeToggle } from "@/components/shared/theme-toggle";
import { BrandMark } from "@/components/shared/brand-mark";
import { ModelSwitcher } from "@/components/layout/model-switcher";
import { NotebookContext } from "@/components/layout/notebook-context";
import { IS_MAC } from "@/lib/shortcuts";


interface AppHeaderProps {
  sidebarOpen: boolean;
  onToggleSidebar: () => void;
  onOpenPalette: () => void;
}

/* macOS labels the palette shortcut with the Command symbol */
const PALETTE_HINT = IS_MAC ? "⌘K" : "Ctrl+K";


export function AppHeader({ sidebarOpen, onToggleSidebar, onOpenPalette }: AppHeaderProps) {
  return (
    <header
      className="flex items-center justify-between h-10 px-4 border-b border-border bg-bg select-none"
      data-tauri-drag-region
    >
      <div className="flex min-w-0 items-center gap-3">
        {/* Hamburger - mobile only */}
        <button
          type="button"
          aria-label="Toggle navigation"
          aria-expanded={sidebarOpen}
          aria-controls="app-sidebar"
          onClick={onToggleSidebar}
          className="md:hidden text-text-3 hover:text-text-1 transition-colors"
        >
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <path d="M3 12h18M3 6h18M3 18h18" />
          </svg>
        </button>

        <span className="flex shrink-0 items-center gap-2">
          <BrandMark className="h-5 w-5" />
          <span className="font-display text-base font-bold tracking-tight text-text-1">
            NotebookLab
          </span>
        </span>

        <NotebookContext />
      </div>

      <div className="flex items-center gap-2">
        <ModelSwitcher />
        <button
          type="button"
          data-tour="search"
          aria-label={`Open universal search (${PALETTE_HINT})`}
          className="hidden sm:flex px-3 py-1 text-xs font-mono text-text-3 bg-surface-2 border border-border
                     hover:border-border-hover focus-visible:border-accent transition-colors"
          onClick={onOpenPalette}
        >
          Search...
          <kbd className="ml-2 text-text-4" aria-hidden="true">{PALETTE_HINT}</kbd>
        </button>

        <ThemeToggle />
      </div>
    </header>
  );
}
