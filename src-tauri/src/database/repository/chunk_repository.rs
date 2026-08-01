/*
 * Name: chunk_repository.rs
 * Purpose: Data access layer for document chunks.
 * Description: Handles bulk insertion during ingestion and retrieval for RAG
 *   context assembly. Chunks are created in batches during
 *   document ingestion. The bulk_create function uses a
 *   transaction for atomicity. Chunks are always queried by
 *   document_id (indexed) for document viewer and by position for
 *   ordered display.
 * Tech Stack: Rust, rusqlite
 * License: MIT
 * Authors: Amey Thakur (https://github.com/Amey-Thakur)
 *          Archit Konde (https://github.com/Archit-Konde)
 * Date: 2026-07-12
 */

use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::database::models::{Chunk, CreateChunk};
use crate::error::AppResult;

/// Insert a batch of chunks within a single transaction.
/// Used by the ingestion pipeline after document parsing and chunking.
pub fn bulk_create(conn: &Connection, chunks: Vec<CreateChunk>) -> AppResult<usize> {
    /* Safety: unchecked_transaction is required because conn is &Connection (not &mut),
    held behind a Mutex. The Mutex guarantees single-thread access, preventing
    concurrent transactions on the same connection. */
    let tx = conn.unchecked_transaction()?;
    let now = chrono::Utc::now().to_rfc3339();
    let count = chunks.len();

    for chunk in chunks {
        let id = Uuid::now_v7().to_string();

        tx.execute(
            "INSERT INTO chunks (id, document_id, content, position, page_number, heading_context, token_count, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![id, chunk.document_id, chunk.content, chunk.position, chunk.page_number, chunk.heading_context, chunk.token_count, now],
        )?;
    }

    tx.commit()?;
    Ok(count)
}

pub fn get_by_document(conn: &Connection, document_id: &str) -> AppResult<Vec<Chunk>> {
    let mut stmt = conn.prepare(
        "SELECT id, document_id, content, position, page_number, heading_context, token_count, created_at
         FROM chunks WHERE document_id = ?1 ORDER BY position ASC",
    )?;

    let chunks = stmt
        .query_map(params![document_id], Chunk::from_row)?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(chunks)
}

/// Count total chunks across all documents. Used for status bar display.
pub fn count_all(conn: &Connection) -> AppResult<i64> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM chunks", [], |row| row.get(0))?;
    Ok(count)
}

/// A representative spread of chunk text across a whole notebook, used by the
/// Studio and the Thinking Partner when no specific focus is given so
/// generation covers all sources.
///
/// This ordered every chunk in the notebook by document date and then took the
/// first N, which is not a spread across the notebook at all: it is the opening
/// of the newest document, and the rest of the library is reached only if that
/// one runs out of chunks first. With the limit at 20 and any real document
/// exceeding that, a study guide or a mind map built from a ten-source notebook
/// was built from one source, and every format asked for read the same opening
/// passages, which is why they came back so alike.
///
/// The even spread already written for an explicit selection is the correct
/// behaviour here too, so it is reused rather than restated.
pub fn sample_for_notebook(
    conn: &Connection,
    notebook_id: &str,
    limit: usize,
) -> AppResult<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT id FROM documents WHERE notebook_id = ?1 ORDER BY created_at DESC")?;

    let document_ids = stmt
        .query_map(params![notebook_id], |row| row.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;

    sample_for_documents(conn, &document_ids, limit)
}

