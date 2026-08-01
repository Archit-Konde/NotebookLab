/*
 * Name: text_utils.rs
 * Purpose: Text processing utilities shared across services and repositories.
 * Description: LIKE pattern escaping prevents metacharacters in user input
 *   from being interpreted as SQL wildcards.
 * Tech Stack: Rust
 * License: MIT
 * Authors: Amey Thakur (https://github.com/Amey-Thakur)
 *          Archit Konde (https://github.com/Archit-Konde)
 * Date: 2026-07-12
 */

/// Rough token estimate, used only for context budgeting. Never for the usage
/// display, which shows real numbers reported by providers.
///
/// Bytes divided by four is a fair rule for English. It is not for CJK, where a
/// character is three bytes and roughly one token, so the same rule guesses
/// three quarters of the real cost and the prompt is built too large. Since the
/// only consequence of over-running is the server silently truncating the system
/// prompt away, the estimate has to err high on that text, not low.
///
/// This lives here rather than beside one caller because two decisions depend on
/// it and they have to agree: how much of the context the RAG prompt may fill,
/// and whether a provider's window can hold a request at all. A router using the
/// naive rule would rule a request small enough for a model the packer had
/// already judged it too big for.
pub fn estimate_tokens(text: &str) -> u32 {
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

/// Escape SQL LIKE metacharacters in user input for safe pattern matching.
/// Returns a pattern wrapped in % for substring search.
pub fn escape_like_pattern(query: &str) -> String {
    let escaped = query
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");

    format!("%{escaped}%")
}

/// Remove an XML-style wrapper a model has put around its whole answer.
///
/// Every prompt here delivers document text inside tags such as
/// `<document_context>`, because that is what keeps source material as data
/// rather than instructions. Models mirror the style back: asked to extract key
/// points, one returns the list wrapped in `<extraction_results>`, and the user
/// sees the tag sitting above and below their answer.
///
/// Only a matching pair enclosing the entire text is removed, and only when the
/// name looks like something a model invented: lowercase, no attributes. That
/// leaves real content alone, including HTML in a code block, which never has
/// the whole answer as a single element with nothing outside it.
pub fn strip_wrapper_tags(text: &str) -> &str {
    let mut current = text.trim();

    /* A model occasionally nests two of them. Bounded so a pathological input
    cannot spin here. */
    for _ in 0..3 {
        let Some(stripped) = strip_one_wrapper(current) else {
            break;
        };
        current = stripped;
    }
    current
}

/// Remove one wrapper, or return None when the text is not wrapped.
fn strip_one_wrapper(text: &str) -> Option<&str> {
    let rest = text.strip_prefix('<')?;
    let name_end = rest.find('>')?;
    let name = &rest[..name_end];

    /* No attributes and no closing slash: a bare tag, which is what a model
    emits. Anything richer is likelier to be real content. */
    if name.is_empty()
        || name.len() > 40
        || !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        return None;
    }

    let closing = format!("</{name}>");
    let inner = rest[name_end + 1..].trim_end();
    let inner = inner.strip_suffix(closing.as_str())?;

    Some(inner.trim())
}

/// Strip a leading `<think>...</think>` block from model output.
/// Reasoning models (DeepSeek R1, Qwen with thinking on) emit their hidden
/// chain of thought before the answer; showing it verbatim buries the answer
/// under minutes of monologue. Only a closed leading block with a non-empty
/// answer after it is stripped: an unclosed block (output cut off mid-think)
/// or a think-only reply is returned unchanged, so the user always sees
/// something rather than an empty message.
pub fn strip_reasoning_block(content: &str) -> &str {
    let trimmed = content.trim_start();
    let Some(rest) = trimmed.strip_prefix("<think>") else {
        return content;
    };
    match rest.find("</think>") {
        Some(position) => {
            let answer = rest[position + "</think>".len()..].trim();
            if answer.is_empty() {
                content
            } else {
                answer
            }
        }
        None => content,
    }
}

