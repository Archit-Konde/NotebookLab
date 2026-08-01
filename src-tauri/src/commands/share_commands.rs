/*
 * Name: share_commands.rs
 * Purpose: Export a notebook to a self-contained file and import it back.
 * Description: Sharing solved for an offline app. A notebook is written as one
 *   portable JSON bundle holding the notebook, its notes, its documents with
 *   their extracted text chunks, and its canvas scene, so the exported file
 *   opens on any other machine with no network and no original source files.
 *   Import creates a brand-new notebook and, if any step fails, removes the
 *   half-built notebook so nothing partial is left behind. The bundle is
 *   versioned, and imports of an unknown format or a newer version are rejected
 *   with a clear message. Both commands run on a blocking worker.
 * Tech Stack: Rust, Tauri v2, serde_json
 * License: MIT
 * Authors: Amey Thakur (https://github.com/Amey-Thakur)
 *          Archit Konde (https://github.com/Archit-Konde)
 * Date: 2026-07-13
 */

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::database::models::{
    CreateChunk, CreateDocument, CreateNote, CreateNotebook, DocumentStatus,
};
use crate::database::repository::{
    canvas_repository, chunk_repository, document_repository, note_repository, notebook_repository,
};
use crate::error::{AppError, AppResult};
use crate::state::AppState;

const BUNDLE_FORMAT: &str = "notebooklab-notebook";
const BUNDLE_VERSION: u32 = 1;
/* Cap the file we will read on import so a hostile bundle cannot exhaust memory. */
const MAX_BUNDLE_BYTES: u64 = 256 * 1024 * 1024;
/* Longer than any name someone types, short enough to render in a sidebar. */
const MAX_NAME_BYTES: usize = 200;

#[derive(Serialize, Deserialize)]
pub struct NotebookBundle {
    pub format: String,
    pub version: u32,
    pub notebook: BundleNotebook,
    pub notes: Vec<BundleNote>,
    pub documents: Vec<BundleDocument>,
    #[serde(default)]
    pub canvas: String,
}

#[derive(Serialize, Deserialize)]
pub struct BundleNotebook {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub color: String,
}

#[derive(Serialize, Deserialize)]
pub struct BundleNote {
    pub title: String,
    pub content: String,
}

#[derive(Serialize, Deserialize)]
pub struct BundleDocument {
    pub title: String,
    pub file_type: String,
    pub file_hash: String,
    pub file_size: i64,
    pub chunks: Vec<BundleChunk>,
}

#[derive(Serialize, Deserialize)]
pub struct BundleChunk {
    pub content: String,
    pub position: i32,
    pub page_number: Option<i32>,
    #[serde(default)]
    pub heading_context: String,
    pub token_count: i32,
}

/// Collect a notebook's contents into a portable bundle.
pub fn build_bundle(conn: &Connection, notebook_id: &str) -> AppResult<NotebookBundle> {
    let notebook = notebook_repository::get_by_id(conn, notebook_id)?;

    let notes = note_repository::list_by_notebook(conn, notebook_id)?
        .into_iter()
        .map(|note| BundleNote {
            title: note.title,
            content: note.content,
        })
        .collect();

    let mut documents = Vec::new();
    for doc in document_repository::list_by_notebook(conn, notebook_id)? {
        let chunks = chunk_repository::get_by_document(conn, &doc.id)?
            .into_iter()
            .map(|chunk| BundleChunk {
                content: chunk.content,
                position: chunk.position,
                page_number: chunk.page_number,
                heading_context: chunk.heading_context,
                token_count: chunk.token_count,
            })
            .collect();
        documents.push(BundleDocument {
            title: doc.title,
            file_type: doc.file_type,
            file_hash: doc.file_hash,
            file_size: doc.file_size,
            chunks,
        });
    }

    let canvas = canvas_repository::find_by_notebook(conn, notebook_id)?
        .map(|c| c.scene)
        .unwrap_or_default();

    Ok(NotebookBundle {
        format: BUNDLE_FORMAT.to_string(),
        version: BUNDLE_VERSION,
        notebook: BundleNotebook {
            name: notebook.name,
            description: notebook.description,
            color: notebook.color,
        },
        notes,
        documents,
        canvas,
    })
}

