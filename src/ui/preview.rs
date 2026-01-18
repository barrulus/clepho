use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};
use image::{DynamicImage, imageops::FilterType};
use ratatui_image::{picker::Picker, protocol::StatefulProtocol, Resize, StatefulImage};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::mpsc;

use crate::app::App;
use crate::config::ImageProtocol;
use crate::db::BoundingBox;

/// Cached metadata for an image
#[derive(Clone)]
pub struct CachedMetadata {
    pub dimensions: Option<(u32, u32)>,
    pub camera_make: Option<String>,
    pub camera_model: Option<String>,
    pub taken_at: Option<String>,
    pub exposure: Option<String>,
}

/// Manages image preview state and caching
pub struct ImagePreviewState {
    picker: Option<Picker>,
    /// Cache of loaded images keyed by path (ready to display)
    image_cache: HashMap<PathBuf, StatefulProtocol>,
    /// Cache of image metadata keyed by path
    metadata_cache: HashMap<PathBuf, CachedMetadata>,
    /// Paths currently being loaded in background (images)
    loading_images: HashSet<PathBuf>,
    /// Paths currently being loaded in background (metadata)
    loading_metadata: HashSet<PathBuf>,
    /// Receiver for async image loading (resized DynamicImage)
    image_receiver: Option<mpsc::Receiver<(PathBuf, DynamicImage)>>,
    /// Sender for async image loading
    image_sender: mpsc::Sender<(PathBuf, DynamicImage)>,
    /// Receiver for async metadata loading
    metadata_receiver: Option<mpsc::Receiver<(PathBuf, CachedMetadata)>>,
    /// Sender for async metadata loading (cloned for each load task)
    metadata_sender: mpsc::Sender<(PathBuf, CachedMetadata)>,
    /// Current image being displayed
    current_path: Option<PathBuf>,
    /// Scroll offset for preview text (metadata + description)
    pub scroll_offset: u16,
    /// Thumbnail size for image loading
    thumbnail_size: u32,
    /// Cache of face crops keyed by "path#face_id"
    face_cache: HashMap<PathBuf, StatefulProtocol>,
    /// Face crops currently being loaded
    loading_faces: HashSet<PathBuf>,
    /// Receiver for async face crop loading
    face_receiver: Option<mpsc::Receiver<(PathBuf, DynamicImage)>>,
    /// Sender for async face crop loading
    face_sender: mpsc::Sender<(PathBuf, DynamicImage)>,
}

impl ImagePreviewState {
    pub fn new(protocol: ImageProtocol) -> Self {
        let picker = Self::create_picker(protocol);
        let (meta_tx, meta_rx) = mpsc::channel();
        let (img_tx, img_rx) = mpsc::channel();
        let (face_tx, face_rx) = mpsc::channel();
        Self {
            picker,
            image_cache: HashMap::new(),
            metadata_cache: HashMap::new(),
            loading_images: HashSet::new(),
            loading_metadata: HashSet::new(),
            image_receiver: Some(img_rx),
            image_sender: img_tx,
            metadata_receiver: Some(meta_rx),
            metadata_sender: meta_tx,
            current_path: None,
            scroll_offset: 0,
            thumbnail_size: 1024,
            face_cache: HashMap::new(),
            loading_faces: HashSet::new(),
            face_receiver: Some(face_rx),
            face_sender: face_tx,
        }
    }

    /// Scroll the preview down
    pub fn scroll_down(&mut self, lines: u16) {
        self.scroll_offset = self.scroll_offset.saturating_add(lines);
    }

    /// Scroll the preview up
    pub fn scroll_up(&mut self, lines: u16) {
        self.scroll_offset = self.scroll_offset.saturating_sub(lines);
    }

    /// Reset scroll when selection changes
    pub fn reset_scroll(&mut self) {
        self.scroll_offset = 0;
    }

