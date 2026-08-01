/*
 * Name: document-detail.tsx
 * Purpose: Shows the chunks/passages extracted from a selected document.
 * Description: Displays heading context, page number, and token count for
 *   each chunk. This is the "source preview" that lets users
 *   verify what the RAG pipeline actually indexed. Useful for
 *   debugging poor search results.
 * Tech Stack: React 19, TanStack Query, Tailwind CSS
 * License: MIT
 * Authors: Amey Thakur (https://github.com/Amey-Thakur)
 *          Archit Konde (https://github.com/Archit-Konde)
 * Date: 2026-07-12
 */

import { useDocumentChunks } from "../hooks/use-documents";
import { DocumentOutline } from "./document-outline";


interface DocumentDetailProps {
  documentId: string;
}


export function DocumentDetail({ documentId }: DocumentDetailProps) {
  const { data: chunks, isLoading } = useDocumentChunks(documentId);

  if (isLoading) {
    return <p className="text-xs text-text-4 font-mono p-4">Loading passages…</p>;
  }

  if (!chunks || chunks.length === 0) {
    return <p className="text-sm text-text-4 p-4">No passages extracted from this document yet.</p>;
  }

  const jump = (chunkId: string) => {
    document.getElementById(`chunk-${chunkId}`)?.scrollIntoView({ behavior: "smooth", block: "start" });
  };

  return (
    <div>
      <DocumentOutline chunks={chunks} onJump={jump} />

      <div className="flex items-center justify-between mb-3">
        <h3 className="text-xs font-mono tracking-widest uppercase text-text-4">
          Passages ({chunks.length})
        </h3>
        <span className="text-xs font-mono text-text-4">
          {chunks.reduce((sum, c) => sum + c.token_count, 0).toLocaleString()} tokens total
        </span>
      </div>

      <div className="space-y-2 max-h-[60vh] overflow-y-auto">
        {chunks.map((chunk, i) => (
          <div key={chunk.id} id={`chunk-${chunk.id}`} className="p-3 border border-border bg-surface scroll-mt-2">
            <div className="flex items-center justify-between mb-2">
              <span className="text-xs font-mono text-text-4">
                #{i + 1}
                {chunk.heading_context && (
                  <span className="ml-2 text-text-3">{chunk.heading_context}</span>
                )}
              </span>
              <div className="flex items-center gap-2">
                {chunk.page_number != null && (
                  <span className="text-xs font-mono text-text-4">p.{chunk.page_number}</span>
                )}
                <span className="text-xs font-mono text-text-4">{chunk.token_count}t</span>
              </div>
            </div>
            <p className="text-sm text-text-2 leading-relaxed whitespace-pre-wrap">
              {chunk.content}
            </p>
          </div>
        ))}
      </div>
    </div>
  );
}