/// Create a new notebook from a bundle. Returns the new notebook id. On any
/// failure the partial notebook is deleted so nothing half-imported remains.
pub fn write_bundle(conn: &Connection, bundle: NotebookBundle) -> AppResult<String> {
    let optional = |value: String| if value.is_empty() { None } else { Some(value) };

    /* A bundle is a file from elsewhere, so its name is not to be trusted. An
    empty one produces a notebook with nothing to click in the sidebar, and an
    absurdly long one breaks every list it appears in. */
    let name = bundle.notebook.name.trim();
    if name.is_empty() {
        return Err(AppError::InvalidInput(
            "This notebook file has no name.".into(),
        ));
    }
    let name =
        crate::utils::text_utils::truncate_to_char_boundary(name, MAX_NAME_BYTES).to_string();

    let notebook = notebook_repository::create(
        conn,
        CreateNotebook {
            name,
            description: optional(bundle.notebook.description),
            color: optional(bundle.notebook.color),
        },
    )?;

    match populate(
        conn,
        &notebook.id,
        bundle.notes,
        bundle.documents,
        &bundle.canvas,
    ) {
        Ok(()) => Ok(notebook.id),
        Err(e) => {
            notebook_repository::delete(conn, &notebook.id).ok();
            Err(e)
        }
    }
}

fn populate(
    conn: &Connection,
    notebook_id: &str,
    notes: Vec<BundleNote>,
    documents: Vec<BundleDocument>,
    canvas: &str,
) -> AppResult<()> {
    for note in notes {
        note_repository::create(
            conn,
            CreateNote {
                notebook_id: notebook_id.to_string(),
                title: Some(note.title),
                content: Some(note.content),
            },
        )?;
    }

    for doc in documents {
        let created = document_repository::create(
            conn,
            CreateDocument {
                notebook_id: notebook_id.to_string(),
                title: doc.title,
                file_path: "(imported)".to_string(),
                file_type: doc.file_type,
                file_hash: doc.file_hash,
                file_size: doc.file_size,
            },
        )?;

        if !doc.chunks.is_empty() {
            /* Positions are rebuilt rather than copied. A bundle carries whatever
            the exporting install held, and installs before 0.8.1 numbered chunks
            from zero for every page, so their documents read out of order. The
            migration repairs the local library once at startup, which a bundle
            imported afterwards would sidestep entirely, leaving a scrambled
            document with nothing left to fix it. Sorting by page and then by the
            recorded position recovers reading order from either shape.

            The sort is stable, so chunks that share a page and a position keep
            the order the bundle lists them in, which is the order they were
            exported and therefore the order they were read. */
            let mut incoming = doc.chunks;
            incoming.sort_by_key(|chunk| (chunk.page_number.unwrap_or(-1), chunk.position));
            let chunks = incoming
                .into_iter()
                .enumerate()
                .map(|(index, chunk)| CreateChunk {
                    document_id: created.id.clone(),
                    content: chunk.content,
                    position: index as i32,
                    page_number: chunk.page_number,
                    heading_context: chunk.heading_context,
                    token_count: chunk.token_count,
                })
                .collect();
            chunk_repository::bulk_create(conn, chunks)?;
        }

        document_repository::update_status(conn, &created.id, DocumentStatus::Processed)?;
    }

    if !canvas.trim().is_empty() {
        let created = canvas_repository::get_or_create(conn, notebook_id)?;
        canvas_repository::update_scene(conn, &created.id, canvas)?;
    }

    Ok(())
}

/// Export a notebook to a self-contained file at `dest_path`.
#[tauri::command(rename_all = "snake_case")]
pub async fn export_notebook(
    app: tauri::AppHandle,
    notebook_id: String,
    dest_path: String,
) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let state: tauri::State<'_, AppState> = app.state();
        let bundle = {
            let conn = state.conn()?;
            build_bundle(&conn, &notebook_id)?
        };
        let json = serde_json::to_string_pretty(&bundle)?;
        std::fs::write(&dest_path, json)?;
        Ok(())
    })
    .await
    .map_err(|e| AppError::Internal(format!("Export task failed: {e}")))?
}

