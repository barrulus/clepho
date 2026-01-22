//! Slideshow mode with presenter view controls.

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, Paragraph},
};
use ratatui_image::{Resize, StatefulImage};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::{Duration, Instant};
use image::{DynamicImage, imageops::FilterType};
use ratatui_image::{picker::Picker, protocol::StatefulProtocol};

use crate::app::App;
use crate::config::ImageProtocol;
use crate::db::Database;

/// Slideshow display mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SlideshowDisplayMode {
    /// Show only the current image fullscreen
    #[default]
    Fullscreen,
    /// Show presenter view with prev/current/next
    Presenter,
}

/// Slideshow state
pub struct SlideshowView {
    /// All image paths in the slideshow
    pub images: Vec<PathBuf>,
    /// Currently displayed image index
    pub current: usize,
    /// Whether slideshow is playing (auto-advance)
    pub playing: bool,
    /// Auto-advance interval in seconds
    pub interval: u64,
    /// Last advance timestamp
    pub last_advance: Instant,
    /// Display mode
    pub display_mode: SlideshowDisplayMode,
    /// Image picker for protocol detection
    picker: Option<Picker>,
    /// Cache of loaded images (keyed by "path#rotation")
    image_cache: HashMap<String, StatefulProtocol>,
    /// Images currently being loaded (keyed by "path#rotation")
    loading: std::collections::HashSet<String>,
    /// Receiver for async image loading
    receiver: Option<mpsc::Receiver<(String, DynamicImage)>>,
    /// Sender for async image loading
    sender: mpsc::Sender<(String, DynamicImage)>,
    /// Source directory
    pub directory: PathBuf,
}

impl SlideshowView {
    pub fn new(directory: PathBuf, images: Vec<PathBuf>, protocol: ImageProtocol) -> Self {
        let picker = Self::create_picker(protocol);
        let (tx, rx) = mpsc::channel();
        Self {
            images,
            current: 0,
            playing: false,
            interval: 5,
            last_advance: Instant::now(),
            display_mode: SlideshowDisplayMode::default(),
            picker,
            image_cache: HashMap::new(),
            loading: std::collections::HashSet::new(),
            receiver: Some(rx),
            sender: tx,
            directory,
        }
    }

    fn create_picker(protocol: ImageProtocol) -> Option<Picker> {
        match protocol {
            ImageProtocol::None => None,
            _ => Picker::from_query_stdio().ok(),
        }
    }

    /// Poll for completed async image loads
    pub fn poll_async_loads(&mut self) {
        if let Some(ref receiver) = self.receiver {
            while let Ok((cache_key, dyn_img)) = receiver.try_recv() {
                self.loading.remove(&cache_key);
                if let Some(ref mut picker) = self.picker {
                    let protocol = picker.new_resize_protocol(dyn_img);
                    self.image_cache.insert(cache_key, protocol);
                }
            }
        }
    }

    /// Check if image preview is available
    pub fn is_available(&self) -> bool {
        self.picker.is_some()
    }

    /// Current image path
    pub fn current_image(&self) -> Option<&PathBuf> {
        self.images.get(self.current)
    }

    /// Previous image path
    pub fn prev_image(&self) -> Option<&PathBuf> {
        if self.current > 0 {
            self.images.get(self.current - 1)
        } else {
            None
        }
    }

    /// Next image path
    pub fn next_image(&self) -> Option<&PathBuf> {
        self.images.get(self.current + 1)
    }

    /// Go to next image
    pub fn next(&mut self) {
        if self.current < self.images.len().saturating_sub(1) {
            self.current += 1;
            self.last_advance = Instant::now();
        }
    }

    /// Go to previous image
    pub fn prev(&mut self) {
        if self.current > 0 {
            self.current -= 1;
            self.last_advance = Instant::now();
        }
    }

    /// Go to first image
    pub fn first(&mut self) {
        self.current = 0;
        self.last_advance = Instant::now();
    }

    /// Go to last image
    pub fn last(&mut self) {
        self.current = self.images.len().saturating_sub(1);
        self.last_advance = Instant::now();
    }

    /// Toggle play/pause
    pub fn toggle_play(&mut self) {
        self.playing = !self.playing;
        self.last_advance = Instant::now();
    }

    /// Toggle display mode
    pub fn toggle_display_mode(&mut self) {
        self.display_mode = match self.display_mode {
            SlideshowDisplayMode::Fullscreen => SlideshowDisplayMode::Presenter,
            SlideshowDisplayMode::Presenter => SlideshowDisplayMode::Fullscreen,
        };
    }

    /// Increase interval
    pub fn increase_interval(&mut self) {
        self.interval = (self.interval + 1).min(30);
    }

    /// Decrease interval
    pub fn decrease_interval(&mut self) {
        self.interval = self.interval.saturating_sub(1).max(1);
    }

