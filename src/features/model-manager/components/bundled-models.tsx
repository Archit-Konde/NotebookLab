/*
 * Name: bundled-models.tsx
 * Purpose: The bundled server's model catalog: download in a click, fully
 *   offline, no limits.
 * Description: Replaces the old single-model download button with the curated
 *   GGUF catalog. Each entry shows its size, memory needs, what it is good
 *   at, and a hardware-fit badge; the strongest model this computer runs
 *   comfortably carries a "Recommended for this computer" mark, chosen by the
 *   tested recommendGguf logic. Downloads stream live progress (one at a
 *   time, matching the backend guard), verified sizes are shown up front, and
 *   a downloaded model appears in the Local AI Server card's picker ready to
 *   start. Collapsed to one line once at least one model exists, so the page
 *   stays calm while more models remain a click away.
 * Tech Stack: React 19, TanStack Query, Tailwind CSS
 * License: MIT
 * Authors: Amey Thakur (https://github.com/Amey-Thakur)
 *          Archit Konde (https://github.com/Archit-Konde)
 * Date: 2026-07-17
 */

import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { hasTauri, tauriInvoke } from "@/services/tauri-client";
import { QUERY_KEYS } from "@/lib/constants";
import { cn, formatBytes } from "@/lib/utils";
import { formatError } from "@/lib/format-error";
import type { GgufCatalogEntry, ModelFileInfo, SidecarStatus } from "@/types/models";

import { useHardwareProfile } from "../hooks/use-model-management";
import { recommendGguf } from "../data/gguf-recommend";

interface DownloadProgress {
  downloaded: number;
  total: number;
  percent: number;
  model_name: string;
  status: string;
}

interface BundledModelsProps {
  onDownloaded: () => void;
}

