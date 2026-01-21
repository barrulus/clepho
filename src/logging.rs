//! Logging configuration with journald support on Linux.
//!
//! This module sets up tracing-based logging that integrates with systemd's
//! journal on Linux systems, with file-based fallback for other platforms
//! or when journald is unavailable.

use anyhow::Result;
use std::path::PathBuf;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Initialize the logging system.
///
/// On Linux, this will attempt to connect to systemd-journald.
/// If unavailable or on other platforms, logs go to a file in the config directory.
///
/// Log level can be controlled via the `CLEPHO_LOG` environment variable:
/// - `CLEPHO_LOG=debug` for verbose output
/// - `CLEPHO_LOG=info` for standard output (default)
/// - `CLEPHO_LOG=warn` for warnings and errors only
/// - `CLEPHO_LOG=error` for errors only
pub fn init(log_dir: Option<PathBuf>) -> Result<()> {
    let env_filter = EnvFilter::try_from_env("CLEPHO_LOG")
        .unwrap_or_else(|_| EnvFilter::new("info"));

    #[cfg(target_os = "linux")]
    {
        // Try to use journald on Linux
        if let Ok(journald_layer) = tracing_journald::layer() {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(journald_layer)
                .init();

            tracing::info!("Logging initialized with journald backend");
            return Ok(());
        }
    }

    // Fallback to file-based logging
    let log_dir = log_dir.unwrap_or_else(|| {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("clepho")
            .join("logs")
    });

    std::fs::create_dir_all(&log_dir)?;

    let file_appender = tracing_appender::rolling::daily(&log_dir, "clepho.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    // Store the guard in a static to prevent it from being dropped
    // This is safe because we only call init() once at startup
    static GUARD: std::sync::OnceLock<tracing_appender::non_blocking::WorkerGuard> = std::sync::OnceLock::new();
    let _ = GUARD.set(_guard);

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer().with_writer(non_blocking).with_ansi(false))
        .init();

    tracing::info!("Logging initialized with file backend at {:?}", log_dir);
    Ok(())
}
