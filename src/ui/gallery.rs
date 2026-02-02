//! Gallery view for displaying photos in a grid layout.

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, Paragraph},
};
use ratatui_image::StatefulImage;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc;
use image::{DynamicImage, imageops::FilterType};
use ratatui_image::{picker::Picker, protocol::StatefulProtocol};

use crate::app::App;
use crate::config::ImageProtocol;

/// Thumbnail size options for gallery view
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThumbnailSize {
    Small,   // ~6 columns
    #[default]
    Medium,  // ~4 columns
    Large,   // ~2 columns
}

impl ThumbnailSize {
    /// Get the approximate cell width in terminal columns
    pub fn cell_width(&self) -> u16 {
        match self {
            ThumbnailSize::Small => 20,
            ThumbnailSize::Medium => 30,
            ThumbnailSize::Large => 50,
        }
    }

    /// Get the approximate cell height in terminal rows
    pub fn cell_height(&self) -> u16 {
        match self {
            ThumbnailSize::Small => 10,
            ThumbnailSize::Medium => 15,
            ThumbnailSize::Large => 25,
        }
    }

    /// Get the pixel size for loading thumbnails
    pub fn pixel_size(&self) -> u32 {
        match self {
            ThumbnailSize::Small => 128,
            ThumbnailSize::Medium => 256,
            ThumbnailSize::Large => 512,
        }
    }

    pub fn cycle_next(&self) -> Self {
        match self {
            ThumbnailSize::Small => ThumbnailSize::Medium,
            ThumbnailSize::Medium => ThumbnailSize::Large,
            ThumbnailSize::Large => ThumbnailSize::Small,
        }
    }

    pub fn cycle_prev(&self) -> Self {
        match self {
            ThumbnailSize::Small => ThumbnailSize::Large,
            ThumbnailSize::Medium => ThumbnailSize::Small,
            ThumbnailSize::Large => ThumbnailSize::Medium,
        }
    }
}

/// Sort options for gallery
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortOption {
    #[default]
    Name,
    Date,
    Size,
}

impl SortOption {
    pub fn cycle(&self) -> Self {
        match self {
            SortOption::Name => SortOption::Date,
            SortOption::Date => SortOption::Size,
            SortOption::Size => SortOption::Name,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            SortOption::Name => "Name",
            SortOption::Date => "Date",
            SortOption::Size => "Size",
        }
    }
}

/// Selection mode for gallery
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectionMode {
    /// Normal mode - single cursor selection
    #[default]
    Normal,
    /// Visual mode - range selection from anchor to cursor
    Visual,
}

/// Gallery view state
pub struct GalleryView {
    /// All image paths in the current directory
    pub images: Vec<PathBuf>,
    /// Currently selected index (cursor position)
    pub selected: usize,
    /// First visible row (for scrolling)
    pub scroll_offset: usize,
    /// Current thumbnail size setting
    pub thumbnail_size: ThumbnailSize,
    /// Current sort option
    pub sort_by: SortOption,
    /// Image picker for protocol detection
    picker: Option<Picker>,
    /// Cache of loaded thumbnail images
    thumbnail_cache: HashMap<PathBuf, StatefulProtocol>,
    /// Set of paths currently being loaded
    loading: HashSet<PathBuf>,
    /// Receiver for async thumbnail loading
    receiver: Option<mpsc::Receiver<(PathBuf, DynamicImage)>>,
    /// Sender for async thumbnail loading
    sender: mpsc::Sender<(PathBuf, DynamicImage)>,
    /// Track last rendered areas to avoid unnecessary re-encoding
    last_render_areas: HashMap<PathBuf, Rect>,
    /// Directory being viewed
    pub directory: PathBuf,
    /// Set of selected indices (for multi-select)
    pub selected_indices: HashSet<usize>,
    /// Selection mode (normal or visual)
    pub selection_mode: SelectionMode,
    /// Visual mode anchor point (start of range selection)
    pub visual_anchor: Option<usize>,
    /// Cached columns count from last render (for navigation)
    cached_columns: usize,
    /// Cached visible rows from last render (for navigation)
    cached_visible_rows: usize,
}

