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
use crate::config::{ImageProtocol, ThumbnailConfig};
use crate::db::{BoundingBox, PhotoMetadata};
use crate::scanner::ThumbnailManager;

/// Manages image preview state and caching
pub struct ImagePreviewState {
    picker: Option<Picker>,
    /// Cache of loaded images keyed by path (ready to display)
    image_cache: HashMap<PathBuf, StatefulProtocol>,
    /// Cache of photo metadata from database keyed by path
    pub metadata_cache: HashMap<PathBuf, Option<PhotoMetadata>>,
    /// Paths currently being loaded in background (images)
    loading_images: HashSet<PathBuf>,
    /// Receiver for async image loading (resized DynamicImage)
    image_receiver: Option<mpsc::Receiver<(PathBuf, DynamicImage)>>,
    /// Sender for async image loading
    image_sender: mpsc::Sender<(PathBuf, DynamicImage)>,
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
    /// Thumbnail manager for accessing pre-generated thumbnails
    thumbnail_manager: ThumbnailManager,
}

impl ImagePreviewState {
    pub fn new(protocol: ImageProtocol, thumbnail_config: &ThumbnailConfig) -> Self {
        let picker = Self::create_picker(protocol);
        let (img_tx, img_rx) = mpsc::channel();
        let (face_tx, face_rx) = mpsc::channel();
        let thumbnail_manager = ThumbnailManager::new(thumbnail_config);
        Self {
            picker,
            image_cache: HashMap::new(),
            metadata_cache: HashMap::new(),
            loading_images: HashSet::new(),
            image_receiver: Some(img_rx),
            image_sender: img_tx,
            current_path: None,
            scroll_offset: 0,
            thumbnail_size: 1024,
            face_cache: HashMap::new(),
            loading_faces: HashSet::new(),
            face_receiver: Some(face_rx),
            face_sender: face_tx,
            thumbnail_manager,
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

    /// Check for completed async image loads
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

    /// Get cached metadata for a path. Returns None if not in cache.
    /// Use App::get_photo_metadata() to load from database.
    pub fn get_cached_metadata(&self, path: &PathBuf) -> Option<&Option<PhotoMetadata>> {
        self.metadata_cache.get(path)
    }

    /// Cache metadata for a path (called from App after database lookup)
    pub fn cache_metadata(&mut self, path: PathBuf, metadata: Option<PhotoMetadata>) {
        self.metadata_cache.insert(path, metadata);
    }

    /// Check if metadata is cached for a path
    #[allow(dead_code)]
    pub fn has_cached_metadata(&self, path: &PathBuf) -> bool {
        self.metadata_cache.contains_key(path)
    }

    /// Clear metadata cache for a specific path (e.g., after rescan)
    #[allow(dead_code)]
    pub fn invalidate_metadata(&mut self, path: &PathBuf) {
        self.metadata_cache.remove(path);
    }

    /// Clear image cache for the current path (e.g., after rotation change)
    /// Also invalidates the on-disk thumbnail cache so it will be regenerated
    pub fn invalidate_cache(&mut self) {
        if let Some(ref path) = self.current_path.clone() {
            self.image_cache.remove(path);
            self.metadata_cache.remove(path);
            // Also invalidate on-disk thumbnail cache for all rotations
            self.thumbnail_manager.invalidate(path);
        }
    }

    /// Invalidate thumbnail for a specific path (used by gallery rotation)
    pub fn invalidate_thumbnail(&mut self, path: &PathBuf) {
        self.image_cache.remove(path);
        self.metadata_cache.remove(path);
        self.thumbnail_manager.invalidate(path);
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
    /// rotation_degrees: 0, 90, 180, or 270 degrees clockwise
    pub fn load_image(&mut self, path: &PathBuf, thumbnail_size: u32, rotation_degrees: i32) -> Option<&mut StatefulProtocol> {
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
            let rotation = rotation_degrees;

            // Check for pre-generated thumbnail from scan (now rotation-aware)
            let cached_thumb = self.thumbnail_manager.get_cached_path(path, rotation);

            std::thread::spawn(move || {
                // Try to load from thumbnail cache first (much faster - small JPEG)
                // Thumbnails now have rotation pre-applied, so we can use cache for all rotations
                let load_result = if let Some(ref thumb_path) = cached_thumb {
                    image::ImageReader::open(thumb_path)
                        .and_then(|r| r.decode().map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e)))
                } else {
                    // Fall back to loading original and resizing with rotation
                    image::ImageReader::open(&path_clone)
                        .and_then(|r| r.decode().map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e)))
                        .map(|img| {
                            let resized = img.resize(size, size, FilterType::Triangle);
                            // Apply rotation since no cached thumbnail available
                            match rotation {
                                90 => resized.rotate90(),
                                180 => resized.rotate180(),
                                270 => resized.rotate270(),
                                _ => resized,
                            }
                        })
                };

                if let Ok(dyn_img) = load_result {
                    // No need to apply rotation - either loaded from pre-rotated thumbnail
                    // or rotation was applied during resize above
                    let _ = sender.send((path_clone, dyn_img));
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
        Self::new(ImageProtocol::Auto, &ThumbnailConfig::default())
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
            // Get metadata from database (cached)
            let metadata = app.get_photo_metadata(&entry.path);
            render_image_preview(frame, app, entry, metadata.as_ref(), block, area);
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
    metadata: Option<&PhotoMetadata>,
    block: Block,
    area: Rect,
) {
    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    // Check if image preview is enabled and available
    let show_image = app.config.preview.image_preview && app.image_preview.is_available();
    let scroll_offset = app.image_preview.scroll_offset;

    if show_image {
        // Adaptive split: smaller image when we have description content
        let has_description = metadata.as_ref().map(|m| m.description.is_some()).unwrap_or(false);
        let image_percent = if has_description { 45 } else { 60 };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(image_percent),
                Constraint::Percentage(100 - image_percent),
            ])
            .split(inner_area);

        // Render image or loading indicator
        let thumbnail_size = app.config.preview.thumbnail_size;
        // Get rotation from database (combines EXIF + user rotation)
        let rotation = app.db.get_photo_rotation(&entry.path).unwrap_or(0);
        if let Some(protocol) = app.image_preview.load_image(&entry.path, thumbnail_size, rotation) {
            let image = StatefulImage::new(None).resize(Resize::Fit(None));
            frame.render_stateful_widget(image, chunks[0], protocol);
        } else if app.image_preview.is_loading_image(&entry.path) {
            // Show loading indicator while image loads
            let loading = Paragraph::new("Loading image...")
                .style(Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC))
                .alignment(Alignment::Center);
            frame.render_widget(loading, chunks[0]);
        }

        // Render metadata below
        render_image_metadata(frame, entry, metadata, chunks[1], scroll_offset);
    } else {
        // Just show metadata (fallback mode)
        render_image_metadata(frame, entry, metadata, inner_area, scroll_offset);
    }
}

