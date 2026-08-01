/*
 * Name: podcast-page.tsx
 * Purpose: Podcast generation and playback page.
 * Description: The LLM generates a conversation script, and the browser's
 *   SpeechSynthesis API reads it aloud with distinct voices. Uses
 *   Web Speech API for TTS (offline, zero-config, cross-platform).
 *   Two different voices are assigned to Speaker A and Speaker B.
 *   The script is stored in component state (not persisted to
 *   disk). Audio quality depends on OS voices. Can be upgraded to
 *   Piper/Kokoro TTS later for better quality.
 * Tech Stack: React 19, TanStack Query, Web Speech API, Tailwind CSS
 * License: MIT
 * Authors: Amey Thakur (https://github.com/Amey-Thakur)
 *          Archit Konde (https://github.com/Archit-Konde)
 * Date: 2026-07-12
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Link } from "react-router";

import { ModelRequiredNotice } from "@/components/shared/model-required-notice";
import { ROUTES } from "@/lib/constants";
import { useNotebookStore } from "@/stores/notebook-store";
import { tauriInvoke } from "@/services/tauri-client";
import { formatError } from "@/lib/format-error";
import {
  GAP_BETWEEN_SENTENCES_MS,
  GAP_BETWEEN_SPEAKERS_MS,
  pickVoices,
  prosodyFor,
  toSegments,
  voicesAreBasic,
} from "../lib/speech";
import { usePersistentDraft, useRetainedState } from "@/lib/use-persistent-draft";
import { NotebookScope } from "@/components/shared/notebook-scope";
import { SourcePicker } from "@/components/shared/source-picker";
import { JobProgress } from "@/components/shared/job-progress";
import { DownloadButton } from "@/components/shared/download-button";
import { downloadText, toFileName } from "@/lib/download";
import { useJobRun } from "@/features/jobs/use-job-run";


interface PodcastTurn {
  speaker: string;
  text: string;
}

interface PodcastScript {
  title: string;
  turns: PodcastTurn[];
}

type AudioFormat =
  | "discussion"
  | "brief"
  | "debate"
  | "critique"
  | "interview"
  | "lecture"
  | "qanda";

const FORMATS: { id: AudioFormat; label: string; blurb: string }[] = [
  { id: "discussion", label: "Discussion", blurb: "Two hosts explore the material together." },
  { id: "brief", label: "Brief", blurb: "A single narrator, the gist in under a minute." },
  { id: "interview", label: "Interview", blurb: "An interviewer presses an expert on the detail." },
  { id: "lecture", label: "Lecture", blurb: "One voice teaches it in order, from first idea to what to remember." },
  { id: "qanda", label: "Questions", blurb: "The questions this material raises, each answered directly." },
  { id: "debate", label: "Debate", blurb: "Two speakers argue opposing sides." },
  { id: "critique", label: "Critique", blurb: "A careful look at strengths and gaps." },
];


/** Read the script the job produced. Returns null rather than throwing, so a
    malformed payload leaves the previous script on screen instead of taking the
    page down from inside an effect. */
function safeParseScript(raw: string): PodcastScript | null {
  try {
    const parsed = JSON.parse(raw) as PodcastScript;
    return Array.isArray(parsed?.turns) && parsed.turns.length > 0 ? parsed : null;
  } catch {
    return null;
  }
}

/** No playback, nothing highlighted. A module constant so setting it twice is
    a no-op re-render rather than a new object each time. */
const IDLE = { playing: false, turn: -1 } as const;

/** Render a script as a plain transcript, one labelled line per turn.
 *
 *  The synthesized speech is generated live by the browser and cannot be
 *  captured, so the transcript is the artefact worth keeping: it is what
 *  someone quotes, edits, or hands to a real voice. */
function toTranscript(script: PodcastScript): string {
  const blocks = script.turns.map(
    (t) => `${t.speaker === "A" ? "Speaker A" : "Speaker B"}: ${t.text}`,
  );
  const body = blocks.join("\n\n");
  return `# ${script.title}\n\n${body}\n`;
}

/** Write the script to a real audio file using the platform's speech engine.
 *
 *  The webview's SpeechSynthesis cannot be recorded: it exposes no stream and
 *  no buffer, so there is nothing to capture while it plays. The backend drives
 *  the operating system's own engine instead, which is the same voice set, and
 *  writes a file directly. */
