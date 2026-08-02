/*
 * Name: job_commands.rs
 * Purpose: Let the frontend watch, resume and stop background AI work.
 * Description: Long generations report progress over the `job-progress` event,
 *   which is the live path. These commands cover the cases an event stream
 *   cannot: a page that mounts after a job started needs the current state, a
 *   window reload needs the whole list back, and a user who changed their mind
 *   needs a way to stop one. Because the registry lives in app state rather
 *   than in a component, a job that began on one page is still running, still
 *   reporting, and still recoverable from any other.
 * Tech Stack: Rust, Tauri IPC
 * License: MIT
 * Authors: Amey Thakur (https://github.com/Amey-Thakur)
 *          Archit Konde (https://github.com/Archit-Konde)
 * Date: 2026-07-28
 */

use tauri::{Manager, State};

use crate::error::AppResult;
use crate::services::job_service::Job;
use crate::state::AppState;

/// Every job this session, newest first. A page that has just mounted calls
/// this once and then follows the event stream.
#[tauri::command(rename_all = "snake_case")]
pub async fn list_jobs(app: tauri::AppHandle) -> AppResult<Vec<Job>> {
    let state: State<'_, AppState> = app.state();
    state.jobs.list()
}

/// Ask a job to stop. It ends at its next checkpoint rather than instantly,
/// because a half-written model response is not worth tearing down mid-token.
#[tauri::command(rename_all = "snake_case")]
pub async fn cancel_job(app: tauri::AppHandle, job_id: String) -> AppResult<()> {
    let state: State<'_, AppState> = app.state();
    state.jobs.cancel(&job_id)
}

/// Drop finished jobs from the list. Running ones are left alone.
#[tauri::command(rename_all = "snake_case")]
pub async fn clear_finished_jobs(app: tauri::AppHandle) -> AppResult<()> {
    let state: State<'_, AppState> = app.state();
    state.jobs.clear_finished()
}