fn render_image_metadata(
    frame: &mut Frame,
    entry: &crate::app::DirEntry,
    metadata: Option<&PhotoMetadata>,
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

    if let Some(meta) = metadata {
        // Dimensions
        if let (Some(w), Some(h)) = (meta.width, meta.height) {
            info_lines.push(Line::from(vec![
                Span::styled("Dimensions: ", Style::default().fg(Color::DarkGray)),
                Span::raw(format!("{}x{}", w, h)),
            ]));
        }

        // Format
        if let Some(ref format) = meta.format {
            info_lines.push(Line::from(vec![
                Span::styled("Format: ", Style::default().fg(Color::DarkGray)),
                Span::raw(format),
            ]));
        }

        // Camera info
        let camera_info: Vec<&str> = [
            meta.camera_make.as_deref(),
            meta.camera_model.as_deref(),
        ]
        .iter()
        .filter_map(|s| *s)
        .collect();
        if !camera_info.is_empty() {
            info_lines.push(Line::from(vec![
                Span::styled("Camera: ", Style::default().fg(Color::DarkGray)),
                Span::raw(camera_info.join(" ")),
            ]));
        }

        // Lens
        if let Some(ref lens) = meta.lens {
            info_lines.push(Line::from(vec![
                Span::styled("Lens: ", Style::default().fg(Color::DarkGray)),
                Span::raw(lens),
            ]));
        }

        // Exposure settings (compact line)
        let mut exposure_parts = Vec::new();
        if let Some(aperture) = meta.aperture {
            exposure_parts.push(format!("f/{:.1}", aperture));
        }
        if let Some(ref shutter) = meta.shutter_speed {
            exposure_parts.push(format!("{}s", shutter));
        }
        if let Some(iso) = meta.iso {
            exposure_parts.push(format!("ISO {}", iso));
        }
        if let Some(focal) = meta.focal_length {
            exposure_parts.push(format!("{:.0}mm", focal));
        }
        if !exposure_parts.is_empty() {
            info_lines.push(Line::from(vec![
                Span::styled("Exposure: ", Style::default().fg(Color::DarkGray)),
                Span::raw(exposure_parts.join(" | ")),
            ]));
        }

        // Date taken
        if let Some(ref taken) = meta.taken_at {
            info_lines.push(Line::from(vec![
                Span::styled("Taken: ", Style::default().fg(Color::DarkGray)),
                Span::raw(taken),
            ]));
        }

        // GPS coordinates
        if let (Some(lat), Some(lon)) = (meta.gps_latitude, meta.gps_longitude) {
            info_lines.push(Line::from(vec![
                Span::styled("GPS: ", Style::default().fg(Color::DarkGray)),
                Span::raw(format!("{:.6}, {:.6}", lat, lon)),
            ]));
        }

        // Faces and people
        if meta.face_count > 0 {
            let face_text = if meta.people_names.is_empty() {
                format!("{} face{}", meta.face_count, if meta.face_count == 1 { "" } else { "s" })
            } else {
                format!("{} ({})", meta.face_count, meta.people_names.join(", "))
            };
            info_lines.push(Line::from(vec![
                Span::styled("Faces: ", Style::default().fg(Color::DarkGray)),
                Span::raw(face_text),
            ]));
        }

        // Scanned timestamp
        if let Some(ref scanned) = meta.scanned_at {
            info_lines.push(Line::from(vec![
                Span::styled("Scanned: ", Style::default().fg(Color::DarkGray)),
                Span::raw(scanned),
            ]));
        }

        // AI Description
        if let Some(ref description) = meta.description {
            info_lines.push(Line::from(""));
            info_lines.push(Line::from(Span::styled(
                "AI Description:",
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            )));
            for line in description.lines() {
                info_lines.push(Line::from(line.to_string()));
            }
        }
    } else {
        // Not in database
        info_lines.push(Line::from(Span::styled(
            "Not scanned yet",
            Style::default().fg(Color::Yellow).add_modifier(Modifier::ITALIC),
        )));
    }

    // Hint for actions
    info_lines.push(Line::from(""));
    let hint = if metadata.as_ref().map(|m| m.description.is_some()).unwrap_or(false) {
        "[D] regenerate | [{ }] scroll"
    } else {
        "[D] describe with AI | [s] scan"
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