    /// Check if should auto-advance
    pub fn should_advance(&self) -> bool {
        self.playing && self.last_advance.elapsed() >= Duration::from_secs(self.interval)
    }

    /// Perform auto-advance if needed
    pub fn auto_advance(&mut self) {
        if self.should_advance() {
            if self.current < self.images.len().saturating_sub(1) {
                self.current += 1;
            } else {
                // Stop at end
                self.playing = false;
            }
            self.last_advance = Instant::now();
        }
    }

    /// Create a cache key that includes path and rotation
    fn cache_key(path: &PathBuf, rotation: i32) -> String {
        format!("{}#{}", path.display(), rotation)
    }

    /// Load an image for display
    /// rotation_degrees: 0, 90, 180, or 270 degrees clockwise
    pub fn load_image(&mut self, path: &PathBuf, max_size: u32, rotation_degrees: i32) -> Option<&mut StatefulProtocol> {
        self.poll_async_loads();

        let cache_key = Self::cache_key(path, rotation_degrees);

        // Check cache first
        if self.image_cache.contains_key(&cache_key) {
            return self.image_cache.get_mut(&cache_key);
        }

        // Start async load if not already loading
        if !self.loading.contains(&cache_key) && self.picker.is_some() {
            self.loading.insert(cache_key.clone());
            let path_clone = path.clone();
            let sender = self.sender.clone();
            let rotation = rotation_degrees;

            std::thread::spawn(move || {
                if let Ok(img) = image::ImageReader::open(&path_clone)
                    .and_then(|r| r.decode().map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e)))
                {
                    let resized = img.resize(max_size, max_size, FilterType::Lanczos3);
                    // Apply rotation
                    let rotated = match rotation {
                        90 => resized.rotate90(),
                        180 => resized.rotate180(),
                        270 => resized.rotate270(),
                        _ => resized,
                    };
                    let cache_key = format!("{}#{}", path_clone.display(), rotation);
                    let _ = sender.send((cache_key, rotated));
                }
            });
        }

        None
    }

    /// Check if an image is currently loading
    pub fn is_loading(&self, path: &PathBuf) -> bool {
        // Check if any rotation variant is loading
        self.loading.iter().any(|k| k.starts_with(&format!("{}#", path.display())))
    }
}

/// Render the slideshow view
pub fn render(frame: &mut Frame, app: &mut App, area: Rect) {
    // Borrow db separately to avoid borrow conflicts with slideshow_view
    let db = &app.db;
    let slideshow = match app.slideshow_view.as_mut() {
        Some(s) => s,
        None => return,
    };

    // Auto-advance if playing
    slideshow.auto_advance();

    // Clear background
    frame.render_widget(Clear, area);

    match slideshow.display_mode {
        SlideshowDisplayMode::Fullscreen => render_fullscreen(frame, slideshow, db, area),
        SlideshowDisplayMode::Presenter => render_presenter(frame, slideshow, db, area),
    }
}

fn render_fullscreen(frame: &mut Frame, slideshow: &mut SlideshowView, db: &Database, area: Rect) {
    // Main layout: image + status bar
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(10), Constraint::Length(2)])
        .split(area);

    // Render current image
    if let Some(path) = slideshow.current_image().cloned() {
        let block = Block::default()
            .borders(Borders::NONE)
            .style(Style::default().bg(Color::Black));
        frame.render_widget(block, chunks[0]);

        // Get rotation from database (combines EXIF + user rotation)
        let rotation = db.get_photo_rotation(&path).unwrap_or(0);
        if let Some(protocol) = slideshow.load_image(&path, 2048, rotation) {
            let image = StatefulImage::new(None).resize(Resize::Fit(None));
            frame.render_stateful_widget(image, chunks[0], protocol);
        } else if slideshow.is_loading(&path) {
            let loading = Paragraph::new("Loading...")
                .style(Style::default().fg(Color::DarkGray))
                .alignment(Alignment::Center);
            frame.render_widget(loading, centered_rect(chunks[0], 20, 1));
        }
    }

    // Status bar
    render_status_bar(frame, slideshow, chunks[1]);
}

fn render_presenter(frame: &mut Frame, slideshow: &mut SlideshowView, db: &Database, area: Rect) {
    // Layout: preview strip at top + main image + status bar
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(12), // Preview strip
            Constraint::Min(10),    // Main image
            Constraint::Length(2),  // Status bar
        ])
        .split(area);

    // Render preview strip (prev | current | next)
    render_preview_strip(frame, slideshow, db, chunks[0]);

    // Render current image
    if let Some(path) = slideshow.current_image().cloned() {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" Current (Audience View) ");
        let inner = block.inner(chunks[1]);
        frame.render_widget(block, chunks[1]);

        // Get rotation from database (combines EXIF + user rotation)
        let rotation = db.get_photo_rotation(&path).unwrap_or(0);
        if let Some(protocol) = slideshow.load_image(&path, 1024, rotation) {
            let image = StatefulImage::new(None).resize(Resize::Fit(None));
            frame.render_stateful_widget(image, inner, protocol);
        } else if slideshow.is_loading(&path) {
            let loading = Paragraph::new("Loading...")
                .style(Style::default().fg(Color::DarkGray))
                .alignment(Alignment::Center);
            frame.render_widget(loading, centered_rect(inner, 20, 1));
        }
    }

    // Status bar
    render_status_bar(frame, slideshow, chunks[2]);
}