    /// Check for completed async loads (images, metadata, and face crops)
    pub fn poll_async_loads(&mut self) {
        // Poll for completed images
        if let Some(ref receiver) = self.image_receiver {
            while let Ok((path, dyn_img)) = receiver.try_recv() {
                self.loading_images.remove(&path);
                // Convert to protocol on main thread (fast)
                if let Some(ref mut picker) = self.picker {
                    let protocol = picker.new_resize_protocol(dyn_img);
                    self.image_cache.insert(path, protocol);
                }
            }
        }

        // Poll for completed metadata
        if let Some(ref receiver) = self.metadata_receiver {
            while let Ok((path, metadata)) = receiver.try_recv() {
                self.loading_metadata.remove(&path);
                self.metadata_cache.insert(path, metadata);
            }
        }

        // Poll for completed face crops
        if let Some(ref receiver) = self.face_receiver {
            while let Ok((cache_key, dyn_img)) = receiver.try_recv() {
                self.loading_faces.remove(&cache_key);
                // Convert to protocol on main thread (fast)
                if let Some(ref mut picker) = self.picker {
                    let protocol = picker.new_resize_protocol(dyn_img);
                    self.face_cache.insert(cache_key, protocol);
                }
            }
        }
    }

    /// Get cached metadata for a path, starting async load if not cached
    pub fn get_metadata(&mut self, path: &PathBuf) -> Option<CachedMetadata> {
        // Check for completed async loads first
        self.poll_async_loads();

        // Return cached if available
        if let Some(metadata) = self.metadata_cache.get(path) {
            return Some(metadata.clone());
        }

        // Start async load if not already loading
        if !self.loading_metadata.contains(path) {
            self.loading_metadata.insert(path.clone());
            let path_clone = path.clone();
            let sender = self.metadata_sender.clone();

            std::thread::spawn(move || {
                let metadata = Self::load_metadata(&path_clone);
                let _ = sender.send((path_clone, metadata));
            });
        }

        // Return None while loading (UI will show basic info)
        None
    }

    /// Load metadata from an image file (expensive operation, cache the result!)
    fn load_metadata(path: &PathBuf) -> CachedMetadata {
        let mut metadata = CachedMetadata {
            dimensions: None,
            camera_make: None,
            camera_model: None,
            taken_at: None,
            exposure: None,
        };

        // Get dimensions
        if let Ok(reader) = image::ImageReader::open(path) {
            if let Ok(dims) = reader.into_dimensions() {
                metadata.dimensions = Some(dims);
            }
        }

        // Get EXIF data
        if let Ok(file) = std::fs::File::open(path) {
            let mut bufreader = std::io::BufReader::new(&file);
            if let Ok(exif) = exif::Reader::new().read_from_container(&mut bufreader) {
                if let Some(field) = exif.get_field(exif::Tag::Make, exif::In::PRIMARY) {
                    metadata.camera_make = Some(field.display_value().to_string());
                }
                if let Some(field) = exif.get_field(exif::Tag::Model, exif::In::PRIMARY) {
                    metadata.camera_model = Some(field.display_value().to_string());
                }
                if let Some(field) = exif.get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY) {
                    metadata.taken_at = Some(field.display_value().to_string());
                }

                // Exposure settings (compact format)
                let mut exposure_parts = Vec::new();
                if let Some(field) = exif.get_field(exif::Tag::FNumber, exif::In::PRIMARY) {
                    exposure_parts.push(format!("f/{}", field.display_value()));
                }
                if let Some(field) = exif.get_field(exif::Tag::ExposureTime, exif::In::PRIMARY) {
                    exposure_parts.push(format!("{}s", field.display_value()));
                }
                if let Some(field) = exif.get_field(exif::Tag::PhotographicSensitivity, exif::In::PRIMARY) {
                    exposure_parts.push(format!("ISO {}", field.display_value()));
                }
                if let Some(field) = exif.get_field(exif::Tag::FocalLength, exif::In::PRIMARY) {
                    exposure_parts.push(format!("{}mm", field.display_value()));
                }
                if !exposure_parts.is_empty() {
                    metadata.exposure = Some(exposure_parts.join(" | "));
                }
            }
        }

