/*
 * Name: use-open-with.ts
 * Purpose: Handle files opened from the OS ("Open with NotebookLab").
 * Description: Two arrival paths, one behavior. Files passed at launch are
 *   collected once from the backend (held there until the frontend mounts);
 *   files opened while the app is already running arrive on the "open-files"
 *   event from the single-instance plugin. Either way the file lands in a
 *   sensible notebook: the active one, else the first, else a fresh "Imported
 *   files" notebook, and the app navigates there so the user watches their
 *   document appear and index. Imports run sequentially through the same
 *   pipeline as every other import; a failed file shows its error status in
 *   the notebook like any failed import would.
 * Tech Stack: React 19, TanStack Query, Tauri v2
 * License: MIT
 * Authors: Amey Thakur (https://github.com/Amey-Thakur)
 *          Archit Konde (https://github.com/Archit-Konde)
 * Date: 2026-07-17
 */

import { useEffect, useRef } from "react";
import { useNavigate } from "react-router";
import { useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";

import { hasTauri, tauriInvoke } from "@/services/tauri-client";
import { QUERY_KEYS } from "@/lib/constants";
import { useNotebookStore } from "@/stores/notebook-store";
import type { Notebook } from "@/types/models";

export function useOpenWith() {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const navigateRef = useRef(navigate);
  useEffect(() => {
    navigateRef.current = navigate;
  });

  useEffect(() => {
    let cancelled = false;

    const importFiles = async (paths: string[]) => {
      if (paths.length === 0) return;

      /* Pick where the files should land. */
      const notebooks = await tauriInvoke<Notebook[]>("list_notebooks");
      const activeId = useNotebookStore.getState().activeNotebookId;
      let target = notebooks.find((nb) => nb.id === activeId) ?? notebooks[0];
      if (!target) {
        target = await tauriInvoke<Notebook>("create_notebook", {
          input: { name: "Imported files", description: null, color: null },
        });
        queryClient.invalidateQueries({ queryKey: [QUERY_KEYS.NOTEBOOKS] });
      }
      if (cancelled) return;

      useNotebookStore.getState().setActiveNotebook(target.id);
      navigateRef.current(`/notebooks/${target.id}`);

      /* Sequential, so several files opened at once do not race the parser
         pool; each shows up (and indexes) in the notebook as it lands. */
      for (const path of paths) {
        try {
          await tauriInvoke<string>("import_document", {
            notebook_id: target.id,
            file_path: path,
          });
        } catch (error) {
          /* A failed file surfaces through its error-status row in the
             notebook, matching every other import failure. */
          console.warn("Open-with import failed:", path, error);
        }
        queryClient.invalidateQueries({ queryKey: [QUERY_KEYS.DOCUMENTS] });
        queryClient.invalidateQueries({ queryKey: [QUERY_KEYS.CHUNK_COUNT] });
      }
    };

    /* Files the app was launched with. */
    tauriInvoke<string[]>("take_startup_files")
      .then((files) => {
        if (!cancelled) return importFiles(files);
      })
      .catch(() => {
        /* Not running under Tauri (dev preview): nothing to collect. */
      });

    if (!hasTauri()) return;

    /* Files opened while the app is already running. */
    const unlisten = listen<string[]>("open-files", (event) => {
      importFiles(event.payload).catch((error) => {
        console.warn("Open-with handling failed:", error);
      });
    });

    return () => {
      cancelled = true;
      unlisten.then((fn) => fn());
    };
  }, [queryClient]);
}