fn render_preview_strip(frame: &mut Frame, slideshow: &mut SlideshowView, db: &Database, area: Rect) {
    // Three-column layout for prev/current/next
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(33),
            Constraint::Percentage(34),
            Constraint::Percentage(33),
        ])
        .split(area);

    // Previous
    let prev_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Previous ");
    let prev_inner = prev_block.inner(cols[0]);
    frame.render_widget(prev_block, cols[0]);

    if let Some(path) = slideshow.prev_image().cloned() {
        let rotation = db.get_photo_rotation(&path).unwrap_or(0);
        if let Some(protocol) = slideshow.load_image(&path, 256, rotation) {
            let image = StatefulImage::new(None).resize(Resize::Fit(None));
            frame.render_stateful_widget(image, prev_inner, protocol);
        }
    }

    // Current (highlighted)
    let curr_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green))
        .title(" Current ");
    let curr_inner = curr_block.inner(cols[1]);
    frame.render_widget(curr_block, cols[1]);

    if let Some(path) = slideshow.current_image().cloned() {
        let rotation = db.get_photo_rotation(&path).unwrap_or(0);
        if let Some(protocol) = slideshow.load_image(&path, 256, rotation) {
            let image = StatefulImage::new(None).resize(Resize::Fit(None));
            frame.render_stateful_widget(image, curr_inner, protocol);
        }
    }

    // Next (preview)
    let next_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(" Next (Preview) ");
    let next_inner = next_block.inner(cols[2]);
    frame.render_widget(next_block, cols[2]);

    if let Some(path) = slideshow.next_image().cloned() {
        let rotation = db.get_photo_rotation(&path).unwrap_or(0);
        if let Some(protocol) = slideshow.load_image(&path, 256, rotation) {
            let image = StatefulImage::new(None).resize(Resize::Fit(None));
            frame.render_stateful_widget(image, next_inner, protocol);
        }
    }
}

fn render_status_bar(frame: &mut Frame, slideshow: &SlideshowView, area: Rect) {
    let play_status = if slideshow.playing { "▶ Playing" } else { "⏸ Paused" };
    let progress = format!("{}/{}", slideshow.current + 1, slideshow.images.len());
    let interval = format!("{}s", slideshow.interval);
    let mode = match slideshow.display_mode {
        SlideshowDisplayMode::Fullscreen => "Fullscreen",
        SlideshowDisplayMode::Presenter => "Presenter",
    };

    let filename = slideshow.current_image()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let status_line = format!(
        " {} | {} | Interval: {} | Mode: {} | {} ",
        play_status, progress, interval, mode, filename
    );

    let help = "Space:play/pause | h/l:prev/next | v:mode | +/-:speed | q:quit";

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(area);

    let status = Paragraph::new(status_line)
        .style(Style::default().fg(Color::Cyan));
    frame.render_widget(status, chunks[0]);

    let help_text = Paragraph::new(help)
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(help_text, chunks[1]);
}

fn centered_rect(area: Rect, width: u16, height: u16) -> Rect {
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width.min(area.width), height.min(area.height))
}

/// Render slideshow help dialog
pub fn render_help(frame: &mut Frame, area: Rect) {
    let dialog_width = 50.min(area.width.saturating_sub(4));
    let dialog_height = 16.min(area.height.saturating_sub(4));

    let x = (area.width - dialog_width) / 2;
    let y = (area.height - dialog_height) / 2;

    let dialog_area = Rect::new(x, y, dialog_width, dialog_height);

    frame.render_widget(Clear, dialog_area);

    let help_text = vec![
        Line::from(Span::styled("Slideshow Controls", Style::default().add_modifier(Modifier::BOLD).fg(Color::Cyan))),
        Line::from(""),
        Line::from("  Space          Play/Pause"),
        Line::from("  h/Left         Previous image"),
        Line::from("  l/Right        Next image"),
        Line::from("  g              First image"),
        Line::from("  G              Last image"),
        Line::from("  v              Toggle view mode"),
        Line::from("  +/=            Slower (more seconds)"),
        Line::from("  -              Faster (fewer seconds)"),
        Line::from("  Esc/q          Exit slideshow"),
        Line::from("  ?              Toggle this help"),
    ];

    let paragraph = Paragraph::new(help_text).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" Slideshow Help "),
    );

    frame.render_widget(paragraph, dialog_area);
}