impl GalleryView {
    pub fn new(directory: PathBuf, images: Vec<PathBuf>, protocol: ImageProtocol) -> Self {
        let picker = Self::create_picker(protocol);
        let (tx, rx) = mpsc::channel();
        Self {
            images,
            selected: 0,
            scroll_offset: 0,
            thumbnail_size: ThumbnailSize::default(),
            sort_by: SortOption::default(),
            picker,
            thumbnail_cache: HashMap::new(),
            loading: HashSet::new(),
            receiver: Some(rx),
            sender: tx,
            directory,
            last_render_areas: HashMap::new(),
            selected_indices: HashSet::new(),
            selection_mode: SelectionMode::Normal,
            visual_anchor: None,
            cached_columns: 4,  // Default, updated on render
            cached_visible_rows: 3,  // Default, updated on render
        }
    }

    /// Update cached layout values from render. Called during render to keep navigation in sync.
    pub fn update_layout_cache(&mut self, columns: usize, visible_rows: usize) {
        self.cached_columns = columns;
        self.cached_visible_rows = visible_rows;
    }

    /// Get cached columns (used for navigation)
    pub fn cached_columns(&self) -> usize {
        self.cached_columns
    }

    /// Get cached visible rows (used for navigation)
    pub fn cached_visible_rows(&self) -> usize {
        self.cached_visible_rows
    }

    fn create_picker(protocol: ImageProtocol) -> Option<Picker> {
        match protocol {
            ImageProtocol::None => None,
            _ => Picker::from_query_stdio().ok(),
        }
    }

    /// Poll for completed async thumbnail loads
    pub fn poll_async_loads(&mut self) {
        if let Some(ref receiver) = self.receiver {
            while let Ok((path, dyn_img)) = receiver.try_recv() {
                self.loading.remove(&path);
                if let Some(ref mut picker) = self.picker {
                    let protocol = picker.new_resize_protocol(dyn_img);
                    self.thumbnail_cache.insert(path, protocol);
                }
            }
        }
    }

    /// Check if image preview is available
    #[allow(dead_code)]
    pub fn is_available(&self) -> bool {
        self.picker.is_some()
    }

    /// Get the number of columns based on terminal width
    pub fn columns(&self, area_width: u16) -> usize {
        let cell_width = self.thumbnail_size.cell_width();
        (area_width / cell_width).max(1) as usize
    }

    /// Get the number of visible rows based on terminal height
    pub fn visible_rows(&self, area_height: u16) -> usize {
        let cell_height = self.thumbnail_size.cell_height();
        (area_height / cell_height).max(1) as usize
    }

    /// Get total number of rows
    #[allow(dead_code)]
    pub fn total_rows(&self, columns: usize) -> usize {
        (self.images.len() + columns - 1) / columns
    }

    /// Get currently selected image path
    pub fn selected_image(&self) -> Option<&PathBuf> {
        self.images.get(self.selected)
    }

