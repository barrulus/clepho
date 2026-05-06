//! Clepho daemon for background task processing.
//!
//! This daemon runs scheduled tasks in the background, allowing:
//! - Scheduled directory scans
//! - Batch LLM description processing
//! - Face detection on new photos
//!
//! The daemon communicates with the TUI via the shared SQLite database.
//!
//! ## Usage
//!
//! ```bash
//! clepho-daemon              # Run in foreground
//! clepho-daemon --once       # Process pending tasks once and exit
//! ```
//!
//! ## systemd Service
//!
//! Install the service file and enable:
//! ```bash
//! sudo cp clepho.service /etc/systemd/system/
//! sudo systemctl enable --now clepho
//! ```

use anyhow::{Context, Result};
use chrono::{Local, NaiveTime};
use std::path::PathBuf;
use std::thread;
use std::time::Duration;
use tracing::{error, info, warn};
use walkdir::WalkDir;

use clepho::config::Config;
use clepho::db::{Database, ScheduledTask, ScheduledTaskType};

/// Daemon configuration
struct DaemonConfig {
    /// Poll interval for checking new tasks (seconds)
    poll_interval: u64,
    /// Run once and exit
    once: bool,
    /// Config path override
    config_path: Option<PathBuf>,
    /// Allow destructive schema reset on legacy DB
    reset_db: bool,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            poll_interval: 60,
            once: false,
            config_path: None,
            reset_db: false,
        }
    }
}

fn main() -> Result<()> {
    // Parse command line arguments
    let daemon_config = parse_args();

    // Initialize logging
    init_logging()?;

    info!("Clepho daemon starting...");

    // Load application config
    let config = load_config(&daemon_config)?;
    info!("Config loaded");

    // Pre-flight: detect schema generation against the SQLite file directly.
    // The v2 schema cannot be auto-applied; legacy DBs require an explicit
    // --reset-db flag (which is destructive).
    preflight_schema_check(&config, daemon_config.reset_db)?;

    // Open database
    let db = Database::open(&config.database)?;
    db.initialize()?;
    info!("Database opened at {:?}", config.db_path());

    // Main loop — v2 pipeline scheduler.
    if daemon_config.once {
        info!("Running in single-shot mode");
        run_pipeline_pass(&db, &config)?;
    } else {
        info!(
            "Running in daemon mode, polling every {} seconds",
            daemon_config.poll_interval
        );
        run_pipeline_loop(&db, &config, daemon_config.poll_interval)?;
    }

    info!("Clepho daemon stopped");
    Ok(())
}

fn parse_args() -> DaemonConfig {
    let args: Vec<String> = std::env::args().collect();
    let mut config = DaemonConfig::default();

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--once" | "-1" => {
                config.once = true;
            }
            "--interval" | "-i" => {
                if i + 1 < args.len() {
                    if let Ok(interval) = args[i + 1].parse() {
                        config.poll_interval = interval;
                    }
                    i += 1;
                }
            }
            "--config" | "-c" => {
                if i + 1 < args.len() {
                    config.config_path = Some(PathBuf::from(&args[i + 1]));
                    i += 1;
                }
            }
            "--reset-db" => {
                config.reset_db = true;
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            _ => {
                eprintln!("Unknown argument: {}", args[i]);
                print_help();
                std::process::exit(1);
            }
        }
        i += 1;
    }

    config
}

fn print_help() {
    println!(
        r#"clepho-daemon - Background task processor for Clepho

USAGE:
    clepho-daemon [OPTIONS]

OPTIONS:
    --once, -1          Process pending tasks once and exit
    --interval, -i N    Poll interval in seconds (default: 60)
    --config, -c PATH   Path to config file
    --reset-db          Drop and recreate the DB if it has the legacy v1 schema
                        (DESTRUCTIVE — all photos must be rescanned)
    --help, -h          Show this help message

ENVIRONMENT:
    CLEPHO_CONFIG       Path to config file (overrides default location)
    RUST_LOG            Log level (trace, debug, info, warn, error)

The daemon processes scheduled tasks stored in the database:
  - Directory scans
  - Batch LLM description processing
  - Face detection

Install as systemd service:
    sudo cp clepho.service /etc/systemd/system/
    sudo systemctl enable --now clepho
"#
    );
}

