/*
 * Name: lib.rs
 * Purpose: Application builder.
 * Description: Registers all plugins, managed state, and command handlers.
 *   This is the central wiring point for the entire backend. Every
 *   module is registered here. State (database pool, model
 *   handles) is injected via app.manage(). Commands are registered
 *   via invoke_handler(). Plugins extend Tauri's capabilities.
 * Tech Stack: Rust, Tauri v2, SQLite
 * License: MIT
 * Authors: Amey Thakur (https://github.com/Amey-Thakur)
 *          Archit Konde (https://github.com/Archit-Konde)
 * Date: 2026-07-12
 */

pub mod api;
pub mod commands;
pub mod database;
pub mod error;
pub mod parsers;
pub mod providers;
pub mod services;
pub mod state;
pub mod utils;

use tauri::Manager;

use services::sidecar_service::SidecarManager;
use state::AppState;

/// Build and run the Tauri application.
/// Called from main.rs on desktop, or from a test harness.
pub fn run() {
    let log_filter = if cfg!(debug_assertions) {
        "notebooklab=debug,tauri=info"
    } else {
        "notebooklab=info,tauri=warn"
    };

    /* Logs tee to the console and to an in-memory ring buffer that powers the
    Settings log view; ANSI stays off so captured lines read cleanly. */
    tracing_subscriber::fmt()
        .with_env_filter(log_filter)
        .with_ansi(false)
        .with_writer(utils::log_buffer::TeeMakeWriter)
        .init();

    tracing::info!("Starting NotebookLab v{}", env!("CARGO_PKG_VERSION"));

    tauri::Builder::default()
        /* Single instance first, so "Open with NotebookLab" on a second file
        focuses the running window and forwards the path instead of starting a
        second app fighting over the database. */
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            use tauri::Emitter;
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_focus();
            }
            let files = commands::system_commands::importable_files(argv.into_iter().skip(1));
            if !files.is_empty() {
                app.emit("open-files", files).ok();
            }
        }))
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let app_state = AppState::initialize(app.handle())?;

            /* Files this launch was asked to open ("Open with NotebookLab").
            Held until the frontend mounts and collects them. */
            let startup_files =
                commands::system_commands::importable_files(std::env::args().skip(1));
            app.manage(commands::system_commands::StartupFiles(
                std::sync::Mutex::new(startup_files),
            ));

            /* Create sample notebook on first run */
            if let Ok(conn) = app_state.conn() {
                services::first_run_service::ensure_sample_notebook(&conn)
                    .unwrap_or_else(|e| tracing::warn!("First-run setup failed: {e}"));
            }

            /* Start the local REST API server with its own read-only DB
            connection. It shares the session token stored in AppState so
            the Settings page can show users how to authenticate. */
            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|e| format!("Failed to resolve data dir: {e}"))?;
            let db_path = data_dir.join("notebooklab.db");
            api::server::start_api_server(db_path, app_state.api_token.clone());

            app.manage(app_state);
            app.manage(SidecarManager::new());

            /* Restore saved providers (cloud connections, model choices) and
            then auto-detect local LLM providers, on a background thread. Runs
            after manage() so state is accessible via Tauri's Arc wrapper, and
            restore runs first so the user's saved active model wins over
            whatever local server answers a probe. This avoids blocking the UI
            (up to 1.5s if all probes timeout). */
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let state: tauri::State<'_, AppState> = handle.state();
                /* Finish with the database before taking the provider write
                lock; holding both at once risks a lock-order deadlock. */
                let saved = state
                    .conn()
                    .map(|conn| services::provider_config_service::load_saved_configs(&conn))
                    .unwrap_or_default();
                /* Apply the saved configs first (fast, no network), then read
                the resulting names, release the lock, and probe local servers
                WITHOUT holding the lock. Registering the results takes the
                lock again only for a network-free moment. This keeps the
                provider lock free during slow probes so chat and every other
                feature never block on startup detection. */
                let existing = if let Ok(mut providers) = state.provider_write() {
                    services::provider_config_service::apply_saved_configs(&mut providers, saved);
                    services::auto_setup_service::provider_names(&providers)
                } else {
                    Vec::new()
                };
                let detected = services::auto_setup_service::probe_local_providers(&existing);
                if !detected.is_empty() {
                    if let Ok(mut providers) = state.provider_write() {
                        services::auto_setup_service::register_detected(&mut providers, detected);
                    }
                }

                /* Keep looking. Detection used to stop here, so a server the
                user started a moment later was never noticed and the app
                insisted there was no model. */
                services::auto_setup_service::watch_local_providers(handle.clone());
            });

            /* Check for updates in the background. When a new version has
            been downloaded and staged, the status bar offers a restart;
            registering the plugin alone never checks anything. */
            let update_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                use tauri::Emitter;
                use tauri_plugin_updater::UpdaterExt;

                let Ok(updater) = update_handle.updater() else {
                    return;
                };
                match updater.check().await {
                    Ok(Some(update)) => {
                        let version = update.version.clone();
                        tracing::info!("Update available: v{version}");
                        if update.download_and_install(|_, _| {}, || {}).await.is_ok() {
                            update_handle.emit("update-ready", version).ok();
                        }
                    }
                    Ok(None) => tracing::debug!("App is up to date"),
                    Err(e) => tracing::debug!("Update check skipped: {e}"),
                }
            });

            tracing::info!("Application state initialized");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::system_commands::get_app_version,
            commands::system_commands::get_data_directory,
            commands::system_commands::get_api_token,
            commands::system_commands::restart_app,
            commands::system_commands::check_for_updates,
            commands::notebook_commands::list_notebooks,
            commands::notebook_commands::get_notebook,
            commands::notebook_commands::create_notebook,
            commands::notebook_commands::update_notebook,
            commands::notebook_commands::delete_notebook,
            commands::note_commands::list_notes,
            commands::note_commands::get_note,
            commands::note_commands::create_note,
            commands::note_commands::update_note,
            commands::note_commands::delete_note,
            commands::note_commands::get_backlinks,
            commands::note_commands::resolve_wiki_link,
            commands::note_commands::export_note,
            commands::note_commands::list_recent_notes,
            commands::note_commands::get_notes_graph,
            commands::audio_commands::audio_export_extension,
            commands::audio_commands::export_audio_file,
            commands::canvas_commands::get_or_create_canvas,
            commands::canvas_commands::update_canvas,
            commands::canvas_commands::read_image_data_url,
            commands::share_commands::export_notebook,
            commands::share_commands::import_notebook,
            commands::document_commands::import_document,
            commands::document_commands::list_documents,
            commands::document_commands::list_recent_documents,
            commands::document_commands::delete_document,
            commands::document_commands::get_document_chunks,
            commands::document_commands::get_chunk_count,
            commands::chat_commands::start_chat,
            commands::chat_commands::send_chat_message,
            commands::chat_commands::list_conversations,
            commands::chat_commands::get_chat_messages,
            commands::chat_commands::get_message_citations,
            commands::chat_commands::delete_conversation,
            commands::search_commands::search,
            commands::thinking_commands::generate_idea_space,
            commands::thinking_commands::generate_socratic_questions,
            commands::job_commands::list_jobs,
            commands::job_commands::cancel_job,
            commands::job_commands::clear_finished_jobs,
            commands::studio_commands::generate_studio,
            commands::transform_commands::transform_document,
            commands::prompt_commands::craft_prompt,
            commands::model_commands::list_providers,
            commands::model_commands::register_provider,
            commands::model_commands::set_active_provider,
            commands::model_commands::get_active_provider_name,
            commands::model_commands::detect_providers,
            commands::model_commands::delete_provider,
            commands::model_commands::list_saved_providers,
            commands::model_commands::get_usage_stats,
            commands::model_commands::set_auto_model,
            commands::ollama_commands::ollama_status,
            commands::ollama_commands::ollama_installed_models,
            commands::ollama_commands::ollama_pull_model,
            commands::ollama_commands::ollama_pull_state,
            commands::ollama_commands::ollama_delete_model,
            commands::system_commands::get_hardware_profile,
            commands::system_commands::get_recent_logs,
            commands::system_commands::take_startup_files,
            commands::skills_commands::fetch_agent_skills,
            commands::skills_commands::fetch_agent_skill_body,
            commands::download_commands::list_gguf_catalog,
            commands::download_commands::download_gguf_model,
            commands::podcast_commands::generate_podcast,
            commands::download_commands::has_local_model,
            commands::sidecar_commands::sidecar_available,
            commands::sidecar_commands::get_sidecar_status,
            commands::sidecar_commands::start_sidecar,
            commands::sidecar_commands::stop_sidecar,
            commands::sidecar_commands::list_local_models,
        ])
        .build(tauri::generate_context!())
        .map(|app| {
            app.run(|app_handle, event| {
                /* Terminate the llama-server child on exit so it never
                outlives the app as an orphaned process. try_state: if setup
                failed before manage(), there is nothing to clean up. */
                if let tauri::RunEvent::Exit = event {
                    if let Some(sidecar) = app_handle.try_state::<SidecarManager>() {
                        sidecar.shutdown();
                    }
                }
            });
        })
        .unwrap_or_else(|e| {
            tracing::error!("Failed to run NotebookLab: {e}");
            eprintln!("Fatal error: {e}");
        });
}
