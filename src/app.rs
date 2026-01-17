use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use crate::config::Config;
use crate::db::Database;
use crate::llm::LlmClient;
use crate::scanner::Scanner;
use crate::ui;
use crate::ui::duplicates::DuplicatesView;
use crate::ui::export_dialog::ExportDialog;
use crate::ui::move_dialog::MoveDialog;
use crate::ui::preview::ImagePreviewState;
use crate::ui::rename_dialog::RenameDialog;
use crate::ui::search_dialog::SearchDialog;
use crate::ui::people_dialog::PeopleDialog;
use crate::faces::{FaceProcessor, FaceProcessingStatus};

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
    Visual,
    Moving,
    Renaming,
    Exporting,
    Searching,
    FaceScanning,
    PeopleManaging,
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
    // Image preview state
    pub image_preview: ImagePreviewState,
    // Multi-select state
    pub selected_files: HashSet<PathBuf>,
    // Visual mode anchor (start of selection range)
    pub visual_anchor: Option<usize>,
    // Move dialog state
    pub move_dialog: Option<MoveDialog>,
    // Rename dialog state
    pub rename_dialog: Option<RenameDialog>,
    // Export dialog state
    pub export_dialog: Option<ExportDialog>,
    // Search dialog state
    pub search_dialog: Option<SearchDialog>,
    // People dialog state
    pub people_dialog: Option<PeopleDialog>,
    // Face scanning state
    pub face_scan_receiver: Option<mpsc::Receiver<FaceProcessingStatus>>,
    pub face_scan_progress: Option<(usize, usize)>,
    pub face_scan_start: Option<std::time::Instant>,
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
        let image_preview = ImagePreviewState::new(config.preview.protocol);
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
            image_preview,
            selected_files: HashSet::new(),
            visual_anchor: None,
            move_dialog: None,
            rename_dialog: None,
            export_dialog: None,
            search_dialog: None,
            people_dialog: None,
            face_scan_receiver: None,
            face_scan_progress: None,
            face_scan_start: None,
        };
        app.load_directory(&current_dir)?;
        Ok(app)
    }

    pub fn load_directory(&mut self, path: &PathBuf) -> Result<()> {
        self.current_dir = path.clone();
        self.entries = self.read_directory(path)?;
        self.selected_index = 0;
        self.scroll_offset = 0;
        // Clear selection when changing directories
        self.selected_files.clear();
        self.visual_anchor = None;

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
            // Check for face scan progress
            self.update_face_scan_progress();

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

        // Handle Moving mode
        if self.mode == AppMode::Moving {
            return self.handle_move_dialog_key(key);
        }

        // Handle Renaming mode
        if self.mode == AppMode::Renaming {
            return self.handle_rename_dialog_key(key);
        }

        // Handle Exporting mode
        if self.mode == AppMode::Exporting {
            return self.handle_export_dialog_key(key);
        }

        // Handle Searching mode
        if self.mode == AppMode::Searching {
            return self.handle_search_dialog_key(key);
        }

        // Handle Face Scanning mode - only allow Escape to cancel
        if self.mode == AppMode::FaceScanning {
            if key.code == KeyCode::Esc {
                self.mode = AppMode::Normal;
                self.face_scan_receiver = None;
                self.face_scan_progress = None;
                self.face_scan_start = None;
                self.status_message = Some("Face scanning cancelled".to_string());
            }
            return Ok(());
        }

        // Handle People Managing mode
        if self.mode == AppMode::PeopleManaging {
            return self.handle_people_dialog_key(key);
        }

        // Handle Visual mode - j/k extends selection, Esc exits
        if self.mode == AppMode::Visual {
            match key.code {
                KeyCode::Esc => {
                    self.exit_visual_mode();
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    self.move_down();
                    self.update_visual_selection();
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.move_up();
                    self.update_visual_selection();
                }
                KeyCode::Char('G') => {
                    self.go_to_bottom();
                    self.update_visual_selection();
                }
                KeyCode::Char('g') => {
                    self.g_pressed = true;
                }
                KeyCode::Char(' ') => {
                    // In visual mode, Space also toggles through range
                    self.update_visual_selection();
                }
                KeyCode::Char('d') | KeyCode::Char('x') => {
                    // Delete marked files (future feature)
                    self.status_message = Some(format!("{} files selected", self.selected_files.len()));
                    self.exit_visual_mode();
                }
                _ => {}
            }

            // Handle gg in visual mode
            if self.g_pressed && key.code == KeyCode::Char('g') {
                self.g_pressed = false;
                self.selected_index = 0;
                self.scroll_offset = 0;
                self.update_visual_selection();
            } else if key.code != KeyCode::Char('g') {
                self.g_pressed = false;
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

            // Preview scroll ({ and } keys)
            KeyCode::Char('{') => {
                self.image_preview.scroll_up(3);
            }
            KeyCode::Char('}') => {
                self.image_preview.scroll_down(3);
            }

            // Home directory
            KeyCode::Char('~') => {
                if let Some(home) = dirs::home_dir() {
                    self.load_directory(&home)?;
                }
            }

            // Multi-select: toggle selection with Space
            KeyCode::Char(' ') => self.toggle_selection(),

            // Visual mode: V to enter, select range
            KeyCode::Char('V') => self.enter_visual_mode(),

            // Clear selection with Escape (only if there's a selection)
            KeyCode::Esc if !self.selected_files.is_empty() || self.mode == AppMode::Visual => {
                self.exit_visual_mode();
                self.clear_selection();
            }

            // Move files
            KeyCode::Char('m') => self.open_move_dialog()?,

            // Rename files
            KeyCode::Char('R') => self.open_rename_dialog()?,

            // Export photos
            KeyCode::Char('E') => self.open_export_dialog()?,

            // Semantic search
            KeyCode::Char('/') => self.open_search_dialog()?,

            // Face detection on current directory
            KeyCode::Char('F') => self.start_face_scan()?,

            // People management dialog
            KeyCode::Char('p') => self.open_people_dialog()?,

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
            self.image_preview.reset_scroll();
        }
    }

    fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            self.image_preview.reset_scroll();
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

    // --- Multi-select and Visual mode methods ---

    fn toggle_selection(&mut self) {
        if let Some(entry) = self.selected_entry() {
            let path = entry.path.clone();
            if self.selected_files.contains(&path) {
                self.selected_files.remove(&path);
            } else {
                self.selected_files.insert(path);
            }
        }
    }

    fn enter_visual_mode(&mut self) {
        self.mode = AppMode::Visual;
        self.visual_anchor = Some(self.selected_index);
        // Add current file to selection
        if let Some(entry) = self.selected_entry() {
            self.selected_files.insert(entry.path.clone());
        }
        self.status_message = Some("-- VISUAL --".to_string());
    }

    fn exit_visual_mode(&mut self) {
        self.mode = AppMode::Normal;
        self.visual_anchor = None;
        let count = self.selected_files.len();
        if count > 0 {
            self.status_message = Some(format!("{} files selected", count));
        } else {
            self.status_message = None;
        }
    }

    fn update_visual_selection(&mut self) {
        if let Some(anchor) = self.visual_anchor {
            let start = anchor.min(self.selected_index);
            let end = anchor.max(self.selected_index);

            // Clear and rebuild selection for the range
            self.selected_files.clear();
            for i in start..=end {
                if let Some(entry) = self.entries.get(i) {
                    self.selected_files.insert(entry.path.clone());
                }
            }
        }
    }

    pub fn clear_selection(&mut self) {
        self.selected_files.clear();
        self.visual_anchor = None;
        self.status_message = None;
    }

    /// Check if a path is currently selected
    pub fn is_selected(&self, path: &PathBuf) -> bool {
        self.selected_files.contains(path)
    }

    /// Get the number of selected files
    pub fn selection_count(&self) -> usize {
        self.selected_files.len()
    }

    // --- Move dialog methods ---

    fn open_move_dialog(&mut self) -> Result<()> {
        // Collect files to move: either selected files or the currently selected file
        let files_to_move: Vec<PathBuf> = if self.selected_files.is_empty() {
            // Move just the currently selected file
            if let Some(entry) = self.selected_entry() {
                if !entry.is_dir {
                    vec![entry.path.clone()]
                } else {
                    self.status_message = Some("Cannot move directories".to_string());
                    return Ok(());
                }
            } else {
                self.status_message = Some("No file selected".to_string());
                return Ok(());
            }
        } else {
            // Move all selected files
            self.selected_files.iter().cloned().collect()
        };

        if files_to_move.is_empty() {
            self.status_message = Some("No files to move".to_string());
            return Ok(());
        }

        self.move_dialog = Some(MoveDialog::new(self.current_dir.clone(), files_to_move));
        self.mode = AppMode::Moving;
        Ok(())
    }

    fn handle_move_dialog_key(&mut self, key: KeyEvent) -> Result<()> {
        if self.move_dialog.is_none() {
            self.mode = AppMode::Normal;
            return Ok(());
        }

        let dialog = self.move_dialog.as_mut().unwrap();

        // Handle input mode
        if dialog.input_mode {
            match key.code {
                KeyCode::Esc => {
                    dialog.input_mode = false;
                }
                KeyCode::Enter => {
                    dialog.confirm_input();
                }
                KeyCode::Backspace => {
                    dialog.backspace();
                }
                KeyCode::Char(c) => {
                    dialog.handle_input(c);
                }
                _ => {}
            }
            return Ok(());
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.move_dialog = None;
                self.mode = AppMode::Normal;
                self.status_message = Some("Move cancelled".to_string());
            }
            KeyCode::Char('j') | KeyCode::Down => {
                dialog.move_down();
            }
            KeyCode::Char('k') | KeyCode::Up => {
                dialog.move_up();
            }
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => {
                dialog.enter_selected();
            }
            KeyCode::Char('h') | KeyCode::Left | KeyCode::Backspace => {
                dialog.go_parent();
            }
            KeyCode::Char('/') => {
                dialog.toggle_input_mode();
            }
            KeyCode::Char('m') => {
                // Confirm move
                self.execute_move()?;
            }
            _ => {}
        }

        Ok(())
    }

    fn execute_move(&mut self) -> Result<()> {
        let dialog = match self.move_dialog.take() {
            Some(d) => d,
            None => return Ok(()),
        };

        let target_dir = dialog.target_dir().clone();
        let files_to_move = dialog.files_to_move;

        let mut moved = 0;
        let mut failed = 0;

        for source_path in &files_to_move {
            if let Some(filename) = source_path.file_name() {
                let target_path = target_dir.join(filename);

                // Check for conflicts
                if target_path.exists() {
                    // Skip existing files (could add overwrite option later)
                    self.status_message = Some(format!(
                        "Skipped {}: already exists",
                        filename.to_string_lossy()
                    ));
                    failed += 1;
                    continue;
                }

                // Perform the move
                match std::fs::rename(source_path, &target_path) {
                    Ok(_) => {
                        // Update database path
                        if let Err(e) = self.db.update_photo_path(source_path, &target_path) {
                            eprintln!("Warning: Failed to update DB path: {}", e);
                        }
                        moved += 1;
                    }
                    Err(_) => {
                        // Try copy + delete for cross-filesystem moves
                        if let Err(copy_err) = std::fs::copy(source_path, &target_path) {
                            self.status_message = Some(format!(
                                "Failed to move {}: {}",
                                filename.to_string_lossy(),
                                copy_err
                            ));
                            failed += 1;
                            continue;
                        }
                        if let Err(del_err) = std::fs::remove_file(source_path) {
                            self.status_message = Some(format!(
                                "Warning: Copied but failed to delete original: {}",
                                del_err
                            ));
                        }
                        // Update database path
                        if let Err(e) = self.db.update_photo_path(source_path, &target_path) {
                            eprintln!("Warning: Failed to update DB path: {}", e);
                        }
                        moved += 1;
                    }
                }
            }
        }

        // Clear selection and refresh directory
        self.selected_files.clear();
        self.load_directory(&self.current_dir.clone())?;

        self.mode = AppMode::Normal;
        if failed > 0 {
            self.status_message = Some(format!("Moved {} files, {} failed", moved, failed));
        } else {
            self.status_message = Some(format!("Moved {} files to {}", moved, target_dir.display()));
        }

        Ok(())
    }

    // --- Rename dialog methods ---

    fn open_rename_dialog(&mut self) -> Result<()> {
        // Collect files to rename: either selected files or the currently selected file
        let files_to_rename: Vec<PathBuf> = if self.selected_files.is_empty() {
            // Rename just the currently selected file
            if let Some(entry) = self.selected_entry() {
                if !entry.is_dir {
                    vec![entry.path.clone()]
                } else {
                    self.status_message = Some("Cannot rename directories".to_string());
                    return Ok(());
                }
            } else {
                self.status_message = Some("No file selected".to_string());
                return Ok(());
            }
        } else {
            // Rename all selected files (filter out directories)
            self.selected_files
                .iter()
                .filter(|p| p.is_file())
                .cloned()
                .collect()
        };

        if files_to_rename.is_empty() {
            self.status_message = Some("No files to rename".to_string());
            return Ok(());
        }

        self.rename_dialog = Some(RenameDialog::new(files_to_rename));
        self.mode = AppMode::Renaming;
        Ok(())
    }

    fn handle_rename_dialog_key(&mut self, key: KeyEvent) -> Result<()> {
        if self.rename_dialog.is_none() {
            self.mode = AppMode::Normal;
            return Ok(());
        }

        let dialog = self.rename_dialog.as_mut().unwrap();

        match key.code {
            KeyCode::Esc => {
                self.rename_dialog = None;
                self.mode = AppMode::Normal;
                self.status_message = Some("Rename cancelled".to_string());
            }
            KeyCode::Enter => {
                // Execute rename
                match dialog.execute() {
                    Ok((success, failed)) => {
                        self.rename_dialog = None;
                        self.mode = AppMode::Normal;
                        self.selected_files.clear();

                        // Refresh directory
                        self.load_directory(&self.current_dir.clone())?;

                        if failed > 0 {
                            self.status_message =
                                Some(format!("Renamed {} files, {} failed", success, failed));
                        } else {
                            self.status_message = Some(format!("Renamed {} files", success));
                        }
                    }
                    Err(e) => {
                        self.status_message = Some(format!("Rename error: {}", e));
                    }
                }
            }
            KeyCode::Left => {
                dialog.move_cursor_left();
            }
            KeyCode::Right => {
                dialog.move_cursor_right();
            }
            KeyCode::Home => {
                dialog.move_cursor_home();
            }
            KeyCode::End => {
                dialog.move_cursor_end();
            }
            KeyCode::Backspace => {
                dialog.backspace();
            }
            KeyCode::Delete => {
                dialog.delete();
            }
            KeyCode::Char(c) => {
                dialog.handle_char(c);
            }
            _ => {}
        }

        Ok(())
    }

    // --- Export dialog methods ---

    fn open_export_dialog(&mut self) -> Result<()> {
        self.export_dialog = Some(ExportDialog::new(self.current_dir.clone()));
        self.mode = AppMode::Exporting;
        Ok(())
    }

    fn handle_export_dialog_key(&mut self, key: KeyEvent) -> Result<()> {
        if self.export_dialog.is_none() {
            self.mode = AppMode::Normal;
            return Ok(());
        }

        let dialog = self.export_dialog.as_mut().unwrap();

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.export_dialog = None;
                self.mode = AppMode::Normal;
                self.status_message = Some("Export cancelled".to_string());
            }
            KeyCode::Char('j') | KeyCode::Down => {
                dialog.move_down();
            }
            KeyCode::Char('k') | KeyCode::Up => {
                dialog.move_up();
            }
            KeyCode::Enter => {
                // Execute export
                let format = dialog.selected_format();
                let output_path = dialog.output_path().clone();

                match crate::export::export_photos(&self.db, &output_path, format) {
                    Ok(count) => {
                        self.export_dialog = None;
                        self.mode = AppMode::Normal;
                        self.status_message = Some(format!(
                            "Exported {} photos to {}",
                            count,
                            output_path.display()
                        ));
                    }
                    Err(e) => {
                        self.status_message = Some(format!("Export error: {}", e));
                    }
                }
            }
            _ => {}
        }

        Ok(())
    }

    // --- Search dialog methods ---

    fn open_search_dialog(&mut self) -> Result<()> {
        self.search_dialog = Some(SearchDialog::new());
        self.mode = AppMode::Searching;
        Ok(())
    }

    fn handle_search_dialog_key(&mut self, key: KeyEvent) -> Result<()> {
        if self.search_dialog.is_none() {
            self.mode = AppMode::Normal;
            return Ok(());
        }

        let dialog = self.search_dialog.as_mut().unwrap();

        match key.code {
            KeyCode::Esc => {
                self.search_dialog = None;
                self.mode = AppMode::Normal;
            }
            KeyCode::Enter => {
                // Execute search
                if !dialog.query.is_empty() {
                    self.execute_semantic_search()?;
                }
            }
            KeyCode::Tab | KeyCode::Down => {
                dialog.move_selection_down();
            }
            KeyCode::BackTab | KeyCode::Up => {
                dialog.move_selection_up();
            }
            KeyCode::Left => {
                dialog.move_cursor_left();
            }
            KeyCode::Right => {
                dialog.move_cursor_right();
            }
            KeyCode::Backspace => {
                dialog.backspace();
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                dialog.clear();
            }
            KeyCode::Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                // Open selected result
                if let Some(result) = dialog.selected_result() {
                    let path = PathBuf::from(&result.path);
                    if let Some(parent) = path.parent() {
                        self.load_directory(&parent.to_path_buf())?;
                        // Try to select the file
                        let filename = path.file_name().map(|n| n.to_string_lossy().to_string());
                        if let Some(fname) = filename {
                            if let Some(idx) = self.entries.iter().position(|e| e.name == fname) {
                                self.selected_index = idx;
                            }
                        }
                    }
                    self.search_dialog = None;
                    self.mode = AppMode::Normal;
                }
            }
            KeyCode::Char(c) => {
                dialog.handle_char(c);
            }
            _ => {}
        }

        Ok(())
    }

    fn execute_semantic_search(&mut self) -> Result<()> {
        let dialog = match self.search_dialog.as_mut() {
            Some(d) => d,
            None => return Ok(()),
        };

        dialog.searching = true;
        dialog.status = Some("Searching...".to_string());

        // Get embedding for query
        let query = dialog.query.clone();

        // Try to get embedding from LLM client
        // For now, we'll use a simple text-based search on descriptions
        // since embedding support depends on the provider
        let results = self.db.semantic_search_by_text(&query, 20)?;

        dialog.set_results(results);
        Ok(())
    }

    // --- Face scanning methods ---

    fn start_face_scan(&mut self) -> Result<()> {
        // Don't start if already scanning
        if self.mode == AppMode::FaceScanning {
            return Ok(());
        }

        // Get photos without faces in current directory
        let photos = self.db.get_photos_without_faces(100)?;

        if photos.is_empty() {
            self.status_message = Some("No unscanned photos found".to_string());
            return Ok(());
        }

        let total = photos.len();
        let (tx, rx) = mpsc::channel();
        let db_path = self.config.db_path.clone();

        // Spawn face scanning in background thread using dlib
        std::thread::spawn(move || {
            let db = match crate::db::Database::open(&db_path) {
                Ok(db) => db,
                Err(e) => {
                    let _ = tx.send(FaceProcessingStatus::Error {
                        message: format!("Failed to open database: {}", e),
                    });
                    return;
                }
            };

            // Use dlib-based face processor (no LLM needed)
            let mut processor = FaceProcessor::new();
            let _ = processor.process_batch(&db, &photos, Some(tx));
        });

        self.face_scan_receiver = Some(rx);
        self.face_scan_progress = Some((0, total));
        self.face_scan_start = Some(std::time::Instant::now());
        self.mode = AppMode::FaceScanning;
        self.status_message = Some(format!("Scanning {} photos for faces...", total));

        Ok(())
    }

    fn update_face_scan_progress(&mut self) {
        let mut should_clear = false;

        // Calculate elapsed time for display
        let elapsed_str = if let Some(start) = self.face_scan_start {
            let elapsed = start.elapsed();
            format!(" ({}s)", elapsed.as_secs())
        } else {
            String::new()
        };

        if let Some(ref receiver) = self.face_scan_receiver {
            while let Ok(status) = receiver.try_recv() {
                match status {
                    FaceProcessingStatus::Starting { total_photos } => {
                        self.face_scan_progress = Some((0, total_photos));
                        self.status_message = Some(format!("Starting face scan for {} photos...", total_photos));
                    }
                    FaceProcessingStatus::InitializingModels => {
                        self.status_message = Some("Loading face detection models...".to_string());
                    }
                    FaceProcessingStatus::Processing { current, total, path } => {
                        self.face_scan_progress = Some((current, total));
                        let filename = std::path::Path::new(&path)
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or(path);
                        self.status_message = Some(format!("[{}/{}] Scanning {}{}", current, total, filename, elapsed_str));
                    }
                    FaceProcessingStatus::FoundFaces { path: _, count } => {
                        // Update status to show face was found
                        if count > 0 {
                            if let Some((current, total)) = self.face_scan_progress {
                                self.status_message = Some(format!("[{}/{}] Found {} face(s){}", current, total, count, elapsed_str));
                            }
                        }
                    }
                    FaceProcessingStatus::Completed { photos_processed, faces_found } => {
                        let total_elapsed = if let Some(start) = self.face_scan_start {
                            format!(" in {}s", start.elapsed().as_secs())
                        } else {
                            String::new()
                        };
                        self.face_scan_progress = None;
                        self.face_scan_start = None;
                        self.mode = AppMode::Normal;
                        self.status_message = Some(format!(
                            "Face scan complete: {} photos, {} faces found{}",
                            photos_processed, faces_found, total_elapsed
                        ));
                        should_clear = true;
                    }
                    FaceProcessingStatus::Error { message } => {
                        self.status_message = Some(format!("Face scan error: {}", message));
                    }
                }
            }
        }

        // Update elapsed time even if no new messages (to show something is happening)
        if self.mode == AppMode::FaceScanning && !should_clear {
            if let Some((current, total)) = self.face_scan_progress {
                if let Some(start) = self.face_scan_start {
                    let elapsed = start.elapsed().as_secs();
                    // Only update every second to avoid excessive updates
                    let current_msg = self.status_message.as_deref().unwrap_or("");
                    if !current_msg.contains(&format!("({}s)", elapsed)) {
                        // Keep the current status but update the time
                        if let Some(pos) = current_msg.rfind(" (") {
                            let base_msg = &current_msg[..pos];
                            self.status_message = Some(format!("{} ({}s)", base_msg, elapsed));
                        } else if !current_msg.is_empty() {
                            self.status_message = Some(format!("{} ({}s)", current_msg, elapsed));
                        } else {
                            self.status_message = Some(format!("[{}/{}] Processing... ({}s)", current, total, elapsed));
                        }
                    }
                }
            }
        }

        if should_clear {
            self.face_scan_receiver = None;
        }
    }

    // --- People dialog methods ---

    fn open_people_dialog(&mut self) -> Result<()> {
        let people = self.db.get_all_people()?;
        let faces = self.db.get_unassigned_faces()?;

        // Always open the dialog, even if empty (shows instructions)
        self.people_dialog = Some(PeopleDialog::new(people, faces));
        self.mode = AppMode::PeopleManaging;
        Ok(())
    }

    fn handle_people_dialog_key(&mut self, key: KeyEvent) -> Result<()> {
        use crate::ui::people_dialog::InputMode;

        if self.people_dialog.is_none() {
            self.mode = AppMode::Normal;
            return Ok(());
        }

        let dialog = self.people_dialog.as_mut().unwrap();

        // Handle naming mode (text input)
        if dialog.input_mode == InputMode::Naming {
            match key.code {
                KeyCode::Esc => {
                    dialog.exit_naming_mode();
                }
                KeyCode::Enter => {
                    // Confirm the name
                    let name = dialog.get_name().to_string();
                    if !name.is_empty() {
                        if let Some(face_id) = dialog.selected_face_id() {
                            // Find existing person or create a new one, then assign the face
                            match self.db.find_or_create_person(&name) {
                                Ok(person_id) => {
                                    if let Err(e) = self.db.assign_face_to_person(face_id, person_id) {
                                        self.status_message = Some(format!("Error assigning face: {}", e));
                                    } else {
                                        self.status_message = Some(format!("Assigned to: {}", name));
                                    }
                                }
                                Err(e) => {
                                    self.status_message = Some(format!("Error: {}", e));
                                }
                            }
                        } else if let Some(person_id) = dialog.selected_person_id() {
                            // Rename person
                            match self.db.update_person_name(person_id, &name) {
                                Ok(_) => {
                                    self.status_message = Some(format!("Renamed to: {}", name));
                                }
                                Err(e) => {
                                    self.status_message = Some(format!("Error: {}", e));
                                }
                            }
                        }

                        // Refresh dialog data
                        let people = self.db.get_all_people()?;
                        let faces = self.db.get_unassigned_faces()?;
                        dialog.update_data(people, faces);
                    }
                    dialog.exit_naming_mode();
                }
                KeyCode::Left => {
                    dialog.move_cursor_left();
                }
                KeyCode::Right => {
                    dialog.move_cursor_right();
                }
                KeyCode::Backspace => {
                    dialog.backspace();
                }
                KeyCode::Char(c) => {
                    dialog.handle_char(c);
                }
                _ => {}
            }
            return Ok(());
        }

        // Normal navigation mode
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.people_dialog = None;
                self.mode = AppMode::Normal;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                dialog.move_down();
            }
            KeyCode::Char('k') | KeyCode::Up => {
                dialog.move_up();
            }
            KeyCode::Tab => {
                dialog.toggle_view_mode();
            }
            KeyCode::Char('n') => {
                // Name the selected cluster or rename the selected person
                if !dialog.is_empty() {
                    dialog.enter_naming_mode();
                }
            }
            KeyCode::Enter => {
                // View photos for selected person
                if let Some(person_id) = dialog.selected_person_id() {
                    let photos = self.db.search_photos_by_person(person_id)?;
                    if !photos.is_empty() {
                        // Navigate to the first photo's directory
                        if let Some((_, path, _)) = photos.first() {
                            let photo_path = PathBuf::from(path);
                            if let Some(parent) = photo_path.parent() {
                                self.load_directory(&parent.to_path_buf())?;
                                // Try to select the file
                                if let Some(fname) = photo_path.file_name() {
                                    let fname_str = fname.to_string_lossy().to_string();
                                    if let Some(idx) = self.entries.iter().position(|e| e.name == fname_str) {
                                        self.selected_index = idx;
                                    }
                                }
                            }
                        }
                        self.people_dialog = None;
                        self.mode = AppMode::Normal;
                        self.status_message = Some(format!("Found {} photos", photos.len()));
                    } else {
                        dialog.status = Some("No photos for this person".to_string());
                    }
                }
            }
            KeyCode::Char('d') => {
                // Delete selected person
                if let Some(person_id) = dialog.selected_person_id() {
                    if let Err(e) = self.db.delete_person(person_id) {
                        self.status_message = Some(format!("Error deleting: {}", e));
                    } else {
                        // Refresh dialog data
                        let people = self.db.get_all_people()?;
                        let faces = self.db.get_unassigned_faces()?;
                        dialog.update_data(people, faces);
                        self.status_message = Some("Person deleted".to_string());
                    }
                }
            }
            _ => {}
        }

        Ok(())
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
