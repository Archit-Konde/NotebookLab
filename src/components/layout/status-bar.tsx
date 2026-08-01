/*
 * Name: status-bar.tsx
 * Purpose: Bottom status bar with a live activity indicator, running
 *   generations, session token usage, and the indexed passage count.
 * Description: The left dot is a real activity signal: amber when no model is
 *   loaded, green when a provider is ready, and a pulsing accent (with a ping
 *   ring) whenever the app is actually working, i.e. any chat, import, or
 *   generation is in flight, or a model is downloading, with the label
 *   narrating the state for screen readers. The state itself is derived by the
 *   pure deriveStatus helper. Animation stands down under reduced motion.
 *   Polls every 10 seconds. Which notebook is active is context rather than
 *   status, so it lives in the header; see notebook-context.tsx.
 * Tech Stack: React 19, TanStack Query, Tailwind CSS
 * License: MIT
 * Authors: Amey Thakur (https://github.com/Amey-Thakur)
 *          Archit Konde (https://github.com/Archit-Konde)
 * Date: 2026-07-14
 */

import { useEffect, useState } from "react";
import { useQuery, useIsMutating } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";

import { tauriInvoke } from "@/services/tauri-client";
import { QUERY_KEYS } from "@/lib/constants";
import { cn } from "@/lib/utils";
import { countRunning, selectJobs, useJobStore } from "@/stores/job-store";
import { deriveStatus } from "./status-state";
import { UsageChip } from "./usage-chip";


interface DownloadProgress {
  percent: number;
  status: string;
}

export function StatusBar() {
  const [updateVersion, setUpdateVersion] = useState<string | null>(null);
  const [downloadPercent, setDownloadPercent] = useState<number | null>(null);

  /* Count in-flight user actions (chat, import, generation, download). Anything
     started with a mutation lights the working state. */
  const workCount = useIsMutating();
  const jobs = useJobStore((s) => s.jobs);
  const running = countRunning(jobs);
  /* Naming them makes the count answerable: "which two?" is the immediate
     question, and the alternative is opening every feature to find out. */
  const runningLabel = selectJobs(jobs)
    .filter((j) => j.status === "running")
    .map((j) => `${j.label} (${j.percent}%)`)
    .join(", ");

  /* The backend downloads updates in the background and announces when one
     is staged; restarting swaps it in. */
  useEffect(() => {
    const unlisten = listen<string>("update-ready", (event) => {
      setUpdateVersion(event.payload);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  /* Follow model-download progress so the indicator can show it from anywhere,
     not just the Models page. Clear it once the download settles. */
  useEffect(() => {
    const unlisten = listen<DownloadProgress>("model-download-progress", (event) => {
      const { percent, status } = event.payload;
      setDownloadPercent(status === "downloading" && percent < 100 ? percent : null);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  const { data: activeProvider } = useQuery({
    queryKey: [QUERY_KEYS.ACTIVE_PROVIDER],
    queryFn: () => tauriInvoke<string | null>("get_active_provider_name"),
    refetchInterval: 10000,
  });

  const { data: chunkCount } = useQuery({
    queryKey: [QUERY_KEYS.CHUNK_COUNT],
    queryFn: () => tauriInvoke<number>("get_chunk_count"),
    refetchInterval: 10000,
  });

  const chunks = chunkCount ?? 0;

  const status = deriveStatus({
    hasProvider: !!activeProvider,
    workCount,
    providerName: activeProvider ?? null,
    downloadPercent,
  });

  return (
    <footer
      className="flex items-center justify-between h-6 px-4 border-t border-border bg-bg"
      role="status"
      aria-label="Application status"
    >
      <div className="flex items-center gap-4 min-w-0">
        <span className="flex items-center gap-2 shrink-0" data-tour="status">
          <span className="relative flex h-2 w-2 items-center justify-center" aria-hidden="true">
            {status.animate && (
              <span
                className={cn(
                  "absolute inline-flex h-full w-full rounded-full opacity-60 animate-ping",
                  "motion-reduce:hidden",
                  status.dotClass,
                )}
              />
            )}
            <span
              className={cn(
                "relative inline-block h-2 w-2 rounded-full transition-colors",
                status.dotClass,
                status.animate ? "animate-pulse motion-reduce:animate-none" : "",
              )}
            />
          </span>
          <span className={cn("font-mono text-2xs transition-colors", status.textClass)}>
            {status.label}
          </span>
        </span>

        {/* Generations keep running wherever the user goes, so the count has to
            be visible from every page rather than only on the one that started
            the work. */}
        {running > 0 && (
          <span className="font-mono text-2xs text-accent shrink-0" title={runningLabel}>
            {running} {running === 1 ? "generation" : "generations"} running
          </span>
        )}
      </div>

      <span className="flex items-center gap-4 shrink-0">
        {updateVersion && (
          <button
            type="button"
            onClick={() => tauriInvoke("restart_app")}
            className="font-mono text-2xs text-accent hover:underline"
          >
            v{updateVersion} ready, restart to update
          </button>
        )}
        <UsageChip />
        <span className={`font-mono text-2xs ${chunks > 0 ? "text-text-3" : "text-text-4"}`}>
          {chunks} {chunks === 1 ? "passage" : "passages"} indexed
        </span>
      </span>
    </footer>
  );
}