/// Sample passages from an explicit set of documents.
///
/// Used when the user picks which sources a generation should read instead of
/// letting it range over the whole notebook. The spread is taken evenly across
/// the chosen documents rather than in one ordered run, so selecting four files
/// does not hand the model the first file and nothing else.
pub fn sample_for_documents(
    conn: &Connection,
    document_ids: &[String],
    limit: usize,
) -> AppResult<Vec<String>> {
    if document_ids.is_empty() {
        return Ok(Vec::new());
    }

    /* Round up, so a limit that does not divide evenly still gives every chosen
    document at least its share rather than starving the last one. */
    let per_doc = limit.div_ceil(document_ids.len()).max(1);
    let mut out = Vec::new();

    let mut stmt = conn.prepare(
        "SELECT content FROM chunks
         WHERE document_id = ?1
         ORDER BY position ASC
         LIMIT ?2",
    )?;

    for id in document_ids {
        let rows = stmt
            .query_map(params![id, per_doc as i64], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        out.extend(rows);
    }

    out.truncate(limit);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn.execute_batch(include_str!(
            "../../../resources/migrations/001_initial_schema.sql"
        ))
        .unwrap();
        conn.execute(
            "INSERT INTO notebooks (id, name) VALUES ('nb', 'Notebook')",
            [],
        )
        .unwrap();
        conn
    }

    /// Add a document with `chunks` passages, each naming its source so a sample
    /// can be traced back to the document it came from.
    fn seed_document(conn: &Connection, id: &str, created_at: &str, chunks: usize) {
        conn.execute(
            "INSERT INTO documents (id, notebook_id, title, file_path, file_hash, file_type, file_size, created_at)
             VALUES (?1, 'nb', ?1, '/x', ?1, 'pdf', 1, ?2)",
            params![id, created_at],
        )
        .unwrap();

        let passages: Vec<CreateChunk> = (0..chunks)
            .map(|position| CreateChunk {
                document_id: id.to_string(),
                content: format!("{id} passage {position}"),
                position: position as i32,
                page_number: Some(1),
                heading_context: String::new(),
                token_count: 3,
            })
            .collect();
        bulk_create(conn, passages).unwrap();
    }

    #[test]
    fn a_notebook_sample_reaches_every_document() {
        /* The bug: ordering the whole notebook by document date and taking the
        first N returns the opening of the newest document and nothing else,
        because any real document has more chunks than the limit. Every Studio
        format and the Thinking Partner sample this way when the user has not
        picked sources, so all of them were built from one document. */
        let conn = memory_db();
        seed_document(&conn, "newest", "2026-03-01T00:00:00+00:00", 50);
        seed_document(&conn, "middle", "2026-02-01T00:00:00+00:00", 50);
        seed_document(&conn, "oldest", "2026-01-01T00:00:00+00:00", 50);

        let sample = sample_for_notebook(&conn, "nb", 20).unwrap();

        for document in ["newest", "middle", "oldest"] {
            assert!(
                sample.iter().any(|passage| passage.starts_with(document)),
                "the sample never reached {document}: {sample:?}"
            );
        }
        assert!(sample.len() <= 20, "the limit must still be respected");
    }

    #[test]
    fn a_notebook_sample_reads_each_document_from_its_start() {
        let conn = memory_db();
        seed_document(&conn, "alpha", "2026-02-01T00:00:00+00:00", 20);
        seed_document(&conn, "beta", "2026-01-01T00:00:00+00:00", 20);

        let sample = sample_for_notebook(&conn, "nb", 4).unwrap();

        assert!(sample.contains(&"alpha passage 0".to_string()));
        assert!(sample.contains(&"beta passage 0".to_string()));
    }

    #[test]
    fn a_single_document_notebook_still_fills_the_limit() {
        /* The even spread must not starve a notebook that has only one source. */
        let conn = memory_db();
        seed_document(&conn, "only", "2026-01-01T00:00:00+00:00", 40);

        let sample = sample_for_notebook(&conn, "nb", 20).unwrap();
        assert_eq!(sample.len(), 20);
    }

    #[test]
    fn an_empty_notebook_samples_to_nothing() {
        let conn = memory_db();
        assert!(sample_for_notebook(&conn, "nb", 20).unwrap().is_empty());
    }

    #[test]
    fn a_notebook_sample_ignores_other_notebooks() {
        let conn = memory_db();
        conn.execute(
            "INSERT INTO notebooks (id, name) VALUES ('other', 'Other')",
            [],
        )
        .unwrap();
        seed_document(&conn, "mine", "2026-01-01T00:00:00+00:00", 5);
        conn.execute(
            "INSERT INTO documents (id, notebook_id, title, file_path, file_hash, file_type, file_size)
             VALUES ('theirs', 'other', 'Theirs', '/x', 'h', 'pdf', 1)",
            [],
        )
        .unwrap();
        bulk_create(
            &conn,
            vec![CreateChunk {
                document_id: "theirs".to_string(),
                content: "theirs passage 0".to_string(),
                position: 0,
                page_number: Some(1),
                heading_context: String::new(),
                token_count: 3,
            }],
        )
        .unwrap();

        let sample = sample_for_notebook(&conn, "nb", 20).unwrap();
        assert!(
            sample.iter().all(|passage| passage.starts_with("mine")),
            "a sample must not read another notebook: {sample:?}"
        );
    }

    #[test]
    fn an_explicit_selection_spreads_across_the_chosen_documents() {
        let conn = memory_db();
        seed_document(&conn, "first", "2026-01-01T00:00:00+00:00", 50);
        seed_document(&conn, "second", "2026-01-02T00:00:00+00:00", 50);
        seed_document(&conn, "unchosen", "2026-01-03T00:00:00+00:00", 50);

        let sample =
            sample_for_documents(&conn, &["first".to_string(), "second".to_string()], 10).unwrap();

        assert!(sample.iter().any(|p| p.starts_with("first")));
        assert!(sample.iter().any(|p| p.starts_with("second")));
        assert!(
            !sample.iter().any(|p| p.starts_with("unchosen")),
            "a document the user did not choose must not be read"
        );
    }

    #[test]
    fn chunks_come_back_in_the_order_they_were_stored() {
        let conn = memory_db();
        seed_document(&conn, "doc", "2026-01-01T00:00:00+00:00", 12);

        let stored = get_by_document(&conn, "doc").unwrap();
        let positions: Vec<i32> = stored.iter().map(|c| c.position).collect();
        assert_eq!(positions, (0..12).collect::<Vec<_>>());
        assert_eq!(stored[0].content, "doc passage 0");
        assert_eq!(stored[11].content, "doc passage 11");
    }

    #[test]
    fn deleting_a_document_takes_its_chunks_with_it() {
        let conn = memory_db();
        seed_document(&conn, "doc", "2026-01-01T00:00:00+00:00", 5);
        assert_eq!(count_all(&conn).unwrap(), 5);

        conn.execute("DELETE FROM documents WHERE id = 'doc'", [])
            .unwrap();

        assert_eq!(
            count_all(&conn).unwrap(),
            0,
            "chunks must not outlive the document they came from"
        );
    }
}
