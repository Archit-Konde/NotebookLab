/*
 * Name: help-page.tsx
 * Purpose: In-app documentation and guide.
 * Description: A single, readable guide to everything in NotebookLab, so the
 *   documentation travels with the app and works offline. Content is grouped
 *   into sections with an on-this-page list that jumps to each one. Written as
 *   plain React so there is no markup injection and it stays on-brand.
 * Tech Stack: React 19, Tailwind CSS
 * License: MIT
 * Authors: Amey Thakur (https://github.com/Amey-Thakur)
 *          Archit Konde (https://github.com/Archit-Konde)
 * Date: 2026-07-13
 */

import type { ReactNode } from "react";

interface Section {
  id: string;
  title: string;
  body: ReactNode;
}

const SECTIONS: Section[] = [
  {
    id: "welcome",
    title: "Welcome",
    body: (
      <>
        <P>
          NotebookLab is an offline-first workspace for reading, questioning, and thinking with your
          own documents. It blends a source-grounded AI assistant with a connected notes editor and a
          spatial canvas, all in one place.
        </P>
        <Callout>
          Everything runs on your machine. Your documents, notes, and answers never leave your
          computer unless you choose a cloud AI provider yourself.
        </Callout>
      </>
    ),
  },
  {
    id: "getting-started",
    title: "Getting started",
    body: (
      <List>
        <li>
          <B>Create a notebook</B> on the Notebooks page. A notebook holds related sources, notes, and
          a canvas.
        </li>
        <li>
          <B>Import a few sources</B> into it (see below). They are indexed automatically for search
          and chat.
        </li>
        <li>
          <B>Ask a question</B> in Chat, or generate a study aid in the Studio. Every answer points
          back to the sources it came from.
        </li>
      </List>
    ),
  },
  {
    id: "sources",
    title: "Bringing in sources",
    body: (
      <>
        <P>
          On a notebook, use Import to bring in files. Supported formats: PDF, Word (<Code>.docx</Code>
          ), plain text, Markdown, and images (<Code>.png</Code>, <Code>.jpg</Code>, <Code>.jpeg</Code>,{" "}
          <Code>.tiff</Code>, <Code>.tif</Code>, <Code>.webp</Code>, <Code>.bmp</Code>).
        </P>
        <List>
          <li>
            <B>Images and scans</B> are read with offline OCR, so a photo or scan of printed text
            becomes searchable content like any other source. OCR works best on clear, printed text.
          </li>
          <li>
            <B>Open with NotebookLab.</B> Right-click a PDF, Word, text, or Markdown file in your
            file manager and choose NotebookLab: the file lands in your current notebook, already
            indexed. If the app is running, the same window picks it up.
          </li>
          <li>You can also drag files straight onto the window, or onto Chat.</li>
          <li>Files are capped at 50 MB each, and a file already in the notebook is not imported twice.</li>
          <li>Legacy binary Word (<Code>.doc</Code>) is not supported; save it as <Code>.docx</Code> first.</li>
        </List>
      </>
    ),
  },
  {
    id: "notes",
    title: "Notes and links",
    body: (
      <P>
        The editor is Markdown with a clean writing surface. Link notes with <Code>[[wiki-links]]</Code>;
        the target note gets a backlink automatically, and the Connections page draws the whole web.
        Notes autosave as you write.
      </P>
    ),
  },
  {
    id: "search",
    title: "Search",
    body: (
      <P>
        Search blends keyword ranking with semantic similarity. Keyword search always works; when your
        AI provider supports embeddings, results also match on meaning, not just wording. Press{" "}
        <Kbd>Ctrl</Kbd>/<Kbd>Cmd</Kbd> <Kbd>K</Kbd> anywhere to open the launcher and jump to a page,
        notebook, or action.
      </P>
    ),
  },
  {
    id: "chat",
    title: "Chat with your sources",
    body: (
      <P>
        Chat answers from the documents in the active notebook. Every answer lists its sources with the
        document, heading, and page, so you can check the claim rather than take it on faith.
      </P>
    ),
  },
  {
    id: "studio",
    title: "Studio",
    body: (
      <>
        <P>
          The Studio turns a notebook's sources into study aids and write-ups, each grounded in your own
          documents and shown in its own view:
        </P>
        <List>
          <li>
            <B>Study guide</B>, <B>flashcards</B>, <B>quiz</B>, and a visual <B>mind map</B>.
          </li>
          <li>
            <B>Timeline</B>, <B>slide deck</B>, and <B>data table</B> for structured material.
          </li>
          <li>
            <B>Briefing doc</B> and <B>blog post</B> as ready-to-read prose.
          </li>
        </List>
        <P>
          Leave the focus box blank to cover the whole notebook, or type a focus to narrow what it
          draws from.
        </P>
      </>
    ),
  },
  {
    id: "thinking-partner",
    title: "Think",
    body: (
      <P>
        Build an idea space from your research: a moving three-dimensional map of the claims,
        evidence, tensions, and open questions in your sources. Or switch to Socratic mode for
        probing questions that push your thinking further. Both work from the active notebook's
        documents; a flat mind map lives in the Studio.
      </P>
    ),
  },
  {
    id: "canvas",
    title: "Canvas",
    body: (
      <>
        <P>
          Every notebook has one open canvas for visual thinking. Pick a tool from the bar at the top,
          then work on the surface:
        </P>
        <List>
          <li>
            <B>Draw</B> freehand with the pen, or add <B>rectangles</B>, <B>ellipses</B>, and <B>text</B>.
          </li>
          <li>
            <B>Add an image</B> from the toolbar, or drag one straight onto the canvas.
          </li>
          <li>
            With the select tool, <B>drag empty space to pan</B> and drag an item to move it; press{" "}
            <Kbd>Delete</Kbd> to remove a selected item.
          </li>
          <li>
            <B>Scroll to zoom</B> toward the cursor, and undo or redo with <Kbd>Ctrl</Kbd>/<Kbd>Cmd</Kbd>{" "}
            <Kbd>Z</Kbd>.
          </li>
        </List>
        <P>Everything you make is saved with the notebook and reloads exactly as you left it.</P>
      </>
    ),
  },
  {
    id: "audio",
    title: "Audio Studio",
    body: (
      <P>
        Your notebook, read aloud in your browser. Choose how it should be told: a two-host{" "}
        <B>discussion</B>, a one-minute <B>brief</B> from a single narrator, an <B>interview</B>{" "}
        that presses an expert on the detail, a <B>lecture</B> that teaches it in order, a run of{" "}
        <B>questions</B> the material raises with direct answers, a <B>debate</B> between opposing
        speakers, or a <B>critique</B> that weighs strengths and gaps.
      </P>
    ),
  },
  {
    id: "transforms",
    title: "Transform and Prompt Studio",
    body: (
      <>
        <P>
          Transforms run a summary, key-point extraction, or your own prompt over a single document.
        </P>
        <P>
          The Prompt Studio writes complete, ready-to-run prompts. Describe the job in plain words and
          it crafts the whole prompt for you, picking the right technique for the task: worked
          examples for sorting and extraction, step-by-step reasoning for logic, role and style for
          creative work. It also recommends the model settings to run it with, and turns any detail it
          does not know into a <Code>{"{variable}"}</Code> you fill in, so prompts stay accurate
          instead of inventing facts. Prefer building by hand? Switch to Build it and compose from
          parts, then upgrade the draft with the same crafter. The Studio can also start you from a
          proven working method, drawn from the same open{" "}
          <a
            href="https://amey-thakur.github.io/AI-SKILLS/"
            className="text-accent underline underline-offset-2"
          >
            AI-SKILLS library
          </a>{" "}
          we maintain for every AI agent.
        </P>
      </>
    ),
  },
  {
    id: "sharing",
    title: "Sharing a notebook",
    body: (
      <P>
        On the Notebooks page, hover a notebook and choose <B>Export</B> to save it as a single{" "}
        <Code>.nblab</Code> file. That file is self-contained: it holds the notebook, its notes, its
        documents as searchable text, and its canvas, with no original files needed. On another machine,
        use <B>Import</B> on the Notebooks page to recreate it. It all works offline.
      </P>
    ),
  },
  {
    id: "models",
    title: "Models and providers",
    body: (
      <P>
        NotebookLab works three ways, and you can mix them. Fully offline: pick from the bundled
        model catalog on the Models page (eight curated models; the strongest one your computer
        runs comfortably is marked "Recommended") and nothing else is needed, with no account and
        no token bill. More local choice: install Ollama and
        the Models page offers a curated catalog of open models (Llama, Gemma, Qwen, DeepSeek,
        Mistral, Phi) with one-click installs, marked by whether they fit this computer's memory.
        Most capable: connect a cloud provider (Anthropic Claude, OpenAI, Google Gemini, or
        DeepSeek) with your own API key through a guided setup; the key is entered once, stored
        only on this machine, and sent only to that provider. The model menu in the top bar shows
        which model is active and switches between them in two clicks, remembered across restarts.
        Turn on Auto in that menu and NotebookLab picks the best of your models for each task,
        preferring free local compute for quick work and your strongest model for hard generation,
        falling back automatically if one stops answering. The status bar counts this session's
        real token use as reported by the providers; click it for the last request's context-window
        fill and a per-model breakdown. Either way, the choice is yours and it is explicit.
      </P>
    ),
  },
  {
    id: "shortcuts",
    title: "Keyboard shortcuts",
    body: (
      <P>
        Press <Kbd>?</Kbd> anywhere to see every shortcut, grouped by area. Navigate by typing{" "}
        <Kbd>G</Kbd> then a key (for example <Kbd>G</Kbd> <Kbd>N</Kbd> for Notebooks). The Settings page
        lists the same shortcuts.
      </P>
    ),
  },
  {
    id: "privacy",
    title: "Privacy and offline",
    body: (
      <P>
        NotebookLab is offline-first by design. Your files, notes, chats, and canvases stay in a local
        database on your machine. There is no telemetry. The only time anything leaves your computer is
        if you deliberately connect a cloud AI provider, and even then only the text you send it does.
      </P>
    ),
  },
  {
    id: "more",
    title: "Learn more",
    body: (
      <P>
        The full source, changelog, and community discussions live at{" "}
        <Code>github.com/Amey-Thakur/NotebookLab</Code>. Every commit and release is signed, and the
        Makers' Pledge on the About page carries the fingerprint of the key that signs them.
      </P>
    ),
  },
];

