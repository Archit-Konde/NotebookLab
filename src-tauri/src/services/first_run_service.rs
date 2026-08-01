/*
 * Name: first_run_service.rs
 * Purpose: First-run experience.
 * Description: Creates a sample notebook with example notes and a couple of
 *   short sample documents so a new user can try every feature (search, chat,
 *   the Studio, transforms, audio overview) without importing anything first.
 *   The documents are chunked through the real chunking service, so they behave
 *   exactly like imported files. Gated on a settings-table flag rather than
 *   notebook count, so a user who deletes every notebook is respected instead
 *   of getting the sample recreated on the next launch. The two sample notes
 *   link to each other, so their wiki-links are re-synced after both rows exist.
 * Tech Stack: Rust, rusqlite
 * License: MIT
 * Authors: Amey Thakur (https://github.com/Amey-Thakur)
 *          Archit Konde (https://github.com/Archit-Konde)
 * Date: 2026-07-14
 */

use rusqlite::Connection;
use sha2::{Digest, Sha256};

use crate::database::models::{CreateDocument, CreateNote, CreateNotebook, DocumentStatus};
use crate::database::repository::{
    chunk_repository, document_repository, note_repository, notebook_repository,
};
use crate::error::AppResult;
use crate::services::chunking_service;

const FIRST_RUN_FLAG: &str = "first_run_done";

/// Create a sample notebook on the very first launch.
pub fn ensure_sample_notebook(conn: &Connection) -> AppResult<()> {
    let already_done: bool = conn
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            rusqlite::params![FIRST_RUN_FLAG],
            |row| row.get::<_, String>(0),
        )
        .map(|v| v == "true")
        .unwrap_or(false);

    if already_done {
        return Ok(());
    }

    /* Record the attempt up front, before any seeding, so a failure partway
    through can never recreate a duplicate sample notebook on the next launch. */
    conn.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, 'true')",
        rusqlite::params![FIRST_RUN_FLAG],
    )?;

    tracing::info!("First run detected. Creating sample notebook...");

    let notebook = notebook_repository::create(
        conn,
        CreateNotebook {
            name: "Getting Started".to_string(),
            description: Some(
                "Sample notes and documents to explore. Open Chat or the Studio and try them."
                    .to_string(),
            ),
            color: Some("#4A70A9".to_string()),
        },
    )?;

    let welcome = note_repository::create(
        conn,
        CreateNote {
            notebook_id: notebook.id.clone(),
            title: Some("Welcome to NotebookLab".to_string()),
            content: Some(
                r#"# Welcome to NotebookLab

Your thinking partner, on your machine.

## What you can do

- **Import documents** (PDF, Word, text, Markdown, and images) and ask questions about them
- **Write notes** with Markdown formatting and [[wiki-links]]
- **Chat with your documents** using AI-powered RAG (Retrieval-Augmented Generation)
- **Generate mind maps** from your research
- **Transform documents** (summarize, extract key points, custom prompts)

## Getting started

1. Go to **Models** in the sidebar and register an AI provider (Ollama is the easiest for local use)
2. Import a document using the notebook detail page
3. Open **Chat** and ask a question about your document
4. Open **Think** in the sidebar to build an idea space or work through Socratic questions

## Wiki-links

You can link between notes using [[double brackets]]. Try linking to [[Research Notes]] below.

See the [[Research Notes]] for an example of how notes connect."#
                    .to_string(),
            ),
        },
    )?;

    note_repository::create(
        conn,
        CreateNote {
            notebook_id: notebook.id.clone(),
            title: Some("Research Notes".to_string()),
            content: Some(
                r#"# Research Notes

This is a sample note demonstrating how notes work in NotebookLab.

## Key Features

- **Bi-directional links**: This note is linked from [[Welcome to NotebookLab]]
- **Auto-save**: Your changes save automatically every 2 seconds
- **Markdown**: Full support for headings, lists, code blocks, and more

## Example Content

> "The best way to predict the future is to invent it." (Alan Kay)

### Code Example

```rust
fn main() {
    println!("Hello from NotebookLab!");
}
```

### Task List

- [x] Install NotebookLab
- [x] Open sample notebook
- [ ] Import your first document
- [ ] Try the AI chat
- [ ] Generate a mind map"#
                    .to_string(),
            ),
        },
    )?;

    /* create() already re-resolves inbound links when the second note appears,
    so both directions resolve on their own. This explicit re-sync of the
    welcome note is a harmless belt-and-suspenders and keeps the seeding robust
    if that behavior ever changes. */
    note_repository::sync_note_links(conn, &welcome.id, &notebook.id, &welcome.content)?;

    /* Seed a couple of short sample documents so search, chat, the Studio, and
    transforms all have real content to work on from the first launch. They are
    chunked through the same service imports use, so nothing about them is
    special downstream. */
    seed_sample_document(conn, &notebook.id, "About NotebookLab", SAMPLE_ABOUT)?;
    seed_sample_document(conn, &notebook.id, "A Guide to NotebookLab", SAMPLE_GUIDE)?;

    tracing::info!("Sample notebook created with 2 notes and 2 documents");
    Ok(())
}