    /// Move selection left
    pub fn move_left(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    /// Move selection right
    pub fn move_right(&mut self) {
        if self.selected < self.images.len().saturating_sub(1) {
            self.selected += 1;
        }
    }

    /// Move selection up (same column, previous row)
    /// If already on first row, stays in place
    pub fn move_up(&mut self, columns: usize) {
        if self.selected >= columns {
            self.selected -= columns;
        }
        // If already on first row, stay in place (don't wrap or go to start)
    }

    /// Get the row number for the current selection
    #[allow(dead_code)]
    pub fn current_row(&self, columns: usize) -> usize {
        self.selected / columns
    }

    /// Check if selection is on the top visible row
    #[allow(dead_code)]
    pub fn is_on_top_visible_row(&self, columns: usize) -> bool {
        self.current_row(columns) == self.scroll_offset
    }

    /// Check if selection is on the bottom visible row
    #[allow(dead_code)]
    pub fn is_on_bottom_visible_row(&self, columns: usize, visible_rows: usize) -> bool {
        let current_row = self.current_row(columns);
        current_row == self.scroll_offset + visible_rows - 1
    }

    /// Move selection down (same column, next row)
    pub fn move_down(&mut self, columns: usize) {
        let new_idx = self.selected + columns;
        if new_idx < self.images.len() {
            self.selected = new_idx;
        } else if self.images.len() > 0 {
            // If moving down would go past the end, go to the last item
            // in the same column if it exists, otherwise stay in place
            let current_col = self.selected % columns;
            let last_row = (self.images.len() - 1) / columns;
            let current_row = self.selected / columns;

            // Only move if we're not already on the last row
            if current_row < last_row {
                // Try to go to same column in last row
                let target = last_row * columns + current_col;
                if target < self.images.len() {
                    self.selected = target;
                } else {
                    // Column doesn't exist in last row, go to last item
                    self.selected = self.images.len() - 1;
                }
            }
        }
    }

    /// Move to first image
    pub fn move_to_start(&mut self) {
        self.selected = 0;
        self.scroll_offset = 0;
    }

    /// Move to last image
    pub fn move_to_end(&mut self) {
        self.selected = self.images.len().saturating_sub(1);
    }

    /// Page up
    pub fn page_up(&mut self, columns: usize, visible_rows: usize) {
        let page_size = columns * visible_rows;
        if self.selected >= page_size {
            self.selected -= page_size;
        } else {
            self.selected = 0;
        }
    }

    /// Page down
    pub fn page_down(&mut self, columns: usize, visible_rows: usize) {
        let page_size = columns * visible_rows;
        let new_idx = self.selected + page_size;
        if new_idx < self.images.len() {
            self.selected = new_idx;
        } else {
            self.selected = self.images.len().saturating_sub(1);
        }
    }

    /// Ensure selected item is visible
    pub fn ensure_visible(&mut self, columns: usize, visible_rows: usize) {
        let selected_row = self.selected / columns;

        // If selected is above visible area
        if selected_row < self.scroll_offset {
            self.scroll_offset = selected_row;
        }

        // If selected is below visible area
        if selected_row >= self.scroll_offset + visible_rows {
            self.scroll_offset = selected_row - visible_rows + 1;
        }
    }

    /// Load a thumbnail for the given path with rotation applied
    /// rotation_degrees: 0, 90, 180, or 270 degrees clockwise
    pub fn load_thumbnail(&mut self, path: &PathBuf, rotation_degrees: i32) -> Option<&mut StatefulProtocol> {
        // Create cache key that includes rotation
        let cache_key = PathBuf::from(format!("{}#{}", path.display(), rotation_degrees));

        // Check cache first (using rotation-aware key)
        if self.thumbnail_cache.contains_key(&cache_key) {
            return self.thumbnail_cache.get_mut(&cache_key);
        }

        // Start async load if not already loading
        if !self.loading.contains(&cache_key) && self.picker.is_some() {
            self.loading.insert(cache_key.clone());
            let path_clone = path.clone();
            let sender = self.sender.clone();
            let size = self.thumbnail_size.pixel_size();
            let rotation = rotation_degrees;

            std::thread::spawn(move || {
                if let Ok(img) = image::ImageReader::open(&path_clone)
                    .and_then(|r| r.decode().map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e)))
                {
                    let resized = img.resize(size, size, FilterType::Triangle);
                    // Apply rotation
                    let rotated = match rotation {
                        90 => resized.rotate90(),
                        180 => resized.rotate180(),
                        270 => resized.rotate270(),
                        _ => resized,
                    };
                    // Send with rotation-aware cache key
                    let cache_key = PathBuf::from(format!("{}#{}", path_clone.display(), rotation));
                    let _ = sender.send((cache_key, rotated));
                }
            });
        }

