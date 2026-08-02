/*
 * Name: download_commands.rs
 * Purpose: Commands for downloading GGUF model files from HuggingFace Hub.
 * Description: Reports progress via Tauri events so the frontend can show a
 *   progress bar. Downloads are streamed to disk in 64KB chunks.
 *   Progress events are emitted every 1% or every 500ms (whichever
 *   comes first). The download directory is
 *   $APP_DATA/models/gguf/. Only huggingface.co URLs are allowed.
 *   A guard prevents concurrent downloads. Temp files are cleaned
 *   up on failure.
 * Tech Stack: Rust, Tauri v2, reqwest
 * License: MIT
 * Authors: Amey Thakur (https://github.com/Amey-Thakur)
 *          Archit Konde (https://github.com/Archit-Konde)
 * Date: 2026-07-12
 */

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{Emitter, Manager};

use crate::error::{AppError, AppResult};

/// Default model for first-launch: small enough for 8GB RAM, good quality.
const DEFAULT_MODEL_URL: &str =
    "https://huggingface.co/bartowski/Llama-3.2-3B-Instruct-GGUF/resolve/main/Llama-3.2-3B-Instruct-Q4_K_M.gguf";
const DEFAULT_MODEL_NAME: &str = "Llama-3.2-3B-Instruct-Q4_K_M.gguf";

/// One bundled-server model the user can download in a click.
#[derive(Clone, serde::Serialize)]
pub struct GgufCatalogEntry {
    pub id: &'static str,
    pub label: &'static str,
    pub params: &'static str,
    pub filename: &'static str,
    /// Verified download size in GB (from the hosting file's content-length).
    pub download_gb: f64,
    pub min_ram_gb: u32,
    pub recommended_ram_gb: u32,
    pub use_note: &'static str,
    #[serde(skip)]
    url: &'static str,
}

/// Curated GGUF models for the bundled llama.cpp server. Every URL was
/// verified live (HTTP 200) with its size read from the response headers, so
/// each entry downloads exactly what it promises. All are Q4_K_M quantized
/// builds from Hugging Face, small enough for consumer hardware; the catalog
/// stays ordered small to large so the recommendation logic can walk it.
const GGUF_CATALOG: &[GgufCatalogEntry] = &[
    GgufCatalogEntry {
        id: "llama-3.2-1b",
        label: "Llama 3.2",
        params: "1B",
        filename: "Llama-3.2-1B-Instruct-Q4_K_M.gguf",
        download_gb: 0.75,
        min_ram_gb: 4,
        recommended_ram_gb: 8,
        use_note: "The lightest start; quick answers on older machines.",
        url: "https://huggingface.co/bartowski/Llama-3.2-1B-Instruct-GGUF/resolve/main/Llama-3.2-1B-Instruct-Q4_K_M.gguf",
    },
    GgufCatalogEntry {
        id: "llama-3.2-3b",
        label: "Llama 3.2",
        params: "3B",
        filename: DEFAULT_MODEL_NAME,
        download_gb: 1.88,
        min_ram_gb: 8,
        recommended_ram_gb: 8,
        use_note: "The dependable starter: capable and light on memory.",
        url: DEFAULT_MODEL_URL,
    },
    GgufCatalogEntry {
        id: "gemma-3-4b",
        label: "Gemma 3",
        params: "4B",
        filename: "google_gemma-3-4b-it-Q4_K_M.gguf",
        download_gb: 2.32,
        min_ram_gb: 8,
        recommended_ram_gb: 16,
        use_note: "Google's compact all-rounder with clean writing.",
        url: "https://huggingface.co/bartowski/google_gemma-3-4b-it-GGUF/resolve/main/google_gemma-3-4b-it-Q4_K_M.gguf",
    },
    GgufCatalogEntry {
        id: "phi-4-mini",
        label: "Phi-4 Mini",
        params: "3.8B",
        filename: "microsoft_Phi-4-mini-instruct-Q4_K_M.gguf",
        download_gb: 2.32,
        min_ram_gb: 8,
        recommended_ram_gb: 16,
        use_note: "Microsoft's small model tuned for logic and math.",
        url: "https://huggingface.co/bartowski/microsoft_Phi-4-mini-instruct-GGUF/resolve/main/microsoft_Phi-4-mini-instruct-Q4_K_M.gguf",
    },
    GgufCatalogEntry {
        id: "qwen3-4b",
        label: "Qwen 3",
        params: "4B",
        filename: "Qwen_Qwen3-4B-Q4_K_M.gguf",
        download_gb: 2.33,
        min_ram_gb: 8,
        recommended_ram_gb: 16,
        use_note: "Punches above its size on reasoning and long documents.",
        url: "https://huggingface.co/bartowski/Qwen_Qwen3-4B-GGUF/resolve/main/Qwen_Qwen3-4B-Q4_K_M.gguf",
    },
    GgufCatalogEntry {
        id: "mistral-7b",
        label: "Mistral",
        params: "7B",
        filename: "Mistral-7B-Instruct-v0.3-Q4_K_M.gguf",
        download_gb: 4.07,
        min_ram_gb: 16,
        recommended_ram_gb: 16,
        use_note: "A proven, efficient classic with a direct style.",
        url: "https://huggingface.co/bartowski/Mistral-7B-Instruct-v0.3-GGUF/resolve/main/Mistral-7B-Instruct-v0.3-Q4_K_M.gguf",
    },
    GgufCatalogEntry {
        id: "qwen2.5-coder-7b",
        label: "Qwen 2.5 Coder",
        params: "7B",
        filename: "Qwen2.5-Coder-7B-Instruct-Q4_K_M.gguf",
        download_gb: 4.36,
        min_ram_gb: 16,
        recommended_ram_gb: 16,
        use_note: "Purpose-built for code: completion and explanation.",
        url: "https://huggingface.co/bartowski/Qwen2.5-Coder-7B-Instruct-GGUF/resolve/main/Qwen2.5-Coder-7B-Instruct-Q4_K_M.gguf",
    },
    GgufCatalogEntry {
        id: "deepseek-r1-7b",
        label: "DeepSeek R1 Distill",
        params: "7B",
        filename: "DeepSeek-R1-Distill-Qwen-7B-Q4_K_M.gguf",
        download_gb: 4.36,
        min_ram_gb: 16,
        recommended_ram_gb: 16,
        use_note: "Deliberate step-by-step reasoning, fully offline. Thinks before answering, so replies take minutes on CPU; pick a smaller model for quick answers.",
        url: "https://huggingface.co/bartowski/DeepSeek-R1-Distill-Qwen-7B-GGUF/resolve/main/DeepSeek-R1-Distill-Qwen-7B-Q4_K_M.gguf",
    },
];

