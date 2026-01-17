use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::prelude::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use crate::config::Config;
use crate::db::Database;
use crate::llm::LlmClient;
use crate::scanner::Scanner;
use crate::ui;
use crate::ui::duplicates::DuplicatesView;

pub use crate::scanner::ScanProgress;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ActivePane {
    Parent,
    Current,
    Preview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppMode {
    Normal,
    Help,
    Scanning,
    Duplicates,
    DuplicatesHelp,
    LlmProcessing,
    LlmBatchProcessing,
}

#[allow(dead_code)]
pub struct App {
    pub config: Config,
    pub db: Database,
    pub current_dir: PathBuf,
    pub entries: Vec<DirEntry>,
    pub parent_entries: Vec<DirEntry>,
    pub selected_index: usize,
    pub parent_selected_index: usize,
    pub scroll_offset: usize,
    pub parent_scroll_offset: usize,
    pub active_pane: ActivePane,
    pub mode: AppMode,
    pub should_quit: bool,
    pub status_message: Option<String>,
    pub g_pressed: bool,
    // Scanning state
    pub scan_progress: Option<ScanProgress>,
    pub scan_receiver: Option<mpsc::Receiver<ScanProgress>>,
    // Duplicates view
    pub duplicates_view: Option<DuplicatesView>,
    // LLM state
    pub llm_client: LlmClient,
    pub llm_descriptions: HashMap<PathBuf, String>,
    pub llm_pending_path: Option<PathBuf>,
    pub llm_receiver: Option<mpsc::Receiver<Result<String, String>>>,
    // Batch LLM processing state
    pub llm_batch_receiver: Option<mpsc::Receiver<crate::llm::LlmTaskStatus>>,
    pub llm_batch_progress: Option<(usize, usize)>, // (current, total)
}

#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<std::time::SystemTime>,
}

impl App {
    pub fn new(config: Config, db: Database) -> Result<Self> {
        let current_dir = std::env::current_dir()?;
        let llm_client = LlmClient::new(&config.llm.endpoint, &config.llm.model);
        let mut app = Self {
            config,
            db,
            current_dir: current_dir.clone(),
            entries: Vec::new(),
            parent_entries: Vec::new(),
            selected_index: 0,
            parent_selected_index: 0,
            scroll_offset: 0,
            parent_scroll_offset: 0,
            active_pane: ActivePane::Current,
            mode: AppMode::Normal,
            should_quit: false,
            status_message: None,
            g_pressed: false,
            scan_progress: None,
            scan_receiver: None,
            duplicates_view: None,
            llm_client,
            llm_descriptions: HashMap::new(),
            llm_pending_path: None,
            llm_receiver: None,
            llm_batch_receiver: None,
            llm_batch_progress: None,
        };
        app.load_directory(&current_dir)?;
        Ok(app)
    }

    pub fn load_directory(&mut self, path: &PathBuf) -> Result<()> {
        self.current_dir = path.clone();
        self.entries = self.read_directory(path)?;
        self.selected_index = 0;
        self.scroll_offset = 0;

        // Load parent directory entries
        if let Some(parent) = path.parent() {
            self.parent_entries = self.read_directory(&parent.to_path_buf())?;
            // Find and select current directory in parent
            if let Some(current_name) = path.file_name() {
                self.parent_selected_index = self
                    .parent_entries
                    .iter()
                    .position(|e| e.path.file_name() == Some(current_name))
                    .unwrap_or(0);
            }
        } else {
            self.parent_entries = Vec::new();
            self.parent_selected_index = 0;
        }

        Ok(())
    }

    fn read_directory(&self, path: &PathBuf) -> Result<Vec<DirEntry>> {
        let mut entries = Vec::new();

        if let Ok(read_dir) = std::fs::read_dir(path) {
            for entry in read_dir.flatten() {
                let metadata = entry.metadata().ok();
                let is_dir = metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false);
                let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
                let modified = metadata.as_ref().and_then(|m| m.modified().ok());

                entries.push(DirEntry {
                    name: entry.file_name().to_string_lossy().to_string(),
                    path: entry.path(),
                    is_dir,
                    size,
                    modified,
                });
            }
        }

