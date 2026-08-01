/*
 * Name: job_runner.rs
 * Purpose: Run a grounded generation as a tracked job, with honest progress.
 * Description: Every AI feature had the same shape: read passages from the
 *   notebook, build a prompt, call the model, return the text. Each did it as
 *   one blocking command the frontend awaited, so the work belonged to a React
 *   component. Navigating away dropped the result, and a local model that
 *   needed minutes looked identical to a hang.
 *
 *   This runs that shape once, for all of them. The command registers a job and
 *   returns its id immediately; the work continues here on a worker thread and
 *   reports weighted phases, so the percentage means something and the job is
 *   still there when the user comes back.
 *
 *   The model call has no token stream to measure, so the generate phase is
 *   advanced against how long generation has actually taken on this machine
 *   with this model (see `JobRegistry::expected_generate_secs`). That is an
 *   estimate, and it is treated as one: the bar is capped short of full until
 *   the call genuinely returns, so it can run late but never claims to be
 *   finished when it is not.
 * Tech Stack: Rust, Tauri v2, std::thread
 * License: MIT
 * Authors: Amey Thakur (https://github.com/Amey-Thakur)
 *          Archit Konde (https://github.com/Archit-Konde)
 * Date: 2026-07-28
 */

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rusqlite::Connection;
use tauri::{AppHandle, Manager};

use crate::error::{AppError, AppResult};
use crate::providers::{ChatMessage, ChatRequest, MessageRole, TaskPurpose};
use crate::services::job_service::{
    JobHandle, PHASE_FINALIZE, PHASE_GENERATE, PHASE_PROMPT, PHASE_SOURCES,
};
use crate::state::AppState;

/// How often the generate phase re-reports while the model call is in flight.
/// Fast enough that the bar visibly moves, slow enough not to flood the webview
/// with events during a call that can run for minutes.
const TICK: Duration = Duration::from_millis(600);

/// The furthest the generate phase will advance on estimate alone. Reaching the
/// end of the phase is reserved for the call actually returning; a bar that sits
/// at 100% while the user keeps waiting is the exact failure this replaces.
const ESTIMATE_CEILING: f32 = 0.95;

/// Most a model on this computer is asked to write.
///
/// Generation time is roughly linear in tokens produced, and a small model on a
/// CPU manages single-digit tokens per second. Two thousand tokens is therefore
/// several minutes of writing before a single word appears. This is the length
/// that comes back in about a minute on the hardware the app is aimed at.
const LOCAL_MAX_TOKENS: u32 = 900;

/// Most of the prompt a model on this computer is asked to read.
///
/// Reading is not free either: the whole prompt is processed before the first
/// token is written, so an eight-thousand-token context adds minutes to a reply
/// that has not started. Roughly 1500 tokens of sources, which is enough for a
/// grounded answer and short enough to be read quickly.
const LOCAL_PROMPT_CHARS: usize = 6000;

/// What to generate, and how to label it while it runs.
pub struct Generation {
    /// Feature family, e.g. "audio" or "studio". The frontend routes a finished
    /// result by this.
    pub kind: &'static str,
    /// Short human label shown next to the bar, e.g. "Debate".
    pub label: String,
    pub notebook_id: String,
    pub system_prompt: String,
    pub max_tokens: u32,
    pub temperature: f32,
    pub purpose: TaskPurpose,
}

/// Read the grounding passages. Called with the database lock held, so it should
/// do no network work and return promptly.
pub type Gather = Box<dyn FnOnce(&Connection) -> AppResult<String> + Send>;

/// Turn the gathered context into the user message.
pub type Compose = Box<dyn FnOnce(&str) -> String + Send>;

/// Post-process the model's reply. Returning an error fails the job with that
/// message, which is how a feature rejects an unusable response.
pub type Finish = Box<dyn FnOnce(String) -> AppResult<String> + Send>;

