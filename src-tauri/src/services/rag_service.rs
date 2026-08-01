/*
 * Name: rag_service.rs
 * Purpose: RAG (Retrieval-Augmented Generation) pipeline.
 * Description: Orchestrates: search chunks -> assemble context -> call LLM ->
 *   save response + citations. The pipeline is split into phases
 *   to minimize lock duration. Phase 1 (DB read): search + history
 *   fetch. Phase 2 (no locks): LLM call. Phase 3 (DB write): save
 *   response + citations. Conversation history is capped to the
 *   last 20 messages to stay within model context windows.
 * Tech Stack: Rust
 * License: MIT
 * Authors: Amey Thakur (https://github.com/Amey-Thakur)
 *          Archit Konde (https://github.com/Archit-Konde)
 * Date: 2026-07-12
 */

use rusqlite::Connection;

use crate::database::models::CreateConversation;
use crate::database::repository::{conversation_repository, note_repository};
use crate::error::{AppError, AppResult};
use crate::providers::{ChatMessage, ChatRequest, MessageRole, ProviderRouter, TaskPurpose};
use crate::services::search_service;

const RAG_SYSTEM_PROMPT: &str = include_str!("../../resources/prompts/rag-system.txt");
const RAG_TOP_K: usize = 10;
const MAX_HISTORY_MESSAGES: usize = 20;
/* Headroom for tokenizer variance, since chars/4 is an estimate. */
const SAFETY_MARGIN_TOKENS: u32 = 256;
/* Below this there is no room for a passage worth reading, so no sources are
sent at all rather than a fragment that crowds out the system prompt. */
const MIN_USEFUL_SOURCE_TOKENS: u32 = 256;
/* The note map stays small: it is a hint, not the payload. */
const NOTE_MAP_MAX_EDGES: usize = 40;

/// Rough token estimate, used only for context budgeting. Never for the usage
/// display, which shows real numbers reported by providers.
///
/// Bytes divided by four is a fair rule for English. It is not for CJK, where a
/// character is three bytes and roughly one token, so the same rule guesses
/// three quarters of the real cost and the prompt is built too large. Since the
/// only consequence of over-running is the server silently truncating the system
/// prompt away, the estimate has to err high on that text, not low.
fn estimate_tokens(text: &str) -> u32 {
    let mut bytes = 0usize;
    let mut wide = 0usize;
    for c in text.chars() {
        let len = c.len_utf8();
        if len >= 3 {
            /* Roughly one token each: count directly rather than by byte. */
            wide += 1;
        } else {
            bytes += len;
        }
    }
    (bytes / 4 + wide + 1) as u32
}

/// A retrieved source chunk with its real relevance score, kept so citations
/// reflect the retrieval ranking instead of invented numbers.
pub struct RetrievedSource {
    pub chunk_id: String,
    pub score: f64,
}

/// Collected data from Phase 1 (DB read) needed for LLM call.
pub struct RagContext {
    pub messages: Vec<ChatMessage>,
    pub sources: Vec<RetrievedSource>,
}

