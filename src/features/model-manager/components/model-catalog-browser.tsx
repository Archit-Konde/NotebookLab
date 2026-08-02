/*
 * Name: model-catalog-browser.tsx
 * Purpose: Browse the curated model catalog and install with one click.
 * Description: Every catalog entry shows its size, RAM needs, use cases, a
 *   quality rating, and a hardware-fit badge computed from this computer's
 *   detected RAM. Installing streams pull progress from the backend's
 *   "ollama-pull-progress" events into a single progress bar (one pull at a
 *   time, matching the backend guard). A model marked too large asks for
 *   explicit confirmation before installing instead of silently letting a
 *   machine thrash. When a pull finishes the installed list refreshes and the
 *   model can be activated in one click.
 * Tech Stack: React 19, TanStack Query, Tailwind CSS
 * License: MIT
 * Authors: Amey Thakur (https://github.com/Amey-Thakur)
 *          Archit Konde (https://github.com/Archit-Konde)
 * Date: 2026-07-17
 */

import { useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useMutation, useQueryClient } from "@tanstack/react-query";

import { hasTauri, tauriInvoke } from "@/services/tauri-client";
import { cn, formatBytes } from "@/lib/utils";
import { formatError } from "@/lib/format-error";
import { QUERY_KEYS } from "@/lib/constants";
import type { OllamaPullFinished, OllamaPullProgress } from "@/types/models";

import {
  classifyFit,
  FIT_LABEL,
  MODEL_CATALOG,
  USE_CASES,
  type CatalogModel,
  type UseCase,
} from "../data/model-catalog";
import { useActivateOllamaModel, useHardwareProfile } from "../hooks/use-model-management";

interface ModelCatalogBrowserProps {
  installedTags: Set<string>;
}