/// Run arbitrary work as a tracked job, reporting its own phases.
///
/// `spawn` covers the common gather-compose-generate shape. Chat does not fit
/// it: it embeds the question before reading the database and writes the answer
/// back afterwards, so it drives the phases itself. This gives it the same job
/// lifecycle, cancellation and progress reporting without pretending its shape
/// is the same.
///
/// The closure returns the string the job carries as its result.
pub fn spawn_task<F>(
    app: &AppHandle,
    kind: &'static str,
    notebook_id: &str,
    label: &str,
    work: F,
) -> AppResult<String>
where
    F: FnOnce(&AppHandle, &mut crate::services::job_service::JobHandle) -> AppResult<String>
        + Send
        + 'static,
{
    let state: tauri::State<'_, AppState> = app.state();
    let mut handle = state.jobs.start(app, kind, notebook_id, label)?;
    let job_id = handle.id.clone();

    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state: tauri::State<'_, AppState> = app.state();
        match work(&app, &mut handle) {
            Ok(result) => handle.succeed(&state.jobs, result),
            Err(_) if handle.cancelled() => handle.cancel(&state.jobs),
            Err(e) => handle.fail(&state.jobs, e.to_string()),
        }
    });

    Ok(job_id)
}

/// The ceiling a model on this computer is held to, for callers outside this
/// module that build their own request.
pub fn local_max_tokens(requested: u32) -> u32 {
    requested.min(LOCAL_MAX_TOKENS)
}

/// Start a generation and return its job id at once.
///
/// The caller does not wait. Progress, the result and any error all arrive
/// through the job, which the frontend is already subscribed to.
pub fn spawn(
    app: &AppHandle,
    spec: Generation,
    gather: Gather,
    compose: Compose,
    finish: Finish,
) -> AppResult<String> {
    let state: tauri::State<'_, AppState> = app.state();
    let mut handle = state
        .jobs
        .start(app, spec.kind, &spec.notebook_id, &spec.label)?;
    let job_id = handle.id.clone();

    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state: tauri::State<'_, AppState> = app.state();
        let jobs = &state.jobs;

        /* Each phase is attempted in turn; the first failure ends the job with a
        message the user can act on, rather than a silent stop. */
        let outcome = (|| -> AppResult<String> {
            handle.begin(jobs, PHASE_SOURCES);
            let context = {
                let conn = state.conn()?;
                gather(&conn)?
            };
            handle.finish_phase(jobs, PHASE_SOURCES);
            if handle.cancelled() {
                return Err(AppError::Internal(CANCELLED.into()));
            }

            handle.begin(jobs, PHASE_PROMPT);
            let composed = compose(&context);
            handle.finish_phase(jobs, PHASE_PROMPT);
            if handle.cancelled() {
                return Err(AppError::Internal(CANCELLED.into()));
            }

            handle.begin(jobs, PHASE_GENERATE);
            let providers = state.provider_read()?;
            /* Key the expectation by the model actually about to answer, so a
            switch from a small local model to a large one is learned rather
            than averaged into one meaningless number. */
            let (key, is_local) = providers.active_profile();
            let expected = jobs.expected_generate_secs(&key, is_local);

            let (user_content, max_tokens) = size_request(&composed, spec.max_tokens, is_local);

            let request = ChatRequest {
                messages: vec![
                    ChatMessage {
                        role: MessageRole::System,
                        content: spec.system_prompt,
                    },
                    ChatMessage {
                        role: MessageRole::User,
                        content: user_content,
                    },
                ],
                max_tokens: Some(max_tokens),
                temperature: Some(spec.temperature),
                purpose: spec.purpose,
            };

            let started = Instant::now();

            /* Tokens written so far, shared with the ticker.
            The first version reported progress from inside the token
            callback, which made the bar depend entirely on the model
            producing something: a stream that stayed quiet left it frozen at
            the phase boundary with no elapsed time and no estimate, which is
            indistinguishable from the app having died. The ticker always
            runs now and takes whichever signal is further along, so the bar
            moves on time alone and sharpens to a real measurement once words
            start arriving. */
            let written = Arc::new(AtomicUsize::new(0));
            let streaming = providers.active_supports_streaming();

            let ticker = Ticker::start(
                &app,
                &handle,
                expected,
                streaming.then(|| (Arc::clone(&written), max_tokens)),
            );

            let outcome = if streaming {
                let counted = Arc::clone(&written);
                let mut on_token = |fragment: &str| {
                    counted.fetch_add(approximate_tokens(fragment), Ordering::Relaxed);
                };
                providers.stream_chat_completion(request.clone(), &mut on_token)
            } else {
                providers.chat_completion(request.clone())
            };

            /* A stream that fails, or finishes having said nothing, is retried
            without streaming. Servers advertise the endpoint and then behave
            differently under `stream: true`, and the user should get their
            answer rather than a report about transport. */
            let outcome = match outcome {
                Ok(response) if !response.content.trim().is_empty() => Ok(response),
                other => {
                    if streaming {
                        if let Err(ref e) = other {
                            tracing::warn!("Streaming failed, retrying without it: {e}");
                        } else {
                            tracing::warn!("Stream produced nothing, retrying without it");
                        }
                        providers.chat_completion(request)
                    } else {
                        other
                    }
                }
            };
            ticker.stop();
            let response = outcome.map_err(|e| AppError::Provider(e.to_string()));
            drop(providers);

            let response = response?;
            jobs.record_generate_secs(&key, started.elapsed().as_secs_f32());
            handle.finish_phase(jobs, PHASE_GENERATE);

            handle.begin(jobs, PHASE_FINALIZE);
            /* Models mirror the prompt's XML style back and wrap their whole
            answer in an invented tag, which the user then reads above and below
            their result. Stripped once here so every feature is covered rather
            than each remembering to. JSON formats are unaffected: they do not
            begin with a tag. */
            let cleaned =
                crate::utils::text_utils::strip_wrapper_tags(&response.content).to_string();
            let finished = finish(cleaned)?;
            handle.finish_phase(jobs, PHASE_FINALIZE);
            Ok(finished)
        })();

        match outcome {
            Ok(result) => handle.succeed(jobs, result),
            /* A cancel beats whatever error the early return carried: the user
            stopping the work is not a failure to report back to them. */
            Err(_) if handle.cancelled() => handle.cancel(jobs),
            Err(e) => handle.fail(jobs, e.to_string()),
        }
    });

    Ok(job_id)
}

