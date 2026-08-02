/*
 * Name: ollama_commands.rs
 * Purpose: Tauri commands for managing Ollama models from the Models page.
 * Description: Thin IPC wrappers over ollama_service. Status and listing are
 *   async because they probe the local server. Pulls run on a background
 *   thread and stream progress to the frontend via "ollama-pull-progress"
 *   events (mirroring the bundled model download's event pattern), guarded so
 *   only one pull runs at a time. All endpoints are the fixed loopback Ollama
 *   address; nothing here takes a URL from the frontend.
 * Tech Stack: Rust, Tauri v2
 * License: MIT
 * Authors: Amey Thakur (https://github.com/Amey-Thakur)
 *          Archit Konde (https://github.com/Archit-Konde)
 * Date: 2026-07-17
 */

use std::sync::Mutex;

use tauri::Emitter;

use crate::error::{AppError, AppResult};
use crate::services::ollama_service::{self, OllamaModel, OllamaStatus, PullProgress};

/// The model currently being pulled, if any. One pull at a time: parallel
/// pulls thrash disk and bandwidth, and the UI shows a single progress bar.
/// Holding the name (not just a flag) lets the Models page restore its
/// progress display after the user navigates away and back mid-download.
static CURRENT_PULL: Mutex<Option<String>> = Mutex::new(None);

/// Holds the pull slot and frees it however the download thread ends, a panic
/// included. Clearing it on the last line of the thread instead meant a thread
/// that died left the slot occupied, and the app then refused every later
/// download saying one was already running, until it was restarted. The model
/// downloader had the same shape and the same fix.
struct PullGuard;

impl PullGuard {
    /// Claim the slot for `model`, or None when a pull is already running.
    fn acquire(model: &str) -> Option<Self> {
        let mut current = CURRENT_PULL.lock().ok()?;
        if current.is_some() {
            return None;
        }
        *current = Some(model.to_string());
        Some(PullGuard)
    }
}

impl Drop for PullGuard {
    fn drop(&mut self) {
        if let Ok(mut current) = CURRENT_PULL.lock() {
            *current = None;
        }
    }
}

/// Terminal event emitted after a pull ends, success or failure.
#[derive(Clone, serde::Serialize)]
struct PullFinished {
    model: String,
    ok: bool,
    error: Option<String>,
}

/// Is Ollama installed and running, and which version?
#[tauri::command(rename_all = "snake_case")]
pub async fn ollama_status() -> AppResult<OllamaStatus> {
    tauri::async_runtime::spawn_blocking(ollama_service::status)
        .await
        .map_err(|e| AppError::Internal(format!("Status task failed: {e}")))
}

/// Locally installed Ollama models with their disk sizes.
#[tauri::command(rename_all = "snake_case")]
pub async fn ollama_installed_models() -> AppResult<Vec<OllamaModel>> {
    tauri::async_runtime::spawn_blocking(ollama_service::installed_models)
        .await
        .map_err(|e| AppError::Internal(format!("List task failed: {e}")))?
}

/// Start pulling a model. Returns immediately; progress arrives via
/// "ollama-pull-progress" events and the end via "ollama-pull-finished".
#[tauri::command(rename_all = "snake_case")]
pub fn ollama_pull_model(app: tauri::AppHandle, model: String) -> AppResult<()> {
    ollama_service::validate_model_name(&model)?;

    let Some(guard) = PullGuard::acquire(&model) else {
        return Err(AppError::InvalidInput(
            "A model download is already in progress".into(),
        ));
    };

    std::thread::spawn(move || {
        /* Held for the whole download and released when this thread ends by any
        route, so a failure cannot lock out every later one. */
        let _guard = guard;
        let result = ollama_service::pull_model(&model, |progress: PullProgress| {
            app.emit("ollama-pull-progress", &progress).ok();
        });

        let (ok, error) = match &result {
            Ok(()) => (true, None),
            Err(e) => (false, Some(e.to_string())),
        };
        app.emit(
            "ollama-pull-finished",
            PullFinished {
                model: model.clone(),
                ok,
                error,
            },
        )
        .ok();

        if let Err(e) = result {
            tracing::warn!("Ollama pull failed for {model}: {e}");
        } else {
            tracing::info!("Ollama pull complete: {model}");
        }
    });

    Ok(())
}

/// The model currently downloading, if any. Lets the Models page restore its
/// progress display after a navigation instead of misreporting an idle state.
#[tauri::command(rename_all = "snake_case")]
pub fn ollama_pull_state() -> AppResult<Option<String>> {
    Ok(CURRENT_PULL
        .lock()
        .map_err(|_| AppError::Internal("Pull guard poisoned".into()))?
        .clone())
}

/// Remove an installed model from disk.
#[tauri::command(rename_all = "snake_case")]
pub async fn ollama_delete_model(model: String) -> AppResult<()> {
    tauri::async_runtime::spawn_blocking(move || ollama_service::delete_model(&model))
        .await
        .map_err(|e| AppError::Internal(format!("Delete task failed: {e}")))?
}

#[cfg(test)]
mod guard_tests {
    use super::{PullGuard, CURRENT_PULL};

    /// One test on purpose: the slot is process-wide and Rust runs tests in
    /// parallel, so two tests claiming it would fight over the same static.
    #[test]
    fn the_pull_slot_admits_one_download_and_always_frees() {
        assert!(
            CURRENT_PULL.lock().unwrap().is_none(),
            "slot should start free"
        );

        {
            let _first = PullGuard::acquire("llama3").expect("slot should be free");
            assert_eq!(CURRENT_PULL.lock().unwrap().as_deref(), Some("llama3"));
            assert!(
                PullGuard::acquire("mistral").is_none(),
                "a second download must be refused while one runs"
            );
        }
        assert!(
            CURRENT_PULL.lock().unwrap().is_none(),
            "leaving the scope must free the slot"
        );

        /* Why the guard exists rather than a clear on the thread's last line: a
        thread that panicked used to leave the slot taken, and every later pull
        was refused as "already in progress" until the app restarted. */
        let panicked = std::panic::catch_unwind(|| {
            let _guard = PullGuard::acquire("gemma").expect("slot should be free");
            panic!("the pull thread died");
        });
        assert!(panicked.is_err(), "the panic should have propagated");
        assert!(
            CURRENT_PULL.lock().unwrap().is_none(),
            "a panicking pull must not block every later download"
        );
    }
}