export function ModelCatalogBrowser({ installedTags }: ModelCatalogBrowserProps) {
  const queryClient = useQueryClient();
  const hardware = useHardwareProfile();
  const activate = useActivateOllamaModel();

  const [filter, setFilter] = useState<UseCase | null>(null);
  const [search, setSearch] = useState("");
  const [pulling, setPulling] = useState<string | null>(null);
  const [progress, setProgress] = useState<OllamaPullProgress | null>(null);
  const [pullError, setPullError] = useState<string | null>(null);
  const [confirmLarge, setConfirmLarge] = useState<CatalogModel | null>(null);

  /* A download keeps running if the user navigates away; on mount, ask the
     backend whether one is in flight so the progress bar picks it back up
     instead of misreporting an idle catalog. */
  useEffect(() => {
    let cancelled = false;
    tauriInvoke<string | null>("ollama_pull_state")
      .then((model) => {
        if (!cancelled && model) setPulling(model);
      })
      .catch(() => {
        /* Backend unavailable (dev preview): nothing to restore. */
      });
    return () => {
      cancelled = true;
    };
  }, []);

  /* Follow the backend's pull events for the lifetime of the browser. */
  useEffect(() => {
    if (!hasTauri()) return;
    const unlistenProgress = listen<OllamaPullProgress>("ollama-pull-progress", (event) => {
      setProgress(event.payload);
    });
    const unlistenFinished = listen<OllamaPullFinished>("ollama-pull-finished", (event) => {
      setPulling(null);
      setProgress(null);
      if (event.payload.ok) {
        setPullError(null);
        queryClient.invalidateQueries({ queryKey: [QUERY_KEYS.OLLAMA_MODELS] });
      } else {
        setPullError(event.payload.error ?? "Download failed");
      }
    });
    return () => {
      unlistenProgress.then((fn) => fn());
      unlistenFinished.then((fn) => fn());
    };
  }, [queryClient]);

  const pull = useMutation({
    mutationFn: (model: string) => tauriInvoke<void>("ollama_pull_model", { model }),
    onMutate: (model) => {
      setPulling(model);
      setPullError(null);
      setProgress(null);
    },
    onError: (error) => {
      setPulling(null);
      setPullError(formatError(error));
    },
  });

  const startInstall = (model: CatalogModel) => {
    const fit = classifyFit(model, hardware.data?.total_ram_gb);
    if (fit === "too-large") {
      setConfirmLarge(model);
      return;
    }
    pull.mutate(model.tag);
  };

  const visible = useMemo(() => {
    const term = search.trim().toLowerCase();
    return MODEL_CATALOG.filter((m) => {
      if (filter && !m.useCases.includes(filter)) return false;
      if (!term) return true;
      return (
        m.tag.toLowerCase().includes(term) ||
        m.family.toLowerCase().includes(term) ||
        m.label.toLowerCase().includes(term)
      );
    });
  }, [filter, search]);

  return (
    <div>
      {/* Filters */}
      <div className="flex flex-wrap items-center gap-2 mb-4">
        <input
          type="search"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="Search models..."
          aria-label="Search the model catalog"
          className="px-3 py-1.5 text-xs bg-surface border border-border text-text-1
                     placeholder:text-text-4 outline-none focus:border-accent-dim w-44"
        />
        {USE_CASES.map((useCase) => (
          <button
            key={useCase}
            type="button"
            aria-pressed={filter === useCase}
            onClick={() => setFilter(filter === useCase ? null : useCase)}
            className={cn(
              "px-2.5 py-1 text-2xs font-mono border transition-colors",
              filter === useCase
                ? "border-accent-dim bg-surface-2 text-text-1"
                : "border-border text-text-3 hover:text-text-1",
            )}
          >
            {useCase}
          </button>
        ))}
      </div>

      {pullError && (
        <p role="alert" className="text-xs text-error mb-3">
          {pullError}
        </p>
      )}

      {/* Catalog list */}
      <div className="space-y-1.5">
        {visible.map((model) => {
          const fit = classifyFit(model, hardware.data?.total_ram_gb);
          const installed = installedTags.has(model.tag);
          const isPulling = pulling === model.tag;
          return (
            <div
              key={model.tag}
              className={cn(
                "border p-3",
                fit === "too-large" ? "border-border opacity-70" : "border-border",
              )}
            >
              <div className="flex flex-wrap items-start justify-between gap-2">
                <div className="min-w-0">
                  <div className="flex flex-wrap items-center gap-2">
                    <span className="text-sm font-medium text-text-1">
                      {model.label} <span className="text-text-3">{model.params}</span>
                    </span>
                    <span className="text-2xs font-mono text-text-4">{model.tag}</span>
                    <Rating value={model.rating} />
                    {fit !== "unknown" && (
                      <span
                        className={cn(
                          "text-2xs font-mono px-1.5 py-0.5 border border-border",
                          fit === "fits" && "text-mark",
                          fit === "tight" && "text-amber-500",
                          fit === "too-large" && "text-error",
                        )}
                      >
                        {FIT_LABEL[fit]}
                      </span>
                    )}
                  </div>
                  <p className="text-xs text-text-3 mt-1">{model.blurb}</p>
                  <p className="text-2xs font-mono text-text-4 mt-1">
                    ~{model.downloadGb.toFixed(1)} GB download · needs {model.minRamGb} GB RAM
                    {model.recommendedRamGb > model.minRamGb
                      ? `, best with ${model.recommendedRamGb} GB`
                      : ""}
                    {" · "}
                    {model.useCases.join(", ")}
                  </p>
                </div>
                <div className="shrink-0">
                  {installed ? (
                    <button
                      type="button"
                      onClick={() => activate.mutate(model.tag)}
                      disabled={activate.isPending}
                      className="px-3 py-1.5 text-xs font-mono border border-border text-text-2
                                 hover:border-accent-dim hover:text-text-1 transition-colors disabled:opacity-50"
                    >
                      Use this model
                    </button>
                  ) : (
                    <button
                      type="button"
                      onClick={() => startInstall(model)}
                      disabled={pulling !== null}
                      className="px-3 py-1.5 text-xs font-mono bg-primary text-on-primary
                                 hover:bg-primary-hover transition-colors disabled:opacity-50"
                    >
                      {isPulling ? "Installing..." : "Install"}
                    </button>
                  )}
                </div>
              </div>

              {/* Live pull progress for this model */}
              {isPulling && (
                <div className="mt-3">
                  <div className="h-1.5 bg-surface-2 border border-border overflow-hidden">
                    <div
                      className="h-full bg-accent transition-all"
                      style={{ width: `${progress?.percent ?? 0}%` }}
                    />
                  </div>
                  <p className="text-2xs font-mono text-text-4 mt-1" aria-live="polite">
                    {progress
                      ? `${progress.status}${
                          progress.total > 0
                            ? ` · ${formatBytes(progress.completed)} of ${formatBytes(progress.total)} (${progress.percent.toFixed(0)}%)`
                            : ""
                        }`
                      : "Contacting Ollama..."}
                  </p>
                </div>
              )}
            </div>
          );
        })}
        {visible.length === 0 && (
          <p className="text-sm text-text-4 py-4">No models match. Clear the search or filter.</p>
        )}
      </div>

      {/* Too-large confirmation */}
      {confirmLarge && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/50"
          role="dialog"
          aria-modal="true"
          aria-label="Model may be too large"
        >
          <div className="w-[380px] max-w-[calc(100vw-32px)] border border-border bg-surface p-5 shadow-xl">
            <h3 className="text-base font-display font-bold text-text-1 mb-2">
              This model may be too large
            </h3>
            <p className="text-sm text-text-2 leading-relaxed mb-4">
              {confirmLarge.label} {confirmLarge.params} wants at least {confirmLarge.minRamGb} GB
              of RAM
              {hardware.data
                ? `, and this computer has ${hardware.data.total_ram_gb.toFixed(0)} GB`
                : ""}
              . It may run very slowly or fail to load. A smaller model, or a cloud provider,
              will feel much better.
            </p>
            <div className="flex justify-end gap-2">
              <button
                type="button"
                onClick={() => setConfirmLarge(null)}
                className="px-3 py-1.5 text-xs font-mono border border-border text-text-3
                           hover:text-text-1 transition-colors"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={() => {
                  pull.mutate(confirmLarge.tag);
                  setConfirmLarge(null);
                }}
                className="px-3 py-1.5 text-xs font-mono bg-primary text-on-primary
                           hover:bg-primary-hover transition-colors"
              >
                Install anyway
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

/** Five-dot quality-for-size rating. */
function Rating({ value }: { value: number }) {
  return (
    <span className="flex items-center gap-0.5" aria-label={`Rated ${value} of 5`}>
      {[1, 2, 3, 4, 5].map((dot) => (
        <span
          key={dot}
          aria-hidden="true"
          className={cn(
            "inline-block w-1.5 h-1.5 rounded-full",
            dot <= value ? "bg-accent" : "bg-border",
          )}
        />
      ))}
    </span>
  );
}