fn init_logging() -> Result<()> {
    use tracing_subscriber::prelude::*;

    // Try to use journald on Linux
    #[cfg(target_os = "linux")]
    {
        if let Ok(journald_layer) = tracing_journald::layer() {
            let subscriber = tracing_subscriber::registry()
                .with(journald_layer)
                .with(tracing_subscriber::filter::EnvFilter::new(
                    std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string())
                ));
            tracing::subscriber::set_global_default(subscriber)
                .context("Failed to set tracing subscriber")?;
            return Ok(());
        }
    }

    // Fall back to stderr
    let subscriber = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::filter::EnvFilter::new(
                std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string())
            )
        )
        .finish();

    tracing::subscriber::set_global_default(subscriber)
        .context("Failed to set tracing subscriber")?;

    Ok(())
}

fn load_config(daemon_config: &DaemonConfig) -> Result<Config> {
    match &daemon_config.config_path {
        Some(path) => {
            Config::load_from(path)
                .context("Failed to load config file")
        }
        None => {
            Config::load()
                .context("Failed to load config")
        }
    }
}

/// Detect schema generation on the SQLite file before the rest of the daemon
/// touches it. Legacy DBs (gen<2) refuse to start unless --reset-db is set;
/// Newer-than-supported always refuses.
///
/// NOTE: this currently only applies to the SQLite backend. Postgres goes
/// through Database::open unmodified.
fn preflight_schema_check(config: &Config, reset_db: bool) -> Result<()> {
    use clepho::db::migrate::{
        apply_v2_schema, detect_schema_state, reset_to_v2, SchemaState, CURRENT_GENERATION,
    };

    #[cfg(feature = "postgres")]
    {
        if config.database.backend == clepho::config::DatabaseType::Postgresql {
            return Ok(()); // Postgres has its own migration story
        }
    }

    let path = &config.database.sqlite_path;
    if !path.exists() {
        return Ok(()); // brand-new install; Database::open will create it
    }

    let conn = rusqlite::Connection::open(path)
        .with_context(|| format!("preflight: open {}", path.display()))?;

    match detect_schema_state(&conn)? {
        SchemaState::Empty => {
            apply_v2_schema(&conn).context("preflight: apply v2 to empty DB")?;
            info!("Initialized fresh v2 schema at {}", path.display());
        }
        SchemaState::Current => {}
        SchemaState::Legacy => {
            if !reset_db {
                eprintln!(
                    "ERROR: Database at {} has the legacy v1 schema. clepho v2 cannot \
                     auto-migrate. Pass --reset-db to drop the existing data and \
                     start fresh, or open the TUI to confirm interactively.",
                    path.display()
                );
                std::process::exit(2);
            }
            warn!(
                "Resetting database at {} to v2 (--reset-db flag set)",
                path.display()
            );
            reset_to_v2(&conn).context("preflight: reset_to_v2")?;
        }
        SchemaState::Newer(v) => {
            eprintln!(
                "ERROR: Database schema is newer ({}) than this binary supports ({}). \
                 Upgrade clepho.",
                v, CURRENT_GENERATION
            );
            std::process::exit(3);
        }
    }
    Ok(())
}

