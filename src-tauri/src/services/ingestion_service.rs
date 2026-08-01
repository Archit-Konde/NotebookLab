/*
 * Name: ingestion_service.rs
 * Purpose: Document ingestion pipeline.
 * Description: Orchestrates: parse -> chunk -> store. This is the core value
 *   proposition pipeline. A file enters as a path, and exits as
 *   indexed chunks ready for RAG retrieval. Each step is modular:
 *   parsers handle format extraction, chunking service handles
 *   splitting, and repositories handle persistence. The pipeline is
 *   split into three phases so a caller can hold the database lock
 *   only for the short create and store steps and release it across
 *   the parse: parsing a large PDF, or running OCR on an image, can
 *   take seconds, and must not block every other database command.
 *   `ingest_file` runs all three in order for simple callers (and
 *   tests); the import command drives them with the lock released
 *   around the parse.
 * Tech Stack: Rust, rusqlite
 * License: MIT
 * Authors: Amey Thakur (https://github.com/Amey-Thakur)
 *          Archit Konde (https://github.com/Archit-Konde)
 * Date: 2026-07-13
 */

use std::path::Path;
use std::sync::Arc;

use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::io::Read;

use crate::database::models::{CreateChunk, CreateDocument, DocumentStatus};
use crate::database::repository::{chunk_repository, document_repository};
use crate::error::{AppError, AppResult};
use crate::parsers;
use crate::parsers::image_ocr_parser::OcrEngineHandle;
use crate::services::chunking_service;

/// A document row created and set to Processing, ready to receive chunks.
pub struct PreparedIngest {
    pub doc_id: String,
}

/// Run the full ingestion pipeline on a file. Returns the created document ID.
///
/// This holds whatever database access the caller passes for the whole call,
/// including the parse. The import command instead runs the three phases below
/// itself so it can release the database lock across the (slow) parse; prefer
/// that for anything that runs while the app is in use.
///
/// `ocr` is the shared OCR engine, needed only for image formats; pass `None`
/// when it is unavailable and image import will report a clear error.
pub fn ingest_file(
    conn: &Connection,
    notebook_id: &str,
    file_path: &Path,
    ocr: Option<Arc<OcrEngineHandle>>,
) -> AppResult<String> {
    let prepared = prepare_ingest(conn, notebook_id, file_path)?;
    match parse_and_chunk(&prepared.doc_id, file_path, ocr) {
        Ok(chunks) => {
            finalize_ingest(conn, &prepared.doc_id, chunks)?;
            Ok(prepared.doc_id)
        }
        Err(e) => {
            mark_ingest_error(conn, &prepared.doc_id);
            Err(e)
        }
    }
}

/// Phase 1: validate, deduplicate, and create the document row (set Processing).
/// Short database work only, so the caller can release the lock afterward.
pub fn prepare_ingest(
    conn: &Connection,
    notebook_id: &str,
    file_path: &Path,
) -> AppResult<PreparedIngest> {
    let metadata = std::fs::metadata(file_path)?;

    let extension = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

    let title = file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Untitled")
        .to_string();

    /* Compute file hash for deduplication */
    let file_hash = compute_file_hash(file_path)?;

    /* Check if this file was already imported */
    if let Some(existing) = document_repository::find_by_hash(conn, notebook_id, &file_hash)? {
        tracing::info!(
            "Document already imported: {} ({})",
            existing.title,
            existing.id
        );
        return Err(AppError::InvalidInput(format!(
            "This file has already been imported as '{}'",
            existing.title
        )));
    }

    /* Create the document record */
    let doc = document_repository::create(
        conn,
        CreateDocument {
            notebook_id: notebook_id.to_string(),
            title,
            file_path: file_path.to_string_lossy().to_string(),
            file_type: extension.to_string(),
            file_hash,
            file_size: metadata.len() as i64,
        },
    )?;

    tracing::info!("Created document: {} ({})", doc.title, doc.id);

    document_repository::update_status(conn, &doc.id, DocumentStatus::Processing)?;

    Ok(PreparedIngest { doc_id: doc.id })
}

