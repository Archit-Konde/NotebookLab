/*
 * Name: state.rs
 * Purpose: Application state managed by Tauri's state system (app.manage()).
 * Description: WAL mode verified after activation. Foreign keys enabled for
 *   cascade deletes. Migrations run from bundled SQL files at
 *   startup. Provider router sits behind an RwLock so long-running
 *   LLM calls (reads) never block each other; only provider
 *   registration takes the write lock.
 * Tech Stack: Rust, Tauri v2, SQLite
 * License: MIT
 * Authors: Amey Thakur (https://github.com/Amey-Thakur)
 *          Archit Konde (https://github.com/Archit-Konde)
 * Date: 2026-07-12
 */

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};

use rusqlite::Connection;
use tauri::path::BaseDirectory;
use tauri::{AppHandle, Manager};

use crate::error::{AppError, AppResult};
use crate::parsers::image_ocr_parser::{OcrEngineHandle, DETECTION_MODEL_FILE};
use crate::providers::ProviderRouter;

pub struct AppState {
    pub db: Mutex<Connection>,
    pub providers: RwLock<ProviderRouter>,
    /// Bearer token for the local REST API, generated fresh each session.
    pub api_token: String,
    /// The OCR engine, built once on first image import and cached. `None` until
    /// then, or whenever the models are not installed (image import degrades to
    /// a clear error in that case). Loading the models is the expensive step, so
    /// it happens lazily rather than at startup.
    ocr: Mutex<Option<Arc<OcrEngineHandle>>>,
    /// Long AI work, tracked so it survives the user leaving the page that
    /// started it and so several features can generate at once.
    pub jobs: crate::services::job_service::JobRegistry,
}

impl AppState {
    /// Acquire the database connection with graceful poison handling.
    pub fn conn(&self) -> AppResult<MutexGuard<'_, Connection>> {
        self.db
            .lock()
            .map_err(|_| AppError::Internal("Database lock poisoned".into()))
    }

    /// Acquire shared read access to the provider router. Chat completions and
    /// embeddings only need read access, so concurrent AI calls do not serialize.
    pub fn provider_read(&self) -> AppResult<RwLockReadGuard<'_, ProviderRouter>> {
        self.providers
            .read()
            .map_err(|_| AppError::Internal("Provider lock poisoned".into()))
    }

    /// Acquire exclusive write access to the provider router (registration only).
    pub fn provider_write(&self) -> AppResult<RwLockWriteGuard<'_, ProviderRouter>> {
        self.providers
            .write()
            .map_err(|_| AppError::Internal("Provider lock poisoned".into()))
    }

    /// The shared OCR engine, or `None` when the models are not installed.
    ///
    /// Built once from the model files and cached; a successful build is reused
    /// for the session. Failures are not cached, so if the models appear later
    /// (for example after a first-run download) a subsequent import can still
    /// pick them up. The returned handle is an owned clone, so the state lock is
    /// released before the caller goes on to touch the database.
    pub fn ocr_engine(&self, app: &AppHandle) -> Option<Arc<OcrEngineHandle>> {
        let mut cached = self.ocr.lock().ok()?;
        if let Some(engine) = cached.as_ref() {
            return Some(engine.clone());
        }

        let model_dir = resolve_ocr_model_dir(app)?;
        match OcrEngineHandle::from_model_dir(&model_dir) {
            Ok(handle) => {
                let engine = Arc::new(handle);
                *cached = Some(engine.clone());
                Some(engine)
            }
            Err(e) => {
                tracing::debug!("OCR engine unavailable: {e}");
                None
            }
        }
    }
}

/// Find the directory holding the OCR model files: the bundled resources first
/// (always present after install), then the app data directory (dev or a
/// first-run download). Returns `None` when neither has the models.
fn resolve_ocr_model_dir(app: &AppHandle) -> Option<PathBuf> {
    if let Ok(dir) = app.path().resolve("models/ocr", BaseDirectory::Resource) {
        if dir.join(DETECTION_MODEL_FILE).exists() {
            return Some(dir);
        }
    }
    if let Ok(data_dir) = app.path().app_data_dir() {
        let dir = data_dir.join("ocr");
        if dir.join(DETECTION_MODEL_FILE).exists() {
            return Some(dir);
        }
    }
    None
}

impl AppState {
    /// Initialize all application state. Called once during app setup.
    /// The REST API token is generated here so both the HTTP server and the
    /// get_api_token command can hand out the same value.
    pub fn initialize(app: &AppHandle) -> Result<Self, Box<dyn std::error::Error>> {
        let data_dir = app
            .path()
            .app_data_dir()
            .map_err(|e| AppError::Internal(format!("Failed to resolve data dir: {e}")))?;

        std::fs::create_dir_all(&data_dir)?;

        let db_path = data_dir.join("notebooklab.db");
        tracing::info!("Database path: {}", db_path.display());

        let conn = Self::open_or_recover(&db_path)?;

        let provider_router = ProviderRouter::new();

        Ok(Self {
            db: Mutex::new(conn),
            providers: RwLock::new(provider_router),
            api_token: format!("nbl-api-{}", uuid::Uuid::new_v4().simple()),
            ocr: Mutex::new(None),
            jobs: crate::services::job_service::JobRegistry::new(),
        })
    }