/// Build the v2 pipeline scheduler from config and an open Database. Returns
/// the scheduler plus the FolderWatcher (None when no managed folders exist).
fn build_scheduler(
    db: &Database,
    config: &Config,
) -> Result<(
    clepho::pipeline::scheduler::Scheduler,
    Option<clepho::pipeline::watcher::FolderWatcher>,
    std::sync::Arc<clepho::pipeline::stages::scan::ScanStage>,
)> {
    use clepho::db::clock::SystemClock;
    use clepho::pipeline::circuit_breaker::CircuitBreaker;
    use clepho::pipeline::llm_adapter::LlmClientAdapter;
    use clepho::pipeline::log::JsonlAppender;
    use clepho::pipeline::scheduler::Scheduler;
    use clepho::pipeline::stages::{
        exif::ExifStage, index::IndexStage, llm::LlmStage, scan::ScanStage, thumb::ThumbStage,
        Stage,
    };
    use clepho::pipeline::watcher::FolderWatcher;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;

    let conn = db
        .raw_sqlite_conn()
        .ok_or_else(|| anyhow::anyhow!("v2 daemon requires SQLite backend"))?;

    // Resolve directories
    let log_dir = config
        .logging
        .log_dir
        .clone()
        .unwrap_or_else(JsonlAppender::default_path);
    let log = Arc::new(JsonlAppender::new(log_dir).context("init JSONL log appender")?);
    log.rotate(config.logging.retention_days)
        .context("rotate JSONL logs")?;

    let thumb_dir = config
        .pipeline
        .thumbnail_cache_dir
        .clone()
        .unwrap_or_else(|| {
            dirs::cache_dir()
                .unwrap_or_default()
                .join("clepho/thumbnails")
        });

    // Build LLM client + adapter
    let llm_client = clepho::llm::client::LlmClient::from_config(&config.llm);

    // Stage instances
    let folders: Vec<std::path::PathBuf> = clepho::db::managed_folders::list(conn)?
        .into_iter()
        .map(|m| std::path::PathBuf::from(m.path))
        .collect();
    let scan = Arc::new(ScanStage::new(
        folders.clone(),
        config.pipeline.allowed_extensions.clone(),
    ));
    let exif = Arc::new(ExifStage);
    let thumb = Arc::new(ThumbStage::new(thumb_dir, config.pipeline.thumbnail_max_edge));
    let llm = Arc::new(LlmStage {
        client: Arc::new(LlmClientAdapter(llm_client)),
        global_prompt_override: None,
    });
    let index = Arc::new(IndexStage);

    let stages: Vec<Arc<dyn Stage>> = vec![
        scan.clone(),
        exif,
        thumb,
        llm,
        index,
    ];

    let breaker = Arc::new(CircuitBreaker::new(
        config.pipeline.circuit_breaker_threshold,
    ));

    let scheduler = Scheduler {
        stages,
        breaker,
        log,
        config: config.pipeline.clone(),
        clock: Arc::new(SystemClock),
        cancel: Arc::new(AtomicBool::new(false)),
    };

    let active: Vec<std::path::PathBuf> = clepho::db::managed_folders::list(conn)?
        .into_iter()
        .filter(|m| !m.paused)
        .map(|m| std::path::PathBuf::from(m.path))
        .collect();
    let watcher = if active.is_empty() {
        None
    } else {
        Some(FolderWatcher::watch(active).context("filesystem watcher")?)
    };

    Ok((scheduler, watcher, scan))
}

/// One pipeline pass over every managed folder (or once over no folder at all
/// if none configured — useful for `--once` against an empty DB).
fn run_pipeline_pass(db: &Database, config: &Config) -> Result<()> {
    let conn = db
        .raw_sqlite_conn()
        .ok_or_else(|| anyhow::anyhow!("v2 daemon requires SQLite backend"))?;
    let (scheduler, _watcher, scan) = build_scheduler(db, config)?;

    let folders = clepho::db::managed_folders::list(conn)?;
    if folders.is_empty() {
        info!("No managed folders configured; running scheduler against entire DB");
        scheduler.run_pass(conn, None)?;
        return Ok(());
    }

    for f in folders {
        if f.paused {
            continue;
        }
        let path = std::path::Path::new(&f.path);
        if let Err(e) = scan.discover(conn, path) {
            warn!("scan.discover({}) failed: {:#}", f.path, e);
        }
        let report = scheduler.run_pass(conn, Some(&f.path))?;
        info!(
            "{}: processed={} succeeded={} failed={} skipped_paused={}",
            f.path, report.processed, report.succeeded, report.failed, report.skipped_paused
        );
        clepho::db::managed_folders::set_last_run(
            conn,
            &f.path,
            &chrono::Utc::now().to_rfc3339(),
        )?;
    }
    Ok(())
}