/// Phase 2: parse the file and split it into chunks. Does no database work, so
/// the caller can run it with the database lock released. This is the slow step
/// (PDF extraction, image OCR), which is exactly why it is kept lock-free.
pub fn parse_and_chunk(
    doc_id: &str,
    file_path: &Path,
    ocr: Option<Arc<OcrEngineHandle>>,
) -> AppResult<Vec<CreateChunk>> {
    let extension = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");

    let parser = parsers::parser_for_extension(extension, ocr.as_ref())?;
    let parsed = parser.parse(file_path)?;

    /* Chunk each page and collect all chunks.

    `chunk_text` numbers positions from zero for the text it is given, so a
    ten-page document came back with ten chunks numbered 0, ten numbered 1, and
    so on. Everything that reads a document back orders by position, so those
    ties put page one's first chunk next to page two's first chunk, and a
    transform assembled the document interleaved rather than in reading order.
    Renumbering across the whole document makes position mean what its readers
    assume it means. */
    let mut all_chunks = Vec::new();
    for page in &parsed.pages {
        let heading_context = page.headings.first().cloned().unwrap_or_default();
        let mut page_chunks = chunking_service::chunk_text(
            doc_id,
            &page.content,
            Some(page.page_number),
            &heading_context,
        );
        all_chunks.append(&mut page_chunks);
    }

    for (index, chunk) in all_chunks.iter_mut().enumerate() {
        chunk.position = index as i32;
    }

    if all_chunks.is_empty() {
        return Err(AppError::InvalidInput(
            "No text content could be extracted from this file".into(),
        ));
    }

    Ok(all_chunks)
}

/// Phase 3: store the chunks and mark the document processed. Short database
/// work only.
pub fn finalize_ingest(conn: &Connection, doc_id: &str, chunks: Vec<CreateChunk>) -> AppResult<()> {
    let chunk_count = chunk_repository::bulk_create(conn, chunks)?;
    tracing::info!("Created {} chunks for document {}", chunk_count, doc_id);
    document_repository::update_status(conn, doc_id, DocumentStatus::Processed)?;
    Ok(())
}

/// Best-effort: flip a document to the error state after a failed parse or
/// store, so it does not linger as Processing forever.
pub fn mark_ingest_error(conn: &Connection, doc_id: &str) {
    document_repository::update_status(conn, doc_id, DocumentStatus::Error).ok();
}

/// Compute SHA-256 hash via streaming reads to avoid loading entire file into memory.
fn compute_file_hash(path: &Path) -> AppResult<String> {
    let file = std::fs::File::open(path)?;
    let mut reader = std::io::BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];

    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}

#[cfg(test)]
mod tests {
    use crate::services::chunking_service;

    /// Rebuild what `parse_and_chunk` does across pages, without needing a real
    /// file or a parser: chunk each page, concatenate, then renumber.
    fn chunk_pages(pages: &[(&str, i32)]) -> Vec<crate::database::models::CreateChunk> {
        let mut all = Vec::new();
        for (text, page_number) in pages {
            let mut chunks = chunking_service::chunk_text("doc", text, Some(*page_number), "");
            all.append(&mut chunks);
        }
        for (index, chunk) in all.iter_mut().enumerate() {
            chunk.position = index as i32;
        }
        all
    }

    #[test]
    fn positions_are_unique_across_a_multi_page_document() {
        /* The bug: chunk_text numbers from zero for the text it is given, so
        every page produced its own chunk 0. Everything that reads a document
        back orders by position, so those ties interleaved the pages and a
        transform assembled the document out of reading order. */
        let long = "word ".repeat(1200);
        let pages = [(long.as_str(), 1), (long.as_str(), 2), (long.as_str(), 3)];
        let chunks = chunk_pages(&pages);
        assert!(chunks.len() > 3, "expected several chunks per page");

        let positions: Vec<i32> = chunks.iter().map(|c| c.position).collect();
        let mut sorted = positions.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), positions.len(), "positions must be unique");
        assert_eq!(positions, (0..positions.len() as i32).collect::<Vec<_>>());
    }

    #[test]
    fn ordering_by_position_reads_the_pages_in_order() {
        /* What the fix is for: sorting by position must walk page 1 through to
        page 3, never jump between them. */
        let long = "word ".repeat(1200);
        let mut chunks = chunk_pages(&[(long.as_str(), 1), (long.as_str(), 2), (long.as_str(), 3)]);
        chunks.sort_by_key(|c| c.position);

        let page_order: Vec<i32> = chunks.iter().filter_map(|c| c.page_number).collect();
        let mut expected = page_order.clone();
        expected.sort_unstable();
        assert_eq!(page_order, expected, "pages must not interleave");
    }

    #[test]
    fn a_single_page_document_is_unaffected() {
        let chunks = chunk_pages(&[("A short page of text.", 1)]);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].position, 0);
    }

    #[test]
    fn page_numbers_survive_renumbering() {
        /* Citations name the page, so renumbering position must not disturb it. */
        let long = "word ".repeat(1200);
        let chunks = chunk_pages(&[(long.as_str(), 7), (long.as_str(), 8)]);
        assert!(chunks.iter().any(|c| c.page_number == Some(7)));
        assert!(chunks.iter().any(|c| c.page_number == Some(8)));
    }
}
