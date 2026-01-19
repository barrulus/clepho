use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Duration;

use crate::config::Config;
use crate::db::{Database, ScheduledTaskType};
use crate::llm::LlmClient;
use crate::scanner::{detect_changes, ChangeDetectionResult, Scanner};
use crate::schedule::ScheduleManager;
use crate::tasks::{BackgroundTaskManager, TaskType, TaskUpdate};
use crate::trash::TrashManager;
use crate::ui;
use crate::ui::changes_dialog::ChangesDialog;
use crate::ui::duplicates::DuplicatesView;
use crate::ui::export_dialog::ExportDialog;
use crate::ui::move_dialog::MoveDialog;
use crate::ui::overdue_dialog::OverdueDialog;
use crate::ui::preview::ImagePreviewState;
use crate::ui::rename_dialog::RenameDialog;
use crate::ui::schedule_dialog::ScheduleDialog;
use crate::ui::search_dialog::SearchDialog;
use crate::ui::people_dialog::PeopleDialog;
use crate::ui::trash_dialog::TrashDialog;

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
    Duplicates,
    DuplicatesHelp,
    Visual,
    Moving,
    Renaming,
    Exporting,
    Searching,
    PeopleManaging,
    TaskList,
    TrashViewing,
    ChangesViewing,
    Scheduling,
    OverdueDialog,
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
    // Duplicates view
    pub duplicates_view: Option<DuplicatesView>,
    // LLM state
    pub llm_client: LlmClient,
    pub llm_descriptions: HashMap<PathBuf, String>,
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
    // Background task manager
    pub task_manager: BackgroundTaskManager,
    // Trash manager and dialog
    pub trash_manager: TrashManager,
    pub trash_dialog: Option<TrashDialog>,
    // Change detection
    pub detected_changes: Option<ChangeDetectionResult>,
    pub changes_dialog: Option<ChangesDialog>,
    // Schedule management
    pub schedule_manager: ScheduleManager,
    pub schedule_dialog: Option<ScheduleDialog>,
    pub overdue_dialog: Option<OverdueDialog>,
}

#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
}

impl App {
    pub fn new(config: Config, db: Database) -> Result<Self> {
        let current_dir = std::env::current_dir()?;
        let llm_client = LlmClient::new(&config.llm.endpoint, &config.llm.model);
        let image_preview = ImagePreviewState::new(config.preview.protocol, &config.thumbnails);
        let trash_manager = TrashManager::new(config.trash.clone());
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
            duplicates_view: None,
            llm_client,
            llm_descriptions: HashMap::new(),
            image_preview,
            selected_files: HashSet::new(),
            visual_anchor: None,
            move_dialog: None,
            rename_dialog: None,
            export_dialog: None,
            search_dialog: None,
            people_dialog: None,
            task_manager: BackgroundTaskManager::new(),
            trash_manager,
            trash_dialog: None,
            detected_changes: None,
            changes_dialog: None,
            schedule_manager: ScheduleManager::new(),
            schedule_dialog: None,
            overdue_dialog: None,
        };
        app.load_directory(&current_dir)?;

        // Check for overdue schedules on startup
        if app.config.schedule.check_overdue_on_startup {
            let overdue = app.schedule_manager.check_overdue(&app.db);
            if !overdue.is_empty() {
                app.overdue_dialog = Some(OverdueDialog::new(overdue));
                app.mode = AppMode::OverdueDialog;
            }
        }

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

        // Check for file changes in this directory
        self.check_for_changes();