/// Polling loop: tick every `poll_interval` seconds, draining filesystem
/// events between ticks so newly-arrived files get picked up immediately.
fn run_pipeline_loop(db: &Database, config: &Config, poll_interval: u64) -> Result<()> {
    let conn = db
        .raw_sqlite_conn()
        .ok_or_else(|| anyhow::anyhow!("v2 daemon requires SQLite backend"))?;
    let (scheduler, watcher, scan) = build_scheduler(db, config)?;

    loop {
        // 1. Drain filesystem events, discover any new files in their parents.
        if let Some(w) = &watcher {
            while let Ok(path) = w.events.try_recv() {
                if path.is_file() {
                    if let Some(parent) = path.parent() {
                        let _ = scan.discover(conn, parent);
                    }
                }
            }
        }

        // 2. Optionally honour configured working hours.
        if !should_process_now(config) {
            info!("Outside hours of operation, skipping this cycle");
            thread::sleep(Duration::from_secs(poll_interval));
            continue;
        }

        // 3. One scheduler pass per managed folder.
        let folders = match clepho::db::managed_folders::list(conn) {
            Ok(f) => f,
            Err(e) => {
                error!("managed_folders::list failed: {:#}", e);
                Vec::new()
            }
        };
        for f in folders {
            if f.paused {
                continue;
            }
            let path = std::path::Path::new(&f.path);
            if let Err(e) = scan.discover(conn, path) {
                warn!("scan.discover({}) failed: {:#}", f.path, e);
            }
            match scheduler.run_pass(conn, Some(&f.path)) {
                Ok(report) => {
                    if report.processed > 0 {
                        info!(
                            "{}: processed={} succeeded={} failed={}",
                            f.path, report.processed, report.succeeded, report.failed
                        );
                    }
                }
                Err(e) => error!("run_pass({}) failed: {:#}", f.path, e),
            }
            if let Err(e) = clepho::db::managed_folders::set_last_run(
                conn,
                &f.path,
                &chrono::Utc::now().to_rfc3339(),
            ) {
                warn!("set_last_run({}) failed: {:#}", f.path, e);
            }
        }

        thread::sleep(Duration::from_secs(poll_interval));
    }
}

#[allow(dead_code)] // Replaced by run_pipeline_loop; deleted by Task 29.
fn run_daemon_loop(
    db: &Database,
    config: &Config,
    poll_interval: u64,
) -> Result<()> {
    loop {
        // Check if we should process (based on hours of operation)
        if should_process_now(config) {
            if let Err(e) = process_pending_tasks(db, config) {
                error!("Error processing tasks: {}", e);
            }
        } else {
            info!("Outside hours of operation, skipping this cycle");
        }

        // Sleep until next poll
        thread::sleep(Duration::from_secs(poll_interval));
    }
}

fn should_process_now(config: &Config) -> bool {
    let (start, end) = match (config.schedule.default_hours_start, config.schedule.default_hours_end) {
        (Some(s), Some(e)) => (s, e),
        _ => return true, // No hours configured, always process
    };

    let now = Local::now().time();
    let start_time = NaiveTime::from_hms_opt(start as u32, 0, 0).unwrap_or(NaiveTime::MIN);
    let end_time = NaiveTime::from_hms_opt(end as u32, 0, 0).unwrap_or(NaiveTime::MIN);

    if start <= end {
        // Normal range: 9:00 - 17:00
        now >= start_time && now < end_time
    } else {
        // Overnight range: 22:00 - 06:00
        now >= start_time || now < end_time
    }
}

fn process_pending_tasks(db: &Database, config: &Config) -> Result<()> {
    let tasks = db.get_due_pending_tasks(10)?;

    if tasks.is_empty() {
        info!("No pending tasks");
        return Ok(());
    }

    info!("Found {} pending task(s)", tasks.len());

    for task in tasks {
        // Check task-specific hours of operation
        if !task_within_hours(&task) {
            info!("Task {} outside its hours of operation, skipping", task.id);
            continue;
        }

        info!("Processing task {} ({})", task.id, task.task_type.as_str());

        // Mark as running
        db.mark_task_running(task.id)?;

        // Execute the task
        let result = execute_task(&task, config, db);

        // Update status based on result
        match result {
            Ok(()) => {
                info!("Task {} completed successfully", task.id);
                db.mark_task_completed(task.id)?;
            }
            Err(e) => {
                error!("Task {} failed: {}", task.id, e);
                db.mark_task_failed(task.id, &e.to_string())?;
            }
        }
    }

    Ok(())
}

fn task_within_hours(task: &ScheduledTask) -> bool {
    let (start, end) = match (task.hours_start, task.hours_end) {
        (Some(s), Some(e)) => (s, e),
        _ => return true, // No hours configured for this task
    };

    let now = Local::now().time();
    let start_time = NaiveTime::from_hms_opt(start as u32, 0, 0).unwrap_or(NaiveTime::MIN);
    let end_time = NaiveTime::from_hms_opt(end as u32, 0, 0).unwrap_or(NaiveTime::MIN);

    if start <= end {
        now >= start_time && now < end_time
    } else {
        now >= start_time || now < end_time
    }
}