        None
    }

    /// Check if a thumbnail is currently loading (rotation-aware)
    pub fn is_loading(&self, path: &PathBuf, rotation_degrees: i32) -> bool {
        let cache_key = PathBuf::from(format!("{}#{}", path.display(), rotation_degrees));
        self.loading.contains(&cache_key)
    }

    /// Clear thumbnail cache (e.g., when changing thumbnail size)
    pub fn clear_cache(&mut self) {
        self.thumbnail_cache.clear();
        self.loading.clear();
        self.last_render_areas.clear();
    }

    /// Change thumbnail size
    pub fn increase_size(&mut self) {
        self.thumbnail_size = self.thumbnail_size.cycle_next();
        self.clear_cache();
    }

    /// Decrease thumbnail size
    pub fn decrease_size(&mut self) {
        self.thumbnail_size = self.thumbnail_size.cycle_prev();
        self.clear_cache();
    }

    /// Cycle sort option
    pub fn cycle_sort(&mut self) {
        self.sort_by = self.sort_by.cycle();
        // Re-sort images
        self.sort_images();
    }

    fn sort_images(&mut self) {
        match self.sort_by {
            SortOption::Name => {
                self.images.sort_by(|a, b| {
                    a.file_name().cmp(&b.file_name())
                });
            }
            SortOption::Date => {
                self.images.sort_by(|a, b| {
                    let a_time = std::fs::metadata(a).and_then(|m| m.modified()).ok();
                    let b_time = std::fs::metadata(b).and_then(|m| m.modified()).ok();
                    b_time.cmp(&a_time) // Newest first
                });
            }
            SortOption::Size => {
                self.images.sort_by(|a, b| {
                    let a_size = std::fs::metadata(a).map(|m| m.len()).unwrap_or(0);
                    let b_size = std::fs::metadata(b).map(|m| m.len()).unwrap_or(0);
                    b_size.cmp(&a_size) // Largest first
                });
            }
        }
    }

    // === Selection Methods ===

    /// Toggle selection of current item (Space key)
    pub fn toggle_select(&mut self) {
        if self.selected_indices.contains(&self.selected) {
            self.selected_indices.remove(&self.selected);
        } else {
            self.selected_indices.insert(self.selected);
        }
    }

    /// Enter visual selection mode (V key)
    pub fn enter_visual_mode(&mut self) {
        self.selection_mode = SelectionMode::Visual;
        self.visual_anchor = Some(self.selected);
        // Clear existing selection and select the anchor
        self.selected_indices.clear();
        self.selected_indices.insert(self.selected);
    }

    /// Exit visual mode (Escape)
    pub fn exit_visual_mode(&mut self) {
        self.selection_mode = SelectionMode::Normal;
        self.visual_anchor = None;
    }

    /// Update visual selection when cursor moves
    fn update_visual_selection(&mut self) {
        if self.selection_mode == SelectionMode::Visual {
            if let Some(anchor) = self.visual_anchor {
                self.selected_indices.clear();
                let start = anchor.min(self.selected);
                let end = anchor.max(self.selected);
                for i in start..=end {
                    self.selected_indices.insert(i);
                }
            }
        }
    }

    /// Clear all selections
    pub fn clear_selection(&mut self) {
        self.selected_indices.clear();
        self.selection_mode = SelectionMode::Normal;
        self.visual_anchor = None;
    }

    /// Select all images
    pub fn select_all(&mut self) {
        for i in 0..self.images.len() {
            self.selected_indices.insert(i);
        }
    }

    /// Get count of selected items
    pub fn selection_count(&self) -> usize {
        self.selected_indices.len()
    }

    /// Check if an index is selected
    pub fn is_selected(&self, idx: usize) -> bool {
        self.selected_indices.contains(&idx)
    }

    /// Get selected image paths
    pub fn get_selected_paths(&self) -> Vec<PathBuf> {
        self.selected_indices
            .iter()
            .filter_map(|&idx| self.images.get(idx).cloned())
            .collect()
    }

    /// Move with visual selection update
    pub fn move_left_with_selection(&mut self) {
        self.move_left();
        self.update_visual_selection();
    }

    pub fn move_right_with_selection(&mut self) {
        self.move_right();
        self.update_visual_selection();
    }

    pub fn move_up_with_selection(&mut self, columns: usize) {
        self.move_up(columns);
        self.update_visual_selection();
    }

    pub fn move_down_with_selection(&mut self, columns: usize) {
        self.move_down(columns);
        self.update_visual_selection();
    }
}

/// Render the gallery view
pub fn render(frame: &mut Frame, app: &mut App, area: Rect) {
    // Borrow db separately to get rotation info
    let db = &app.db;

    let gallery = match app.gallery_view.as_mut() {
        Some(g) => g,
        None => return,
    };

    // Poll for completed thumbnail loads once per frame (not per cell)
    gallery.poll_async_loads();

    // Calculate grid layout
    let columns = gallery.columns(area.width);
    let visible_rows = gallery.visible_rows(area.height.saturating_sub(3)); // -3 for header/footer
    gallery.update_layout_cache(columns, visible_rows);
    gallery.ensure_visible(columns, visible_rows);

    // Main layout: header + grid + footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),  // Header
            Constraint::Min(10),    // Grid
            Constraint::Length(2),  // Footer
        ])
        .split(area);

    // Render header
    render_header(frame, gallery, chunks[0]);

    // Render thumbnail grid with database access for rotation
    render_grid(frame, gallery, db, chunks[1], columns, visible_rows);

    // Render footer with controls
    render_footer(frame, gallery, chunks[2]);
}