        metadata
    }

    fn create_picker(protocol: ImageProtocol) -> Option<Picker> {
        match protocol {
            ImageProtocol::None => None,
            // For all other cases, try to auto-detect terminal capabilities
            // ratatui-image v3.0 handles protocol selection automatically
            _ => Picker::from_query_stdio().ok(),
        }
    }

    /// Load an image for the given path asynchronously, returns cached if available
    pub fn load_image(&mut self, path: &PathBuf, thumbnail_size: u32) -> Option<&mut StatefulProtocol> {
        // Poll for any completed loads first
        self.poll_async_loads();

        // Update current path and thumbnail size
        self.current_path = Some(path.clone());
        self.thumbnail_size = thumbnail_size;

        // Check cache first - return immediately if available
        if self.image_cache.contains_key(path) {
            return self.image_cache.get_mut(path);
        }

        // Start async load if not already loading
        if !self.loading_images.contains(path) && self.picker.is_some() {
            self.loading_images.insert(path.clone());
            let path_clone = path.clone();
            let sender = self.image_sender.clone();
            let size = thumbnail_size;

            std::thread::spawn(move || {
                if let Ok(dyn_img) = image::ImageReader::open(&path_clone)
                    .and_then(|r| r.decode().map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e)))
                {
                    // Resize in background thread using high-quality Lanczos3 filter
                    let resized = dyn_img.resize(size, size, FilterType::Lanczos3);
                    let _ = sender.send((path_clone, resized));
                }
            });
        }

        // Return None while loading
        None
    }

    /// Check if an image is currently loading
    pub fn is_loading_image(&self, path: &PathBuf) -> bool {
        self.loading_images.contains(path)
    }

    /// Load a face crop for the given path and bounding box
    pub fn load_face_crop(
        &mut self,
        path: &PathBuf,
        bbox: &BoundingBox,
        face_id: i64,
        thumbnail_size: u32,
    ) -> Option<&mut StatefulProtocol> {
        // Poll for any completed loads first
        self.poll_async_loads();

        // Create unique cache key for this face
        let cache_key = PathBuf::from(format!("{}#face_{}", path.display(), face_id));

        // Check cache first
        if self.face_cache.contains_key(&cache_key) {
            return self.face_cache.get_mut(&cache_key);
        }

        // Start async load if not already loading
        if !self.loading_faces.contains(&cache_key) && self.picker.is_some() {
            self.loading_faces.insert(cache_key.clone());
            let path_clone = path.clone();
            let sender = self.face_sender.clone();
            let bbox_x = bbox.x;
            let bbox_y = bbox.y;
            let bbox_w = bbox.width;
            let bbox_h = bbox.height;

            std::thread::spawn(move || {
                if let Ok(dyn_img) = image::ImageReader::open(&path_clone)
                    .and_then(|r| r.decode().map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e)))
                {
                    // Calculate crop region with padding (20% extra on each side)
                    let img_width = dyn_img.width() as i32;
                    let img_height = dyn_img.height() as i32;

                    let padding_x = (bbox_w as f32 * 0.3) as i32;
                    let padding_y = (bbox_h as f32 * 0.3) as i32;

                    let crop_x = (bbox_x - padding_x).max(0) as u32;
                    let crop_y = (bbox_y - padding_y).max(0) as u32;
                    let crop_w = ((bbox_w + padding_x * 2) as i32)
                        .min(img_width - crop_x as i32)
                        .max(1) as u32;
                    let crop_h = ((bbox_h + padding_y * 2) as i32)
                        .min(img_height - crop_y as i32)
                        .max(1) as u32;

                    // Crop to face region
                    let cropped = dyn_img.crop_imm(crop_x, crop_y, crop_w, crop_h);

                    // Only downscale if crop is larger than target, never upscale
                    // (upscaling small face crops makes them blurry)
                    let final_image = if cropped.width() > thumbnail_size || cropped.height() > thumbnail_size {
                        cropped.resize(thumbnail_size, thumbnail_size, FilterType::Lanczos3)
                    } else {
                        cropped
                    };
                    let _ = sender.send((cache_key, final_image));
                }
            });
        }

        // Return None while loading
        None
    }

    /// Check if a face crop is currently loading
    pub fn is_loading_face(&self, cache_key: &PathBuf) -> bool {
        self.loading_faces.contains(cache_key)
    }

    /// Check if image preview is available
    pub fn is_available(&self) -> bool {
        self.picker.is_some()
    }
}