/// Rough token count for a streamed fragment.
///
/// Only used to move a progress bar, so being a little off is harmless; what
/// matters is that it is monotonic and cheap enough to run on every fragment.
/// Most fragments are a single token, which is why the floor is one.
fn approximate_tokens(fragment: &str) -> usize {
    fragment.split_whitespace().count().max(1)
}

/// Fit a request to the machine that will answer it.
///
/// A hosted model reads eight thousand tokens and writes two thousand in
/// seconds. A 3B model on a CPU manages single-digit tokens per second, so the
/// same request is twenty minutes of work and reads as a hang however patient
/// the timeout is. Asking a local model for less produces a shorter answer,
/// which beats a longer one the user never receives.
///
/// Cloud requests are returned untouched: there is no reason to shorten an
/// answer from a model that can write it quickly.
fn size_request(prompt: &str, max_tokens: u32, is_local: bool) -> (String, u32) {
    if !is_local {
        return (prompt.to_string(), max_tokens);
    }
    (
        crate::utils::text_utils::truncate_to_char_boundary(prompt, LOCAL_PROMPT_CHARS).to_string(),
        max_tokens.min(LOCAL_MAX_TOKENS),
    )
}

/// Marker for the early return taken when the user cancels between phases. It
/// never reaches the user: the match on the outcome turns it into a cancelled
/// job, which the frontend renders as such.
const CANCELLED: &str = "cancelled";