    /// Open the database, and if the file turns out to be unreadable, set it
    /// aside and start a fresh one.
    ///
    /// Everything here used to run inline, and every failure aborted `setup`,
    /// which means the window never appears. A database damaged by a power cut
    /// or a half-written file therefore left the app unable to start at all,
    /// showing a line like "database disk image is malformed" and offering
    /// nothing to do about it. The app opening with an empty library is a bad
    /// day; the app never opening again is a worse one.
    ///
    /// The damaged file is renamed, never deleted, so nothing is destroyed and
    /// a copy remains for anyone who wants to try to recover it. Only genuine
    /// corruption triggers this: a locked or unreadable file is a different
    /// problem, and quietly starting fresh there would hide it.
    fn open_or_recover(
        db_path: &std::path::Path,
    ) -> Result<Connection, Box<dyn std::error::Error>> {
        match Self::open_database(db_path) {
            Ok(conn) => Ok(conn),
            Err(error) if Self::is_corruption(&error) => {
                let preserved = Self::set_corrupt_database_aside(db_path)?;
                tracing::error!(
                    "The database at {} could not be read ({error}). It has been kept at {} and                      a new, empty one created in its place.",
                    db_path.display(),
                    preserved.display()
                );
                Ok(Self::open_database(db_path)?)
            }
            Err(error) => Err(Box::new(error)),
        }
    }

    /// Open a connection, apply the connection pragmas, and bring the schema up
    /// to date.
    fn open_database(db_path: &std::path::Path) -> Result<Connection, rusqlite::Error> {
        let conn = Connection::open(db_path)?;

        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA busy_timeout = 5000;
             PRAGMA foreign_keys = ON;",
        )?;

        let wal_mode: String = conn.query_row("PRAGMA journal_mode", [], |r| r.get(0))?;
        if wal_mode != "wal" {
            tracing::warn!("WAL mode not active, got: {wal_mode}");
        }