        Ok(())
    }

    /// Check for new/modified files in the current directory.
    fn check_for_changes(&mut self) {
        let result = detect_changes(
            &self.current_dir,
            &self.db,
            &self.config.scanner.image_extensions,
        );

        match result {
            Ok(changes) if changes.has_changes() => {
                self.detected_changes = Some(changes);
            }
            _ => {
                self.detected_changes = None;
            }
        }
    }

    fn read_directory(&self, path: &PathBuf) -> Result<Vec<DirEntry>> {
        let mut entries = Vec::new();

        if let Ok(read_dir) = std::fs::read_dir(path) {
            for entry in read_dir.flatten() {
                let metadata = entry.metadata().ok();
                let is_dir = metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false);
                let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);

                entries.push(DirEntry {
                    name: entry.file_name().to_string_lossy().to_string(),
                    path: entry.path(),
                    is_dir,
                    size,
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
            // Poll for task updates and handle completions
            let completions = self.task_manager.poll_updates();
            for completion in completions {
                let prefix = completion.task_type.display_name();
                if completion.success {
                    self.status_message = Some(format!("{}: {}", prefix, completion.message));

                    // Clear metadata cache after scan completes so preview shows fresh data
                    if matches!(completion.task_type, TaskType::Scan | TaskType::LlmSingle | TaskType::LlmBatch | TaskType::FaceDetection) {
                        self.image_preview.metadata_cache.clear();
                    }
                } else {
                    self.status_message = Some(format!("{} - {}", prefix, completion.message));
                }
            }

            // Poll for scheduled tasks that are due
            let _ = self.poll_schedules();

            terminal.draw(|frame| ui::render(frame, self))?;

            if event::poll(Duration::from_millis(100))? {
                match event::read()? {
                    Event::Key(key) => self.handle_key(key)?,
                    Event::Mouse(mouse) => {
                        let size = terminal.size()?;
                        let area = Rect::new(0, 0, size.width, size.height);
                        match self.mode {
                            AppMode::PeopleManaging => self.handle_people_dialog_mouse(mouse, area)?,
                            AppMode::Normal => self.handle_mouse(mouse, area)?,
                            _ => {} // Other modes don't have mouse support yet
                        }
                    }
                    Event::Resize(_, _) => {}
                    _ => {}
                }
            }
        }

        Ok(())
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

        // Handle People Managing mode
        if self.mode == AppMode::PeopleManaging {
            return self.handle_people_dialog_key(key);
        }

        // Handle TaskList mode
        if self.mode == AppMode::TaskList {
            return self.handle_task_list_key(key);
        }

        // Handle TrashViewing mode
        if self.mode == AppMode::TrashViewing {
            return self.handle_trash_dialog_key(key);
        }

        // Handle ChangesViewing mode
        if self.mode == AppMode::ChangesViewing {
            return self.handle_changes_dialog_key(key);
        }

        // Handle Scheduling mode
        if self.mode == AppMode::Scheduling {
            return self.handle_schedule_dialog_key(key);
        }

        // Handle OverdueDialog mode
        if self.mode == AppMode::OverdueDialog {
            return self.handle_overdue_dialog_key(key);
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

            // ESC: Cancel most recent running task, or clear selection
            KeyCode::Esc => {
                if self.task_manager.has_running_tasks() {
                    if self.task_manager.cancel_most_recent() {
                        self.status_message = Some("Task cancelled".to_string());
                    }
                } else if !self.selected_files.is_empty() || self.mode == AppMode::Visual {
                    self.exit_visual_mode();
                    self.clear_selection();
                }
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

            // Face clustering
            KeyCode::Char('C') => self.cluster_faces()?,

            // People management dialog
            KeyCode::Char('p') => self.open_people_dialog()?,

            // Task list dialog
            KeyCode::Char('T') => {
                self.mode = AppMode::TaskList;
            }

            // Trash dialog
            KeyCode::Char('t') => self.open_trash_dialog()?,

            // Changes dialog (check/view file changes)
            KeyCode::Char('c') => self.open_changes_dialog()?,

            // Schedule dialog
            KeyCode::Char('@') => self.open_schedule_dialog()?,

            _ => {}
        }

        Ok(())
    }

    fn handle_mouse(&mut self, mouse: MouseEvent, area: Rect) -> Result<()> {
        // Calculate pane layout for all mouse events
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

        // Determine which pane the mouse is in
        let in_parent_pane = x < chunks[0].right() && y >= chunks[0].y && y < chunks[0].bottom();
        let in_current_pane = x >= chunks[1].x && x < chunks[1].right() && y >= chunks[1].y && y < chunks[1].bottom();
        let in_preview_pane = x >= chunks[2].x && x < chunks[2].right() && y >= chunks[2].y && y < chunks[2].bottom();

        match mouse.kind {
            MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                if in_parent_pane {
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
                } else if in_current_pane {
                    // Clicked in current pane
                    let clicked_index = (y - chunks[1].y - 1) as usize + self.scroll_offset;
                    if clicked_index < self.entries.len() {
                        self.selected_index = clicked_index;
                        // Navigate into directory if clicked on one (like yazi)
                        if self.entries[clicked_index].is_dir {
                            let path = self.entries[clicked_index].path.clone();
                            self.load_directory(&path)?;
                        }
                        self.image_preview.reset_scroll();
                    }
                }
                // Left click in preview pane - terminal handles text selection with Shift+drag
            }
            MouseEventKind::Down(crossterm::event::MouseButton::Right) => {
                // Right click to open with system default application
                if let Some(entry) = self.selected_entry().cloned() {
                    if !entry.is_dir {
                        self.open_with_system(&entry.path)?;
                    }
                }
            }
            MouseEventKind::ScrollDown => {
                if in_preview_pane {
                    // Scroll preview text down
                    self.image_preview.scroll_down(3);
                } else {
                    // Scroll file list down
                    self.move_down();
                }
            }
            MouseEventKind::ScrollUp => {
                if in_preview_pane {
                    // Scroll preview text up
                    self.image_preview.scroll_up(3);
                } else {
                    // Scroll file list up
                    self.move_up();
                }
            }
            _ => {}
        }

        Ok(())
    }

    fn handle_people_dialog_mouse(&mut self, mouse: MouseEvent, area: Rect) -> Result<()> {
        use crate::ui::people_dialog::{InputMode, PeopleViewMode};

        let dialog = match self.people_dialog.as_mut() {
            Some(d) => d,
            None => return Ok(()),
        };

        // Don't handle mouse events in naming mode (let keyboard handle input)
        if dialog.input_mode == InputMode::Naming {
            return Ok(());
        }

        // Calculate dialog dimensions (matching render logic in people_dialog.rs)
        let base_width = if dialog.view_mode == PeopleViewMode::Faces { 100 } else { 70 };
        let dialog_width = base_width.min(area.width.saturating_sub(4));
        let dialog_height = 30.min(area.height.saturating_sub(4));

        let dialog_x = (area.width - dialog_width) / 2;
        let dialog_y = (area.height - dialog_height) / 2;

        let mouse_x = mouse.column;
        let mouse_y = mouse.row;

        // Check if click is within dialog bounds
        if mouse_x < dialog_x || mouse_x >= dialog_x + dialog_width
            || mouse_y < dialog_y || mouse_y >= dialog_y + dialog_height
        {
            return Ok(());
        }

        // Convert to dialog-local coordinates (accounting for border)
        let local_x = mouse_x - dialog_x - 1;
        let local_y = mouse_y - dialog_y - 1;

        // Inner dialog dimensions (after border)
        let inner_width = dialog_width.saturating_sub(2);
        let inner_height = dialog_height.saturating_sub(2);

        // Layout matches people_dialog.rs render():
        // - Row 0-1: Tab bar (height 2)
        // - Row 2+: List area (or list+preview in Faces view)
        // - Bottom rows: input, status, footer

        let tab_bar_height = 2;
        let footer_height = 5; // status + footer + potential name input
        let list_start_y = tab_bar_height;
        let list_height = inner_height.saturating_sub(tab_bar_height + footer_height);

        match mouse.kind {
            MouseEventKind::Down(crossterm::event::MouseButton::Left) => {
                // Check if click is in tab bar area
                if local_y < tab_bar_height {
                    // Rough detection: "People" is at the start, "Faces" is further right
                    // Tab text: " People (N)  |  Faces (N)   [Tab to switch]"
                    // People tab roughly spans columns 1-15, Faces tab spans 18-35
                    if local_x < 17 {
                        // Clicked on People tab
                        if dialog.view_mode != PeopleViewMode::People {
                            dialog.toggle_view_mode();
                        }
                    } else if local_x < 35 {
                        // Clicked on Faces tab
                        if dialog.view_mode != PeopleViewMode::Faces {
                            dialog.toggle_view_mode();
                        }
                    }
                } else if local_y >= list_start_y && local_y < list_start_y + list_height {
                    // Click in list area
                    // Account for list border (1 row for top border)
                    let list_local_y = local_y - list_start_y - 1;

                    // Each item takes 2 rows (name + subtext)
                    let clicked_index = (list_local_y / 2) as usize;

                    let max_index = match dialog.view_mode {
                        PeopleViewMode::People => dialog.people.len().saturating_sub(1),
                        PeopleViewMode::Faces => dialog.faces.len().saturating_sub(1),
                    };

                    // In Faces view, only left half is the list (right half is preview)
                    let in_list_area = if dialog.view_mode == PeopleViewMode::Faces {
                        local_x < inner_width / 2
                    } else {
                        true
                    };

                    if in_list_area && clicked_index <= max_index {
                        dialog.selected_index = clicked_index;
                    }
                }
            }
            MouseEventKind::ScrollDown => {
                // Scroll down in list
                dialog.move_down();
            }
            MouseEventKind::ScrollUp => {
                // Scroll up in list
                dialog.move_up();
            }
            _ => {}
        }

        Ok(())
    }

    /// Open a file with the system default application or configured viewer
    fn open_with_system(&self, path: &std::path::Path) -> Result<()> {
        let opener = if let Some(ref viewer) = self.config.preview.external_viewer {
            viewer.as_str()
        } else {
            // Use system default
            #[cfg(target_os = "linux")]
            { "xdg-open" }
            #[cfg(target_os = "macos")]
            { "open" }
            #[cfg(target_os = "windows")]
            { "start" }
            #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
            { "xdg-open" }
        };

        std::process::Command::new(opener)
            .arg(path)
            .spawn()
            .map_err(|e| anyhow::anyhow!("Failed to open file: {}", e))?;

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

    /// Get full photo metadata from database (cached via ImagePreviewState)
    pub fn get_photo_metadata(&mut self, path: &std::path::PathBuf) -> Option<crate::db::PhotoMetadata> {
        // Check if already cached in the preview state
        if let Some(cached) = self.image_preview.get_cached_metadata(path) {
            return cached.clone();
        }

        // Fetch from database
        let metadata = self.db.get_photo_metadata(path).ok().flatten();

        // Cache for future lookups
        self.image_preview.cache_metadata(path.clone(), metadata.clone());

        metadata
    }

    fn start_scan(&mut self) -> Result<()> {
        // Don't start a new scan if one is already running
        if self.task_manager.is_running(TaskType::Scan) {
            self.status_message = Some("Scan already running".to_string());
            return Ok(());
        }

        let (_task_id, tx, cancel_flag) = self.task_manager.register_task(TaskType::Scan);
        let dir = self.current_dir.clone();
        let config = self.config.clone();
        let db_path = self.config.db_path.clone();

        // Spawn scanning in a background thread
        std::thread::spawn(move || {
            let db = match Database::open(&db_path) {
                Ok(db) => db,
                Err(e) => {
                    let _ = tx.send(TaskUpdate::Failed {
                        error: format!("Failed to open database: {}", e),
                    });
                    return;
                }
            };

            if let Err(e) = db.initialize() {
                let _ = tx.send(TaskUpdate::Failed {
                    error: format!("Failed to initialize database: {}", e),
                });
                return;
            }

            let scanner = Scanner::new(config);
            scanner.scan_directory_cancellable(&dir, &db, tx, cancel_flag);
        });

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

            // Move marked to trash (safe deletion)
            KeyCode::Char('x') => {
                let marked = self.db.get_marked_not_trashed()?;
                if marked.is_empty() {
                    self.status_message = Some("No photos marked for deletion".to_string());
                } else {
                    let mut moved = 0;
                    for photo in &marked {
                        let path = std::path::PathBuf::from(&photo.path);
                        match self.trash_manager.move_to_trash(&path) {
                            Ok(trash_path) => {
                                if let Err(e) = self.db.mark_trashed(photo.id, &trash_path) {
                                    self.status_message = Some(format!("DB error: {}", e));
                                } else {
                                    moved += 1;
                                }
                            }
                            Err(e) => {
                                self.status_message = Some(format!("Error moving to trash: {}", e));
                            }
                        }
                    }
                    self.status_message = Some(format!("Moved {} files to trash", moved));

                    // Refresh duplicates view
                    self.find_duplicates()?;
                }
            }

            // Permanently delete marked photos (dangerous)
            KeyCode::Char('X') => {
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
                    self.status_message = Some(format!("Permanently deleted {} photos", count));

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

        // Don't start if already running LLM single
        if self.task_manager.is_running(TaskType::LlmSingle) {
            self.status_message = Some("LLM description already running".to_string());
            return Ok(());
        }

        let (_task_id, tx, cancel_flag) = self.task_manager.register_task(TaskType::LlmSingle);
        let path = entry.path.clone();
        let endpoint = self.config.llm.endpoint.clone();
        let model = self.config.llm.model.clone();
        let db_path = self.config.db_path.clone();

        // Spawn LLM request in background thread
        std::thread::spawn(move || {
            // Check cancellation
            if cancel_flag.load(Ordering::SeqCst) {
                let _ = tx.send(TaskUpdate::Cancelled);
                return;
            }

            let client = LlmClient::new(&endpoint, &model);
            let _ = tx.send(TaskUpdate::Started { total: 1 });

            match client.describe_image(&path) {
                Ok(description) => {
                    // Save to database
                    if let Ok(db) = Database::open(&db_path) {
                        let _ = db.save_description(&path, &description);
                    }
                    let _ = tx.send(TaskUpdate::Completed {
                        message: format!("Description saved for {}", path.file_name().unwrap_or_default().to_string_lossy()),
                    });
                }
                Err(e) => {
                    let _ = tx.send(TaskUpdate::Failed {
                        error: e.to_string(),
                    });
                }
            }
        });

        self.status_message = Some(format!("Describing {}...", entry.name));

        Ok(())
    }

    fn start_batch_llm(&mut self) -> Result<()> {
        // Don't start if already processing
        if self.task_manager.is_running(TaskType::LlmBatch) {
            self.status_message = Some("Batch LLM already running".to_string());
            return Ok(());
        }

        // Get photos without descriptions in current directory
        let tasks = self.db.get_photos_without_description_in_dir(&self.current_dir)?;

        if tasks.is_empty() {
            self.status_message = Some("No unprocessed photos in this directory".to_string());
            return Ok(());
        }

        let total = tasks.len();
        let (_task_id, tx, cancel_flag) = self.task_manager.register_task(TaskType::LlmBatch);
        let endpoint = self.config.llm.endpoint.clone();
        let model = self.config.llm.model.clone();
        let db_path = self.config.db_path.clone();

        // Spawn batch processing in background thread
        std::thread::spawn(move || {
            let db = match Database::open(&db_path) {
                Ok(db) => db,
                Err(e) => {
                    let _ = tx.send(TaskUpdate::Failed {
                        error: format!("Failed to open database: {}", e),
                    });
                    return;
                }
            };

            let client = LlmClient::new(&endpoint, &model);
            let mut queue = crate::llm::LlmQueue::new(client);
            queue.add_tasks(tasks);
            queue.process_all_cancellable(&db, tx, cancel_flag);
        });

        self.status_message = Some(format!("Processing {} photos...", total));

        Ok(())
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

        let query = dialog.query.clone();

        // Try embedding-based search if the provider supports it and we have embeddings
        let results = if self.llm_client.supports_embeddings() {
            // Try to get embedding for the query
            match self.llm_client.get_text_embedding(&query) {
                Ok(query_embedding) => {
                    // Use vector similarity search
                    match self.db.semantic_search(&query_embedding, 20, 0.3) {
                        Ok(results) if !results.is_empty() => results,
                        _ => {
                            // Fallback to text search if no embedding results
                            self.db.semantic_search_by_text(&query, 20)?
                        }
                    }
                }
                Err(_) => {
                    // Fallback to text search if embedding fails
                    self.db.semantic_search_by_text(&query, 20)?
                }
            }
        } else {
            // Provider doesn't support embeddings, use text search
            self.db.semantic_search_by_text(&query, 20)?
        };

        dialog.set_results(results);
        Ok(())
    }

    // --- Face scanning methods ---

    fn start_face_scan(&mut self) -> Result<()> {
        // Don't start if already scanning
        if self.task_manager.is_running(TaskType::FaceDetection) {
            self.status_message = Some("Face scan already running".to_string());
            return Ok(());
        }

        // Get photos without faces in current directory
        let photos = self.db.get_photos_without_faces(100)?;

        if photos.is_empty() {
            self.status_message = Some("No unscanned photos found".to_string());
            return Ok(());
        }

        let total = photos.len();
        let (_task_id, tx, cancel_flag) = self.task_manager.register_task(TaskType::FaceDetection);
        let db_path = self.config.db_path.clone();

        // Spawn face scanning in background thread using dlib
        std::thread::spawn(move || {
            let db = match crate::db::Database::open(&db_path) {
                Ok(db) => db,
                Err(e) => {
                    let _ = tx.send(TaskUpdate::Failed {
                        error: format!("Failed to open database: {}", e),
                    });
                    return;
                }
            };

            // Use dlib-based face processor (no LLM needed)
            let mut processor = crate::faces::FaceProcessor::new();
            processor.process_batch_cancellable(&db, &photos, tx, cancel_flag);
        });

        self.status_message = Some(format!("Scanning {} photos for faces...", total));

        Ok(())
    }

    /// Cluster detected faces by similarity
    fn cluster_faces(&mut self) -> Result<()> {
        // Use a default threshold of 0.6 for face similarity
        let threshold = 0.6;

        match crate::faces::cluster_faces(&self.db, threshold) {
            Ok(result) => {
                if result.clusters_created == 0 {
                    self.status_message = Some("No faces to cluster (run face detection first)".to_string());
                } else {
                    self.status_message = Some(format!(
                        "Created {} clusters from {} faces{}",
                        result.clusters_created,
                        result.faces_clustered,
                        if result.faces_skipped > 0 {
                            format!(" ({} skipped, no embedding)", result.faces_skipped)
                        } else {
                            String::new()
                        }
                    ));
                }
            }
            Err(e) => {
                self.status_message = Some(format!("Clustering error: {}", e));
            }
        }

        Ok(())
    }

    // --- Task list dialog methods ---

    fn handle_task_list_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            // Exit task list
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('T') => {
                self.mode = AppMode::Normal;
            }
            // Cancel task by number
            KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
                let index = c.to_digit(10).unwrap() as usize - 1;
                if let Some(task_id) = self.task_manager.get_running_task_by_index(index) {
                    if self.task_manager.cancel_task(task_id) {
                        self.status_message = Some("Task cancelled".to_string());
                    }
                }
            }
            // Cancel all tasks
            KeyCode::Char('c') => {
                self.task_manager.cancel_all();
                self.status_message = Some("All tasks cancelled".to_string());
            }
            _ => {}
        }
        Ok(())
    }

    // --- Trash dialog methods ---

    fn open_trash_dialog(&mut self) -> Result<()> {
        let trashed = self.db.get_trashed_photos()?;
        let total_size = self.db.get_trash_total_size()?;
        self.trash_dialog = Some(TrashDialog::new(
            trashed,
            total_size,
            self.trash_manager.max_size_bytes(),
        ));
        self.mode = AppMode::TrashViewing;
        Ok(())
    }

    fn handle_trash_dialog_key(&mut self, key: KeyEvent) -> Result<()> {
        if self.trash_dialog.is_none() {
            self.mode = AppMode::Normal;
            return Ok(());
        }

        let dialog = self.trash_dialog.as_mut().unwrap();

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.trash_dialog = None;
                self.mode = AppMode::Normal;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                dialog.move_down();
            }
            KeyCode::Char('k') | KeyCode::Up => {
                dialog.move_up();
            }
            // Restore selected file
            KeyCode::Enter | KeyCode::Char('r') => {
                if let Some(entry) = dialog.selected_entry() {
                    let photo_id = entry.id;
                    let trash_path = std::path::PathBuf::from(&entry.path);
                    let original_path = std::path::PathBuf::from(&entry.original_path);

                    match self.trash_manager.restore(&trash_path, &original_path) {
                        Ok(_) => {
                            if let Err(e) = self.db.restore_photo(photo_id) {
                                self.status_message = Some(format!("DB error: {}", e));
                            } else {
                                self.status_message = Some(format!("Restored to {}", original_path.display()));
                                // Refresh dialog
                                let trashed = self.db.get_trashed_photos()?;
                                let total_size = self.db.get_trash_total_size()?;
                                dialog.refresh(trashed, total_size);
                            }
                        }
                        Err(e) => {
                            self.status_message = Some(format!("Restore error: {}", e));
                        }
                    }
                }
            }
            // Permanently delete selected file
            KeyCode::Char('d') => {
                if let Some(entry) = dialog.selected_entry() {
                    let photo_id = entry.id;
                    let trash_path = std::path::PathBuf::from(&entry.path);

                    match self.trash_manager.delete_permanently(&trash_path) {
                        Ok(_) => {
                            if let Err(e) = self.db.delete_trashed_photo(photo_id) {
                                self.status_message = Some(format!("DB error: {}", e));
                            } else {
                                self.status_message = Some("Permanently deleted".to_string());
                                // Refresh dialog
                                let trashed = self.db.get_trashed_photos()?;
                                let total_size = self.db.get_trash_total_size()?;
                                dialog.refresh(trashed, total_size);
                            }
                        }
                        Err(e) => {
                            self.status_message = Some(format!("Delete error: {}", e));
                        }
                    }
                }
            }
            // Cleanup old files
            KeyCode::Char('c') => {
                let max_age = self.trash_manager.max_age_days();
                let old_photos = self.db.get_old_trashed_photos(max_age)?;
                let mut deleted = 0;
                for photo in &old_photos {
                    let trash_path = std::path::PathBuf::from(&photo.path);
                    if self.trash_manager.delete_permanently(&trash_path).is_ok() {
                        if self.db.delete_trashed_photo(photo.id).is_ok() {
                            deleted += 1;
                        }
                    }
                }
                if deleted > 0 {
                    self.status_message = Some(format!("Cleaned up {} old files", deleted));
                    // Refresh dialog
                    let trashed = self.db.get_trashed_photos()?;
                    let total_size = self.db.get_trash_total_size()?;
                    dialog.refresh(trashed, total_size);
                } else {
                    self.status_message = Some("No files older than limit".to_string());
                }
            }
            _ => {}
        }

        Ok(())
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

    // --- Changes dialog methods ---

    fn open_changes_dialog(&mut self) -> Result<()> {
        // Refresh change detection first
        self.check_for_changes();

        if let Some(changes) = self.detected_changes.take() {
            self.changes_dialog = Some(ChangesDialog::new(changes));
            self.mode = AppMode::ChangesViewing;
        } else {
            self.status_message = Some("No file changes detected".to_string());
        }
        Ok(())
    }

    fn handle_changes_dialog_key(&mut self, key: KeyEvent) -> Result<()> {
        if self.changes_dialog.is_none() {
            self.mode = AppMode::Normal;
            return Ok(());
        }

        let dialog = self.changes_dialog.as_mut().unwrap();

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                // Put changes back so indicator stays visible
                let changes = dialog.changes.clone();
                self.detected_changes = Some(changes);
                self.changes_dialog = None;
                self.mode = AppMode::Normal;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                dialog.move_down();
            }
            KeyCode::Char('k') | KeyCode::Up => {
                dialog.move_up();
            }
            KeyCode::Tab => {
                dialog.switch_tab();
            }
            KeyCode::Char(' ') => {
                dialog.toggle_selection();
            }
            KeyCode::Char('a') => {
                dialog.select_all();
            }
            KeyCode::Enter => {
                // Rescan selected/all files
                let files = dialog.files_to_rescan();
                let count = files.len();

                if count > 0 {
                    // Trigger a scan (the scan will pick these up)
                    self.status_message = Some(format!("Rescanning {} files...", count));
                    self.start_scan()?;
                }

                self.changes_dialog = None;
                self.detected_changes = None;
                self.mode = AppMode::Normal;
            }
            _ => {}
        }

        Ok(())
    }

    // --- Schedule dialog methods ---

    fn open_schedule_dialog(&mut self) -> Result<()> {
        // Collect files to schedule: either selected files or current directory for scan
        let files: Vec<PathBuf> = if self.selected_files.is_empty() {
            Vec::new() // Will use current directory
        } else {
            self.selected_files.iter().cloned().collect()
        };

        self.schedule_dialog = Some(ScheduleDialog::new(files, self.current_dir.clone()));
        self.mode = AppMode::Scheduling;
        Ok(())
    }

    fn handle_schedule_dialog_key(&mut self, key: KeyEvent) -> Result<()> {
        if self.schedule_dialog.is_none() {
            self.mode = AppMode::Normal;
            return Ok(());
        }

        let dialog = self.schedule_dialog.as_mut().unwrap();

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.schedule_dialog = None;
                self.mode = AppMode::Normal;
                self.status_message = Some("Schedule cancelled".to_string());
            }
            KeyCode::Tab | KeyCode::Char('j') | KeyCode::Down => {
                dialog.next_field();
            }
            KeyCode::BackTab | KeyCode::Char('k') | KeyCode::Up => {
                dialog.prev_field();
            }
            KeyCode::Char('+') | KeyCode::Char('=') | KeyCode::Right => {
                dialog.increment();
            }
            KeyCode::Char('-') | KeyCode::Left => {
                dialog.decrement();
            }
            KeyCode::Enter => {
                // Create the scheduled task
                let scheduled_at = dialog.scheduled_at();
                let target_path = dialog.target_path();
                let (hours_start, hours_end) = dialog.hours_of_operation()
                    .map_or((None, None), |(s, e)| (Some(s), Some(e)));

                match self.db.create_scheduled_task(
                    dialog.task_type,
                    &target_path,
                    None, // photo_ids
                    &scheduled_at,
                    hours_start,
                    hours_end,
                ) {
                    Ok(_id) => {
                        self.status_message = Some(format!(
                            "Scheduled {} for {}",
                            dialog.task_type.display_name(),
                            &scheduled_at[..16]
                        ));
                    }
                    Err(e) => {
                        self.status_message = Some(format!("Error scheduling: {}", e));
                    }
                }

                self.schedule_dialog = None;
                self.mode = AppMode::Normal;
            }
            KeyCode::Char('n') => {
                // Run now instead of scheduling
                self.status_message = Some(format!("Running {} now...", dialog.task_type.display_name()));

                // Start the appropriate task
                match dialog.task_type {
                    ScheduledTaskType::Scan => {
                        self.start_scan()?;
                    }
                    ScheduledTaskType::LlmBatch => {
                        self.start_batch_llm()?;
                    }
                    ScheduledTaskType::FaceDetection => {
                        self.start_face_scan()?;
                    }
                }

                self.schedule_dialog = None;
                self.mode = AppMode::Normal;
            }
            _ => {}
        }

        Ok(())
    }

    // --- Overdue dialog methods ---

    fn handle_overdue_dialog_key(&mut self, key: KeyEvent) -> Result<()> {
        if self.overdue_dialog.is_none() {
            self.mode = AppMode::Normal;
            return Ok(());
        }

        let dialog = self.overdue_dialog.as_mut().unwrap();

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.overdue_dialog = None;
                self.mode = AppMode::Normal;
            }
            KeyCode::Char('j') | KeyCode::Down => {
                dialog.move_down();
            }
            KeyCode::Char('k') | KeyCode::Up => {
                dialog.move_up();
            }
            KeyCode::Char(' ') => {
                dialog.toggle_selection();
            }
            KeyCode::Char('a') => {
                dialog.select_all();
            }
            KeyCode::Enter => {
                // Run selected tasks
                let task_ids = dialog.tasks_to_run();
                let count = task_ids.len();

                // For now, just cancel the overdue status since we'll run them now
                // The actual execution would need to check task types and run appropriately
                for id in &task_ids {
                    let _ = self.db.update_schedule_status(
                        *id,
                        crate::db::ScheduleStatus::Running,
                        None,
                    );
                }

                self.status_message = Some(format!("Running {} overdue tasks...", count));
                self.start_scan()?; // Simple: just start a scan for now

                self.overdue_dialog = None;
                self.mode = AppMode::Normal;
            }
            KeyCode::Char('c') => {
                // Cancel all overdue tasks
                let task_ids = dialog.all_task_ids();
                for id in &task_ids {
                    let _ = self.db.cancel_schedule(*id);
                }

                self.status_message = Some("Cancelled all overdue tasks".to_string());
                self.overdue_dialog = None;
                self.mode = AppMode::Normal;
            }
            _ => {}
        }

        Ok(())
    }

    // --- Schedule polling (called from main loop) ---

    /// Poll for and execute any due scheduled tasks.
    pub fn poll_schedules(&mut self) -> Result<()> {
        let due_tasks = self.schedule_manager.poll_schedules(&self.db);

        for task in due_tasks {
            // Mark as running
            let _ = crate::schedule::mark_task_running(&task, &self.db);

            // Execute based on task type
            match task.task_type {
                ScheduledTaskType::Scan => {
                    self.status_message = Some(format!("Starting scheduled scan..."));
                    let _ = self.start_scan();
                }
                ScheduledTaskType::LlmBatch => {
                    self.status_message = Some(format!("Starting scheduled LLM batch..."));
                    let _ = self.start_batch_llm();
                }
                ScheduledTaskType::FaceDetection => {
                    self.status_message = Some(format!("Starting scheduled face detection..."));
                    let _ = self.start_face_scan();
                }
            }

            // Mark as completed (the background task will report its own status)
            let _ = crate::schedule::mark_task_completed(task.id, &self.db);
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