export function BundledModels({ onDownloaded }: BundledModelsProps) {
  const queryClient = useQueryClient();
  const hardware = useHardwareProfile();
  const [progress, setProgress] = useState<DownloadProgress | null>(null);
  const [expanded, setExpanded] = useState(false);

  const { data: catalog } = useQuery({
    queryKey: [QUERY_KEYS.GGUF_CATALOG],
    queryFn: () => tauriInvoke<GgufCatalogEntry[]>("list_gguf_catalog"),
    staleTime: Infinity,
  });

  const { data: installed } = useQuery({
    queryKey: [QUERY_KEYS.SIDECAR, "models"],
    queryFn: () => tauriInvoke<ModelFileInfo[]>("list_local_models"),
  });

  /* Follow download progress; refresh the local model list when one lands. */
  useEffect(() => {
    if (!hasTauri()) return;
    const unlisten = listen<DownloadProgress>("model-download-progress", (event) => {
      const payload = event.payload;
      if (payload.status === "complete") {
        setProgress(null);
        queryClient.invalidateQueries({ queryKey: [QUERY_KEYS.SIDECAR] });
        onDownloaded();
        /* Bridge download -> running: if the bundled server is idle, start it
           with the downloaded model so the user lands on a working state rather
           than a downloaded-but-inert model. Best-effort; the Local AI Server
           card and the model notice both still offer a manual Start. */
        void (async () => {
          try {
            if (!(await tauriInvoke<boolean>("sidecar_available"))) return;
            const s = await tauriInvoke<SidecarStatus>("get_sidecar_status");
            if (s.state === "stopped" || s.state === "crashed") {
              await tauriInvoke<SidecarStatus>("start_sidecar", { model_path: null });
              queryClient.invalidateQueries({ queryKey: [QUERY_KEYS.SIDECAR, "status"] });
              queryClient.invalidateQueries({ queryKey: [QUERY_KEYS.ACTIVE_PROVIDER] });
            }
          } catch {
            /* Ignore: a manual Start remains available on the Models page. */
          }
        })();
      } else if (payload.status.startsWith("error")) {
        setProgress(payload);
      } else {
        setProgress(payload);
      }
    });
    return () => {
      unlisten.then((fn) => fn());
    };
    /* onDownloaded is stable enough here; the parent passes invalidations. */
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [queryClient]);

  const download = useMutation({
    mutationFn: (id: string) => tauriInvoke<string>("download_gguf_model", { id }),
  });

  const installedStems = new Set(
    (installed ?? []).map((m) => m.name.toLowerCase()),
  );
  const isInstalled = (entry: GgufCatalogEntry) =>
    installedStems.has(entry.filename.replace(/\.gguf$/i, "").toLowerCase());

  const recommended = recommendGguf(catalog ?? [], hardware.data?.total_ram_gb);
  const anyInstalled = (installed?.length ?? 0) > 0;
  const downloading = progress !== null && !progress.status.startsWith("error");
  const showList = expanded || !anyInstalled;

  return (
    <div className="border border-border bg-surface-2 p-5 mb-8">
      <div className="flex flex-wrap items-center justify-between gap-2 mb-1">
        <h3 className="text-sm font-semibold text-text-1">
          Bundled models
          <span className="ml-2 text-2xs font-mono text-text-4">
            offline · no account · no limits
          </span>
        </h3>
        {anyInstalled && (
          <button
            type="button"
            onClick={() => setExpanded(!expanded)}
            aria-expanded={showList}
            className="px-2.5 py-1 text-2xs font-mono border border-border text-text-3
                       hover:border-accent-dim hover:text-text-1 transition-colors"
          >
            {showList ? "Hide the list" : "Get more models"}
          </button>
        )}
      </div>
      <p className="text-xs text-text-3 mb-3">
        One-time downloads that run entirely on this machine with the built-in server. Nothing you
        write ever leaves your computer, and there is no token bill.
      </p>

      {showList && (
        <div className="space-y-1.5">
          {(catalog ?? []).map((entry) => {
            const installedHere = isInstalled(entry);
            const isDownloadingThis = downloading && progress?.model_name === entry.filename;
            const isRecommended = recommended?.id === entry.id;
            const ram = hardware.data?.total_ram_gb;
            const fitsMin = !ram || entry.min_ram_gb <= ram + 0.75;
            return (
              <div key={entry.id} className={cn("border border-border p-4", !fitsMin && "opacity-70")}>
                <div className="flex flex-wrap items-start justify-between gap-2">
                  <div className="min-w-0">
                    <div className="flex flex-wrap items-center gap-2">
                      <span className="text-sm font-medium text-text-1">
                        {entry.label} <span className="text-text-3">{entry.params}</span>
                      </span>
                      {isRecommended && (
                        <span className="text-2xs font-mono px-1.5 py-0.5 border border-accent-dim text-accent">
                          Recommended for this computer
                        </span>
                      )}
                      {!fitsMin && (
                        <span className="text-2xs font-mono px-1.5 py-0.5 border border-border text-error">
                          Needs {entry.min_ram_gb} GB RAM
                        </span>
                      )}
                    </div>
                    <p className="text-xs text-text-3 mt-1">{entry.use_note}</p>
                    <p className="text-2xs font-mono text-text-4 mt-1">
                      {entry.download_gb.toFixed(1)} GB download · needs {entry.min_ram_gb} GB RAM
                      {entry.recommended_ram_gb > entry.min_ram_gb
                        ? `, best with ${entry.recommended_ram_gb} GB`
                        : ""}
                    </p>
                  </div>
                  <div className="shrink-0">
                    {installedHere ? (
                      <span className="text-2xs font-mono text-mark px-2 py-1">Downloaded</span>
                    ) : (
                      <button
                        type="button"
                        onClick={() => download.mutate(entry.id)}
                        disabled={downloading || download.isPending}
                        className="px-3 py-1.5 text-xs font-mono bg-primary text-on-primary
                                   hover:bg-primary-hover transition-colors disabled:opacity-50"
                      >
                        {isDownloadingThis ? "Downloading..." : "Download"}
                      </button>
                    )}
                  </div>
                </div>

                {isDownloadingThis && (
                  <div className="mt-3">
                    <div className="h-1.5 bg-surface border border-border overflow-hidden">
                      <div
                        className="h-full bg-accent transition-all"
                        style={{ width: `${progress?.percent ?? 0}%` }}
                      />
                    </div>
                    <p className="text-2xs font-mono text-text-4 mt-1" aria-live="polite">
                      {/* A server that sends no content-length leaves the total at
                          zero, which read as "12 MB of 0 B (0%)": a bar frozen at
                          nothing while the download was in fact running. Show the
                          bytes that are known instead of a percentage that is not. */}
                      {progress && progress.total > 0
                        ? `${formatBytes(progress.downloaded)} of ${formatBytes(progress.total)} (${progress.percent.toFixed(0)}%)`
                        : `${formatBytes(progress?.downloaded ?? 0)} downloaded`}
                    </p>
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}

      {progress?.status.startsWith("error") && (
        <p role="alert" className="text-xs text-error mt-2">
          {progress.status.replace(/^error:\s*/, "Download failed: ")}
        </p>
      )}
      {download.isError && (
        <p role="alert" className="text-xs text-error mt-2">
          {formatError(download.error)}
        </p>
      )}
    </div>
  );
}