        Self::run_migrations(&conn)?;
        Ok(conn)
    }

    /// Whether the error says the file is not a usable database, as opposed to
    /// being locked, missing, or unreadable for a reason worth reporting.
    fn is_corruption(error: &rusqlite::Error) -> bool {
        matches!(
            error,
            rusqlite::Error::SqliteFailure(inner, _)
                if matches!(
                    inner.code,
                    rusqlite::ErrorCode::DatabaseCorrupt | rusqlite::ErrorCode::NotADatabase
                )
        )
    }

    /// Rename the damaged database out of the way, with its write-ahead log and
    /// shared-memory files, and return where it went. Those two companions have
    /// to move with it: left behind, SQLite would try to replay them into the
    /// new database and damage that one too.
    fn set_corrupt_database_aside(
        db_path: &std::path::Path,
    ) -> Result<PathBuf, Box<dyn std::error::Error>> {
        let stamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
        let preserved = db_path.with_file_name(format!("notebooklab-unreadable-{stamp}.db"));
        std::fs::rename(db_path, &preserved)?;

        for suffix in ["-wal", "-shm"] {
            let companion = PathBuf::from(format!("{}{suffix}", db_path.display()));
            if companion.exists() {
                let moved = PathBuf::from(format!("{}{suffix}", preserved.display()));
                let _ = std::fs::rename(&companion, &moved);
            }
        }
        Ok(preserved)
    }

    fn run_migrations(conn: &Connection) -> Result<(), rusqlite::Error> {
        let migrations = [
            include_str!("../resources/migrations/001_initial_schema.sql"),
            include_str!("../resources/migrations/002_chat_tables.sql"),
            include_str!("../resources/migrations/003_fts5_search.sql"),
            include_str!("../resources/migrations/004_embeddings.sql"),
            include_str!("../resources/migrations/005_canvas.sql"),
            include_str!("../resources/migrations/006_providers.sql"),
            include_str!("../resources/migrations/007_chunk_positions.sql"),
        ];

        /* Track the applied version in the database so each migration runs
        exactly once. Re-running them every launch is wasteful and, for the
        non-idempotent FTS backfill in 003, would append duplicate postings and
        corrupt search ranking. */
        let current: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        let mut applied = 0;
        for (index, sql) in migrations.iter().enumerate() {
            let version = (index + 1) as i64;
            if version <= current {
                continue;
            }
            conn.execute_batch(sql)?;
            /* user_version takes a literal, not a bind parameter; version is a
            computed integer, so this is safe. */
            conn.execute_batch(&format!("PRAGMA user_version = {version};"))?;
            applied += 1;
        }

        tracing::info!(
            "Database migrations: {applied} applied, schema at version {}",
            migrations.len()
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    /// The repair in 007, applied to a database that already carries it, so the
    /// bug shape can be created first and then fixed.
    const RENUMBER: &str = include_str!("../resources/migrations/007_chunk_positions.sql");

    fn migrated() -> Connection {
        let conn = Connection::open_in_memory().expect("open");
        super::AppState::run_migrations(&conn).expect("migrate");
        conn
    }

    fn seed_document(conn: &Connection, id: &str) {
        conn.execute(
            "INSERT OR IGNORE INTO notebooks (id, name) VALUES ('nb', 'Notebook')",
            [],
        )
        .expect("notebook");
        conn.execute(
            "INSERT INTO documents (id, notebook_id, title, file_path, file_hash, file_type, file_size)
             VALUES (?1, 'nb', ?1, '/x', ?1, 'pdf', 1)",
            [id],
        )
        .expect("document");
    }

    fn positions(conn: &Connection, document: &str) -> Vec<i64> {
        let mut stmt = conn
            .prepare("SELECT position FROM chunks WHERE document_id = ?1 ORDER BY position")
            .expect("prepare");
        let rows = stmt
            .query_map([document], |row| row.get(0))
            .expect("query")
            .collect::<Result<Vec<i64>, _>>()
            .expect("collect");
        rows
    }

    /// A directory of its own per test, since these write real files.
    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("notebooklab-state-{name}"));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_healthy_database_opens_and_is_left_where_it_is() {
        let dir = scratch("healthy");
        let db = dir.join("notebooklab.db");

        let conn = super::AppState::open_or_recover(&db).expect("first open");
        conn.execute("INSERT INTO notebooks (id, name) VALUES ('nb', 'Kept')", [])
            .unwrap();
        drop(conn);

        let conn = super::AppState::open_or_recover(&db).expect("second open");
        let name: String = conn
            .query_row("SELECT name FROM notebooks WHERE id = 'nb'", [], |r| {
                r.get(0)
            })
            .expect("the notebook should still be there");
        assert_eq!(name, "Kept", "a healthy database must not be replaced");

        let set_aside: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("unreadable"))
            .collect();
        assert!(set_aside.is_empty(), "nothing should have been set aside");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn an_unreadable_database_is_set_aside_and_the_app_still_starts() {
        /* Without this the whole of setup fails and the window never appears,
        so a file damaged by a power cut left the app unable to open at all. */
        let dir = scratch("corrupt");
        let db = dir.join("notebooklab.db");
        std::fs::write(&db, b"this is not a database, not even slightly").unwrap();

        let conn = super::AppState::open_or_recover(&db).expect("the app must still start");

        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(version, 7, "the replacement should be fully migrated");

        let names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert!(
            names
                .iter()
                .any(|n| n.contains("unreadable") && n.ends_with(".db")),
            "the damaged file must be kept, not deleted: {names:?}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn setting_a_database_aside_takes_its_companion_files() {
        /* The write-ahead log and shared-memory file belong to the database
        they were written for, and a log left beside the replacement is a log
        SQLite may try to replay into it.

        This calls the move directly rather than going through a failed open,
        because it cannot be observed that way: closing the connection after the
        failed open makes SQLite delete the log itself, so it is already gone by
        the time the file is renamed. The move still has to be right for the
        cases where a log does survive, such as a process killed mid-write. */
        let dir = scratch("companions");
        let db = dir.join("notebooklab.db");
        std::fs::write(&db, b"not a database").unwrap();
        std::fs::write(dir.join("notebooklab.db-wal"), b"stale log").unwrap();
        std::fs::write(dir.join("notebooklab.db-shm"), b"stale shm").unwrap();

        let preserved = super::AppState::set_corrupt_database_aside(&db).unwrap();

        assert!(preserved.exists(), "the database itself must be kept");
        let wal = std::path::PathBuf::from(format!("{}-wal", preserved.display()));
        let shm = std::path::PathBuf::from(format!("{}-shm", preserved.display()));
        assert!(wal.exists(), "the write-ahead log must move with it");
        assert!(shm.exists(), "the shared-memory file must move with it");
        assert!(
            !dir.join("notebooklab.db-wal").exists(),
            "no stale log may be left where the new database will be created"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn only_a_broken_file_counts_as_corruption() {
        /* Starting fresh on any error would hide a locked or unreadable file
        behind an empty library, which looks like silent data loss. */
        let dir = scratch("classify");
        let db = dir.join("notebooklab.db");
        std::fs::write(&db, b"not a database").unwrap();

        let error = super::AppState::open_database(&db).expect_err("should refuse to open");
        assert!(
            super::AppState::is_corruption(&error),
            "a file that is not a database must be recognised as such: {error}"
        );

        /* A plain failure to open, such as a path that is a directory, is not
        corruption and must be reported rather than swept aside. */
        let as_dir = dir.join("a-directory.db");
        std::fs::create_dir_all(&as_dir).unwrap();
        if let Err(other) = super::AppState::open_database(&as_dir) {
            assert!(
                !super::AppState::is_corruption(&other),
                "an unopenable path is not corruption: {other}"
            );
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn every_migration_applies_to_a_fresh_database() {
        let conn = migrated();
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("version");
        assert_eq!(version, 7, "user_version must track the migration count");
    }

    #[test]
    fn renumbering_repairs_a_document_imported_before_the_fix() {
        /* Before v0.8.1 the chunker was called once per page and numbered from
        zero each time, so a three-page document held three chunks numbered 0,
        three numbered 1, and so on. Readers order by position, so the pages
        interleaved. */
        let conn = migrated();
        seed_document(&conn, "broken");
        for page in 1..=3 {
            for position in 0..3 {
                conn.execute(
                    "INSERT INTO chunks (id, document_id, content, position, page_number)
                     VALUES (?1, 'broken', ?2, ?3, ?4)",
                    rusqlite::params![
                        format!("b{page}{position}"),
                        format!("page {page} part {position}"),
                        position,
                        page
                    ],
                )
                .expect("chunk");
            }
        }

        conn.execute_batch(RENUMBER).expect("renumber");

        assert_eq!(positions(&conn, "broken"), (0..9).collect::<Vec<i64>>());

        let mut stmt = conn
            .prepare("SELECT content FROM chunks WHERE document_id='broken' ORDER BY position")
            .expect("prepare");
        let order = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .expect("query")
            .collect::<Result<Vec<String>, _>>()
            .expect("collect");
        let expected: Vec<String> = (1..=3)
            .flat_map(|page| (0..3).map(move |part| format!("page {page} part {part}")))
            .collect();
        assert_eq!(order, expected, "pages must read in order, not interleave");
    }

    #[test]
    fn renumbering_leaves_a_correctly_numbered_document_alone() {
        let conn = migrated();
        seed_document(&conn, "fine");
        for position in 0..5 {
            conn.execute(
                "INSERT INTO chunks (id, document_id, content, position, page_number)
                 VALUES (?1, 'fine', 'text', ?2, 1)",
                rusqlite::params![format!("f{position}"), position],
            )
            .expect("chunk");
        }

        conn.execute_batch(RENUMBER).expect("renumber");

        assert_eq!(positions(&conn, "fine"), (0..5).collect::<Vec<i64>>());
    }

    #[test]
    fn renumbering_runs_twice_without_changing_anything_further() {
        /* Migrations are guarded by user_version, but a repair that only works
        once is a repair that cannot be re-run to check it. */
        let conn = migrated();
        seed_document(&conn, "twice");
        for page in 1..=2 {
            for position in 0..2 {
                conn.execute(
                    "INSERT INTO chunks (id, document_id, content, position, page_number)
                     VALUES (?1, 'twice', 'text', ?2, ?3)",
                    rusqlite::params![format!("t{page}{position}"), position, page],
                )
                .expect("chunk");
            }
        }

        conn.execute_batch(RENUMBER).expect("first");
        let after_first = positions(&conn, "twice");
        conn.execute_batch(RENUMBER).expect("second");

        assert_eq!(after_first, (0..4).collect::<Vec<i64>>());
        assert_eq!(positions(&conn, "twice"), after_first);
    }

    #[test]
    fn full_text_search_still_finds_a_renumbered_chunk() {
        /* The FTS index is keyed on rowid and synced by insert and delete
        triggers. Renumbering must not disturb it, which holds only because the
        repair updates a column and never rewrites a row. */
        let conn = migrated();
        seed_document(&conn, "search");
        for page in 1..=2 {
            for position in 0..2 {
                conn.execute(
                    "INSERT INTO chunks (id, document_id, content, position, page_number)
                     VALUES (?1, 'search', 'quantum entanglement', ?2, ?3)",
                    rusqlite::params![format!("s{page}{position}"), position, page],
                )
                .expect("chunk");
            }
        }

        conn.execute_batch(RENUMBER).expect("renumber");

        let hits: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM chunks_fts f JOIN chunks c ON c.rowid = f.rowid
                 WHERE chunks_fts MATCH 'entanglement'",
                [],
                |row| row.get(0),
            )
            .expect("search");
        assert_eq!(hits, 4, "every chunk must still be reachable by search");
    }
}