/// Phase 1: Read from database (needs db lock). Returns context for LLM call.
/// `query_vector` is the embedded user question when the active provider
/// supports embeddings; hybrid search then blends semantic and keyword hits.
pub fn prepare_rag_context(
    conn: &Connection,
    conversation_id: &str,
    notebook_id: &str,
    user_message: &str,
    query_vector: Option<&[f32]>,
    context_window: u32,
    answer_tokens: u32,
) -> AppResult<RagContext> {
    /* Verify conversation belongs to this notebook */
    let convo = conversation_repository::get_conversation(conn, conversation_id)?;
    if convo.notebook_id != notebook_id {
        return Err(AppError::InvalidInput(
            "Conversation does not belong to this notebook".into(),
        ));
    }

    /* Save the user message */
    conversation_repository::add_message(conn, conversation_id, "user", user_message)?;

    /* Search for relevant chunks (hybrid when a query embedding exists) */
    let search_results = search_service::search_chunks_hybrid(
        conn,
        notebook_id,
        user_message,
        query_vector,
        RAG_TOP_K,
    )?;

    /* Build LLM messages: system, then prior conversation, then the retrieved
    context and the current question together as the final user turn. */
    let mut messages = vec![ChatMessage {
        role: MessageRole::System,
        content: RAG_SYSTEM_PROMPT.to_string(),
    }];

    /* Prior conversation. Fetch MAX+1 because the just-saved user message is in
    the DB; drop it here so it does not duplicate the current turn added below. */
    let history = conversation_repository::get_recent_messages(
        conn,
        conversation_id,
        MAX_HISTORY_MESSAGES + 1,
    )?;
    let history_without_current = if history
        .last()
        .map(|m| m.role == "user" && m.content == user_message)
        .unwrap_or(false)
    {
        &history[..history.len() - 1]
    } else {
        &history
    };
    for msg in history_without_current {
        messages.push(ChatMessage {
            role: if msg.role == "user" {
                MessageRole::User
            } else {
                MessageRole::Assistant
            },
            content: msg.content.clone(),
        });
    }

    /* A compact map of how the notebook's notes link, so the model starts
    with the shape of this body of work, not just isolated passages. */
    let note_map = build_note_map(conn, notebook_id);

    /* Pack retrieved chunks into what the model's context window actually
    has room for: window minus the fixed costs (system prompt, history, the
    question, the note map, the reserved answer room). Chunks arrive ranked
    by relevance, so filling in order keeps the best ones. Citations are
    built ONLY from the chunks that made it in, so every citation refers to
    something the model really saw. */
    let fixed_cost = estimate_tokens(RAG_SYSTEM_PROMPT)
        + messages[1..]
            .iter()
            .map(|m| estimate_tokens(&m.content))
            .sum::<u32>()
        + estimate_tokens(user_message)
        + estimate_tokens(&note_map)
        + answer_tokens
        + SAFETY_MARGIN_TOKENS;
    /* What is genuinely left for sources.

    This used to be `saturating_sub(fixed_cost).max(512)`. The floor was meant to
    guarantee some context, but it did the opposite where it mattered: once a long
    conversation pushed the fixed cost past the window, the subtraction saturated
    to zero and the floor then added 512 tokens of sources on top of a prompt that
    was already too big. The server does not reject that, it truncates from the
    front, and the front is the system prompt, so the model loses the instruction
    to cite anything at exactly the moment the prompt is most crowded.

    Nothing is better than something here: dropping sources costs grounding for
    one turn, while overrunning costs the rules for the whole answer. */
    let mut budget = context_window.saturating_sub(fixed_cost);
    if budget < MIN_USEFUL_SOURCE_TOKENS {
        /* Not enough room for even one worthwhile passage. */
        budget = 0;
    }

    let mut context_blocks: Vec<String> = Vec::new();
    let mut sources: Vec<RetrievedSource> = Vec::new();
    for (i, r) in search_results.iter().enumerate() {
        let source_label = if let Some(page) = r.page_number {
            format!(
                "[Source {}: document='{}', heading='{}', page={}]",
                i + 1,
                r.document_title,
                r.heading_context,
                page
            )
        } else {
            format!(
                "[Source {}: document='{}', heading='{}']",
                i + 1,
                r.document_title,
                r.heading_context
            )
        };
        let block = format!("{}\n{}\n", source_label, r.content);
        let cost = estimate_tokens(&block);
        /* Stop rather than truncate mid-passage. The `!is_empty()` exception
        that used to be here let the highest-ranked chunk in whatever its size,
        so a single long passage could overrun the window on its own. */
        if cost > budget {
            break;
        }
        budget = budget.saturating_sub(cost);
        context_blocks.push(block);
        sources.push(RetrievedSource {
            chunk_id: r.chunk_id.clone(),
            score: r.score,
        });
    }
    let context = context_blocks.join("\n---\n");

    /* The current question is the final user turn. When retrieval found
    context, carry it in the same message at User role, so injected document
    text stays below system-instruction privilege. Without this final turn the
    model would receive context and history but never the actual question. */
    let final_turn = match (context.is_empty(), note_map.is_empty()) {
        (true, true) => user_message.to_string(),
        (true, false) => {
            format!("<document_context>\n{note_map}\n</document_context>\n\n{user_message}")
        }
        (false, _) => format!(
            "<document_context>\n{context}{}\n</document_context>\n\nBased on these documents, answer the following question:\n\n{user_message}",
            if note_map.is_empty() {
                String::new()
            } else {
                format!("\n---\n{note_map}")
            }
        ),
    };
    messages.push(ChatMessage {
        role: MessageRole::User,
        content: final_turn,
    });

    Ok(RagContext { messages, sources })
}