fn execute_task(task: &ScheduledTask, config: &Config, db: &Database) -> Result<()> {
    match task.task_type {
        ScheduledTaskType::Scan => execute_scan_task(&task.target_path, db),
        ScheduledTaskType::LlmBatch => execute_llm_batch_task(&task.target_path, config, db),
        ScheduledTaskType::FaceDetection => execute_face_detection_task(&task.target_path, db),
    }
}

fn execute_scan_task(target_path: &str, db: &Database) -> Result<()> {
    info!("Scanning directory: {}", target_path);

    let extensions = ["jpg", "jpeg", "png", "gif", "webp", "heic", "heif"];
    let mut count = 0;

    for entry in WalkDir::new(target_path).follow_links(true) {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path();
        let ext = path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_lowercase())
            .unwrap_or_default();

        if !extensions.contains(&ext.as_str()) {
            continue;
        }

        let path_str = path.to_string_lossy();

        // Check if already in database
        if db.photo_exists_by_path(&path_str) {
            continue;
        }

        // Insert basic photo record
        let filename = path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let directory = path.parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        let size = std::fs::metadata(path)
            .map(|m| m.len() as i64)
            .unwrap_or(0);

        db.insert_basic_photo(&path_str, &filename, &directory, size)?;

        count += 1;
    }

    info!("Scan complete: {} new photos added", count);
    Ok(())
}

fn execute_llm_batch_task(
    target_path: &str,
    config: &Config,
    db: &Database,
) -> Result<()> {
    use clepho::llm::LlmClient;
    use std::path::Path;

    info!("Running LLM batch processing for: {}", target_path);

    // Look up per-folder prompt, falling back to global config
    let dir_prompt = db.get_directory_prompt(target_path)
        .ok()
        .flatten();

    // Get photos without descriptions in this directory
    let photos = db.get_photos_without_description_in_directory(target_path, 50)?;

    if photos.is_empty() {
        info!("No photos need LLM processing");
        return Ok(());
    }

    info!("Processing {} photos with LLM", photos.len());

    // Build LlmClient using the shared provider system
    let mut llm_config = config.llm.clone();
    if let Some(prompt) = dir_prompt {
        llm_config.custom_prompt = Some(prompt);
    }
    let client = LlmClient::from_config(&llm_config);

    let mut consecutive_failures = 0u32;
    const MAX_CONSECUTIVE_FAILURES: u32 = 3;

    for (id, path) in photos {
        match client.describe_and_tag_image(Path::new(&path)) {
            Ok((description, tags)) => {
                if tags.is_empty() {
                    warn!(path = %path, "LLM returned empty tags for photo");
                }
                let tags_json = serde_json::to_string(&tags).unwrap_or_default();
                let _ = db.save_llm_result(id, &description, &tags_json);

                if client.supports_embeddings() {
                    if let Ok(embedding) = client.get_text_embedding(&description) {
                        let _ = db.store_embedding(id, &embedding, "text-embedding");
                    }
                }

                info!("Generated description for {}", path);
                consecutive_failures = 0;
            }
            Err(e) => {
                consecutive_failures += 1;
                warn!("Failed to generate description for {}: {}", path, e);

                if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                    error!("Aborting LLM batch: {} consecutive failures (server may be unavailable)", consecutive_failures);
                    break;
                }
            }
        }

        // Small delay between requests to avoid overwhelming the LLM
        thread::sleep(Duration::from_millis(500));
    }

    Ok(())
}

fn execute_face_detection_task(target_path: &str, db: &Database) -> Result<()> {
    info!("Running face detection for: {}", target_path);

    // Note: Full face detection requires ONNX models which are complex to set up.
    // The daemon logs what would happen but defers to the main app for actual detection.

    let count = db.count_photos_without_faces_in_dir(target_path)?;

    if count == 0 {
        info!("No photos need face detection");
        return Ok(());
    }

    warn!(
        "Face detection requires ONNX models. {} photos pending - use main app or run 'clepho --face-scan {}'",
        count, target_path
    );

    // For daemon mode, we mark these photos as needing face detection
    // The main app or a future enhancement can handle the actual detection

    Ok(())
}
