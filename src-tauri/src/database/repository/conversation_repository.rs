/*
 * Name: conversation_repository.rs
 * Purpose: Data access layer for chat conversations and messages.
 * Description: Messages are ordered chronologically within a conversation.
 *   Citations link assistant messages to source chunks for RAG
 *   attribution.
 * Tech Stack: Rust, rusqlite
 * License: MIT
 * Authors: Amey Thakur (https://github.com/Amey-Thakur)
 *          Archit Konde (https://github.com/Archit-Konde)
 * Date: 2026-07-12
 */

use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::database::models::{Conversation, CreateConversation, Message};
use crate::error::{AppError, AppResult};

pub fn create_conversation(
    conn: &Connection,
    input: CreateConversation,
) -> AppResult<Conversation> {
    let id = Uuid::now_v7().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let title = input.title.unwrap_or_else(|| "New Chat".to_string());

    conn.execute(
        "INSERT INTO conversations (id, notebook_id, title, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, input.notebook_id, title, now, now],
    )?;

    get_conversation(conn, &id)
}

pub fn get_conversation(conn: &Connection, id: &str) -> AppResult<Conversation> {
    conn.query_row(
        "SELECT id, notebook_id, title, created_at, updated_at
         FROM conversations WHERE id = ?1",
        params![id],
        Conversation::from_row,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => {
            AppError::NotFound(format!("Conversation not found: {id}"))
        }
        other => AppError::Database(other),
    })
}

pub fn list_by_notebook(conn: &Connection, notebook_id: &str) -> AppResult<Vec<Conversation>> {
    let mut stmt = conn.prepare(
        "SELECT id, notebook_id, title, created_at, updated_at
         FROM conversations WHERE notebook_id = ?1 ORDER BY updated_at DESC",
    )?;

    let convos = stmt
        .query_map(params![notebook_id], Conversation::from_row)?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(convos)
}

pub fn add_message(
    conn: &Connection,
    conversation_id: &str,
    role: &str,
    content: &str,
) -> AppResult<Message> {
    let id = Uuid::now_v7().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    conn.execute(
        "INSERT INTO messages (id, conversation_id, role, content, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![id, conversation_id, role, content, now],
    )?;

    /* Update conversation's updated_at timestamp */
    conn.execute(
        "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
        params![now, conversation_id],
    )?;

    conn.query_row(
        "SELECT id, conversation_id, role, content, created_at
         FROM messages WHERE id = ?1",
        params![id],
        Message::from_row,
    )
    .map_err(AppError::Database)
}

pub fn get_messages(conn: &Connection, conversation_id: &str) -> AppResult<Vec<Message>> {
    /* rowid breaks ties, and ties are reachable: the timestamp is taken from the
    wall clock, so two messages written inside the same tick carry the same one
    and the order between them would otherwise be whatever the query planner
    happened to produce. rowid is insertion order, which for a conversation is
    the order the messages were sent. */
    let mut stmt = conn.prepare(
        "SELECT id, conversation_id, role, content, created_at
         FROM messages WHERE conversation_id = ?1 ORDER BY created_at ASC, rowid ASC",
    )?;

    let msgs = stmt
        .query_map(params![conversation_id], Message::from_row)?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(msgs)
}

/// Get the most recent N messages for a conversation (for RAG context window).
/// Returns messages in chronological order (oldest first).
pub fn get_recent_messages(
    conn: &Connection,
    conversation_id: &str,
    limit: usize,
) -> AppResult<Vec<Message>> {
    /* Both orderings tie-break on rowid, and they must break it the same way
    round: the inner one decides which messages the model gets to remember, the
    outer one decides what order it reads them in. A tie resolved differently in
    the two would drop one message and duplicate its neighbour. */
    let mut stmt = conn.prepare(
        "SELECT id, conversation_id, role, content, created_at
         FROM (
             SELECT id, conversation_id, role, content, created_at, rowid
             FROM messages
             WHERE conversation_id = ?1
             ORDER BY created_at DESC, rowid DESC
             LIMIT ?2
         ) ORDER BY created_at ASC, rowid ASC",
    )?;

    let msgs = stmt
        .query_map(params![conversation_id, limit as i64], Message::from_row)?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(msgs)
}

pub fn add_citation(
    conn: &Connection,
    message_id: &str,
    chunk_id: &str,
    relevance_score: f64,
) -> AppResult<()> {
    let id = Uuid::now_v7().to_string();

    conn.execute(
        "INSERT INTO citations (id, message_id, chunk_id, relevance_score)
         VALUES (?1, ?2, ?3, ?4)",
        params![id, message_id, chunk_id, relevance_score],
    )?;

    Ok(())
}

/// A citation joined with its chunk and document so the chat UI can render
/// a meaningful source chip (document title, heading, page, snippet).
#[derive(Debug, serde::Serialize)]
pub struct CitationSource {
    pub chunk_id: String,
    pub document_id: String,
    pub document_title: String,
    pub heading_context: String,
    pub page_number: Option<i32>,
    pub snippet: String,
    pub relevance_score: f64,
}