/// The curated bundled-server catalog for the Models page.
#[tauri::command(rename_all = "snake_case")]
pub fn list_gguf_catalog() -> Vec<GgufCatalogEntry> {
    GGUF_CATALOG.to_vec()
}

/// Download a catalog model by id. Progress arrives on the same
/// "model-download-progress" events as the default download, keyed by
/// filename, and the same single-download guard applies.
#[tauri::command(rename_all = "snake_case")]
pub fn download_gguf_model(app: tauri::AppHandle, id: String) -> AppResult<String> {
    let entry = GGUF_CATALOG
        .iter()
        .find(|e| e.id == id)
        .ok_or_else(|| AppError::InvalidInput(format!("Unknown model id: {id}")))?;
    download_model(app, entry.url.to_string(), entry.filename.to_string())
}

/// Allowed download hosts. Only trusted model repositories.
const ALLOWED_HOSTS: &[&str] = &["huggingface.co"];

/// Whether an https URL points at a host downloads are permitted from.
///
/// This guards what gets written to disk and then executed as a model, so it is
/// worth being exact about. The naive version compared the text between "https://"
/// and the first slash, which was case sensitive (DNS is not, so a legitimate
/// `HuggingFace.co` link was refused) and kept any userinfo and port in the
/// string it compared.
///
/// A subdomain is allowed, which is deliberate: Hugging Face serves model files
/// from `cdn-lfs.huggingface.co`. The dot is required, so `evil-huggingface.co`
/// does not qualify.
fn host_is_allowed(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };

    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    /* Anything before an @ is userinfo, not the host. Without dropping it,
    "https://huggingface.co@evil.example/x" compares the whole string, which
    happens to fail here but only by luck. */
    let host_and_port = match authority.rsplit_once('@') {
        Some((_userinfo, host)) => host,
        None => authority,
    };
    /* Strip a port. An IPv6 literal is bracketed, so only split on the last
    colon when it comes after the closing bracket. */
    let host = match host_and_port.rsplit_once(':') {
        Some((head, tail)) if tail.chars().all(|c| c.is_ascii_digit()) && !tail.is_empty() => head,
        _ => host_and_port,
    };
    let host = host.trim().to_ascii_lowercase();

    if host.is_empty() {
        return false;
    }

    ALLOWED_HOSTS.iter().any(|allowed| {
        if host == *allowed {
            return true;
        }
        /* A subdomain: the dot is required, so `evil-huggingface.co` does not
        qualify. Built once rather than borrowed inline, which clippy reads as
        a reference it has to dereference straight back. */
        let suffix = format!(".{allowed}");
        host.ends_with(suffix.as_str())
    })
}