/// Create a sample document from in-memory text, chunked exactly as an imported
/// file would be. No file is written; the stored path is a marker, since search
/// and chat read the chunks, not the original file.
fn seed_sample_document(
    conn: &Connection,
    notebook_id: &str,
    title: &str,
    content: &str,
) -> AppResult<()> {
    let file_hash = format!("{:x}", Sha256::digest(content.as_bytes()));

    let doc = document_repository::create(
        conn,
        CreateDocument {
            notebook_id: notebook_id.to_string(),
            title: title.to_string(),
            file_path: format!("sample://{title}"),
            file_type: "txt".to_string(),
            file_hash,
            file_size: content.len() as i64,
        },
    )?;

    document_repository::update_status(conn, &doc.id, DocumentStatus::Processing)?;
    let chunks = chunking_service::chunk_text(&doc.id, content, Some(1), "");
    chunk_repository::bulk_create(conn, chunks)?;
    document_repository::update_status(conn, &doc.id, DocumentStatus::Processed)?;

    Ok(())
}

const SAMPLE_ABOUT: &str = r#"About NotebookLab

NotebookLab is an offline-first workspace for reading, questioning, and thinking with your own documents. It brings a source-grounded AI assistant together with a connected notes editor and a spatial canvas, all running on your own machine.

The idea behind it is simple. The tools we think with have quietly become products that watch us: the notebook turned into a subscription, and the private note into a data point. NotebookLab is built the other way around. Your files, notes, chats, and canvases live in a local database on your computer. There is no account to create and no telemetry. Nothing you write leaves the machine unless you deliberately connect a cloud AI provider, and even then only the text you send it does.

At its center is retrieval-augmented chat. You import documents into a notebook, and when you ask a question, NotebookLab finds the most relevant passages and asks a language model to answer using them. Every answer lists the sources it drew from, with the document, heading, and page, so you can check a claim instead of taking it on faith.

The AI can run entirely on your machine through a bundled local model server, so the app works with no internet at all. If you prefer a larger model, you can connect any OpenAI-compatible provider, such as Ollama or LM Studio, and the choice stays yours and explicit.

NotebookLab reads PDFs, Word files, plain text, Markdown, and even images and scans, which it turns into searchable text with offline optical character recognition. Once a document is in a notebook, it flows into search, chat, and the Studio alike.

It is free and open source, released under the MIT License, and made to earn a small, quiet place in how you work every day."#;

const SAMPLE_GUIDE: &str = r#"A Guide to NotebookLab

NotebookLab is organised around notebooks. A notebook holds related sources, notes, and one spatial canvas, and everything you do is scoped to the notebook you have open.

Bringing in sources. Use Import on a notebook, or drag a file onto the window, to add a PDF, Word document, text or Markdown file, or an image. Images and scans are read with offline optical character recognition, so a photo of printed text becomes searchable content like any other source.

Search. Search blends keyword ranking with semantic similarity. Keyword search always works; when your AI provider supports embeddings, results also match on meaning, not just wording. Press Ctrl+K anywhere to jump to a page, a notebook, or an action.

Chat. Ask questions about the documents in the active notebook and get answers grounded in cited sources. You can drop a file straight onto the chat to add it as a source first.

Studio. Turn a notebook's sources into study aids and write-ups: a study guide, flashcards, a quiz, a visual mind map, a timeline, a slide deck, a data table, a briefing doc, or a blog post. Each one is drawn from your own documents.

Think. Build an idea space from your research, a moving three-dimensional map of the claims, evidence, tensions, and open questions in your sources. Or switch to Socratic mode for probing questions that push your thinking further.