fn render_header(frame: &mut Frame, gallery: &GalleryView, area: Rect) {
    let dir_name = gallery.directory.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| gallery.directory.to_string_lossy().to_string());

    let header = format!(
        " Gallery: {} | {} images | Sort: {} | Size: {:?}",
        dir_name,
        gallery.images.len(),
        gallery.sort_by.label(),
        gallery.thumbnail_size
    );

    let paragraph = Paragraph::new(header)
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
    frame.render_widget(paragraph, area);
}

fn render_grid(frame: &mut Frame, gallery: &mut GalleryView, db: &crate::db::Database, area: Rect, columns: usize, visible_rows: usize) {
    let cell_width = gallery.thumbnail_size.cell_width();
    let cell_height = gallery.thumbnail_size.cell_height();

    // Create grid constraints
    let col_constraints: Vec<Constraint> = (0..columns)
        .map(|_| Constraint::Length(cell_width))
        .collect();

    let row_constraints: Vec<Constraint> = (0..visible_rows)
        .map(|_| Constraint::Length(cell_height))
        .collect();

    // Create row layout
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints(row_constraints)
        .split(area);

    for (row_idx, row_area) in rows.iter().enumerate() {
        let actual_row = gallery.scroll_offset + row_idx;

        // Create column layout for this row
        let cols = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(col_constraints.clone())
            .split(*row_area);

        for (col_idx, cell_area) in cols.iter().enumerate() {
            let image_idx = actual_row * columns + col_idx;

            if image_idx < gallery.images.len() {
                let is_cursor = image_idx == gallery.selected;
                let is_selected = gallery.is_selected(image_idx);
                let path = gallery.images[image_idx].clone();
                // Get rotation from database (combines EXIF + user rotation)
                let rotation = db.get_photo_rotation(&path).unwrap_or(0);
                render_thumbnail_cell(frame, gallery, &path, *cell_area, is_cursor, is_selected, rotation);
            }
        }
    }
}

fn render_thumbnail_cell(
    frame: &mut Frame,
    gallery: &mut GalleryView,
    path: &PathBuf,
    area: Rect,
    is_cursor: bool,
    is_selected: bool,
    rotation_degrees: i32,
) {
    // Create block with selection highlighting
    // Cursor = current position (cyan), Selected = in selection set (green)
    let (border_color, border_type) = match (is_cursor, is_selected) {
        (true, true) => (Color::Yellow, Borders::ALL),    // Cursor + Selected
        (true, false) => (Color::Cyan, Borders::ALL),     // Cursor only
        (false, true) => (Color::Green, Borders::ALL),    // Selected only
        (false, false) => (Color::DarkGray, Borders::ALL), // Neither
    };

    let filename = path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    // Truncate filename to fit cell width
    let max_name_len = (area.width as usize).saturating_sub(4);
    let display_name = if filename.len() > max_name_len {
        format!("{}...", &filename[..max_name_len.saturating_sub(3)])
    } else {
        filename
    };

    let block = Block::default()
        .borders(border_type)
        .border_style(Style::default().fg(border_color))
        .title(display_name);

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Skip if area too small
    if inner.width < 2 || inner.height < 2 {
        return;
    }

    // Try to render the thumbnail with rotation
    if let Some(protocol) = gallery.load_thumbnail(path, rotation_degrees) {
        // Use StatefulImage without explicit resize - protocol handles it
        // This avoids potential re-encoding on every frame
        let image = StatefulImage::new(None);
        frame.render_stateful_widget(image, inner, protocol);
    } else if gallery.is_loading(path, rotation_degrees) {
        // Show loading indicator
        let loading = Paragraph::new("Loading...")
            .style(Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC))
            .alignment(Alignment::Center);

        // Center vertically
        if inner.height > 1 {
            let y_offset = inner.height / 2;
            let centered = Rect::new(inner.x, inner.y + y_offset, inner.width, 1);
            frame.render_widget(loading, centered);
        }
    } else {
        // Show placeholder
        let placeholder = Paragraph::new("[ ]")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);

        if inner.height > 1 {
            let y_offset = inner.height / 2;
            let centered = Rect::new(inner.x, inner.y + y_offset, inner.width, 1);
            frame.render_widget(placeholder, centered);
        }
    }
}

