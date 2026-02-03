mod app;
mod centralise;
mod clip;
mod export;
mod faces;
mod llm;
mod logging;
mod scanner;
mod schedule;
mod tasks;
mod trash;
mod ui;

// Re-export shared modules from library crate so binary submodules
// can use them via `crate::config` and `crate::db`.
pub(crate) use clepho::config;
pub(crate) use clepho::db;

use anyhow::Result;
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::io;
use std::path::PathBuf;

use app::App;
use config::Config;

enum CliAction {
    RunTui(Option<PathBuf>),
    #[cfg(feature = "postgres")]
    MigrateToPostgres { config_path: Option<PathBuf>, postgres_url: String },
}

fn parse_args() -> CliAction {
    let args: Vec<String> = std::env::args().collect();
    let mut config_path = None;
    #[cfg(feature = "postgres")]
    let mut migrate_url: Option<String> = None;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            "--version" | "-V" => {
                println!("clepho {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "--config" | "-c" => {
                if i + 1 < args.len() {
                    config_path = Some(PathBuf::from(&args[i + 1]));
                    i += 1;
                } else {
                    eprintln!("Error: --config requires a path argument");
                    std::process::exit(1);
                }
            }
            #[cfg(feature = "postgres")]
            "--migrate-to-postgres" => {
                if i + 1 < args.len() {
                    migrate_url = Some(args[i + 1].clone());
                    i += 1;
                } else {
                    eprintln!("Error: --migrate-to-postgres requires a PostgreSQL URL argument");
                    std::process::exit(1);
                }
            }
            #[cfg(not(feature = "postgres"))]
            "--migrate-to-postgres" => {
                eprintln!("Error: --migrate-to-postgres requires the 'postgres' feature");
                eprintln!("Rebuild with: cargo build --features postgres");
                std::process::exit(1);
            }
            _ => {
                eprintln!("Unknown argument: {}", args[i]);
                print_help();
                std::process::exit(1);
            }
        }
        i += 1;
    }

    #[cfg(feature = "postgres")]
    if let Some(url) = migrate_url {
        return CliAction::MigrateToPostgres { config_path, postgres_url: url };
    }

    CliAction::RunTui(config_path)
}

fn print_help() {
    println!(
        r#"clepho - TUI photo management application

USAGE:
    clepho [OPTIONS]

OPTIONS:
    --config, -c PATH                 Path to config file
    --migrate-to-postgres URL         Migrate SQLite database to PostgreSQL (requires postgres feature)
    --version, -V                     Show version
    --help, -h                        Show this help message

ENVIRONMENT:
    CLEPHO_CONFIG       Path to config file (overrides default location)
    RUST_LOG            Log level (trace, debug, info, warn, error)

Config file location: $XDG_CONFIG_HOME/clepho/config.toml

See also: clepho-daemon --help"#
    );
}

#[tokio::main]
async fn main() -> Result<()> {
    let action = parse_args();

    // Initialize logging (uses journald on Linux, file fallback otherwise)
    let _ = logging::init(Some(Config::config_dir().join("logs")));

    match action {
        CliAction::RunTui(config_path) => {
            // Load configuration
            let config = match config_path {
                Some(path) => Config::load_from(&path)?,
                None => Config::load()?,
            };

            // Initialize database
            let db = db::Database::open(&config.database)?;
            db.initialize()?;

            // Setup terminal
            enable_raw_mode()?;
            let mut stdout = io::stdout();
            execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
            let backend = CrosstermBackend::new(stdout);
            let mut terminal = Terminal::new(backend)?;

            // Create and run app
            let mut app = App::new(config, db)?;
            let result = app.run(&mut terminal).await;

            // Restore terminal
            disable_raw_mode()?;
            execute!(
                terminal.backend_mut(),
                LeaveAlternateScreen,
                DisableMouseCapture
            )?;
            terminal.show_cursor()?;

            result
        }
        #[cfg(feature = "postgres")]
        CliAction::MigrateToPostgres { config_path, postgres_url } => {
            let config = match config_path {
                Some(path) => Config::load_from(&path)?,
                None => Config::load()?,
            };

            let sqlite_path = &config.database.sqlite_path;
            eprintln!("Migrating from SQLite ({}) to PostgreSQL...", sqlite_path.display());
            db::migrate::migrate_sqlite_to_postgres(sqlite_path, &postgres_url)?;
            Ok(())
        }
    }
}
