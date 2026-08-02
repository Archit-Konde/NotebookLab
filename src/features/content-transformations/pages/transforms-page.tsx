/*
 * Name: transforms-page.tsx
 * Purpose: Content transformations page.
 * Description: Apply AI-powered transformations (summarize, extract key
 *   points, custom prompts) to imported documents. Requires an
 *   active notebook with processed documents. Results are
 *   displayed inline and can be copied.
 * Tech Stack: React 19, TanStack Query, Tailwind CSS
 * License: MIT
 * Authors: Amey Thakur (https://github.com/Amey-Thakur)
 *          Archit Konde (https://github.com/Archit-Konde)
 * Date: 2026-07-12
 */

import { useEffect, useState } from "react";
import { Link } from "react-router";

import { ModelRequiredNotice } from "@/components/shared/model-required-notice";
import { NotebookScope } from "@/components/shared/notebook-scope";
import { JobProgress } from "@/components/shared/job-progress";
import { DownloadButton } from "@/components/shared/download-button";
import { downloadText, toFileName } from "@/lib/download";
import { useJobRun } from "@/features/jobs/use-job-run";
import { ROUTES } from "@/lib/constants";
import { usePersistentDraft, useRetainedState } from "@/lib/use-persistent-draft";
import { useNotebookStore } from "@/stores/notebook-store";
import { useDocuments } from "@/features/documents/hooks/use-documents";


type TransformType = "summarize" | "extractkeypoints" | "custom";

const TRANSFORM_LABELS: Record<TransformType, string> = {
  summarize: "Summarize",
  extractkeypoints: "Extract Key Points",
  custom: "Custom Prompt",
};