/// A one-line-per-link summary of the notebook's note connections, capped
/// small. Gives the model the lay of the land in a handful of tokens; empty
/// when the notebook has no linked notes.
fn build_note_map(conn: &Connection, notebook_id: &str) -> String {
    let Ok(graph) = note_repository::notes_graph(conn, notebook_id) else {
        return String::new();
    };
    if graph.edges.is_empty() {
        return String::new();
    }

    let titles: std::collections::HashMap<&str, &str> = graph
        .nodes
        .iter()
        .map(|n| (n.id.as_str(), n.title.as_str()))
        .collect();

    let links: Vec<String> = graph
        .edges
        .iter()
        .filter_map(|e| {
            let from = titles.get(e.source.as_str())?;
            let to = titles.get(e.target.as_str())?;
            Some(format!("{from} -> {to}"))
        })
        .take(NOTE_MAP_MAX_EDGES)
        .collect();

    if links.is_empty() {
        return String::new();
    }
    format!(
        "[Note map: how this notebook's notes link together]\n{}",
        links.join("; ")
    )
}

/// Most an answer from a model on this computer is allowed to run to.
///
/// The same reasoning as the generation features: time to answer scales with
/// tokens written, and a small model on a CPU writes slowly enough that a
/// 2048-token ceiling is minutes of silence. A chat answer rarely needs to be
/// that long anyway, and a shorter reply that arrives beats a longer one that
/// times out.
const LOCAL_CHAT_MAX_TOKENS: u32 = 900;

/// What a hosted model is allowed to write.
const CLOUD_CHAT_MAX_TOKENS: u32 = 2048;

/// How many tokens to set aside for the answer when packing the context.
///
/// This has to be the number the request will actually ask for. It was a
/// constant 2048 while local models were capped at 900, so on every local model
/// more than a thousand tokens of context window were reserved for an answer
/// that could never use them, and passages that would have fitted were dropped.
/// Retrieval got quietly worse on exactly the setup the app is built around.
///
/// One function decides it, and both the packing and the request read it here.
pub fn answer_allowance(is_local: bool) -> u32 {
    if is_local {
        LOCAL_CHAT_MAX_TOKENS
    } else {
        CLOUD_CHAT_MAX_TOKENS
    }
}

/// Phase 2: Call LLM provider (no locks needed).
pub fn call_llm(providers: &ProviderRouter, context: &RagContext) -> AppResult<String> {
    call_llm_streaming(providers, context, &mut |_| {})
}

/// Phase 2, reporting the answer as it is written.
///
/// Chat is where waiting is felt most: the user has asked a question and is
/// watching for the reply. Streaming turns minutes of nothing into words
/// appearing, without changing what is finally saved.
pub fn call_llm_streaming(
    providers: &ProviderRouter,
    context: &RagContext,
    on_token: &mut dyn FnMut(&str),
) -> AppResult<String> {
    let (_, is_local) = providers.active_profile();
    let request = ChatRequest {
        messages: context.messages.clone(),
        max_tokens: Some(answer_allowance(is_local)),
        temperature: Some(0.3),
        purpose: TaskPurpose::Balanced,
    };

    let streaming = providers.active_supports_streaming();
    let outcome = if streaming {
        providers.stream_chat_completion(request.clone(), on_token)
    } else {
        providers.chat_completion(request.clone())
    };

    /* A stream that fails, or ends having said nothing, is retried without it.
    Some servers advertise the endpoint and then behave differently under
    `stream: true`, and the user should get their answer either way. */
    let response = match outcome {
        Ok(response) if !response.content.trim().is_empty() => response,
        other => if streaming {
            tracing::warn!("Chat stream unusable, retrying without streaming");
            providers.chat_completion(request)
        } else {
            other
        }
        .map_err(|e| AppError::Provider(e.to_string()))?,
    };

    Ok(crate::utils::text_utils::strip_wrapper_tags(&response.content).to_string())
}

/// Phase 3: Save response and citations (needs db lock).
pub fn save_response(
    conn: &Connection,
    conversation_id: &str,
    response_content: &str,
    sources: &[RetrievedSource],
) -> AppResult<String> {
    let assistant_msg =
        conversation_repository::add_message(conn, conversation_id, "assistant", response_content)?;

    /* Store real retrieval scores for the top sources that shaped the answer */
    for source in sources.iter().take(5) {
        conversation_repository::add_citation(
            conn,
            &assistant_msg.id,
            &source.chunk_id,
            source.score,
        )?;
    }

    Ok(assistant_msg.id)
}