fn render_footer(frame: &mut Frame, gallery: &GalleryView, area: Rect) {
    let selection_count = gallery.selection_count();
    let mode_indicator = match gallery.selection_mode {
        SelectionMode::Normal => "",
        SelectionMode::Visual => " [VISUAL]",
    };

    let selected_info = if let Some(path) = gallery.selected_image() {
        let filename = path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let size = std::fs::metadata(path)
            .map(|m| format_size(m.len()))
            .unwrap_or_default();
        let selection_str = if selection_count > 0 {
            format!(" | {} selected", selection_count)
        } else {
            String::new()
        };
        format!("{} ({}) | {}/{}{}{}", filename, size, gallery.selected + 1, gallery.images.len(), selection_str, mode_indicator)
    } else {
        "No selection".to_string()
    };

    let help = if gallery.selection_mode == SelectionMode::Visual {
        "Arrows:select range | Esc:exit visual | Space:toggle | d:trash | y:cut | ]:rotate"
    } else {
        "Space:select | v:visual | S:view | +/-:size | s:sort | d:trash | y:cut | p:paste | ?:help"
    };

    let footer_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);

    let info = Paragraph::new(selected_info)
        .style(Style::default().fg(Color::Yellow));
    frame.render_widget(info, footer_chunks[0]);

    let help_text = Paragraph::new(help)
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(help_text, footer_chunks[1]);
}

fn format_size(size: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if size >= GB {
        format!("{:.1}G", size as f64 / GB as f64)
    } else if size >= MB {
        format!("{:.1}M", size as f64 / MB as f64)
    } else if size >= KB {
        format!("{:.1}K", size as f64 / KB as f64)
    } else {
        format!("{}B", size)
    }
}

/// Render gallery help dialog
pub fn render_help(frame: &mut Frame, area: Rect) {
    let dialog_width = 60.min(area.width.saturating_sub(4));
    let dialog_height = 28.min(area.height.saturating_sub(4));

    let x = (area.width - dialog_width) / 2;
    let y = (area.height - dialog_height) / 2;

    let dialog_area = Rect::new(x, y, dialog_width, dialog_height);

    frame.render_widget(Clear, dialog_area);

    let help_text = vec![
        Line::from(Span::styled("Gallery View", Style::default().add_modifier(Modifier::BOLD).fg(Color::Cyan))),
        Line::from(""),
        Line::from(Span::styled("Navigation", Style::default().add_modifier(Modifier::BOLD))),
        Line::from("  h/Left           Move left"),
        Line::from("  l/Right          Move right"),
        Line::from("  k/Up             Move up"),
        Line::from("  j/Down           Move down"),
        Line::from("  g                Go to first"),
        Line::from("  G                Go to last"),
        Line::from("  PgUp/Ctrl+B      Page up"),
        Line::from("  PgDn/Ctrl+F      Page down"),
        Line::from(""),
        Line::from(Span::styled("Selection", Style::default().add_modifier(Modifier::BOLD))),
        Line::from("  Space            Toggle select"),
        Line::from("  v / V            Visual select mode"),
        Line::from("  Ctrl+A           Select all"),
        Line::from("  Esc              Clear selection / Exit visual"),
        Line::from(""),
        Line::from(Span::styled("Actions", Style::default().add_modifier(Modifier::BOLD))),
        Line::from("  ] / [            Rotate CW / CCW"),
        Line::from("  d / Delete       Move to trash"),
        Line::from("  y / x            Cut to clipboard"),
        Line::from("  p                Paste from clipboard"),
        Line::from("  S                View image (slideshow)"),
        Line::from("  Enter            Open in external viewer"),
        Line::from("  +/-              Thumbnail size"),
        Line::from("  s                Cycle sort"),
        Line::from("  q                Exit gallery"),
        Line::from("  ?                Toggle this help"),
    ];

    let paragraph = Paragraph::new(help_text).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" Gallery Help "),
    );

    frame.render_widget(paragraph, dialog_area);
}