export function TransformsPage() {
  const activeNotebookId = useNotebookStore((s) => s.activeNotebookId);
  const [selectedDoc, setSelectedDoc] = useState<string>("");
  const [transformType, setTransformType] = useRetainedState<TransformType>(
    "notebooklab-state-transform-type",
    "summarize",
  );
  /* Preserve the typed custom instruction across navigation and reload. */
  const [customPrompt, setCustomPrompt] = usePersistentDraft("notebooklab-draft-transform-custom");
  const [result, setResult] = useRetainedState<string | null>(
    "notebooklab-state-transform-result",
    null,
  );
  const [copied, setCopied] = useState(false);

  const { data: documents } = useDocuments(activeNotebookId ?? undefined);

  const processedDocs = documents?.filter((d) => d.status === "processed") || [];

  const run = useJobRun("transform_document", "notebooklab-job-transform");

  /* Which document and which transform the running job is for. A result can
     arrive after the user has changed either, and showing it against the new
     selection would present a summary of one document as if it were another. */
  const [ranFor, setRanFor] = useRetainedState<{ doc: string; type: TransformType } | null>(
    "notebooklab-state-transform-ran-for",
    null,
  );

  useEffect(() => {
    if (run.result) setResult(run.result);
  }, [run.result, setResult]);

  const resultMatchesSelection =
    !!ranFor && ranFor.doc === selectedDoc && ranFor.type === transformType;

  const transform = () => {
    setRanFor({ doc: selectedDoc, type: transformType });
    void run.start({
      document_id: selectedDoc,
      notebook_id: activeNotebookId,
      transform_type: transformType,
      custom_prompt: transformType === "custom" ? customPrompt : null,
    });
  };

  if (!activeNotebookId) {
    return (
      <div className="flex flex-col items-center justify-center h-full text-text-3 p-8">
        <p className="text-lg mb-2">No notebook selected</p>
        <p className="text-sm text-text-4 mb-4">Open a notebook first to transform documents.</p>
        <Link
          to={ROUTES.NOTEBOOKS}
          className="px-4 py-2 text-sm font-mono border border-border text-text-2 hover:border-accent-dim transition-colors"
        >
          Go to Notebooks
        </Link>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      <div className="px-8 pt-6 pb-4">
        <h1 className="text-2xl font-display font-bold text-text-1 mb-1">Transform</h1>
        <NotebookScope />

        <ModelRequiredNotice action="Transforms" />

        {/* Document selector */}
        <div className="mb-4">
          <label htmlFor="transform-document" className="block text-xs font-mono text-text-4 mb-1">
            Document
          </label>
          <select
            id="transform-document"
            value={selectedDoc}
            onChange={(e) => { setSelectedDoc(e.target.value); setResult(null); }}
            className="w-full px-3 py-2 text-sm bg-surface border border-border text-text-1
                       outline-none focus:border-accent-dim"
          >
            <option value="">Select a document...</option>
            {processedDocs.map((doc) => (
              <option key={doc.id} value={doc.id}>{doc.title} (.{doc.file_type})</option>
            ))}
          </select>
        </div>

        {/* Transform type */}
        <div className="flex gap-1 mb-4" role="group" aria-label="Transformation type">
          {(Object.entries(TRANSFORM_LABELS) as [TransformType, string][]).map(([type, label]) => (
            <button
              key={type}
              type="button"
              aria-pressed={transformType === type}
              onClick={() => { setTransformType(type); setResult(null); }}
              className={`px-4 py-2 text-sm font-mono border transition-colors ${
                transformType === type
                  ? "border-accent-dim text-text-1 bg-surface-2"
                  : "border-border text-text-3 hover:text-text-1"
              }`}
            >
              {label}
            </button>
          ))}
        </div>

        {/* Custom prompt input */}
        {transformType === "custom" && (
          <div className="mb-4">
            <label htmlFor="transform-custom-prompt" className="block text-xs font-mono text-text-4 mb-1">
              Custom instruction
            </label>
            <input
              id="transform-custom-prompt"
              type="text"
              value={customPrompt}
              onChange={(e) => setCustomPrompt(e.target.value)}
              placeholder="e.g., Extract all statistics and data points..."
              className="w-full px-3 py-2 text-sm bg-surface border border-border text-text-1
                         placeholder:text-text-4 outline-none focus:border-accent-dim"
            />
          </div>
        )}

        <button
          type="button"
          onClick={transform}
          disabled={!selectedDoc || run.isRunning || (transformType === "custom" && !customPrompt.trim())}
          className="px-4 py-2 text-sm font-mono bg-primary text-on-primary disabled:opacity-50"
        >
          {run.isRunning ? "Working..." : "Transform"}
        </button>
      </div>

      {/* Result */}
      <div className="flex-1 overflow-auto px-8 py-4">
        {run.job && run.job.status !== "done" && (
          <div className="mb-4">
            <JobProgress job={run.job} onCancel={run.cancel} />
          </div>
        )}

        {run.error && (
          <div role="alert" className="p-3 border border-error text-xs text-error">
            {run.error}
          </div>
        )}

        {result && resultMatchesSelection && (
          <div className="p-6 border border-border bg-surface">
            <div className="flex items-center justify-between mb-4">
              <h2 className="text-xs font-mono tracking-widest uppercase text-text-4">
                {TRANSFORM_LABELS[transformType]} Result
              </h2>
              <button
                type="button"
                onClick={() => {
                  navigator.clipboard.writeText(result).then(
                    () => {
                      setCopied(true);
                      setTimeout(() => setCopied(false), 1500);
                    },
                    () => setCopied(false),
                  );
                }}
                className="text-xs font-mono text-text-3 hover:text-text-1"
              >
                {copied ? "Copied" : "Copy"}
              </button>
              <DownloadButton
                format="Markdown"
                what="the result"
                onDownload={() =>
                  downloadText(
                    result,
                    toFileName(`notebooklab-${transformType}`, "md"),
                    "text/markdown",
                  )
                }
              />
            </div>
            <pre className="text-sm font-body text-text-2 whitespace-pre-wrap leading-relaxed">
              {result}
            </pre>
          </div>
        )}

        {(!result || !resultMatchesSelection) && !run.isRunning && run.ready && (
          <div className="flex items-center justify-center h-full text-text-4">
            <p className="text-sm">Select a document and choose a transformation.</p>
          </div>
        )}
      </div>
    </div>
  );
}