/// Start a new RAG conversation in a notebook.
pub fn start_conversation(
    conn: &Connection,
    notebook_id: &str,
    title: Option<String>,
) -> AppResult<String> {
    /* Validate notebook exists by trying to read it */
    crate::database::repository::notebook_repository::get_by_id(conn, notebook_id)?;

    let convo = conversation_repository::create_conversation(
        conn,
        CreateConversation {
            notebook_id: notebook_id.to_string(),
            title,
        },
    )?;

    Ok(convo.id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_local_answer_is_shorter_than_a_hosted_one() {
        assert!(answer_allowance(true) < answer_allowance(false));
    }

    #[test]
    fn the_allowance_is_what_the_request_will_ask_for() {
        /* This is the bug this function exists to prevent. The packing reserved
        a constant 2048 while a local request asked for 900, so on every local
        model more than a thousand tokens of context window were held back for an
        answer that could never use them, and source passages that would have
        fitted were dropped instead. Both sides read this now, so they cannot
        drift apart again without this test failing. */
        assert_eq!(answer_allowance(true), LOCAL_CHAT_MAX_TOKENS);
        assert_eq!(answer_allowance(false), CLOUD_CHAT_MAX_TOKENS);
    }

    #[test]
    fn the_allowance_leaves_room_in_a_small_window() {
        /* A 4k window is the smallest thing anyone runs. Reserving the answer
        plus the margin must still leave usable room for sources, or retrieval
        returns nothing and every answer is ungrounded. */
        let smallest_window = 4096;
        let reserved = answer_allowance(true) + SAFETY_MARGIN_TOKENS;
        assert!(
            reserved < smallest_window / 2,
            "reserved {reserved} of {smallest_window} leaves too little for sources"
        );
    }

    /// The budget rule, extracted so it can be exercised without a database.
    fn source_budget(context_window: u32, fixed_cost: u32) -> u32 {
        let budget = context_window.saturating_sub(fixed_cost);
        if budget < MIN_USEFUL_SOURCE_TOKENS {
            0
        } else {
            budget
        }
    }

    #[test]
    fn a_crowded_window_sends_no_sources_rather_than_overrunning() {
        /* The old floor returned 512 here, which added sources to a prompt that
        was already over the window. The server truncates from the front, so the
        system prompt went first and the model lost the instruction to cite. */
        assert_eq!(source_budget(4096, 5000), 0);
        assert_eq!(source_budget(4096, 4096), 0);
        assert_eq!(source_budget(4096, 4000), 0, "96 spare is not a passage");
    }

    #[test]
    fn a_roomy_window_spends_what_is_left() {
        assert_eq!(source_budget(8192, 2000), 6192);
        assert_eq!(source_budget(128_000, 3000), 125_000);
    }

    #[test]
    fn the_allowance_change_widens_the_budget_on_a_local_model() {
        /* The concrete gain from matching the reservation to the request: on the
        conservative 4096 window every local chat gets this much more room for
        sources than it did. */
        let fixed_without_answer = 800;
        let before = source_budget(4096, fixed_without_answer + 2048 + SAFETY_MARGIN_TOKENS);
        let after = source_budget(
            4096,
            fixed_without_answer + answer_allowance(true) + SAFETY_MARGIN_TOKENS,
        );
        assert!(
            after > before,
            "expected more room, got {before} then {after}"
        );
        assert_eq!(after - before, 2048 - answer_allowance(true));
    }

    #[test]
    fn wide_characters_are_not_under_counted() {
        /* Bytes over four guesses three quarters of the real cost for CJK, and
        under-counting is the dangerous direction: it builds a prompt too large,
        and the server truncates the system prompt away rather than complaining. */
        let japanese = "\u{6a5f}\u{68b0}\u{5b66}\u{7fd2}";
        assert!(
            estimate_tokens(japanese) >= 4,
            "four characters must cost at least four tokens"
        );
    }

    #[test]
    fn ascii_costs_about_a_quarter_of_its_length() {
        /* The English rule is unchanged, so existing budgets behave as before. */
        let text = "a".repeat(400);
        let estimate = estimate_tokens(&text);
        assert!((99..=102).contains(&estimate), "got {estimate}");
    }

    #[test]
    fn token_estimate_grows_with_length() {
        /* Used only for budgeting, so it needs to be monotonic and never zero,
        not accurate. A zero would let unlimited text through the budget. */
        assert!(estimate_tokens("") >= 1);
        assert!(estimate_tokens("a short line") < estimate_tokens(&"a longer line ".repeat(20)));
    }
}
