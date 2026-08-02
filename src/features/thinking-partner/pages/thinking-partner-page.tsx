/*
 * Name: thinking-partner-page.tsx
 * Purpose: Thinking Partner page.
 * Description: Two modes. Idea Space maps how the claims, evidence, tensions
 *   and open questions in the sources stand against each other, drawn
 *   in three dimensions; Socratic returns probing questions to push
 *   the user's own thinking. Requires an active notebook with
 *   documents.
 *
 *   Idea Space exists because this page used to render the Studio's
 *   mind map from the Studio's prompt, so both features produced the
 *   same picture and one of them had no reason to exist. A hierarchy
 *   answers what the sources contain; this answers how they disagree.
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
import { SourcePicker } from "@/components/shared/source-picker";
import { JobProgress } from "@/components/shared/job-progress";
import { DownloadButton } from "@/components/shared/download-button";
import { downloadText, toFileName } from "@/lib/download";
import { ROUTES } from "@/lib/constants";
import { useNotebookStore } from "@/stores/notebook-store";
import { useRetainedState } from "@/lib/use-persistent-draft";
import { useJobRun } from "@/features/jobs/use-job-run";
import { safeJson } from "@/features/studio/api/studio-api";
import { IdeaSpaceView, type IdeaSpace } from "../components/idea-space-view";
import { prepareSpace } from "../lib/prepare-space";


type Mode = "ideas" | "socratic";


export function ThinkingPartnerPage() {
  const activeNotebookId = useNotebookStore((s) => s.activeNotebookId);
  const [mode, setMode] = useRetainedState<Mode>("notebooklab-state-think-mode", "ideas");
  const [input, setInput] = useState("");
  const [result, setResult] = useRetainedState<string | null>(
    "notebooklab-state-think-result",
    null,
  );

  /* One run per mode, each remembering its own job, so switching between Idea
     Space and Socratic does not detach from a generation that is still going. */
  const ideas = useJobRun("generate_idea_space", "notebooklab-job-ideaspace");
  const socratic = useJobRun("generate_socratic_questions", "notebooklab-job-socratic");
  const run = mode === "ideas" ? ideas : socratic;

  const [sources, setSources] = useRetainedState<string[]>(
    "notebooklab-think-sources",
    [],
  );

  /* A finished job is the source of truth; the retained copy is what survives
     the job history being cleared. */
  useEffect(() => {
    if (run.result) setResult(run.result);
  }, [run.result, setResult]);

  const submit = () => {
    const text = input.trim();
    if (!text) return;
    setResult(null);
    void run.start(
      mode === "ideas"
        ? { notebook_id: activeNotebookId, topic: text, document_ids: sources }
        : { notebook_id: activeNotebookId, thinking: text, document_ids: sources },
    );
  };

  if (!activeNotebookId) {
    return (
      <div className="flex flex-col items-center justify-center h-full text-text-3 p-8">
        <p className="text-lg mb-2">No notebook selected</p>
        <p className="text-sm text-text-4 mb-4">Open a notebook first to think with your sources.</p>
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
        <h1 className="text-2xl font-display font-bold text-text-1 mb-1">Think</h1>
        <NotebookScope />

        <ModelRequiredNotice action="The thinking partner" />

        {/* Mode toggle */}
        <div className="flex gap-1 mb-4" role="group" aria-label="Thinking mode">
          {([["ideas", "Idea Space"], ["socratic", "Socratic"]] as const).map(([m, label]) => (
            <button
              key={m}
              type="button"
              aria-pressed={mode === m}
              onClick={() => { setMode(m); setResult(null); }}
              className={`px-4 py-2 text-sm font-mono border transition-colors ${
                mode === m
                  ? "border-accent-dim text-text-1 bg-surface-2"
                  : "border-border text-text-3 hover:text-text-1"
              }`}
            >
              {label}
            </button>
          ))}
        </div>

        <SourcePicker
          notebookId={activeNotebookId}
          value={sources}
          onChange={setSources}
          disabled={run.isRunning}
        />

        {/* Input */}
        <div className="flex gap-2">
          <input
            type="text"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && submit()}
            placeholder={mode === "ideas" ? "What are you trying to work out?" : "Describe your current thinking..."}
            aria-label={mode === "ideas" ? "What are you trying to work out" : "Describe your current thinking"}
            className="flex-1 px-4 py-3 text-sm bg-surface border border-border text-text-1
                       placeholder:text-text-4 outline-none focus:border-accent-dim"
          />
          <button
            type="button"
            onClick={submit}
            disabled={run.isRunning || !input.trim()}
            className="px-4 py-3 text-sm font-mono bg-primary text-on-primary disabled:opacity-50"
          >
            {mode === "ideas" ? "Map it" : "Ask"}
          </button>
        </div>
      </div>

      {/* Results */}
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

        {result && (
          <div className="p-6 border border-border bg-surface">
            <div className="flex items-center gap-3 mb-4">
              <h2 className="text-xs font-mono tracking-widest uppercase text-text-4">
                {mode === "ideas" ? "Idea Space" : "Socratic Questions"}
              </h2>
              <div className="ml-auto">
                <DownloadButton
                  format={mode === "ideas" ? "JSON" : "Markdown"}
                  what={mode === "ideas" ? "the idea space" : "the questions"}
                  onDownload={() =>
                    downloadText(
                      result,
                      toFileName(
                        mode === "ideas" ? "notebooklab-idea-space" : "notebooklab-socratic",
                        mode === "ideas" ? "json" : "md",
                      ),
                      mode === "ideas" ? "application/json" : "text/markdown",
                    )
                  }
                />
              </div>
            </div>
            {mode === "ideas" ? (
              <IdeaSpaceResult text={result} />
            ) : (
              <pre className="text-sm font-body text-text-2 whitespace-pre-wrap leading-relaxed">
                {result}
              </pre>
            )}
          </div>
        )}

        {!result && !run.job && run.ready && (
          <div className="flex items-center justify-center h-full text-text-4">
            <p className="text-sm">
              {mode === "ideas"
                ? "Name what you are working out. This maps the claims, the evidence, the tensions between them, and what the sources leave open."
                : "Describe what you're thinking about and get probing questions."}
            </p>
          </div>
        )}
      </div>
    </div>
  );
}

/* Parse the generated space and draw it, falling back to a plain message if the
   model's reply was not usable. Parsing is in safeJson so no try/catch sits in
   the render path. */
function IdeaSpaceResult({ text }: { text: string }) {
  const parsed = safeJson<IdeaSpace>(text);
  if ("error" in parsed) {
    return (
      <p role="alert" className="text-sm text-error">
        {parsed.error} Generate it again.
      </p>
    );
  }
  /* Clean before drawing. A duplicate id, a self-edge or an edge naming an id
     the model never defined are all things it emits, and all of them become
     silent drawing faults rather than errors. */
  const space = prepareSpace(parsed.data);
  if (space.nodes.length === 0) {
    return (
      <p role="alert" className="text-sm text-error">
        The map came back empty. Try naming the question more concretely.
      </p>
    );
  }
  return (
    <>
      <IdeaSpaceView data={space} />
      {space.repaired && (
        <p className="mt-2 text-xs text-text-4">
          Part of the reply could not be used and was left out.
        </p>
      )}
    </>
  );
}