/// Global download guard: prevents concurrent downloads.
static DOWNLOAD_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

/// Holds the download guard and releases it however the download ends, a panic
/// in the worker thread included. Releasing it by hand at each exit meant every
/// new early return had to remember to do so, and a missed one leaves the app
/// refusing every later download until it is restarted, with nothing on screen
/// to explain the refusal.
struct DownloadGuard;

impl DownloadGuard {
    /// Take the guard, or None when a download is already running.
    fn acquire() -> Option<Self> {
        DOWNLOAD_IN_PROGRESS
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| DownloadGuard)
    }
}

impl Drop for DownloadGuard {
    fn drop(&mut self) {
        DOWNLOAD_IN_PROGRESS.store(false, Ordering::Release);
    }
}

/// Progress event emitted to the frontend during download.
#[derive(Clone, serde::Serialize)]
pub struct DownloadProgress {
    pub downloaded: u64,
    pub total: u64,
    pub percent: f64,
    pub model_name: String,
    pub status: String, /* "downloading", "complete", "error" */
}

/// Download a GGUF model. Progress is reported via "model-download-progress"
/// Tauri events, and the returned path is where the file will land, which it
/// does not occupy until the download finishes.
///
/// Not exposed over IPC: the only way in is `download_gguf_model`, which takes a
/// catalog id, so no interface can name an arbitrary URL. The url and filename
/// were optional while a separate command downloaded a hardcoded default; the
/// catalog carries that model now, so both are always supplied.
fn download_model(app: tauri::AppHandle, url: String, filename: String) -> AppResult<String> {
    /* Prevent concurrent downloads. The guard releases itself on every path
    out of this function, including the ones that return early below. */
    let Some(guard) = DownloadGuard::acquire() else {
        return Err(AppError::InvalidInput(
            "A download is already in progress".into(),
        ));
    };

    let download_url = url.as_str();

    /* Sanitize filename: reject path separators and traversal */
    let model_name = filename.as_str();
    if model_name.contains('/') || model_name.contains('\\') || model_name.contains("..") {
        return Err(AppError::InvalidInput(
            "Invalid filename: must not contain path separators".into(),
        ));
    }

    /* Validate URL: must be HTTPS and from an allowed host */
    if !download_url.starts_with("https://") {
        return Err(AppError::InvalidInput(
            "Model download URL must use HTTPS".into(),
        ));
    }

    if !host_is_allowed(download_url) {
        return Err(AppError::InvalidInput(format!(
            "Downloads only allowed from: {}",
            ALLOWED_HOSTS.join(", ")
        )));
    }

    /* Resolve output path */
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Internal(format!("Failed to resolve data dir: {e}")))?;
    let models_dir = data_dir.join("models").join("gguf");
    std::fs::create_dir_all(&models_dir)
        .map_err(|e| AppError::Internal(format!("Failed to create models dir: {e}")))?;

    let output_path = models_dir.join(model_name);

    /* Clean up any stale .downloading temp files from previous failed attempts */
    cleanup_temp_files(&models_dir);

    /* Skip if already downloaded (file exists with .gguf extension and non-zero size) */
    if output_path.exists() {
        let size = std::fs::metadata(&output_path)
            .map(|m| m.len())
            .unwrap_or(0);
        if size > 0 {
            tracing::info!(
                "Model already exists: {} ({} MB)",
                model_name,
                size / 1_048_576
            );
            app.emit(
                "model-download-progress",
                DownloadProgress {
                    downloaded: size,
                    total: size,
                    percent: 100.0,
                    model_name: model_name.to_string(),
                    status: "complete".to_string(),
                },
            )
            .ok();
            return Ok(output_path.to_string_lossy().to_string());
        }
    }

    tracing::info!("Downloading model: {model_name}");

    /* Start download on a background thread */
    let app_clone = app.clone();
    let url_owned = download_url.to_string();
    let name_owned = model_name.to_string();
    let path_owned = output_path.clone();

    std::thread::spawn(move || {
        /* Held for the life of the download, and dropped when this thread ends
        by any route: success, error, or panic. */
        let _guard = guard;
        let result = do_download(&app_clone, &url_owned, &name_owned, &path_owned);

        if let Err(e) = result {
            tracing::error!("Model download failed: {e}");

            /* Clean up partial temp file */
            let tmp_path = path_owned.with_extension("gguf.downloading");
            let _ = std::fs::remove_file(&tmp_path);

            app_clone
                .emit(
                    "model-download-progress",
                    DownloadProgress {
                        downloaded: 0,
                        total: 0,
                        percent: 0.0,
                        model_name: name_owned,
                        status: format!("error: {e}"),
                    },
                )
                .ok();
        }
    });

    Ok(output_path.to_string_lossy().to_string())
}

