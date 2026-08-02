/*
 * Name: search_service.rs
 * Purpose: Search across document chunks.
 * Description: Combines FTS5 keyword ranking with vector similarity when
 *   embeddings are available (hybrid search). FTS5 with BM25
 *   handles keyword relevance; the embedding service supplies
 *   cosine-similarity hits. The two lists are merged with
 *   reciprocal rank fusion so neither signal dominates. Falls back
 *   to LIKE-based search if the FTS5 table does not exist yet.
 * Tech Stack: Rust, rusqlite, FTS5
 * License: MIT
 * Authors: Amey Thakur (https://github.com/Amey-Thakur)
 *          Archit Konde (https://github.com/Archit-Konde)
 * Date: 2026-07-12
 */

use rusqlite::{params, Connection};
use serde::Serialize;

use crate::error::AppResult;
use crate::services::embedding_service;
use crate::utils::text_utils;

#[derive(Debug, Clone, Serialize)]
pub struct SearchResult {
    pub chunk_id: String,
    pub document_id: String,
    pub document_title: String,
    pub content: String,
    pub heading_context: String,
    pub page_number: Option<i32>,
    pub score: f64,
}

/// Search document chunks by keyword within a notebook's documents.
/// Uses FTS5 with BM25 ranking. Falls back to LIKE if FTS5 is unavailable.
pub fn search_chunks(
    conn: &Connection,
    notebook_id: &str,
    query: &str,
    limit: usize,
) -> AppResult<Vec<SearchResult>> {
    let limit = limit.min(1000);

    /* Try FTS5 first for ranked results */
    match search_fts5(conn, notebook_id, query, limit) {
        Ok(results) if !results.is_empty() => return Ok(results),
        Ok(_) => {}  /* Empty results, fall through to LIKE */
        Err(_) => {} /* FTS5 table might not exist yet */
    }

    /* Fallback: LIKE-based search (slower but always works) */
    search_like(conn, notebook_id, query, limit)
}

/// Hybrid search: merge keyword hits with vector-similarity hits when a query
/// embedding is available. Uses reciprocal rank fusion (k=60) so a chunk that
/// ranks well on either signal surfaces, and one that ranks on both wins.
/// Order two fused results: best score first, chunk id to settle a tie.
///
/// Reciprocal rank fusion ties readily. A passage ranked first by keyword and
/// fourth by vector scores exactly what a passage ranked fourth by keyword and
/// first by vector scores, because the two contributions are the same pair of
/// numbers added in the other order. The fused scores are collected in a
/// HashMap, whose iteration order Rust varies on purpose, so without a
/// tie-break those two passages come back in a different order from one launch
/// to the next. They become the sources a chat answer cites, which would mean
/// the same question answered from different quotes.
fn rank_order(a: &SearchResult, b: &SearchResult) -> std::cmp::Ordering {
    b.score
        .partial_cmp(&a.score)
        .unwrap_or(std::cmp::Ordering::Equal)
        .then_with(|| a.chunk_id.cmp(&b.chunk_id))
}

pub fn search_chunks_hybrid(
    conn: &Connection,
    notebook_id: &str,
    query: &str,
    query_vector: Option<&[f32]>,
    limit: usize,
) -> AppResult<Vec<SearchResult>> {
    let keyword_hits = search_chunks(conn, notebook_id, query, limit)?;

    let Some(vector) = query_vector else {
        return Ok(keyword_hits);
    };

    let vector_hits =
        embedding_service::search_similar(conn, vector, notebook_id, limit).unwrap_or_default();
    if vector_hits.is_empty() {
        return Ok(keyword_hits);
    }

    /* Reciprocal rank fusion over the two ranked lists */
    const RRF_K: f64 = 60.0;
    let mut fused: std::collections::HashMap<String, f64> = std::collections::HashMap::new();

    for (rank, hit) in keyword_hits.iter().enumerate() {
        *fused.entry(hit.chunk_id.clone()).or_insert(0.0) += 1.0 / (RRF_K + rank as f64 + 1.0);
    }
    for (rank, (chunk_id, _score)) in vector_hits.iter().enumerate() {
        *fused.entry(chunk_id.clone()).or_insert(0.0) += 1.0 / (RRF_K + rank as f64 + 1.0);
    }

    /* Load metadata for vector-only hits that keyword search did not return */
    let mut by_id: std::collections::HashMap<String, SearchResult> = keyword_hits
        .into_iter()
        .map(|r| (r.chunk_id.clone(), r))
        .collect();

    for (chunk_id, _) in &vector_hits {
        if !by_id.contains_key(chunk_id) {
            if let Some(result) = load_chunk_result(conn, chunk_id)? {
                by_id.insert(chunk_id.clone(), result);
            }
        }
    }

    let mut merged: Vec<SearchResult> = fused
        .into_iter()
        .filter_map(|(chunk_id, score)| {
            by_id.remove(&chunk_id).map(|mut r| {
                r.score = score;
                r
            })
        })
        .collect();

    /* Chunk id breaks ties, and ties are ordinary here: two passages at the same
    rank in their respective lists score identically, and the fused scores are
    collected in a HashMap, whose iteration order Rust deliberately varies. Sorting
    on score alone therefore left the same query returning the same passages in a
    different order from one run to the next, and, since these become the sources a
    chat answer is built from, the same question quoting different passages. */
    merged.sort_by(rank_order);
    merged.truncate(limit);
    Ok(merged)
}

