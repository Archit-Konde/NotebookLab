/*
 * Name: system_commands.rs
 * Purpose: System-level Tauri commands: app version, data directory, REST API
 *   token, and update restart.
 * Description: These commands are always available regardless of model or
 *   database state. They back the Settings page and the update flow.
 * Tech Stack: Rust, Tauri v2
 * License: MIT
 * Authors: Amey Thakur (https://github.com/Amey-Thakur)
 *          Archit Konde (https://github.com/Archit-Konde)
 * Date: 2026-07-12
 */

use std::sync::Mutex;

use tauri::{AppHandle, Manager};

use crate::error::AppResult;

/// Files passed on the command line at launch ("Open with NotebookLab").
/// Held until the frontend is mounted and asks for them, because an event
/// emitted during setup would fire before any listener exists.
pub struct StartupFiles(pub Mutex<Vec<String>>);

/// Keep only arguments that are real, importable files. Flags and stray
/// arguments from the shell or updater are ignored.
pub fn importable_files(args: impl Iterator<Item = String>) -> Vec<String> {
    args.filter(|arg| !arg.starts_with('-'))
        .filter(|arg| {
            let path = std::path::Path::new(arg);
            path.is_file()
                && path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(crate::parsers::is_supported_extension)
        })
        .collect()
}

/// The backend's most recent log lines for the Settings log view. Read-only,
/// in-memory, capped at 500 lines; nothing is written to disk.
#[tauri::command(rename_all = "snake_case")]
pub fn get_recent_logs() -> Vec<String> {
    crate::utils::log_buffer::recent_lines()
}

/// Hand over (and clear) the files the app was launched with, so a double
/// opened PDF lands in a notebook exactly once.
#[tauri::command(rename_all = "snake_case")]
pub fn take_startup_files(state: tauri::State<'_, StartupFiles>) -> Vec<String> {
    state
        .0
        .lock()
        .map(|mut files| std::mem::take(&mut *files))
        .unwrap_or_default()
}

#[tauri::command(rename_all = "snake_case")]
pub fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

#[tauri::command(rename_all = "snake_case")]
pub fn get_data_directory(app: AppHandle) -> AppResult<String> {
    let path = app
        .path()
        .app_data_dir()
        .map_err(|e| crate::error::AppError::Internal(e.to_string()))?;

    Ok(path.to_string_lossy().to_string())
}

/// Return the bearer token for the local REST API so users can authenticate
/// scripts against it. Shown in Settings with a copy control.
#[tauri::command(rename_all = "snake_case")]
pub fn get_api_token(state: tauri::State<'_, crate::state::AppState>) -> String {
    state.api_token.clone()
}

/// Relaunch the app so a downloaded update takes effect.
/// Only offered by the status bar after the updater stages a new version.
/// The sidecar is stopped explicitly first so the relaunched app never
/// collides with an orphaned llama-server holding its port.
#[tauri::command(rename_all = "snake_case")]
pub fn restart_app(app: AppHandle) {
    if let Some(sidecar) = app.try_state::<crate::services::sidecar_service::SidecarManager>() {
        sidecar.shutdown();
    }
    app.restart();
}

/// Check GitHub for a newer release and, if one exists, download and stage it.
/// Returns a short status the Settings page shows. The same background check
/// runs at startup; this is the manual "Check for updates" button. When an
/// update is staged it emits "update-ready" so the status bar offers a restart,
/// and the caller can restart with `restart_app`.
#[tauri::command(rename_all = "snake_case")]
pub async fn check_for_updates(app: AppHandle) -> AppResult<String> {
    use tauri::Emitter;
    use tauri_plugin_updater::UpdaterExt;

    let updater = app
        .updater()
        .map_err(|e| crate::error::AppError::Internal(format!("Updater unavailable: {e}")))?;

    match updater.check().await {
        Ok(Some(update)) => {
            let version = update.version.clone();
            update
                .download_and_install(|_, _| {}, || {})
                .await
                .map_err(|e| crate::error::AppError::Internal(format!("Download failed: {e}")))?;
            app.emit("update-ready", version.clone()).ok();
            Ok(format!(
                "Version {version} downloaded. Restart to apply it."
            ))
        }
        Ok(None) => Ok("You are on the latest version.".to_string()),
        Err(e) => Err(crate::error::AppError::Internal(format!(
            "Could not check for updates: {e}"
        ))),
    }
}