/// Fetch citations for a message with document context for display.
/// The snippet is capped in SQL so large chunks never cross the IPC boundary.
pub fn get_citation_sources(conn: &Connection, message_id: &str) -> AppResult<Vec<CitationSource>> {
    let mut stmt = conn.prepare(
        "SELECT ch.id, d.id, d.title, ch.heading_context, ch.page_number,
                substr(ch.content, 1, 240), c.relevance_score
         FROM citations c
         INNER JOIN chunks ch ON c.chunk_id = ch.id
         INNER JOIN documents d ON ch.document_id = d.id
         WHERE c.message_id = ?1
         ORDER BY c.relevance_score DESC",
    )?;

    let sources = stmt
        .query_map(params![message_id], |row| {
            Ok(CitationSource {
                chunk_id: row.get(0)?,
                document_id: row.get(1)?,
                document_title: row.get(2)?,
                heading_context: row.get(3)?,
                page_number: row.get(4)?,
                snippet: row.get(5)?,
                relevance_score: row.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(sources)
}

pub fn delete_conversation(conn: &Connection, id: &str) -> AppResult<()> {
    let affected = conn.execute("DELETE FROM conversations WHERE id = ?1", params![id])?;

    if affected == 0 {
        return Err(AppError::NotFound(format!("Conversation not found: {id}")));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An in-memory database with the real schema, so these exercise the same
    /// statements and constraints the app runs against. Foreign keys are off by
    /// default in SQLite, and every cascade in the chat tables depends on them.
    fn memory_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn.execute_batch(include_str!(
            "../../../resources/migrations/001_initial_schema.sql"
        ))
        .unwrap();
        conn.execute_batch(include_str!(
            "../../../resources/migrations/002_chat_tables.sql"
        ))
        .unwrap();
        conn.execute(
            "INSERT INTO notebooks (id, name) VALUES ('nb', 'Notebook')",
            [],
        )
        .unwrap();
        conn
    }

    fn new_conversation(conn: &Connection) -> Conversation {
        create_conversation(
            conn,
            CreateConversation {
                notebook_id: "nb".to_string(),
                title: None,
            },
        )
        .unwrap()
    }

    #[test]
    fn a_conversation_without_a_title_gets_a_usable_one() {
        /* The sidebar renders the title, so an empty one is an unclickable row. */
        let conn = memory_db();
        let convo = new_conversation(&conn);
        assert!(!convo.title.trim().is_empty());
    }

    #[test]
    fn messages_come_back_in_the_order_they_were_sent() {
        let conn = memory_db();
        let convo = new_conversation(&conn);
        for turn in 0..6 {
            let role = if turn % 2 == 0 { "user" } else { "assistant" };
            add_message(&conn, &convo.id, role, &format!("turn {turn}")).unwrap();
        }

        let messages = get_messages(&conn, &convo.id).unwrap();
        let contents: Vec<String> = messages.into_iter().map(|m| m.content).collect();
        let expected: Vec<String> = (0..6).map(|turn| format!("turn {turn}")).collect();
        assert_eq!(contents, expected, "chat history must read in order");
    }

    #[test]
    fn the_recent_window_keeps_the_newest_messages_in_reading_order() {
        /* This feeds the model its memory of the conversation. Taking the newest
        messages requires ordering descending, and handing them to the model
        requires ordering back ascending: getting the second half wrong reverses
        the conversation without failing anything. */
        let conn = memory_db();
        let convo = new_conversation(&conn);
        for turn in 0..10 {
            add_message(&conn, &convo.id, "user", &format!("turn {turn}")).unwrap();
        }

        let recent = get_recent_messages(&conn, &convo.id, 4).unwrap();
        let contents: Vec<String> = recent.into_iter().map(|m| m.content).collect();
        assert_eq!(
            contents,
            vec!["turn 6", "turn 7", "turn 8", "turn 9"],
            "the window must be the newest messages, oldest first"
        );
    }

    #[test]
    fn the_recent_window_copes_with_a_shorter_conversation() {
        let conn = memory_db();
        let convo = new_conversation(&conn);
        add_message(&conn, &convo.id, "user", "only one").unwrap();

        let recent = get_recent_messages(&conn, &convo.id, 20).unwrap();
        assert_eq!(recent.len(), 1);
    }

    #[test]
    fn a_new_message_moves_its_conversation_to_the_top() {
        /* The list is ordered by last activity, which is the only reason the
        conversation being typed in stays reachable. */
        let conn = memory_db();
        let first = new_conversation(&conn);
        let second = new_conversation(&conn);

        /* The timestamps are written by the code under test, so nudge the older
        conversation into the past rather than sleeping for a real second. */
        conn.execute(
            "UPDATE conversations SET updated_at = '2020-01-01T00:00:00+00:00' WHERE id = ?1",
            params![first.id],
        )
        .unwrap();

        let listed = list_by_notebook(&conn, "nb").unwrap();
        assert_eq!(listed[0].id, second.id, "newest activity sorts first");

        add_message(&conn, &first.id, "user", "reviving it").unwrap();
        let listed = list_by_notebook(&conn, "nb").unwrap();
        assert_eq!(listed[0].id, first.id, "a reply must resurface its thread");
    }

    #[test]
    fn deleting_a_conversation_takes_its_messages_and_citations_with_it() {
        /* Without foreign keys enabled these rows survive their conversation and
        accumulate in the database with nothing able to reach them again. */
        let conn = memory_db();
        let convo = new_conversation(&conn);
        let message = add_message(&conn, &convo.id, "assistant", "An answer").unwrap();
        conn.execute(
            "INSERT INTO documents (id, notebook_id, title, file_path, file_hash, file_type, file_size)
             VALUES ('doc', 'nb', 'A document', '/x', 'h', 'pdf', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO chunks (id, document_id, content, position) VALUES ('ch', 'doc', 'A passage', 0)",
            [],
        )
        .unwrap();
        add_citation(&conn, &message.id, "ch", 0.9).unwrap();

        delete_conversation(&conn, &convo.id).unwrap();

        let messages: i64 = conn
            .query_row("SELECT COUNT(*) FROM messages", [], |row| row.get(0))
            .unwrap();
        let citations: i64 = conn
            .query_row("SELECT COUNT(*) FROM citations", [], |row| row.get(0))
            .unwrap();
        assert_eq!(messages, 0, "messages must not outlive their conversation");
        assert_eq!(citations, 0, "citations must not outlive their message");
    }

    #[test]
    fn deleting_a_conversation_that_is_already_gone_says_so() {
        let conn = memory_db();
        let err = delete_conversation(&conn, "no-such-id").unwrap_err();
        assert!(
            matches!(err, AppError::NotFound(_)),
            "expected NotFound, got {err:?}"
        );
    }

    #[test]
    fn a_missing_conversation_is_not_reported_as_a_database_failure() {
        /* NotFound reaches the user as a plain message; a Database error reaches
        them as a fault in the app. */
        let conn = memory_db();
        let err = get_conversation(&conn, "no-such-id").unwrap_err();
        assert!(
            matches!(err, AppError::NotFound(_)),
            "expected NotFound, got {err:?}"
        );
    }

    #[test]
    fn citations_carry_their_document_and_arrive_best_first() {
        let conn = memory_db();
        let convo = new_conversation(&conn);
        let message = add_message(&conn, &convo.id, "assistant", "An answer").unwrap();
        conn.execute(
            "INSERT INTO documents (id, notebook_id, title, file_path, file_hash, file_type, file_size)
             VALUES ('doc', 'nb', 'Cited document', '/x', 'h', 'pdf', 1)",
            [],
        )
        .unwrap();
        for (id, score) in [("weak", 0.1), ("strong", 0.9), ("middling", 0.5)] {
            conn.execute(
                "INSERT INTO chunks (id, document_id, content, position, page_number, heading_context)
                 VALUES (?1, 'doc', ?2, 0, 4, 'A heading')",
                params![id, format!("passage {id}")],
            )
            .unwrap();
            add_citation(&conn, &message.id, id, score).unwrap();
        }

        let sources = get_citation_sources(&conn, &message.id).unwrap();
        assert_eq!(
            sources.iter().map(|s| s.chunk_id.as_str()).collect::<Vec<_>>(),
            vec!["strong", "middling", "weak"],
            "the strongest source must be offered first"
        );
        assert_eq!(sources[0].document_title, "Cited document");
        assert_eq!(sources[0].page_number, Some(4));
        assert_eq!(sources[0].heading_context, "A heading");
    }

    #[test]
    fn a_citation_snippet_is_capped_before_it_crosses_the_boundary() {
        /* A chunk can be thousands of characters. The chip shows a preview, so
        the cap is what stops the whole chunk being serialised for every source
        on every message. */
        let conn = memory_db();
        let convo = new_conversation(&conn);
        let message = add_message(&conn, &convo.id, "assistant", "An answer").unwrap();
        conn.execute(
            "INSERT INTO documents (id, notebook_id, title, file_path, file_hash, file_type, file_size)
             VALUES ('doc', 'nb', 'A document', '/x', 'h', 'pdf', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO chunks (id, document_id, content, position) VALUES ('ch', 'doc', ?1, 0)",
            params!["x".repeat(5000)],
        )
        .unwrap();
        add_citation(&conn, &message.id, "ch", 0.5).unwrap();

        let sources = get_citation_sources(&conn, &message.id).unwrap();
        assert_eq!(sources[0].snippet.chars().count(), 240);
    }

    #[test]
    fn conversations_stay_inside_their_own_notebook() {
        let conn = memory_db();
        conn.execute(
            "INSERT INTO notebooks (id, name) VALUES ('other', 'Other')",
            [],
        )
        .unwrap();
        new_conversation(&conn);
        create_conversation(
            &conn,
            CreateConversation {
                notebook_id: "other".to_string(),
                title: Some("Elsewhere".to_string()),
            },
        )
        .unwrap();

        assert_eq!(list_by_notebook(&conn, "nb").unwrap().len(), 1);
        assert_eq!(list_by_notebook(&conn, "other").unwrap().len(), 1);
    }
}