/// Import a notebook file, creating a new notebook. Returns its id.
#[tauri::command(rename_all = "snake_case")]
pub async fn import_notebook(app: tauri::AppHandle, src_path: String) -> AppResult<String> {
    tauri::async_runtime::spawn_blocking(move || {
        let metadata = std::fs::metadata(&src_path)?;
        if metadata.len() > MAX_BUNDLE_BYTES {
            return Err(AppError::InvalidInput(
                "This notebook file is too large to import.".into(),
            ));
        }

        let json = std::fs::read_to_string(&src_path)?;
        let bundle: NotebookBundle = serde_json::from_str(&json).map_err(|_| {
            AppError::InvalidInput("This is not a valid NotebookLab notebook file.".into())
        })?;

        if bundle.format != BUNDLE_FORMAT {
            return Err(AppError::InvalidInput(
                "This file is not a NotebookLab notebook.".into(),
            ));
        }
        if bundle.version > BUNDLE_VERSION {
            return Err(AppError::InvalidInput(
                "This notebook was exported by a newer version of NotebookLab.".into(),
            ));
        }

        let state: tauri::State<'_, AppState> = app.state();
        let conn = state.conn()?;
        write_bundle(&conn, bundle)
    })
    .await
    .map_err(|e| AppError::Internal(format!("Import task failed: {e}")))?
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bundle with the given notebook name and nothing else in it.
    fn bundle(name: &str) -> NotebookBundle {
        NotebookBundle {
            format: BUNDLE_FORMAT.to_string(),
            version: BUNDLE_VERSION,
            notebook: BundleNotebook {
                name: name.to_string(),
                description: String::new(),
                color: String::new(),
            },
            notes: Vec::new(),
            documents: Vec::new(),
            canvas: String::new(),
        }
    }

    /// An in-memory database with the real schema, so these exercise the same
    /// statements and constraints the app runs against.
    fn memory_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn.execute_batch(include_str!(
            "../../resources/migrations/001_initial_schema.sql"
        ))
        .unwrap();
        conn.execute_batch(include_str!("../../resources/migrations/005_canvas.sql"))
            .unwrap();
        conn
    }

    #[test]
    fn a_nameless_bundle_is_refused() {
        /* Importing one produced a notebook with nothing to click on. */
        let conn = memory_db();
        assert!(write_bundle(&conn, bundle("")).is_err());
        assert!(write_bundle(&conn, bundle("   ")).is_err());
    }

    #[test]
    fn a_name_is_trimmed_and_capped() {
        let conn = memory_db();
        let id = write_bundle(&conn, bundle("  Reading list  ")).unwrap();
        let created = notebook_repository::get_by_id(&conn, &id).unwrap();
        assert_eq!(created.name, "Reading list");

        let long = "n".repeat(MAX_NAME_BYTES * 3);
        let id = write_bundle(&conn, bundle(&long)).unwrap();
        let created = notebook_repository::get_by_id(&conn, &id).unwrap();
        assert!(created.name.len() <= MAX_NAME_BYTES);
    }

    #[test]
    fn an_import_rebuilds_chunk_order_from_an_old_bundle() {
        /* A bundle exported by an install before 0.8.1 carries per-page
        positions: every page numbered from zero. Migration 007 repairs the
        library once at startup, so a bundle imported after that point would
        keep its scrambled order forever unless the import fixes it. */
        let conn = memory_db();
        let mut b = bundle("Old export");
        let chunks = (1..=3)
            .flat_map(|page| {
                (0..2).map(move |position| BundleChunk {
                    content: format!("page {page} part {position}"),
                    position,
                    page_number: Some(page),
                    heading_context: String::new(),
                    token_count: 3,
                })
            })
            .collect();
        b.documents.push(BundleDocument {
            title: "Scrambled".into(),
            file_type: "pdf".into(),
            file_hash: "old".into(),
            file_size: 10,
            chunks,
        });

        let id = write_bundle(&conn, b).unwrap();
        let docs = document_repository::list_by_notebook(&conn, &id).unwrap();
        let stored = chunk_repository::get_by_document(&conn, &docs[0].id).unwrap();

        assert_eq!(
            stored.iter().map(|c| c.position).collect::<Vec<_>>(),
            (0..6).collect::<Vec<i32>>(),
            "positions must be renumbered continuously"
        );
        let expected: Vec<String> = (1..=3)
            .flat_map(|page| (0..2).map(move |part| format!("page {page} part {part}")))
            .collect();
        assert_eq!(
            stored.iter().map(|c| c.content.clone()).collect::<Vec<_>>(),
            expected,
            "pages must read in order, not interleave"
        );
    }

    #[test]
    fn an_import_keeps_the_order_of_a_pageless_bundle() {
        /* Text and Markdown carry no page number. Sorting must fall back to the
        recorded position rather than collapsing them into bundle order. */
        let conn = memory_db();
        let mut b = bundle("Pageless");
        b.documents.push(BundleDocument {
            title: "Notes".into(),
            file_type: "txt".into(),
            file_hash: "txt".into(),
            file_size: 10,
            chunks: vec![
                BundleChunk {
                    content: "third".into(),
                    position: 2,
                    page_number: None,
                    heading_context: String::new(),
                    token_count: 1,
                },
                BundleChunk {
                    content: "first".into(),
                    position: 0,
                    page_number: None,
                    heading_context: String::new(),
                    token_count: 1,
                },
                BundleChunk {
                    content: "second".into(),
                    position: 1,
                    page_number: None,
                    heading_context: String::new(),
                    token_count: 1,
                },
            ],
        });

        let id = write_bundle(&conn, b).unwrap();
        let docs = document_repository::list_by_notebook(&conn, &id).unwrap();
        let stored = chunk_repository::get_by_document(&conn, &docs[0].id).unwrap();
        assert_eq!(
            stored.iter().map(|c| c.content.clone()).collect::<Vec<_>>(),
            vec!["first", "second", "third"]
        );
    }

    #[test]
    fn an_import_round_trips_notes_and_chunks() {
        let conn = memory_db();
        let mut b = bundle("Imported");
        b.notes.push(BundleNote {
            title: "A note".into(),
            content: "Its body".into(),
        });
        b.documents.push(BundleDocument {
            title: "A document".into(),
            file_type: "pdf".into(),
            file_hash: "abc123".into(),
            file_size: 10,
            chunks: vec![BundleChunk {
                content: "A passage".into(),
                position: 0,
                page_number: Some(1),
                heading_context: "Intro".into(),
                token_count: 3,
            }],
        });

        let id = write_bundle(&conn, b).unwrap();
        let notes = note_repository::list_by_notebook(&conn, &id).unwrap();
        assert_eq!(notes.len(), 1);
        let docs = document_repository::list_by_notebook(&conn, &id).unwrap();
        assert_eq!(docs.len(), 1);
        /* Imported documents must read as ready: their text is already in the
        bundle, so leaving them pending would hide them from every feature that
        filters on processed. */
        assert_eq!(docs[0].status, DocumentStatus::Processed);
        let chunks = chunk_repository::get_by_document(&conn, &docs[0].id).unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].content, "A passage");
    }

    #[test]
    fn a_failed_import_leaves_no_notebook_behind() {
        /* A document with no title violates the schema, so this fails partway
        through and must roll the whole thing back rather than leave a
        half-built notebook in the sidebar. */
        let conn = memory_db();
        let before = notebook_repository::list_all(&conn).unwrap().len();

        let mut b = bundle("Doomed");
        b.documents.push(BundleDocument {
            title: "Fine".into(),
            file_type: "pdf".into(),
            file_hash: "h".into(),
            file_size: 1,
            chunks: vec![BundleChunk {
                content: "text".into(),
                /* A chunk pointing at no document would break the foreign key;
                position is fine, so force the failure with the canvas below. */
                position: 0,
                page_number: None,
                heading_context: String::new(),
                token_count: 1,
            }],
        });
        /* An oversized canvas is refused by the repository, which is the
        failure this exercises. */
        b.canvas = "x".repeat(canvas_repository::MAX_SCENE_BYTES + 1);

        assert!(write_bundle(&conn, b).is_err());
        assert_eq!(notebook_repository::list_all(&conn).unwrap().len(), before);
    }

    #[test]
    fn an_oversized_canvas_cannot_be_written_through_import() {
        /* The ceiling used to live only in the command, so this path wrote a
        scene of any size straight into the database. */
        let conn = memory_db();
        let notebook = notebook_repository::create(
            &conn,
            CreateNotebook {
                name: "Host".into(),
                description: None,
                color: None,
            },
        )
        .unwrap();
        let canvas = canvas_repository::get_or_create(&conn, &notebook.id).unwrap();
        let huge = "x".repeat(canvas_repository::MAX_SCENE_BYTES + 1);
        assert!(canvas_repository::update_scene(&conn, &canvas.id, &huge).is_err());
    }
}