Transform. Run a source through a summary, a key-point extraction, or a prompt you write yourself.

Canvas. Every notebook has one open canvas for visual thinking. Draw freehand, add shapes and text, and drop in images, then pan, zoom, and rearrange.

Audio Studio. Your notebook, read aloud in your browser: a two-host discussion, a short brief, an interview, a lecture, a run of questions, a debate, or a critique.

Prompt Studio. Describe a job in plain words and NotebookLab writes a complete, ready-to-run prompt for any AI model, choosing the right technique for the task.

Sharing. Export a notebook to a single self-contained file, then import it on another machine to recreate it, fully offline."#;

#[cfg(test)]
mod tests {
    use super::{SAMPLE_ABOUT, SAMPLE_GUIDE};

    /* The sidebar is the source of truth for what each tool is called, so the
    test reads it rather than restating the names. Sample content that tells a
    new user to open something the sidebar labels differently sends them looking
    for a menu item that does not exist, which is how "Thinking Partner" came to
    point at a button labelled "Think".

    If the frontend is reorganised this include stops compiling. That is a louder
    failure than the silent drift it exists to catch. */
    const SIDEBAR: &str = include_str!("../../../src/components/layout/app-sidebar.tsx");

    /// Every tool the guide introduces, as the guide writes it.
    const TOOLS_IN_GUIDE: [&str; 7] = [
        "Search",
        "Chat",
        "Studio",
        "Think",
        "Transform",
        "Audio Studio",
        "Prompt Studio",
    ];

    fn sidebar_has_label(label: &str) -> bool {
        SIDEBAR.contains(&format!("label: \"{label}\""))
    }

    #[test]
    fn the_guide_names_every_tool_the_way_the_sidebar_does() {
        for tool in TOOLS_IN_GUIDE {
            assert!(
                sidebar_has_label(tool),
                "the guide sends the user to \"{tool}\", which is not a sidebar label"
            );
            assert!(
                SAMPLE_GUIDE.contains(tool),
                "the guide no longer mentions \"{tool}\""
            );
        }
    }

    #[test]
    fn the_welcome_note_points_at_real_destinations() {
        let welcome = welcome_note_content();
        for destination in ["Models", "Think"] {
            assert!(
                sidebar_has_label(destination),
                "the welcome note sends the user to \"{destination}\", which is not a sidebar label"
            );
            assert!(
                welcome.contains(destination),
                "the welcome note no longer mentions \"{destination}\""
            );
        }
    }

    #[test]
    fn the_samples_do_not_promise_a_file_type_the_app_cannot_read() {
        /* Parsers exist for PDF, Word, plain text, Markdown and images. Naming a
        format the app cannot open is worse than naming none. */
        for claimed in ["PDF", "Word", "Markdown"] {
            assert!(
                SAMPLE_ABOUT.contains(claimed) || SAMPLE_GUIDE.contains(claimed),
                "no sample mentions {claimed}"
            );
        }
        for absent in ["PowerPoint", "Excel", "EPUB", "spreadsheet"] {
            assert!(
                !SAMPLE_ABOUT.contains(absent) && !SAMPLE_GUIDE.contains(absent),
                "the samples promise {absent}, which has no parser"
            );
        }
    }

    #[test]
    fn the_sample_documents_chunk_into_reading_order() {
        /* The samples go through the same chunker an import uses, so they are a
        standing check that a single-call chunking still numbers continuously. */
        for text in [SAMPLE_ABOUT, SAMPLE_GUIDE] {
            let chunks = crate::services::chunking_service::chunk_text("doc", text, Some(1), "");
            assert!(!chunks.is_empty(), "a sample produced no chunks");
            let positions: Vec<i32> = chunks.iter().map(|c| c.position).collect();
            assert_eq!(
                positions,
                (0..positions.len() as i32).collect::<Vec<_>>(),
                "sample chunk positions must run continuously from zero"
            );
        }
    }

    /// The welcome note body, lifted from the seeding call so the test reads the
    /// text that actually ships rather than a copy that can drift from it.
    fn welcome_note_content() -> &'static str {
        let source = include_str!("first_run_service.rs");
        let start = source
            .find("# Welcome to NotebookLab")
            .expect("welcome note body");
        let end = source[start..]
            .find("\"#")
            .expect("end of the welcome note literal");
        &source[start..start + end]
    }
}