/// What this computer can run, for the local-model recommendations in the
/// Models page. GPU detection is best-effort via nvidia-smi when present;
/// absence simply means recommendations key off system RAM alone.
#[derive(serde::Serialize)]
pub struct HardwareProfile {
    pub total_ram_gb: f64,
    pub cpu_name: String,
    pub cpu_cores: usize,
    pub gpu_name: Option<String>,
    pub gpu_vram_gb: Option<f64>,
}

/// Detect RAM, CPU, and (best-effort) GPU. Async because the sysinfo refresh
/// and the nvidia-smi probe both take real time.
#[tauri::command(rename_all = "snake_case")]
pub async fn get_hardware_profile() -> AppResult<HardwareProfile> {
    tauri::async_runtime::spawn_blocking(|| {
        use sysinfo::System;

        let mut system = System::new();
        system.refresh_memory();
        system.refresh_cpu_all();

        let cpu_name = system
            .cpus()
            .first()
            .map(|cpu| cpu.brand().trim().to_string())
            .filter(|brand| !brand.is_empty())
            .unwrap_or_else(|| "Unknown CPU".to_string());

        let (gpu_name, gpu_vram_gb) = detect_nvidia_gpu();

        HardwareProfile {
            total_ram_gb: system.total_memory() as f64 / 1_073_741_824.0,
            cpu_name,
            cpu_cores: system.cpus().len(),
            gpu_name,
            gpu_vram_gb,
        }
    })
    .await
    .map_err(|e| crate::error::AppError::Internal(format!("Hardware probe failed: {e}")))
}

/// How long the GPU probe is allowed to take before it is abandoned. Long
/// enough for a cold driver to answer, short enough not to read as a freeze.
const NVIDIA_SMI_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Query nvidia-smi for the first GPU's name and VRAM. Returns (None, None)
/// when the tool is absent (AMD/Intel/no GPU); recommendations then rely on
/// system RAM, which is the safe lower bound.
fn detect_nvidia_gpu() -> (Option<String>, Option<f64>) {
    let mut command = std::process::Command::new("nvidia-smi");
    command.args([
        "--query-gpu=name,memory.total",
        "--format=csv,noheader,nounits",
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    /* nvidia-smi can hang indefinitely when the driver is wedged or a GPU is
    mid-reset, and this probe sits in front of the Models page. Waiting forever
    for an optional detail would present as the app never finishing setup, so the
    probe is given a deadline and killed if it misses it. Losing the GPU name
    only costs a recommendation keyed off RAM instead. */
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::null());

    let Ok(mut child) = command.spawn() else {
        return (None, None);
    };

    let deadline = std::time::Instant::now() + NVIDIA_SMI_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return (None, None);
                }
                break;
            }
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    tracing::warn!("nvidia-smi did not answer in time; skipping GPU detection");
                    let _ = child.kill();
                    let _ = child.wait();
                    return (None, None);
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            Err(_) => return (None, None),
        }
    }

    let Ok(output) = child.wait_with_output() else {
        return (None, None);
    };
    parse_nvidia_output(&String::from_utf8_lossy(&output.stdout))
}

/// Read the first GPU out of nvidia-smi's CSV. Split from the process call so
/// the shapes it has to survive can be tested without a GPU present.
fn parse_nvidia_output(stdout: &str) -> (Option<String>, Option<f64>) {
    let Some(line) = stdout.lines().find(|line| !line.trim().is_empty()) else {
        return (None, None);
    };
    /* Split from the right: a card's marketing name can itself contain a comma,
    and the memory figure never does. */
    let mut parts = line.rsplitn(2, ',');
    let vram_mb: Option<f64> = parts.next().and_then(|v| v.trim().parse().ok());
    let name = parts
        .next()
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty());
    /* A name with no readable memory figure is still worth reporting, but a
    memory figure with no name is not a GPU, it is a parse that went wrong. */
    match name {
        Some(name) => (Some(name), vram_mb.map(|mb| mb / 1024.0)),
        None => (None, None),
    }
}

