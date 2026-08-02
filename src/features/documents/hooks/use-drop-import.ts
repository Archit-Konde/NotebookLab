/*
 * Name: use-drop-import.ts
 * Purpose: Drag a file onto the window, get it imported.
 * Description: Subscribes to Tauri's native drag and drop events, which
 *   deliver real file paths rather than browser File objects, filters to
 *   the supported document types, and feeds each dropped file through the
 *   existing import pipeline. Exposes the drag state so pages can show a
 *   drop target overlay while a file hovers over the window.
 * Tech Stack: React 19, Tauri v2 webview events, TanStack Query
 * License: MIT
 * Authors: Amey Thakur (https://github.com/Amey-Thakur)
 *          Archit Konde (https://github.com/Archit-Konde)
 * Date: 2026-07-12
 */

import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";

import { isSupportedFileType, SUPPORTED_FILE_TYPES } from "@/lib/constants";
import { useImportDocument } from "./use-documents";


export function useDropImport(notebookId: string | undefined) {
  const importDoc = useImportDocument(notebookId);
  const [isDragging, setIsDragging] = useState(false);

  /* The mutation object changes identity per render; the ref keeps the
     event subscription stable for the lifetime of the notebook. */
  const importRef = useRef(importDoc);
  useEffect(() => {
    importRef.current = importDoc;
  }, [importDoc]);

  useEffect(() => {
    if (!notebookId) return;
    /* getCurrentWebview reads Tauri's injected globals and throws outright when
       they are absent, which takes the whole page down rather than costing it
       drag and drop. The packaged app always injects them, so this only bites
       when the frontend is opened in a plain browser, which is how the layout
       gets checked during development. The same guard is on the chat page's
       event listener. */
    if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) return;

    let unlisten: (() => void) | undefined;
    let cancelled = false;

    getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "enter") {
          setIsDragging(true);
        } else if (event.payload.type === "leave") {
          setIsDragging(false);
        } else if (event.payload.type === "drop") {
          setIsDragging(false);
          for (const path of event.payload.paths.filter(isSupportedFileType)) {
            importRef.current.mutate(path);
          }
        }
      })
      .then((stop) => {
        if (cancelled) stop();
        else unlisten = stop;
      })
      .catch(() => {
        /* The webview drag-drop API is unavailable outside the desktop app
           (for example in unit tests); dropping simply does nothing then. */
      });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [notebookId]);

  /* Open the native file picker and import what is chosen, so a file can be
     added right from the chat box (like attaching in a chat app) without going
     to Documents first. Uses the same import pipeline as drag and drop. */
  const attach = useCallback(async () => {
    if (!notebookId) return;
    const picked = await open({
      multiple: true,
      filters: [{ name: "Documents", extensions: SUPPORTED_FILE_TYPES.map((e) => e.slice(1)) }],
    }).catch(() => null);
    if (!picked) return;
    const paths = Array.isArray(picked) ? picked : [picked];
    for (const path of paths.filter(isSupportedFileType)) {
      importRef.current.mutate(path);
    }
  }, [notebookId]);

  return {
    isDragging,
    isImporting: importDoc.isPending,
    importError: importDoc.isError ? importDoc.error : null,
    attach,
  };
}