async function saveAudio(script: PodcastScript): Promise<void> {
  const [{ save }, extension] = await Promise.all([
    import("@tauri-apps/plugin-dialog"),
    tauriInvoke<string>("audio_export_extension"),
  ]);
  const target = await save({
    defaultPath: toFileName(`notebooklab-${script.title}`, extension),
    filters: [{ name: "Audio", extensions: [extension] }],
  });
  if (!target) return;
  await tauriInvoke<string>("export_audio_file", {
    text: toSpokenText(script),
    file_path: target,
  });
}

/** The words only: speaker labels are for reading, not for listening to. */
function toSpokenText(script: PodcastScript): string {
  return script.turns.map((t) => t.text).join("\n\n");
}

export function PodcastPage() {
  const activeNotebookId = useNotebookStore((s) => s.activeNotebookId);
  /* Preserve the typed topic across navigation and reload. */
  const [topic, setTopic] = usePersistentDraft("notebooklab-draft-podcast-topic");
  const [format, setFormat] = useRetainedState<AudioFormat>(
    "notebooklab-state-audio-format",
    "discussion",
  );
  /* One script per format, not one overall. Generating a debate used to throw
     away the discussion generated a minute earlier, so hearing both meant
     waiting for the model twice. */
  const [scripts, setScripts] = useRetainedState<Partial<Record<AudioFormat, PodcastScript>>>(
    "notebooklab-state-audio-scripts",
    {},
  );
  const script = scripts[format] ?? null;
  /* Playing and which turn is speaking are one fact, not two. Kept together so
     starting, stopping and adopting a new script is a single state write rather
     than a pair that can be seen half-applied mid-render. */
  const [playback, setPlayback] = useState<{ playing: boolean; turn: number }>({
    playing: false,
    turn: -1,
  });
  const { playing: isPlaying, turn: currentTurn } = playback;
  const [voices, setVoices] = useState<SpeechSynthesisVoice[]>([]);
  const synthRef = useRef(window.speechSynthesis);
  /* Set before cancel() so the interrupt event cannot re-enter the play loop and
     resume from the next turn. */
  const cancelledRef = useRef(false);

  /* Load available voices */
  useEffect(() => {
    const loadVoices = () => {
      const available = synthRef.current.getVoices();
      if (available.length > 0) setVoices(available);
    };
    loadVoices();
    speechSynthesis.addEventListener("voiceschanged", loadVoices);
    return () => speechSynthesis.removeEventListener("voiceschanged", loadVoices);
  }, []);

  /* Stop playback when leaving the page; otherwise the chained utterances
     keep reading with no visible way to stop them. */
  useEffect(() => {
    const synth = synthRef.current;
    return () => {
      cancelledRef.current = true;
      synth.cancel();
    };
  }, []);

  const [sources, setSources] = useRetainedState<string[]>("notebooklab-audio-sources", []);
  /* "saving" while the engine writes, then any error it reported. */
  const [audioSave, setAudioSave] = useState<"idle" | "saving" | string>("idle");
  const run = useJobRun("generate_podcast", "notebooklab-job-audio");

  const generate = () =>
    void run.start({
      notebook_id: activeNotebookId,
      topic: topic || null,
      format,
      document_ids: sources,
    });

  /* The finished script arrives as JSON in the job result, because a job result
     is a string. */
  useEffect(() => {
    if (!run.result) return;
    const parsed = safeParseScript(run.result);
    if (!parsed) return;
    /* Storing an identical value would build a new object, change the
       dependency, and re-run this forever. */
    if (scripts[format]?.title === parsed.title && scripts[format]?.turns.length === parsed.turns.length) {
      return;
    }
    setScripts({ ...scripts, [format]: parsed });
  }, [run.result, format, scripts, setScripts]);

  /* A new script stops playback. Without this the previous utterance chain
     keeps reading while the highlights track the new turns, and the new script
     cannot be played until the user hits Stop. Keyed on the script rather than
     on the job, so it covers a script adopted any other way too. */
  useEffect(() => {
    if (!script) return;
    cancelledRef.current = true;
    synthRef.current.cancel();
    /* Stopping the speech engine is the effect; this is the local mirror of the
       state that engine is now in, so it has to be written here rather than
       derived during render. */
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setPlayback(IDLE);
  }, [script]);

  /* Two voices chosen by quality, not by guessing at names. An explicit choice
     overrides it, because taste is not something a score can settle. */
  const [voiceA, setVoiceA] = useRetainedState<string>("notebooklab-audio-voice-a", "");
  const [voiceB, setVoiceB] = useRetainedState<string>("notebooklab-audio-voice-b", "");
  const chosenVoices = useMemo(() => {
    const auto = pickVoices(voices);
    const byName = (name: string) => voices.find((v) => v.name === name);
    return { a: byName(voiceA) ?? auto.a, b: byName(voiceB) ?? auto.b };
  }, [voices, voiceA, voiceB]);

  /* English voices, best first, for the two pickers. */
  const voiceOptions = useMemo(
    () =>
      voices
        .filter((v) => v.lang.toLowerCase().startsWith("en"))
        .slice()
        .sort((x, y) => x.name.localeCompare(y.name)),
    [voices],
  );

  const playScript = useCallback(() => {
    if (!script || isPlaying) return;

    cancelledRef.current = false;
    const segments = toSegments(script.turns);
    if (segments.length === 0) return;
    setPlayback({ playing: true, turn: 0 });

    const speak = (index: number) => {
      /* A cancel() during playback fires the current utterance's end/error,
      which would otherwise re-enter here; bail so Stop and navigation
      truly stop. */
      if (cancelledRef.current) return;
      if (index >= segments.length) {
        setPlayback(IDLE);
        return;
      }

      const segment = segments[index];
      const utterance = new SpeechSynthesisUtterance(segment.text);
      const voice = segment.speaker === "A" ? chosenVoices.a : chosenVoices.b;
      if (voice) utterance.voice = voice;

      /* Vary the delivery a little per sentence. Identical settings for every
      sentence is what makes read-aloud drone; the variation is derived from the
      sentence so the same script always sounds the same. */
      const prosody = prosodyFor(segment.text, index);
      utterance.rate = prosody.rate;
      /* A nudge on top, not a costume: the two voices already sound different,
      so this only reinforces which is which. */
      utterance.pitch = prosody.pitch * (segment.speaker === "A" ? 1.03 : 0.97);

      utterance.onstart = () => setPlayback({ playing: true, turn: segment.turn });

      const next = () => {
        if (cancelledRef.current) return;
        const following = segments[index + 1];
        /* A beat where the speaker changes, a shorter one between sentences.
        Running them together is what made this sound like one person reading
        a transcript rather than two people talking. */
        const gap =
          following && following.speaker !== segment.speaker
            ? GAP_BETWEEN_SPEAKERS_MS
            : GAP_BETWEEN_SENTENCES_MS;
        window.setTimeout(() => speak(index + 1), gap);
      };
      utterance.onend = next;
      utterance.onerror = next;

      synthRef.current.speak(utterance);
    };

    speak(0);
  }, [script, isPlaying, chosenVoices]);

  const stopPlayback = () => {
    cancelledRef.current = true;
    synthRef.current.cancel();
    setPlayback(IDLE);
  };

  if (!activeNotebookId) {
    return (
      <div className="flex flex-col items-center justify-center h-full text-text-3 p-8">
        <p className="text-lg font-display font-bold mb-2">Audio Studio</p>
        <p className="text-sm text-text-4 mb-4">Select a notebook first to record from it.</p>
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
    <div className="p-8 max-w-3xl mx-auto">
      <h1 className="text-2xl font-display font-bold text-text-1 mb-2">Audio Studio</h1>
      <p className="text-sm text-text-3 mb-6">
        Your notebook, read aloud. Choose how it should be told, from a quick brief to a
        full lecture, and it is spoken from your own sources.
      </p>

      <NotebookScope />


      <ModelRequiredNotice action="Audio Studio" />

      {/* Generation form */}
      <div className="border border-border bg-surface-2 p-4 mb-6">
        <h2 className="text-xs font-mono tracking-widest uppercase text-text-4 mb-3">
          Generate audio
        </h2>

        {/* Format picker */}
        <div className="flex flex-wrap gap-2 mb-2" role="group" aria-label="Audio format">
          {FORMATS.map((f) => (
            <button
              key={f.id}
              type="button"
              aria-pressed={format === f.id}
              onClick={() => setFormat(f.id)}
              className={`px-3 py-1.5 text-sm font-mono border transition-colors ${
                format === f.id
                  ? "border-accent-dim text-text-1 bg-surface"
                  : "border-border text-text-3 hover:text-text-1"
              }`}
            >
              {f.label}
              {/* A quiet mark on the formats already generated, so what the
                  notebook holds is visible without clicking through each. */}
              {scripts[f.id] && (
                <span
                  aria-label="already generated"
                  className="inline-block ml-2 w-1.5 h-1.5 rounded-full bg-accent align-middle"
                />
              )}
            </button>
          ))}
        </div>
        <p className="text-xs text-text-4 mb-3">{FORMATS.find((f) => f.id === format)?.blurb}</p>

        <SourcePicker

          notebookId={activeNotebookId}

          value={sources}

          onChange={setSources}

          disabled={run.isRunning}

        />


        <div className="flex gap-2 mb-3">
          <input
            type="text"
            value={topic}
            onChange={(e) => setTopic(e.target.value)}
            placeholder="Topic (optional)"
            aria-label="Podcast topic (optional)"
            className="flex-1 px-3 py-2 text-sm bg-surface border border-border text-text-1
                       placeholder:text-text-4 outline-none focus:border-accent-dim"
          />
          <button
            type="button"
            onClick={generate}
            disabled={run.isRunning}
            className="px-4 py-2 text-sm font-mono bg-primary text-on-primary
                       hover:bg-primary-hover transition-colors disabled:opacity-50"
          >
            {run.isRunning ? "Working..." : "Generate"}
          </button>
        </div>
        {run.job && run.job.status !== "done" && (
          <div className="mt-3">
            <JobProgress job={run.job} onCancel={run.cancel} compact />
          </div>
        )}

        {run.error && (
          <p role="alert" className="text-xs text-error">{run.error}</p>
        )}
      </div>

      {/* Script display + playback */}
      {/* Voice choice. The automatic pick is by quality markers, which cannot
          settle taste, and on a machine with several good voices the pair
          matters more to how human this sounds than any pacing does. */}
      {voiceOptions.length > 1 && (
        <div className="border border-border bg-surface-2 p-4 mb-6">
          <h2 className="text-xs font-mono tracking-widest uppercase text-text-4 mb-3">
            Voices
          </h2>
          <div className="flex flex-col sm:flex-row gap-3">
            {([
              ["A", voiceA, setVoiceA] as const,
              ["B", voiceB, setVoiceB] as const,
            ]).map(([which, value, set]) => (
              <label key={which} className="flex-1 text-xs text-text-3">
                Speaker {which}
                <select
                  value={value}
                  onChange={(e) => set(e.target.value)}
                  className="mt-1 w-full px-2 py-2 text-sm bg-surface border border-border
                             text-text-1 outline-none focus:border-accent-dim"
                >
                  <option value="">Best available</option>
                  {voiceOptions.map((v) => (
                    <option key={v.name} value={v.name}>
                      {v.name}
                    </option>
                  ))}
                </select>
              </label>
            ))}
          </div>

          {voicesAreBasic(voices) && (
            <p className="mt-3 text-xs text-text-4">
              Only basic system voices are installed, which is why this sounds
              robotic. On Windows, Settings then Time &amp; language then Speech
              offers free natural voices; installing one and picking it here is the
              single biggest improvement available.
            </p>
          )}
        </div>
      )}

      {script && (
        <div>
          <div className="flex items-center justify-between mb-4">
            <h2 className="text-xs font-mono tracking-widest uppercase text-text-4">
              {script.title} ({script.turns.length} turns)
            </h2>
            <div className="flex gap-2">
              <DownloadButton
                format={audioSave === "saving" ? "Saving..." : "Audio"}
                what="the audio"
                disabled={audioSave === "saving"}
                onDownload={() => {
                  setAudioSave("saving");
                  saveAudio(script)
                    .then(() => setAudioSave("idle"))
                    .catch((e) => setAudioSave(formatError(e)));
                }}
              />
              <DownloadButton
                format="Transcript"
                what="the audio script"
                onDownload={() =>
                  downloadText(
                    toTranscript(script),
                    toFileName(`notebooklab-${script.title}`, "md"),
                    "text/markdown",
                  )
                }
              />
            </div>

            <div className="flex gap-2">
              {!isPlaying ? (
                <button
                  type="button"
                  onClick={playScript}
                  className="px-3 py-1 text-xs font-mono bg-primary text-on-primary"
                >
                  Play
                </button>
              ) : (
                <button
                  type="button"
                  onClick={stopPlayback}
                  className="px-3 py-1 text-xs font-mono border border-error text-error"
                >
                  Stop
                </button>
              )}
            </div>
            {audioSave !== "idle" && audioSave !== "saving" && (
              <p role="alert" className="text-xs text-error mb-3">{audioSave}</p>
            )}
          </div>

          <div className="space-y-2">
            {script.turns.map((turn, i) => (
              <div
                key={i}
                className={`p-3 border transition-colors ${
                  currentTurn === i
                    ? "border-accent-dim bg-surface-2"
                    : "border-border"
                }`}
              >
                <span className={`text-xs font-mono font-bold mr-2 ${
                  turn.speaker === "A" ? "text-accent" : "text-mark"
                }`}>
                  Speaker {turn.speaker}
                </span>
                <span className="text-sm text-text-2">{turn.text}</span>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