#[cfg(test)]
mod tests {
    use super::{importable_files, parse_nvidia_output};

    #[test]
    fn a_gpu_is_read_out_of_the_expected_csv() {
        let (name, vram) = parse_nvidia_output("NVIDIA GeForce RTX 4090, 24564\n");
        assert_eq!(name.as_deref(), Some("NVIDIA GeForce RTX 4090"));
        assert_eq!(vram.map(|v| (v * 10.0).round() / 10.0), Some(24.0));
    }

    #[test]
    fn a_card_whose_name_contains_a_comma_survives() {
        /* Splitting from the left would cut this name in half and then fail to
        parse the rest of it as a number, losing the card entirely. */
        let (name, vram) = parse_nvidia_output("NVIDIA RTX A4000, Laptop GPU, 8192");
        assert_eq!(name.as_deref(), Some("NVIDIA RTX A4000, Laptop GPU"));
        assert_eq!(vram, Some(8.0));
    }

    #[test]
    fn only_the_first_gpu_is_reported() {
        let (name, _) = parse_nvidia_output("First GPU, 8192\nSecond GPU, 16384\n");
        assert_eq!(name.as_deref(), Some("First GPU"));
    }

    #[test]
    fn leading_blank_lines_do_not_swallow_the_answer() {
        let (name, vram) = parse_nvidia_output("\n\nNVIDIA A100, 40960\n");
        assert_eq!(name.as_deref(), Some("NVIDIA A100"));
        assert_eq!(vram, Some(40.0));
    }

    #[test]
    fn no_output_at_all_means_no_gpu() {
        for empty in ["", "\n", "   \n  \n"] {
            assert_eq!(parse_nvidia_output(empty), (None, None));
        }
    }

    #[test]
    fn a_line_with_no_name_is_not_reported_as_a_gpu() {
        /* Reporting a nameless card would put an empty row in the Models page
        that reads as a detection failure rather than an absent GPU. */
        assert_eq!(parse_nvidia_output(", 8192"), (None, None));
    }

    #[test]
    fn an_unreadable_memory_figure_still_names_the_card() {
        /* Older drivers answer "[Not Supported]" for memory on some cards. The
        name alone is still worth having. */
        let (name, vram) = parse_nvidia_output("NVIDIA GeForce GTX 1060, [Not Supported]");
        assert_eq!(name.as_deref(), Some("NVIDIA GeForce GTX 1060"));
        assert_eq!(vram, None);
    }

    #[test]
    fn launch_arguments_that_are_not_files_are_ignored() {
        /* argv carries the executable itself, updater flags, and whatever the
        shell added. Only real, supported documents may reach the importer. */
        let args = [
            "--updated".to_string(),
            "-v".to_string(),
            "C:\\definitely\\not\\a\\real\\path.pdf".to_string(),
        ];
        assert!(importable_files(args.into_iter()).is_empty());
    }

    #[test]
    fn a_real_supported_file_is_kept_and_an_unsupported_one_is_not() {
        let dir = std::env::temp_dir().join("notebooklab-startup-args-test");
        std::fs::create_dir_all(&dir).unwrap();
        let doc = dir.join("notes.md");
        let other = dir.join("archive.zip");
        std::fs::write(&doc, "# Notes").unwrap();
        std::fs::write(&other, "not a document").unwrap();

        let kept = importable_files(
            [
                doc.to_string_lossy().to_string(),
                other.to_string_lossy().to_string(),
                dir.to_string_lossy().to_string(),
            ]
            .into_iter(),
        );

        assert_eq!(kept.len(), 1, "only the Markdown file is importable");
        assert!(kept[0].ends_with("notes.md"));

        std::fs::remove_dir_all(&dir).ok();
    }
}