impl Default for ImagePreviewState {
    fn default() -> Self {
        Self::new(ImageProtocol::Auto)
    }
}

pub fn render(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title("Preview");

    // Clone entry to avoid borrow conflicts
    let selected = app.selected_entry().cloned();

    match selected {
        Some(ref entry) if entry.is_dir => {
            render_directory_preview(frame, &entry.path, block, area);
        }
        Some(ref entry) if is_image(&entry.name) => {
            let description = app.get_llm_description();
            render_image_preview(frame, app, entry, description.as_deref(), block, area);
        }
        Some(ref entry) => {
            render_file_preview(frame, entry, block, area);
        }
        None => {
            let paragraph = Paragraph::new("No selection")
                .block(block)
                .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(paragraph, area);
        }
    }
}

fn render_directory_preview(frame: &mut Frame, path: &std::path::Path, block: Block, area: Rect) {
    let entries: Vec<ListItem> = match fs::read_dir(path) {
        Ok(dir) => dir
            .filter_map(|e| e.ok())
            .take(50)
            .map(|entry| {
                let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                let name = entry.file_name().to_string_lossy().to_string();
                let icon = if is_dir { "/ " } else { "  " };
                let style = if is_dir {
                    Style::default().fg(Color::Cyan)
                } else {
                    Style::default()
                };
                ListItem::new(format!("{}{}", icon, name)).style(style)
            })
            .collect(),
        Err(_) => vec![ListItem::new("Cannot read directory").style(Style::default().fg(Color::Red))],
    };

    let list = List::new(entries).block(block);
    frame.render_widget(list, area);
}

fn render_image_preview(
    frame: &mut Frame,
    app: &mut App,
    entry: &crate::app::DirEntry,
    llm_description: Option<&str>,
    block: Block,
    area: Rect,
) {
    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    // Check if image preview is enabled and available
    let show_image = app.config.preview.image_preview && app.image_preview.is_available();
    let scroll_offset = app.image_preview.scroll_offset;

    // Get cached metadata (loads lazily if not cached)
    let metadata = app.image_preview.get_metadata(&entry.path);

    if show_image {
        // Adaptive split: smaller image when we have description content
        let image_percent = if llm_description.is_some() { 45 } else { 60 };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(image_percent),
                Constraint::Percentage(100 - image_percent),
            ])
            .split(inner_area);

        // Render image or loading indicator
        let thumbnail_size = app.config.preview.thumbnail_size;
        if let Some(protocol) = app.image_preview.load_image(&entry.path, thumbnail_size) {
            let image = StatefulImage::new(None).resize(Resize::Fit(None));
            frame.render_stateful_widget(image, chunks[0], protocol);
        } else if app.image_preview.is_loading_image(&entry.path) {
            // Show loading indicator while image loads
            let loading = Paragraph::new("Loading image...")
                .style(Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC))
                .alignment(Alignment::Center);
            frame.render_widget(loading, chunks[0]);
        }

        // Render metadata below (using cached data)
        render_image_metadata(frame, entry, metadata.as_ref(), llm_description, chunks[1], scroll_offset);
    } else {
        // Just show metadata (fallback mode)
        render_image_metadata(frame, entry, metadata.as_ref(), llm_description, inner_area, scroll_offset);
    }
}