/// Perform the actual download with progress reporting.
fn do_download(
    app: &tauri::AppHandle,
    url: &str,
    model_name: &str,
    output_path: &std::path::Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(3600))
        .connect_timeout(std::time::Duration::from_secs(30))
        .build()?;

    let response = client.get(url).send()?;

    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()).into());
    }

    let total = response.content_length().unwrap_or(0);

    /* Write to a temp file first, rename on completion */
    let tmp_path = output_path.with_extension("gguf.downloading");
    let mut file = std::fs::File::create(&tmp_path)?;

    let mut downloaded: u64 = 0;
    let mut last_report = std::time::Instant::now();
    let mut last_percent: f64 = 0.0;

    let mut reader = response;
    let mut buf = vec![0u8; 65536];

    loop {
        let bytes_read = std::io::Read::read(&mut reader, &mut buf)?;
        if bytes_read == 0 {
            break;
        }

        file.write_all(&buf[..bytes_read])?;
        downloaded += bytes_read as u64;

        let percent = if total > 0 {
            ((downloaded as f64 / total as f64) * 100.0).min(100.0)
        } else {
            0.0
        };

        let elapsed = last_report.elapsed();
        if percent - last_percent >= 1.0 || elapsed.as_millis() >= 500 {
            app.emit(
                "model-download-progress",
                DownloadProgress {
                    downloaded,
                    total,
                    percent,
                    model_name: model_name.to_string(),
                    status: "downloading".to_string(),
                },
            )
            .ok();
            last_report = std::time::Instant::now();
            last_percent = percent;
        }
    }

    file.flush()?;
    drop(file);

    /* A body that stops early arrives as a clean end of stream, not an error,
    so without this the partial file would be renamed into place and then
    treated as installed for good: the "already downloaded" check only asks
    whether the file exists and has a non-zero size, and nothing ever
    re-downloads one that does. The result is a model that loads as garbage or
    not at all, with no way to tell from the app that anything went wrong.

    Only checkable when the server declared a length; without one, the size
    cannot be confirmed here and the loader is left to reject a bad file. */
    if total > 0 && downloaded != total {
        let _ = std::fs::remove_file(&tmp_path);
        return Err(format!(
            "Download incomplete: received {downloaded} of {total} bytes. Check your connection and try again."
        )
        .into());
    }

    /* Rename temp file to final name */
    std::fs::rename(&tmp_path, output_path)?;

    tracing::info!(
        "Model download complete: {} ({} MB)",
        model_name,
        downloaded / 1_048_576
    );

    app.emit(
        "model-download-progress",
        DownloadProgress {
            downloaded,
            total: downloaded,
            percent: 100.0,
            model_name: model_name.to_string(),
            status: "complete".to_string(),
        },
    )
    .ok();

    Ok(())
}

/// Clean up stale .downloading temp files from previous failed attempts.
fn cleanup_temp_files(models_dir: &std::path::Path) {
    if let Ok(entries) = std::fs::read_dir(models_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("downloading") {
                tracing::debug!("Cleaning up stale temp file: {}", path.display());
                let _ = std::fs::remove_file(&path);
            }
        }
    }
}

/// Check if a model file exists and is likely complete.
#[tauri::command(rename_all = "snake_case")]
pub fn has_local_model(app: tauri::AppHandle) -> AppResult<bool> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::Internal(format!("Failed to resolve data dir: {e}")))?;
    let models_dir = data_dir.join("models").join("gguf");

    let models = crate::services::sidecar_service::find_model_files(&models_dir);
    Ok(!models.is_empty())
}