/// Truncate a string to at most `max_bytes` without splitting a UTF-8 character.
/// Byte-index slicing panics on multi-byte boundaries; this walks back to the
/// nearest valid boundary instead.
pub fn truncate_to_char_boundary(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_model_invented_wrapper_is_removed() {
        /* Exactly what the user saw above and below their key points. */
        let out = strip_wrapper_tags("<extraction_results>\n- One\n- Two\n</extraction_results>");
        assert_eq!(out, "- One\n- Two");
    }

    #[test]
    fn nested_wrappers_are_removed() {
        let out = strip_wrapper_tags("<results><summary>Body text</summary></results>");
        assert_eq!(out, "Body text");
    }

    #[test]
    fn ordinary_text_is_untouched() {
        assert_eq!(strip_wrapper_tags("Just an answer."), "Just an answer.");
        assert_eq!(strip_wrapper_tags("- One\n- Two"), "- One\n- Two");
        assert_eq!(strip_wrapper_tags(""), "");
    }

    #[test]
    fn markdown_and_code_survive() {
        /* An answer that merely contains a tag must not be mangled; only a pair
        wrapping the entire thing counts. */
        let html = "Use this:\n\n```html\n<div>hello</div>\n```";
        assert_eq!(strip_wrapper_tags(html), html);
        let heading = "# Key points\n\n- One";
        assert_eq!(strip_wrapper_tags(heading), heading);
    }

    #[test]
    fn an_unmatched_tag_is_left_alone() {
        /* Removing the opener without its closer would change meaning on a
        guess. Better to leave it than to cut the wrong thing. */
        let text = "<results>\n- One";
        assert_eq!(strip_wrapper_tags(text), text);
    }

    #[test]
    fn a_tag_with_attributes_is_left_alone() {
        /* Real markup, not a model's mirror of the prompt style. */
        let text = "<div class=\"x\">content</div>";
        assert_eq!(strip_wrapper_tags(text), text);
    }

    #[test]
    fn a_wrapper_around_only_part_of_the_answer_is_kept() {
        /* The closing tag is not at the end, so this is content, not a wrapper. */
        let text = "<note>a</note> and then more prose";
        assert_eq!(strip_wrapper_tags(text), text);
    }

    #[test]
    fn strip_reasoning_removes_closed_leading_block() {
        let output = "<think>Let me work through this...</think>\n\nThe answer is 4.";
        assert_eq!(strip_reasoning_block(output), "The answer is 4.");
    }

    #[test]
    fn strip_reasoning_leaves_plain_output_unchanged() {
        assert_eq!(
            strip_reasoning_block("The answer is 4."),
            "The answer is 4."
        );
        /* A think tag mid-text is content, not a reasoning preamble. */
        let mid = "The tag <think> appears in HTML-like text.";
        assert_eq!(strip_reasoning_block(mid), mid);
    }

    #[test]
    fn strip_reasoning_keeps_unclosed_block() {
        /* Output cut off mid-think: better the partial monologue than nothing. */
        let cut = "<think>Still reasoning about";
        assert_eq!(strip_reasoning_block(cut), cut);
    }

    #[test]
    fn strip_reasoning_keeps_think_only_reply() {
        let only = "<think>All thought, no answer.</think>";
        assert_eq!(strip_reasoning_block(only), only);
        let whitespace_after = "<think>thought</think>   \n ";
        assert_eq!(strip_reasoning_block(whitespace_after), whitespace_after);
    }

    #[test]
    fn strip_reasoning_tolerates_leading_whitespace() {
        let output = "\n  <think>hmm</think>Answer.";
        assert_eq!(strip_reasoning_block(output), "Answer.");
    }

    #[test]
    fn truncate_ascii_at_limit() {
        assert_eq!(truncate_to_char_boundary("hello", 5), "hello");
        assert_eq!(truncate_to_char_boundary("hello", 3), "hel");
    }

    #[test]
    fn truncate_shorter_than_limit_is_unchanged() {
        assert_eq!(truncate_to_char_boundary("hi", 100), "hi");
    }

    #[test]
    fn truncate_never_splits_multibyte_chars() {
        /* Each e-acute is 2 bytes; slicing at byte 3 would panic with [..3] */
        let text = "\u{e9}\u{e9}\u{e9}";
        let cut = truncate_to_char_boundary(text, 3);
        assert_eq!(cut, "\u{e9}");
        assert!(text.is_char_boundary(cut.len()));
    }

    #[test]
    fn truncate_at_zero_returns_empty() {
        assert_eq!(truncate_to_char_boundary("data", 0), "");
    }
}
