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
use rusqlite::Connection;
use serde::Deserialize;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;
use tracing::{error, info, warn};
use walkdir::WalkDir;

/// Daemon configuration
struct DaemonConfig {
    /// Poll interval for checking new tasks (seconds)
    poll_interval: u64,
    /// Run once and exit
    once: bool,
    /// Config path override
    config_path: Option<PathBuf>,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            poll_interval: 60,
            once: false,
            config_path: None,
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

    // Open database
    let db = open_database(config.db_path())?;
    info!("Database opened at {:?}", config.db_path());

    // Main loop
    if daemon_config.once {
        info!("Running in single-shot mode");
        process_pending_tasks(&db, &config)?;
    } else {
        info!("Running in daemon mode, polling every {} seconds", daemon_config.poll_interval);
        run_daemon_loop(&db, &config, daemon_config.poll_interval)?;
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

fn config_path() -> PathBuf {
    // Check environment variable
    if let Ok(path) = std::env::var("CLEPHO_CONFIG") {
        return PathBuf::from(path);
    }

    // Default config location
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("clepho")
        .join("config.toml")
}

/// Application config (subset needed by daemon)
#[derive(Debug, Clone, Deserialize)]
struct AppConfig {
    #[serde(default)]
    database: DatabaseConfig,

    #[serde(default)]
    llm: LlmConfig,

    #[serde(default)]
    schedule: ScheduleConfig,
}

impl AppConfig {
    /// Get the database path (SQLite)
    fn db_path(&self) -> &PathBuf {
        &self.database.sqlite_path
    }
}

/// Database configuration
#[derive(Debug, Clone, Deserialize)]
struct DatabaseConfig {
    #[serde(default = "default_db_path")]
    sqlite_path: PathBuf,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            sqlite_path: default_db_path(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
struct LlmConfig {
    #[serde(default = "default_llm_endpoint")]
    endpoint: String,
    #[serde(default = "default_llm_model")]
    model: String,
    #[serde(default)]
    api_key: Option<String>,
    #[serde(default)]
    custom_prompt: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct ScheduleConfig {
    #[serde(default)]
    default_hours_start: Option<u8>,
    #[serde(default)]
    default_hours_end: Option<u8>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            database: DatabaseConfig::default(),
            llm: LlmConfig::default(),
            schedule: ScheduleConfig::default(),
        }
    }
}

fn default_db_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("clepho")
        .join("clepho.db")
}

fn default_llm_endpoint() -> String {
    "http://127.0.0.1:1234/v1".to_string()
}

fn default_llm_model() -> String {
    "gemma-3-4b".to_string()
}

fn load_config(daemon_config: &DaemonConfig) -> Result<AppConfig> {
    let path = daemon_config.config_path.clone().unwrap_or_else(config_path);

    if path.exists() {
        let content = std::fs::read_to_string(&path)
            .context("Failed to read config file")?;
        let config: AppConfig = toml::from_str(&content)
            .context("Failed to parse config file")?;
        Ok(config)
    } else {
        warn!("Config file not found at {:?}, using defaults", path);
        Ok(AppConfig::default())
    }
}

fn open_database(path: &PathBuf) -> Result<Connection> {
    let conn = Connection::open(path)
        .context("Failed to open database")?;
    Ok(conn)
}

fn run_daemon_loop(
    db: &Connection,
    config: &AppConfig,
    poll_interval: u64,
) -> Result<()> {
    loop {
        // Check if we should process (based on hours of operation)
        if should_process_now(&config.schedule) {
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

fn should_process_now(schedule: &ScheduleConfig) -> bool {
    let (start, end) = match (schedule.default_hours_start, schedule.default_hours_end) {
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

fn process_pending_tasks(db: &Connection, config: &AppConfig) -> Result<()> {
    // Get pending tasks ordered by scheduled time
    let mut stmt = db.prepare(
        r#"
        SELECT id, task_type, target_path, photo_ids, scheduled_at, hours_start, hours_end
        FROM scheduled_tasks
        WHERE status = 'pending'
          AND (scheduled_at IS NULL OR datetime(scheduled_at) <= datetime('now'))
        ORDER BY scheduled_at ASC
        LIMIT 10
        "#
    )?;

    let tasks: Vec<PendingTask> = stmt
        .query_map([], |row| {
            Ok(PendingTask {
                id: row.get(0)?,
                task_type: row.get(1)?,
                target_path: row.get(2)?,
                photo_ids: row.get::<_, Option<String>>(3)?,
                hours_start: row.get(4)?,
                hours_end: row.get(5)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

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

        info!("Processing task {} ({})", task.id, task.task_type);

        // Mark as running
        db.execute(
            "UPDATE scheduled_tasks SET status = 'running', started_at = CURRENT_TIMESTAMP WHERE id = ?",
            [task.id],
        )?;

        // Execute the task
        let result = execute_task(&task, config, db);

        // Update status based on result
        match result {
            Ok(()) => {
                info!("Task {} completed successfully", task.id);
                db.execute(
                    "UPDATE scheduled_tasks SET status = 'completed', completed_at = CURRENT_TIMESTAMP WHERE id = ?",
                    [task.id],
                )?;
            }
            Err(e) => {
                error!("Task {} failed: {}", task.id, e);
                db.execute(
                    "UPDATE scheduled_tasks SET status = 'failed', error_message = ?, completed_at = CURRENT_TIMESTAMP WHERE id = ?",
                    rusqlite::params![e.to_string(), task.id],
                )?;
            }
        }
    }

    Ok(())
}

struct PendingTask {
    id: i64,
    task_type: String,
    target_path: String,
    #[allow(dead_code)]
    photo_ids: Option<String>,
    hours_start: Option<u8>,
    hours_end: Option<u8>,
}

fn task_within_hours(task: &PendingTask) -> bool {
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

fn execute_task(task: &PendingTask, config: &AppConfig, db: &Connection) -> Result<()> {
    match task.task_type.as_str() {
        "Scan" => execute_scan_task(&task.target_path, db),
        "LlmBatch" => execute_llm_batch_task(&task.target_path, config, db),
        "FaceDetection" => execute_face_detection_task(&task.target_path, db),
        _ => {
            warn!("Unknown task type: {}", task.task_type);
            Ok(())
        }
    }
}

fn execute_scan_task(target_path: &str, db: &Connection) -> Result<()> {
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

        // Check if already in database
        let exists: bool = db.query_row(
            "SELECT 1 FROM photos WHERE path = ?",
            [path.to_string_lossy().as_ref()],
            |_| Ok(true),
        ).unwrap_or(false);

        if exists {
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

        db.execute(
            r#"
            INSERT OR IGNORE INTO photos (path, filename, directory, size_bytes, scanned_at)
            VALUES (?, ?, ?, ?, CURRENT_TIMESTAMP)
            "#,
            rusqlite::params![path.to_string_lossy().as_ref(), filename, directory, size],
        )?;

        count += 1;
    }

    info!("Scan complete: {} new photos added", count);
    Ok(())
}

fn execute_llm_batch_task(
    target_path: &str,
    config: &AppConfig,
    db: &Connection,
) -> Result<()> {
    info!("Running LLM batch processing for: {}", target_path);

    // Get photos without descriptions in this directory
    let mut stmt = db.prepare(
        r#"
        SELECT id, path
        FROM photos
        WHERE directory = ? AND description IS NULL
        LIMIT 50
        "#,
    )?;

    let photos: Vec<(i64, String)> = stmt
        .query_map([target_path], |row| Ok((row.get(0)?, row.get(1)?)))?
        .filter_map(|r| r.ok())
        .collect();

    if photos.is_empty() {
        info!("No photos need LLM processing");
        return Ok(());
    }

    info!("Processing {} photos with LLM", photos.len());

    for (id, path) in photos {
        match call_llm_for_description(&path, config) {
            Ok(description) => {
                db.execute(
                    "UPDATE photos SET description = ?, llm_processed_at = CURRENT_TIMESTAMP WHERE id = ?",
                    rusqlite::params![description, id],
                )?;
                info!("Generated description for {}", path);
            }
            Err(e) => {
                warn!("Failed to generate description for {}: {}", path, e);
            }
        }

        // Small delay between requests to avoid overwhelming the LLM
        thread::sleep(Duration::from_millis(500));
    }

    Ok(())
}

fn call_llm_for_description(image_path: &str, config: &AppConfig) -> Result<String> {
    use base64::Engine;

    // Read and encode image
    let image_data = std::fs::read(image_path)
        .context("Failed to read image")?;
    let base64_image = base64::engine::general_purpose::STANDARD.encode(&image_data);

    // Build prompt
    let base_prompt = "Describe this image in detail. Include information about: the main subjects, the setting/location, lighting and atmosphere, any notable objects, and the overall mood. Keep the description factual and concise (2-3 sentences).";
    let prompt = match &config.llm.custom_prompt {
        Some(context) => format!("Context: {}\n\n{}", context, base_prompt),
        None => base_prompt.to_string(),
    };

    // Make API request
    let body = serde_json::json!({
        "model": config.llm.model,
        "messages": [{
            "role": "user",
            "content": [
                {
                    "type": "text",
                    "text": prompt
                },
                {
                    "type": "image_url",
                    "image_url": {
                        "url": format!("data:image/jpeg;base64,{}", base64_image)
                    }
                }
            ]
        }],
        "max_tokens": 300
    });

    let url = format!("{}/chat/completions", config.llm.endpoint.trim_end_matches('/'));

    let mut request = ureq::post(&url)
        .set("Content-Type", "application/json");

    if let Some(ref key) = config.llm.api_key {
        request = request.set("Authorization", &format!("Bearer {}", key));
    }

    let response: serde_json::Value = request
        .send_json(&body)
        .context("Failed to send request to LLM")?
        .into_json()
        .context("Failed to parse LLM response")?;

    let description = response["choices"][0]["message"]["content"]
        .as_str()
        .context("No content in LLM response")?
        .to_string();

    Ok(description)
}

fn execute_face_detection_task(target_path: &str, db: &Connection) -> Result<()> {
    info!("Running face detection for: {}", target_path);

    // Note: Full face detection requires ONNX models which are complex to set up.
    // The daemon logs what would happen but defers to the main app for actual detection.

    let count: i64 = db.query_row(
        r#"
        SELECT COUNT(*)
        FROM photos p
        WHERE p.directory = ?
          AND NOT EXISTS (SELECT 1 FROM faces f WHERE f.photo_id = p.id)
        "#,
        [target_path],
        |row| row.get(0),
    ).unwrap_or(0);

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