export function HelpPage() {
  return (
    <div className="mx-auto max-w-3xl p-8">
      <h1 className="mb-1 font-display text-2xl font-bold text-text-1">Guide</h1>
      <p className="mb-8 text-sm text-text-3">
        Everything NotebookLab can do, in one place. This guide is part of the app, so it works offline.
      </p>

      <nav aria-label="On this page" className="mb-10 border border-border bg-surface-2 p-4">
        <p className="mb-2 font-mono text-2xs uppercase tracking-widest text-text-4">On this page</p>
        <ul className="grid grid-cols-2 gap-x-4 gap-y-1 sm:grid-cols-3">
          {SECTIONS.map((section) => (
            <li key={section.id}>
              <a
                href={`#${section.id}`}
                className="text-sm text-accent hover:text-text-1 transition-colors"
              >
                {section.title}
              </a>
            </li>
          ))}
        </ul>
      </nav>

      <div className="space-y-10">
        {SECTIONS.map((section) => (
          <section key={section.id} id={section.id} className="scroll-mt-6">
            <h2 className="mb-3 font-display text-lg font-bold text-text-1">{section.title}</h2>
            {section.body}
          </section>
        ))}
      </div>
    </div>
  );
}

function P({ children }: { children: ReactNode }) {
  return <p className="font-body text-sm leading-relaxed text-text-2">{children}</p>;
}

function List({ children }: { children: ReactNode }) {
  return (
    <ul className="list-disc space-y-1.5 pl-5 font-body text-sm leading-relaxed text-text-2">
      {children}
    </ul>
  );
}

function B({ children }: { children: ReactNode }) {
  return <strong className="font-semibold text-text-1">{children}</strong>;
}

function Code({ children }: { children: ReactNode }) {
  return (
    <code className="rounded-sm bg-surface-2 px-1 py-0.5 font-mono text-xs text-text-1">{children}</code>
  );
}

function Kbd({ children }: { children: ReactNode }) {
  return (
    <kbd className="rounded-sm border border-border bg-surface px-1.5 py-0.5 font-mono text-xs text-text-2">
      {children}
    </kbd>
  );
}

function Callout({ children }: { children: ReactNode }) {
  return (
    <div className="mt-3 border-l-2 border-accent bg-surface-2 px-4 py-3">
      <p className="font-body text-sm leading-relaxed text-text-2">{children}</p>
    </div>
  );
}