/// Load a single chunk as a SearchResult (used for vector-only hits).
fn load_chunk_result(conn: &Connection, chunk_id: &str) -> AppResult<Option<SearchResult>> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.document_id, d.title, c.content, c.heading_context, c.page_number
         FROM chunks c
         INNER JOIN documents d ON c.document_id = d.id
         WHERE c.id = ?1",
    )?;

    let result = stmt
        .query_map(params![chunk_id], |row| {
            Ok(SearchResult {
                chunk_id: row.get(0)?,
                document_id: row.get(1)?,
                document_title: row.get(2)?,
                content: row.get(3)?,
                heading_context: row.get(4)?,
                page_number: row.get(5)?,
                score: 0.0,
            })
        })?
        .next()
        .transpose()?;

    Ok(result)
}

/// FTS5-based search with BM25 relevance ranking.
/// Query is sanitized to prevent FTS5 syntax injection (AND, OR, NEAR, etc).
fn search_fts5(
    conn: &Connection,
    notebook_id: &str,
    query: &str,
    limit: usize,
) -> AppResult<Vec<SearchResult>> {
    /* Sanitize: quote each word to force literal matching, strip FTS5 operators */
    let safe_query = sanitize_fts5_query(query);
    if safe_query.is_empty() {
        return Ok(Vec::new());
    }

    let mut stmt = conn.prepare(
        "SELECT c.id, c.document_id, d.title, c.content, c.heading_context, c.page_number,
                bm25(chunks_fts) as rank
         FROM chunks_fts
         INNER JOIN chunks c ON chunks_fts.rowid = c.rowid
         INNER JOIN documents d ON c.document_id = d.id
         WHERE chunks_fts MATCH ?1 AND d.notebook_id = ?2
         ORDER BY rank
         LIMIT ?3",
    )?;

    let results = stmt
        .query_map(params![safe_query, notebook_id, limit as i64], |row| {
            Ok(SearchResult {
                chunk_id: row.get(0)?,
                document_id: row.get(1)?,
                document_title: row.get(2)?,
                content: row.get(3)?,
                heading_context: row.get(4)?,
                page_number: row.get(5)?,
                score: row.get::<_, f64>(6)?.abs(),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(results)
}

/// Sanitize a user query for safe use with FTS5 MATCH.
/// Wraps each word in double quotes to force literal matching, preventing
/// injection of FTS5 operators (AND, OR, NOT, NEAR, *, ^, etc).
fn sanitize_fts5_query(query: &str) -> String {
    query
        .split_whitespace()
        .map(|word| {
            /* Strip quotes and FTS5 special chars, then wrap in quotes */
            let clean: String = word
                .chars()
                .filter(|c| !matches!(c, '"' | '*' | '^' | '{' | '}' | '(' | ')'))
                .collect();
            if clean.is_empty() {
                String::new()
            } else {
                format!("\"{clean}\"")
            }
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

/// LIKE-based fallback search (no ranking, full table scan).
fn search_like(
    conn: &Connection,
    notebook_id: &str,
    query: &str,
    limit: usize,
) -> AppResult<Vec<SearchResult>> {
    let pattern = text_utils::escape_like_pattern(query);

    let mut stmt = conn.prepare(
        "SELECT c.id, c.document_id, d.title, c.content, c.heading_context, c.page_number
         FROM chunks c
         INNER JOIN documents d ON c.document_id = d.id
         WHERE d.notebook_id = ?1 AND c.content LIKE ?2 ESCAPE '\\'
         LIMIT ?3",
    )?;

    let results = stmt
        .query_map(params![notebook_id, pattern, limit as i64], |row| {
            Ok(SearchResult {
                chunk_id: row.get(0)?,
                document_id: row.get(1)?,
                document_title: row.get(2)?,
                content: row.get(3)?,
                heading_context: row.get(4)?,
                page_number: row.get(5)?,
                score: 1.0,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A database with the real schema and the real FTS5 table, holding one
    /// chunk of the given text.
    fn db_with_chunk(content: &str) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        for sql in [
            include_str!("../../resources/migrations/001_initial_schema.sql"),
            include_str!("../../resources/migrations/002_chat_tables.sql"),
            include_str!("../../resources/migrations/003_fts5_search.sql"),
        ] {
            conn.execute_batch(sql).unwrap();
        }
        conn.execute(
            "INSERT INTO notebooks (id, name, created_at, updated_at)
             VALUES ('nb', 'Test', '2026-01-01', '2026-01-01')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO documents (id, notebook_id, title, file_path, file_type, file_hash,
                                    file_size, status, created_at, updated_at)
             VALUES ('doc', 'nb', 'Doc', '(test)', 'txt', 'h', 1, 'processed',
                     '2026-01-01', '2026-01-01')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO chunks (id, document_id, content, position, heading_context,
                                 token_count, created_at)
             VALUES ('c1', 'doc', ?1, 0, '', 10, '2026-01-01')",
            rusqlite::params![content],
        )
        .unwrap();
        conn
    }

    /// A database with several chunks and an embedding for each, so the two
    /// ranked lists that hybrid search fuses both have something in them.
    fn db_with_embedded_chunks(count: usize) -> Connection {
        let conn = db_with_chunk("quantum entanglement appears here");
        conn.execute_batch(include_str!(
            "../../resources/migrations/004_embeddings.sql"
        ))
        .unwrap();
        for i in 0..count {
            let id = format!("c{}", i + 2);
            conn.execute(
                "INSERT INTO chunks (id, document_id, content, position, heading_context,
                                     token_count, created_at)
                 VALUES (?1, 'doc', 'quantum entanglement appears here too', ?2, '', 10, '2026-01-01')",
                rusqlite::params![id, (i + 1) as i32],
            )
            .unwrap();
        }
        /* Identical vectors, so every chunk scores the same and the fused ranks
        tie: exactly the case whose order used to depend on hash iteration. */
        let vector: Vec<f32> = vec![0.5, 0.5, 0.5, 0.5];
        let blob: Vec<u8> = vector.iter().flat_map(|f| f.to_le_bytes()).collect();
        for i in 0..=count {
            let id = if i == 0 {
                "c1".to_string()
            } else {
                format!("c{}", i + 1)
            };
            conn.execute(
                "INSERT INTO embeddings (chunk_id, vector, dimensions, created_at)
                 VALUES (?1, ?2, 4, '2026-01-01')",
                rusqlite::params![id, blob],
            )
            .unwrap();
        }
        conn
    }

    fn result(chunk_id: &str, score: f64) -> SearchResult {
        SearchResult {
            chunk_id: chunk_id.to_string(),
            document_id: "doc".to_string(),
            document_title: "Doc".to_string(),
            content: String::new(),
            heading_context: String::new(),
            page_number: None,
            score,
        }
    }

    #[test]
    fn equal_scores_are_ordered_by_chunk_id() {
        /* Reciprocal rank fusion produces exact ties: first-by-keyword and
        fourth-by-vector scores the same as fourth-by-keyword and first-by-vector,
        being the same two numbers added the other way round. The fused scores sit
        in a HashMap, whose iteration order Rust varies deliberately, so without
        this rule those two passages swap places between launches, and they are
        the sources a chat answer quotes.

        The rule is tested directly rather than through a search. Repeating a
        search in one process proves nothing, because the hash seed is chosen once
        per process and every call in a single run therefore agrees with itself. */
        let tied = 1.0 / 61.0 + 1.0 / 64.0;
        let mut results = vec![
            result("zzz", tied),
            result("aaa", tied),
            result("mmm", tied),
        ];
        results.sort_by(rank_order);
        assert_eq!(
            results
                .iter()
                .map(|r| r.chunk_id.as_str())
                .collect::<Vec<_>>(),
            ["aaa", "mmm", "zzz"],
            "tied passages must be ordered by chunk id, not by hash order"
        );
    }

    #[test]
    fn a_better_score_still_wins_over_the_tie_break() {
        let mut results = vec![result("aaa", 0.01), result("zzz", 0.99)];
        results.sort_by(rank_order);
        assert_eq!(
            results
                .iter()
                .map(|r| r.chunk_id.as_str())
                .collect::<Vec<_>>(),
            ["zzz", "aaa"],
            "the chunk id must only settle ties, never outrank a score"
        );
    }

    #[test]
    fn hybrid_search_without_a_query_vector_is_plain_keyword_search() {
        let conn = db_with_embedded_chunks(3);
        let hits = search_chunks_hybrid(&conn, "nb", "quantum", None, 5).unwrap();
        assert!(!hits.is_empty(), "keyword search should still answer");
    }

    #[test]
    fn english_search_finds_a_word() {
        let conn = db_with_chunk("Retrieval augmented generation grounds every answer.");
        let hits = search_chunks(&conn, "nb", "augmented", 10).unwrap();
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn a_japanese_substring_is_still_found() {
        /* FTS5's default tokenizer treats a run of CJK as a single token, so a
        search for part of a compound matches nothing through the index. The
        LIKE fallback is what rescues it, and this test exists so that fallback
        is never optimised away as redundant: without it, search and chat return
        nothing at all for these languages. */
        let conn = db_with_chunk("\u{6a5f}\u{68b0}\u{5b66}\u{7fd2}\u{306f}\u{4eba}\u{5de5}\u{77e5}\u{80fd}\u{306e}\u{4e00}\u{5206}\u{91ce}\u{3067}\u{3059}");
        let hits = search_chunks(&conn, "nb", "\u{6a5f}\u{68b0}", 10).unwrap();
        assert_eq!(hits.len(), 1, "a partial CJK match must still be found");
    }

    #[test]
    fn search_is_scoped_to_its_notebook() {
        /* Leaking another notebook's passages into an answer would be a privacy
        failure, not just a relevance one. */
        let conn = db_with_chunk("Retrieval augmented generation.");
        let hits = search_chunks(&conn, "other-notebook", "augmented", 10).unwrap();
        assert!(hits.is_empty());
    }

    #[test]
    fn a_query_of_only_operators_finds_nothing_and_does_not_error() {
        /* The sanitiser empties this, and the LIKE fallback then runs with a
        wildcard-free pattern. Neither may raise. */
        let conn = db_with_chunk("Retrieval augmented generation.");
        assert!(search_chunks(&conn, "nb", "***", 10).unwrap().is_empty());
    }

    #[test]
    fn a_like_wildcard_in_the_query_is_not_treated_as_a_wildcard() {
        /* Otherwise a search for "%" returns every chunk in the notebook. */
        let conn = db_with_chunk("Retrieval augmented generation.");
        assert!(search_chunks(&conn, "nb", "%", 10).unwrap().is_empty());
        assert!(search_chunks(&conn, "nb", "_", 10).unwrap().is_empty());
    }

    #[test]
    fn ordinary_words_become_quoted_phrases() {
        /* Quoting is what forces literal matching. Without it every word is
        parsed as FTS5 syntax. */
        assert_eq!(
            sanitize_fts5_query("prompt engineering"),
            "\"prompt\" \"engineering\""
        );
    }

    #[test]
    fn a_double_quote_cannot_escape_the_quoting() {
        /* The one character that matters. If it survived, a query could close
        the phrase and write its own FTS5 expression. */
        let out = sanitize_fts5_query("foo\" OR \"bar");
        assert!(!out.contains("\"\""), "no empty phrase: {out}");
        assert_eq!(out, "\"foo\" \"OR\" \"bar\"");
    }

    #[test]
    fn operators_are_stripped_or_neutralised() {
        /* Prefix and column syntax must not survive as syntax. */
        assert_eq!(sanitize_fts5_query("data*"), "\"data\"");
        assert_eq!(sanitize_fts5_query("^start"), "\"start\"");
        assert_eq!(sanitize_fts5_query("(a)"), "\"a\"");
        /* A colon is a column filter outside quotes and a literal inside one,
        so quoting is enough and the character can be kept. */
        assert_eq!(sanitize_fts5_query("content:secret"), "\"content:secret\"");
    }

    #[test]
    fn boolean_keywords_are_searched_for_literally() {
        /* Someone searching for the word "and" should find it, not run a
        conjunction. */
        assert_eq!(
            sanitize_fts5_query("cats AND dogs"),
            "\"cats\" \"AND\" \"dogs\""
        );
        assert_eq!(sanitize_fts5_query("a NOT b"), "\"a\" \"NOT\" \"b\"");
        assert_eq!(sanitize_fts5_query("x NEAR y"), "\"x\" \"NEAR\" \"y\"");
    }

    #[test]
    fn a_query_of_only_operators_comes_back_empty() {
        /* The caller returns no results for an empty query rather than passing
        an empty MATCH to SQLite, which is an error. */
        assert_eq!(sanitize_fts5_query("***"), "");
        assert_eq!(sanitize_fts5_query("\"\""), "");
        assert_eq!(sanitize_fts5_query("   "), "");
        assert_eq!(sanitize_fts5_query(""), "");
    }

    #[test]
    fn hyphens_and_apostrophes_survive() {
        /* Both are ordinary in real queries and harmless inside a phrase. A
        hyphen outside quotes would be a NOT. */
        assert_eq!(sanitize_fts5_query("well-known"), "\"well-known\"");
        assert_eq!(sanitize_fts5_query("it's"), "\"it's\"");
    }

    #[test]
    fn non_ascii_is_preserved() {
        /* Stripping by byte rather than by character would corrupt these. */
        assert_eq!(sanitize_fts5_query("café"), "\"café\"");
        assert_eq!(sanitize_fts5_query("日本語"), "\"日本語\"");
    }
}