#[cfg(test)]
mod host_tests {
    use super::*;

    #[test]
    fn the_catalog_urls_are_allowed() {
        /* Whatever else this rejects, it must not reject the app's own models. */
        assert!(host_is_allowed(DEFAULT_MODEL_URL));
        for entry in GGUF_CATALOG {
            assert!(host_is_allowed(entry.url), "refused {}", entry.url);
        }
    }

    #[test]
    fn a_subdomain_is_allowed() {
        /* Hugging Face serves the actual files from a CDN subdomain. */
        assert!(host_is_allowed(
            "https://cdn-lfs.huggingface.co/repo/model.gguf"
        ));
    }

    #[test]
    fn the_host_is_matched_case_insensitively() {
        /* DNS is case insensitive, so this was refusing a legitimate link. */
        assert!(host_is_allowed("https://HuggingFace.CO/repo/model.gguf"));
    }

    #[test]
    fn a_lookalike_host_is_refused() {
        /* The dot matters: without it, anyone can register the suffix. */
        assert!(!host_is_allowed("https://evil-huggingface.co/model.gguf"));
        assert!(!host_is_allowed(
            "https://huggingface.co.evil.example/model.gguf"
        ));
    }

    #[test]
    fn userinfo_cannot_disguise_the_host() {
        /* Everything before the @ is a username, not a host. Browsers have shown
        this trick for decades. */
        assert!(!host_is_allowed(
            "https://huggingface.co@evil.example/model.gguf"
        ));
        assert!(!host_is_allowed(
            "https://huggingface.co:pass@evil.example/x"
        ));
    }

    #[test]
    fn a_port_does_not_break_a_legitimate_host() {
        assert!(host_is_allowed(
            "https://huggingface.co:443/repo/model.gguf"
        ));
    }

    #[test]
    fn plain_http_and_other_schemes_are_refused() {
        assert!(!host_is_allowed("http://huggingface.co/model.gguf"));
        assert!(!host_is_allowed("file:///etc/passwd"));
        assert!(!host_is_allowed("https://"));
        assert!(!host_is_allowed(""));
    }

    #[test]
    fn a_query_or_fragment_is_not_part_of_the_host() {
        assert!(host_is_allowed("https://huggingface.co?x=1"));
        assert!(!host_is_allowed("https://evil.example?x=huggingface.co"));
        assert!(!host_is_allowed("https://evil.example#huggingface.co"));
    }
}

#[cfg(test)]
mod guard_tests {
    use super::{DownloadGuard, DOWNLOAD_IN_PROGRESS};
    use std::sync::atomic::Ordering;

    /// Every assertion about the guard lives in one test on purpose: it is a
    /// process-wide flag, and Rust runs tests in parallel, so two tests taking
    /// it would fight over the same static and fail at random.
    #[test]
    fn the_download_guard_admits_one_holder_and_always_releases() {
        assert!(
            !DOWNLOAD_IN_PROGRESS.load(Ordering::Acquire),
            "nothing should hold the guard before this test"
        );

        {
            let _first = DownloadGuard::acquire().expect("the guard should be free");
            assert!(
                DownloadGuard::acquire().is_none(),
                "a second download must be refused while one is running"
            );
        }

        assert!(
            !DOWNLOAD_IN_PROGRESS.load(Ordering::Acquire),
            "leaving the scope must release the guard"
        );
        assert!(
            DownloadGuard::acquire().is_some(),
            "the guard must be reusable after a completed download"
        );
        assert!(
            !DOWNLOAD_IN_PROGRESS.load(Ordering::Acquire),
            "the temporary above should have released on drop"
        );

        /* The reason the guard exists rather than a store at each exit: a panic
        in the download thread used to leave the flag set, and the app then
        refused every later download until it was restarted, saying one was
        already in progress when none was. */
        let panicked = std::panic::catch_unwind(|| {
            let _guard = DownloadGuard::acquire().expect("the guard should be free");
            panic!("the download thread died");
        });
        assert!(panicked.is_err(), "the panic should have propagated");
        assert!(
            !DOWNLOAD_IN_PROGRESS.load(Ordering::Acquire),
            "a panicking download must not leave downloads blocked for good"
        );
    }
}