/// Advances the generate phase on a timer while the model call blocks.
///
/// Public so features that drive their own phases, such as Chat, get the same
/// guarantee that the bar keeps moving whatever the provider does.
pub struct Ticker {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Ticker {
    /// `tokens` carries a live count and the ceiling it is measured against,
    /// when the provider streams. Without it the phase advances on elapsed time
    /// against what this model has taken before.
    pub fn start(
        app: &AppHandle,
        handle: &JobHandle,
        expected: f32,
        tokens: Option<(Arc<AtomicUsize>, u32)>,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stop);
        let app = app.clone();
        let id = handle.id.clone();
        let banked = handle.done_weight();

        let thread = std::thread::spawn(move || {
            let started = Instant::now();
            while !flag.load(Ordering::SeqCst) {
                std::thread::sleep(TICK);
                if flag.load(Ordering::SeqCst) {
                    break;
                }

                let by_time = started.elapsed().as_secs_f32() / expected.max(1.0);
                /* Whichever signal is further along. Time keeps the bar moving
                when the model is quiet; the token count overtakes it once the
                answer is genuinely being written, which is the honest
                measurement. */
                let within = match &tokens {
                    Some((counter, ceiling)) => {
                        let by_tokens =
                            counter.load(Ordering::Relaxed) as f32 / (*ceiling).max(1) as f32;
                        by_time.max(by_tokens)
                    }
                    None => by_time,
                }
                .min(ESTIMATE_CEILING);

                let state: tauri::State<'_, AppState> = app.state();
                state.jobs.report(&app, &id, PHASE_GENERATE, banked, within);
            }
        });

        Self {
            stop,
            thread: Some(thread),
        }
    }

    pub fn stop(mut self) {
        self.halt();
    }

    fn halt(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            t.join().ok();
        }
    }
}

impl Drop for Ticker {
    /// Stops the thread even when the generate phase returns early through the
    /// `?` on the provider error, so a failed call cannot leave a thread
    /// reporting progress for a job that is already over.
    fn drop(&mut self) {
        self.halt();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cloud_request_is_left_alone() {
        let long = "x".repeat(LOCAL_PROMPT_CHARS * 3);
        let (prompt, tokens) = size_request(&long, 2048, false);
        assert_eq!(prompt.len(), long.len(), "cloud prompts are not truncated");
        assert_eq!(tokens, 2048, "cloud answers keep their full length");
    }

    #[test]
    fn a_local_request_is_cut_to_fit() {
        let long = "x".repeat(LOCAL_PROMPT_CHARS * 3);
        let (prompt, tokens) = size_request(&long, 2048, true);
        assert!(prompt.len() <= LOCAL_PROMPT_CHARS);
        assert_eq!(tokens, LOCAL_MAX_TOKENS);
    }

    #[test]
    fn a_short_local_request_is_not_padded_or_raised() {
        /* Sizing is a ceiling, not a target: a feature that asks for less than
        the local limit must still get what it asked for. */
        let (prompt, tokens) = size_request("short prompt", 300, true);
        assert_eq!(prompt, "short prompt");
        assert_eq!(tokens, 300);
    }

    #[test]
    fn truncation_never_splits_a_character() {
        /* Cutting a multi-byte character in half panics on the slice, which
        would take down the job for any notebook containing non-ASCII text. */
        let text = "é".repeat(LOCAL_PROMPT_CHARS);
        let (prompt, _) = size_request(&text, 900, true);
        assert!(prompt.len() <= LOCAL_PROMPT_CHARS);
        assert!(
            text.starts_with(&prompt),
            "the cut is a prefix of the input"
        );
    }

    #[test]
    fn a_fragment_always_counts_as_progress() {
        /* Fragments are usually one token and often have no whitespace at all.
        Counting words would score those as zero and freeze the bar for the
        whole answer. */
        assert_eq!(approximate_tokens("Hello"), 1);
        assert_eq!(approximate_tokens(" world"), 1);
        assert_eq!(approximate_tokens(""), 1);
        assert_eq!(approximate_tokens("two words"), 2);
    }

    #[test]
    fn local_max_tokens_helper_agrees_with_sizing() {
        assert_eq!(local_max_tokens(2048), LOCAL_MAX_TOKENS);
        assert_eq!(local_max_tokens(100), 100);
    }
}