        // Sort: directories first, then alphabetically
        entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        });

        Ok(entries)
    }

    pub async fn run(&mut self, terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> Result<()> {
        while !self.should_quit {
            // Check for scan progress updates
            self.update_scan_progress();
            // Check for LLM results
            self.update_llm_progress();
            // Check for batch LLM progress
            self.update_batch_llm_progress();

            terminal.draw(|frame| ui::render(frame, self))?;

            if event::poll(Duration::from_millis(100))? {
                match event::read()? {
                    Event::Key(key) => self.handle_key(key)?,
                    Event::Mouse(mouse) => {
                        let size = terminal.size()?;
                        let area = Rect::new(0, 0, size.width, size.height);
                        self.handle_mouse(mouse, area)?;
                    }
                    Event::Resize(_, _) => {}
                    _ => {}
                }
            }
        }

        Ok(())
    }

    fn update_llm_progress(&mut self) {
        let mut result = None;
        let mut should_clear = false;

        if let Some(ref receiver) = self.llm_receiver {
            if let Ok(r) = receiver.try_recv() {
                result = Some(r);
                should_clear = true;
            }
        }

        if let Some(r) = result {
            match r {
                Ok(description) => {
                    if let Some(path) = self.llm_pending_path.take() {
                        // Save to database for persistence
                        if let Err(e) = self.db.save_description(&path, &description) {
                            self.status_message = Some(format!("Warning: Failed to save to DB: {}", e));
                        } else {
                            self.status_message = Some("Description generated and saved".to_string());
                        }
                        // Also cache in memory for quick access
                        self.llm_descriptions.insert(path, description);
                    }
                }
                Err(e) => {
                    self.llm_pending_path = None;
                    self.status_message = Some(format!("LLM error: {}", e));
                }
            }
            self.mode = AppMode::Normal;
        }

        if should_clear {
            self.llm_receiver = None;
        }
    }

    fn update_scan_progress(&mut self) {
        // Collect all progress updates first to avoid borrow issues
        let mut updates = Vec::new();
        let mut should_clear_receiver = false;

        if let Some(ref receiver) = self.scan_receiver {
            while let Ok(progress) = receiver.try_recv() {
                if matches!(&progress, ScanProgress::Completed { .. }) {
                    should_clear_receiver = true;
                }
                updates.push(progress);
            }
        }

        // Now process the updates
        for progress in updates {
            match &progress {
                ScanProgress::Completed { scanned, new, updated } => {
                    self.status_message = Some(format!(
                        "Scan complete: {} scanned, {} new, {} updated",
                        scanned, new, updated
                    ));
                    self.mode = AppMode::Normal;
                }
                ScanProgress::Error { message } => {
                    self.status_message = Some(format!("Scan error: {}", message));
                }
                _ => {}
            }
            self.scan_progress = Some(progress);
        }

        if should_clear_receiver {
            self.scan_receiver = None;
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        // Handle help mode
        if self.mode == AppMode::Help {
            match key.code {
                KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q') => {
                    self.mode = AppMode::Normal;
                }
                _ => {}
            }
            return Ok(());
        }

        // Handle duplicates help mode
        if self.mode == AppMode::DuplicatesHelp {
            match key.code {
                KeyCode::Esc | KeyCode::Char('?') => {
                    self.mode = AppMode::Duplicates;
                }
                _ => {}
            }
            return Ok(());
        }

        // Handle duplicates mode
        if self.mode == AppMode::Duplicates {
            return self.handle_duplicates_key(key);
        }

        // Handle scanning mode - only allow Escape to cancel
        if self.mode == AppMode::Scanning {
            if key.code == KeyCode::Esc {
                self.mode = AppMode::Normal;
                self.scan_receiver = None;
                self.scan_progress = None;
                self.status_message = Some("Scan cancelled".to_string());
            }
            return Ok(());
        }

        // Handle LLM processing mode - only allow Escape to cancel
        if self.mode == AppMode::LlmProcessing {
            if key.code == KeyCode::Esc {
                self.mode = AppMode::Normal;
                self.llm_receiver = None;
                self.status_message = Some("LLM processing cancelled".to_string());
            }
            return Ok(());
        }

        // Handle batch LLM processing mode - only allow Escape to cancel
        if self.mode == AppMode::LlmBatchProcessing {
            if key.code == KeyCode::Esc {
                self.mode = AppMode::Normal;
                self.llm_batch_receiver = None;
                self.llm_batch_progress = None;
                self.status_message = Some("Batch processing cancelled".to_string());
            }
            return Ok(());
        }

        // Handle g prefix for gg (go to top)
        if self.g_pressed {
            self.g_pressed = false;
            if key.code == KeyCode::Char('g') {
                self.selected_index = 0;
                self.scroll_offset = 0;
                return Ok(());
            }
        }

        match key.code {
            // Quit
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }

            // Help
            KeyCode::Char('?') => self.mode = AppMode::Help,

            // Scan current directory
            KeyCode::Char('s') => self.start_scan()?,

            // Find duplicates
            KeyCode::Char('d') => self.find_duplicates()?,

            // LLM describe selected image
            KeyCode::Char('D') => self.describe_with_llm()?,

            // Batch LLM processing for current directory
            KeyCode::Char('P') => self.start_batch_llm()?,

            // Navigation
            KeyCode::Char('j') | KeyCode::Down => self.move_down(),
            KeyCode::Char('k') | KeyCode::Up => self.move_up(),
            KeyCode::Char('h') | KeyCode::Left | KeyCode::Backspace => self.go_parent()?,
            KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => self.enter_selected()?,

            // Jump to top/bottom
            KeyCode::Char('g') => self.g_pressed = true,
            KeyCode::Char('G') => self.go_to_bottom(),

            // Page navigation
            KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.page_down();
            }
            KeyCode::Char('b') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.page_up();
            }

            // Home directory
            KeyCode::Char('~') => {
                if let Some(home) = dirs::home_dir() {
                    self.load_directory(&home)?;
                }
            }

            _ => {}
        }

        Ok(())
    }

    fn handle_mouse(&mut self, mouse: MouseEvent, area: Rect) -> Result<()> {
        match mouse.kind {
            MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                // Calculate which pane was clicked based on the layout
                let chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Percentage(20),
                        Constraint::Percentage(40),
                        Constraint::Percentage(40),
                    ])
                    .split(area);

                let x = mouse.column;
                let y = mouse.row;

                if x < chunks[0].right() && y >= chunks[0].y && y < chunks[0].bottom() {
                    // Clicked in parent pane
                    let clicked_index = (y - chunks[0].y - 1) as usize + self.parent_scroll_offset;
                    if clicked_index < self.parent_entries.len() {
                        self.parent_selected_index = clicked_index;
                        // Navigate to clicked parent entry
                        let path = self.parent_entries[clicked_index].path.clone();
                        if self.parent_entries[clicked_index].is_dir {
                            self.load_directory(&path)?;
                        }
                    }
                } else if x < chunks[1].right() && y >= chunks[1].y && y < chunks[1].bottom() {
                    // Clicked in current pane
                    let clicked_index = (y - chunks[1].y - 1) as usize + self.scroll_offset;
                    if clicked_index < self.entries.len() {
                        self.selected_index = clicked_index;
                    }
                }
            }
            MouseEventKind::ScrollDown => self.move_down(),
            MouseEventKind::ScrollUp => self.move_up(),
            _ => {}
        }

        Ok(())
    }

    fn move_down(&mut self) {
        if !self.entries.is_empty() && self.selected_index < self.entries.len() - 1 {
            self.selected_index += 1;
        }
    }

    fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    fn go_parent(&mut self) -> Result<()> {
        if let Some(parent) = self.current_dir.parent() {
            let parent = parent.to_path_buf();
            self.load_directory(&parent)?;
        }
        Ok(())
    }

    fn enter_selected(&mut self) -> Result<()> {
        if let Some(entry) = self.entries.get(self.selected_index) {
            if entry.is_dir {
                let path = entry.path.clone();
                self.load_directory(&path)?;
            }
        }
        Ok(())
    }

    fn go_to_bottom(&mut self) {
        if !self.entries.is_empty() {
            self.selected_index = self.entries.len() - 1;
        }
    }

    fn page_down(&mut self) {
        let page_size = 20;
        self.selected_index = (self.selected_index + page_size).min(
            self.entries.len().saturating_sub(1)
        );
    }

    fn page_up(&mut self) {
        let page_size = 20;
        self.selected_index = self.selected_index.saturating_sub(page_size);
    }

    pub fn selected_entry(&self) -> Option<&DirEntry> {
        self.entries.get(self.selected_index)
    }

    pub fn get_llm_description(&mut self) -> Option<String> {
        let entry = self.selected_entry()?.clone();

        // Check memory cache first
        if let Some(desc) = self.llm_descriptions.get(&entry.path) {
            return Some(desc.clone());
        }

        // Check database
        if let Ok(Some(desc)) = self.db.get_description(&entry.path) {
            // Cache it for future access
            self.llm_descriptions.insert(entry.path, desc.clone());
            return Some(desc);
        }

        None
    }

    fn start_scan(&mut self) -> Result<()> {
        // Don't start a new scan if one is already running
        if self.mode == AppMode::Scanning {
            return Ok(());
        }

        let (tx, rx) = mpsc::channel();
        let dir = self.current_dir.clone();
        let config = self.config.clone();

        // Get a reference to the database path to open a new connection in the thread
        let db_path = self.config.db_path.clone();

        // Spawn scanning in a background thread
        std::thread::spawn(move || {
            let db = match Database::open(&db_path) {
                Ok(db) => db,
                Err(e) => {
                    let _ = tx.send(ScanProgress::Error {
                        message: format!("Failed to open database: {}", e),
                    });
                    return;
                }
            };

            if let Err(e) = db.initialize() {
                let _ = tx.send(ScanProgress::Error {
                    message: format!("Failed to initialize database: {}", e),
                });
                return;
            }

            let scanner = Scanner::new(config);
            let _ = scanner.scan_directory(&dir, &db, Some(tx));
        });

        self.scan_receiver = Some(rx);
        self.mode = AppMode::Scanning;
        self.status_message = Some(format!("Scanning {}...", self.current_dir.display()));

        Ok(())
    }

    fn find_duplicates(&mut self) -> Result<()> {
        self.status_message = Some("Finding duplicates...".to_string());

        // Find exact duplicates (by SHA256 hash)
        let mut all_groups = self.db.find_exact_duplicates()?;

        // Find perceptual duplicates (similar images)
        let threshold = self.config.scanner.similarity_threshold;
        let perceptual_groups = self.db.find_perceptual_duplicates(threshold)?;
        all_groups.extend(perceptual_groups);

        if all_groups.is_empty() {
            self.status_message = Some("No duplicates found".to_string());
            return Ok(());
        }

        let count = all_groups.len();
        self.duplicates_view = Some(DuplicatesView::new(all_groups));
        self.mode = AppMode::Duplicates;
        self.status_message = Some(format!("Found {} duplicate groups", count));

        Ok(())
    }

    fn handle_duplicates_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            // Exit duplicates view
            KeyCode::Esc | KeyCode::Char('q') => {
                self.mode = AppMode::Normal;
                self.duplicates_view = None;
            }

            // Help
            KeyCode::Char('?') => {
                self.mode = AppMode::DuplicatesHelp;
            }

            // Navigate photos within group
            KeyCode::Char('j') | KeyCode::Down => {
                if let Some(ref mut view) = self.duplicates_view {
                    view.next_photo();
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if let Some(ref mut view) = self.duplicates_view {
                    view.prev_photo();
                }
            }

            // Navigate between groups
            KeyCode::Char('J') => {
                if let Some(ref mut view) = self.duplicates_view {
                    view.next_group();
                }
            }
            KeyCode::Char('K') => {
                if let Some(ref mut view) = self.duplicates_view {
                    view.prev_group();
                }
            }

            // Toggle deletion mark
            KeyCode::Char(' ') => {
                if let Some(ref mut view) = self.duplicates_view {
                    if let Some(photo) = view.current_photo() {
                        let id = photo.id;
                        let currently_marked = photo.marked_for_deletion;
                        if currently_marked {
                            self.db.unmark_for_deletion(id)?;
                        } else {
                            self.db.mark_for_deletion(id)?;
                        }
                    }
                    view.toggle_deletion();
                }
            }

            // Auto-select duplicates for deletion (keep best quality)
            KeyCode::Char('a') => {
                if let Some(ref mut view) = self.duplicates_view {
                    view.auto_select_for_deletion();
                    // Sync marks to database
                    for group in &view.groups {
                        for photo in &group.photos {
                            if photo.marked_for_deletion {
                                self.db.mark_for_deletion(photo.id)?;
                            } else {
                                self.db.unmark_for_deletion(photo.id)?;
                            }
                        }
                    }
                    self.status_message = Some("Auto-selected duplicates for deletion".to_string());
                }
            }

            // Execute deletions (delete files and remove from DB)
            KeyCode::Char('x') => {
                let marked = self.db.get_marked_for_deletion()?;
                if marked.is_empty() {
                    self.status_message = Some("No photos marked for deletion".to_string());
                } else {
                    let count = marked.len();
                    // Delete actual files
                    for photo in &marked {
                        if let Err(e) = std::fs::remove_file(&photo.path) {
                            self.status_message = Some(format!("Error deleting {}: {}", photo.path, e));
                        }
                    }
                    // Remove from database
                    self.db.delete_marked_photos()?;
                    self.status_message = Some(format!("Deleted {} photos", count));

                    // Refresh duplicates view
                    self.find_duplicates()?;
                }
            }

            _ => {}
        }

        Ok(())
    }

    fn describe_with_llm(&mut self) -> Result<()> {
        // Check if we have a selected file that's an image
        let entry = match self.selected_entry() {
            Some(e) if !e.is_dir && is_image(&e.name) => e.clone(),
            _ => {
                self.status_message = Some("Select an image file first".to_string());
                return Ok(());
            }
        };

        let (tx, rx) = mpsc::channel();
        let path = entry.path.clone();
        let endpoint = self.config.llm.endpoint.clone();
        let model = self.config.llm.model.clone();

        // Spawn LLM request in background thread
        std::thread::spawn(move || {
            let client = LlmClient::new(&endpoint, &model);
            let result = client.describe_image(&path);
            let _ = tx.send(result.map_err(|e| e.to_string()));
        });

        self.llm_pending_path = Some(entry.path.clone());
        self.llm_receiver = Some(rx);
        self.mode = AppMode::LlmProcessing;
        self.status_message = Some(format!("Describing {}...", entry.name));

        Ok(())
    }

    fn start_batch_llm(&mut self) -> Result<()> {
        // Don't start if already processing
        if self.mode == AppMode::LlmBatchProcessing {
            return Ok(());
        }

        // Get photos without descriptions in current directory
        let tasks = self.db.get_photos_without_description_in_dir(&self.current_dir)?;

        if tasks.is_empty() {
            self.status_message = Some("No unprocessed photos in this directory".to_string());
            return Ok(());
        }

        let total = tasks.len();
        let (tx, rx) = mpsc::channel();
        let endpoint = self.config.llm.endpoint.clone();
        let model = self.config.llm.model.clone();
        let db_path = self.config.db_path.clone();

        // Spawn batch processing in background thread
        std::thread::spawn(move || {
            let db = match Database::open(&db_path) {
                Ok(db) => db,
                Err(e) => {
                    let _ = tx.send(crate::llm::LlmTaskStatus::Error {
                        message: format!("Failed to open database: {}", e),
                    });
                    return;
                }
            };

            let client = LlmClient::new(&endpoint, &model);
            let mut queue = crate::llm::LlmQueue::new(client);
            queue.add_tasks(tasks);
            let _ = queue.process_all(&db, Some(tx));
        });

        self.llm_batch_receiver = Some(rx);
        self.llm_batch_progress = Some((0, total));
        self.mode = AppMode::LlmBatchProcessing;
        self.status_message = Some(format!("Processing {} photos...", total));

        Ok(())
    }

    fn update_batch_llm_progress(&mut self) {
        use crate::llm::LlmTaskStatus;

        let mut should_clear = false;

        if let Some(ref receiver) = self.llm_batch_receiver {
            while let Ok(status) = receiver.try_recv() {
                match status {
                    LlmTaskStatus::Queued { total } => {
                        self.llm_batch_progress = Some((0, total));
                        self.status_message = Some(format!("Queued {} photos for processing", total));
                    }
                    LlmTaskStatus::Processing { current, total, path } => {
                        self.llm_batch_progress = Some((current, total));
                        let filename = std::path::Path::new(&path)
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or(path);
                        self.status_message = Some(format!("[{}/{}] Processing {}", current, total, filename));
                    }
                    LlmTaskStatus::Completed { processed, failed } => {
                        self.llm_batch_progress = None;
                        self.mode = AppMode::Normal;
                        if failed > 0 {
                            self.status_message = Some(format!(
                                "Batch complete: {} processed, {} failed",
                                processed, failed
                            ));
                        } else {
                            self.status_message = Some(format!(
                                "Batch complete: {} photos processed",
                                processed
                            ));
                        }
                        should_clear = true;
                    }
                    LlmTaskStatus::Error { message } => {
                        self.status_message = Some(format!("Error: {}", message));
                    }
                }
            }
        }

        if should_clear {
            self.llm_batch_receiver = None;
        }
    }
}

fn is_image(filename: &str) -> bool {
    let lower = filename.to_lowercase();
    lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".png")
        || lower.ends_with(".gif")
        || lower.ends_with(".webp")
        || lower.ends_with(".heic")
        || lower.ends_with(".heif")
}