fn render_image_metadata(
    frame: &mut Frame,
    entry: &crate::app::DirEntry,
    metadata: Option<&CachedMetadata>,
    llm_description: Option<&str>,
    area: Rect,
    scroll_offset: u16,
) {
    let mut info_lines = vec![
        Line::from(vec![
            Span::styled("File: ", Style::default().fg(Color::DarkGray)),
            Span::raw(&entry.name),
        ]),
        Line::from(vec![
            Span::styled("Size: ", Style::default().fg(Color::DarkGray)),
            Span::raw(format_size(entry.size)),
        ]),
    ];

    // Show loading indicator or use cached metadata
    if metadata.is_none() {
        info_lines.push(Line::from(Span::styled(
            "Loading metadata...",
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
        )));
    }

    if let Some(meta) = metadata {
        if let Some((width, height)) = meta.dimensions {
            info_lines.push(Line::from(vec![
                Span::styled("Dimensions: ", Style::default().fg(Color::DarkGray)),
                Span::raw(format!("{}x{}", width, height)),
            ]));
        }

        if let Some(ref camera) = meta.camera_make {
            info_lines.push(Line::from(vec![
                Span::styled("Camera: ", Style::default().fg(Color::DarkGray)),
                Span::raw(camera),
            ]));
        }

        if let Some(ref model) = meta.camera_model {
            info_lines.push(Line::from(vec![
                Span::styled("Model: ", Style::default().fg(Color::DarkGray)),
                Span::raw(model),
            ]));
        }

        if let Some(ref taken) = meta.taken_at {
            info_lines.push(Line::from(vec![
                Span::styled("Taken: ", Style::default().fg(Color::DarkGray)),
                Span::raw(taken),
            ]));
        }

        if let Some(ref exposure) = meta.exposure {
            info_lines.push(Line::from(vec![
                Span::styled("Exposure: ", Style::default().fg(Color::DarkGray)),
                Span::raw(exposure),
            ]));
        }
    }

    // LLM description if available
    if let Some(description) = llm_description {
        info_lines.push(Line::from(""));
        info_lines.push(Line::from(Span::styled(
            "AI Description:",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        )));
        for line in description.lines() {
            info_lines.push(Line::from(line.to_string()));
        }
    }

    // Hint for AI description and scroll
    info_lines.push(Line::from(""));
    let hint = if llm_description.is_some() {
        "[D] regenerate | [{ }] scroll"
    } else {
        "[D] describe with AI"
    };
    info_lines.push(Line::from(Span::styled(hint, Style::default().fg(Color::DarkGray))));

    let text = Text::from(info_lines);
    let paragraph = Paragraph::new(text)
        .wrap(Wrap { trim: true })
        .scroll((scroll_offset, 0));
    frame.render_widget(paragraph, area);
}

fn render_file_preview(frame: &mut Frame, entry: &crate::app::DirEntry, block: Block, area: Rect) {
    let info_lines = vec![
        Line::from(vec![
            Span::styled("File: ", Style::default().fg(Color::DarkGray)),
            Span::raw(&entry.name),
        ]),
        Line::from(vec![
            Span::styled("Size: ", Style::default().fg(Color::DarkGray)),
            Span::raw(format_size(entry.size)),
        ]),
    ];

    let text = Text::from(info_lines);
    let paragraph = Paragraph::new(text).block(block).wrap(Wrap { trim: true });
    frame.render_widget(paragraph, area);
}

fn format_size(size: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if size >= GB {
        format!("{:.1} GB", size as f64 / GB as f64)
    } else if size >= MB {
        format!("{:.1} MB", size as f64 / MB as f64)
    } else if size >= KB {
        format!("{:.1} KB", size as f64 / KB as f64)
    } else {
        format!("{} bytes", size)
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
        || lower.ends_with(".raw")
        || lower.ends_with(".cr2")
        || lower.ends_with(".nef")
        || lower.ends_with(".arw")
}
