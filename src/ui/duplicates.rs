use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};
use ratatui_image::{Resize, StatefulImage};
use std::path::PathBuf;

use crate::app::App;
use crate::db::{PhotoRecord, SimilarityGroup, calculate_quality_score};

#[allow(dead_code)]
pub struct DuplicatesView {
    pub groups: Vec<SimilarityGroup>,
    pub current_group: usize,
    pub selected_photo: usize,
    pub group_scroll: usize,
}

impl DuplicatesView {
    pub fn new(groups: Vec<SimilarityGroup>) -> Self {
        Self {
            groups,
            current_group: 0,
            selected_photo: 0,
            group_scroll: 0,
        }
    }

    pub fn current_group(&self) -> Option<&SimilarityGroup> {
        self.groups.get(self.current_group)
    }

    pub fn current_photo(&self) -> Option<&PhotoRecord> {
        self.current_group()
            .and_then(|g| g.photos.get(self.selected_photo))
    }

    pub fn next_group(&mut self) {
        if self.current_group < self.groups.len().saturating_sub(1) {
            self.current_group += 1;
            self.selected_photo = 0;
        }
    }

    pub fn prev_group(&mut self) {
        if self.current_group > 0 {
            self.current_group -= 1;
            self.selected_photo = 0;
        }
    }

    pub fn next_photo(&mut self) {
        if let Some(group) = self.current_group() {
            if self.selected_photo < group.photos.len().saturating_sub(1) {
                self.selected_photo += 1;
            }
        }
    }

    pub fn prev_photo(&mut self) {
        if self.selected_photo > 0 {
            self.selected_photo -= 1;
        }
    }

    pub fn toggle_deletion(&mut self) {
        if let Some(group) = self.groups.get_mut(self.current_group) {
            if let Some(photo) = group.photos.get_mut(self.selected_photo) {
                photo.marked_for_deletion = !photo.marked_for_deletion;
            }
        }
    }

    pub fn auto_select_for_deletion(&mut self) {
        for group in &mut self.groups {
            if group.photos.len() <= 1 {
                continue;
            }

            // Score all photos
            let mut scored: Vec<(usize, i32)> = group
                .photos
                .iter()
                .enumerate()
                .map(|(i, p)| (i, calculate_quality_score(p)))
                .collect();

            // Sort by score descending - highest score is the keeper
            scored.sort_by(|a, b| b.1.cmp(&a.1));

            // Mark all but the best for deletion
            for (i, _) in scored.iter().skip(1) {
                group.photos[*i].marked_for_deletion = true;
            }
        }
    }
}

pub fn render(frame: &mut Frame, app: &mut App, area: Rect) {
    let view = match app.duplicates_view.as_ref() {
        Some(v) => v,
        None => return,
    };

    // Clear and create overlay
    frame.render_widget(Clear, area);

    // Check if image preview is available
    let has_preview = app.config.preview.image_preview && app.image_preview.is_available();

    if has_preview {
        // Three-column layout: groups | photos | preview
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(25),  // Groups
                Constraint::Percentage(40),  // Photos
                Constraint::Percentage(35),  // Preview
            ])
            .split(area);

        render_group_list(frame, view, chunks[0]);
        render_photo_list(frame, view, chunks[1]);
        render_preview(frame, app, chunks[2]);
    } else {
        // Two-column layout (no preview)
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(area);

        render_group_list(frame, view, chunks[0]);
        render_photo_list(frame, view, chunks[1]);
    }
}

fn render_group_list(frame: &mut Frame, view: &DuplicatesView, area: Rect) {
    let items: Vec<ListItem> = view
        .groups
        .iter()
        .enumerate()
        .map(|(i, group)| {
            let marker = if i == view.current_group { ">" } else { " " };
            let type_icon = if group.group_type == "exact" { "=" } else { "~" };
            let count = group.photos.len();
            let marked = group.photos.iter().filter(|p| p.marked_for_deletion).count();

            let style = if i == view.current_group {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            ListItem::new(format!(
                "{} {} Grp {} ({}/{})",
                marker, type_icon, i + 1, marked, count
            ))
            .style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Blue))
            .title(format!(" Groups ({}) ", view.groups.len())),
    );

    frame.render_widget(list, area);
}

fn render_photo_list(frame: &mut Frame, view: &DuplicatesView, area: Rect) {
    let inner_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(2)])
        .split(area);

    // Photo list for current group
    if let Some(group) = view.current_group() {
        let items: Vec<ListItem> = group
            .photos
            .iter()
            .enumerate()
            .map(|(i, photo)| {
                let marker = if i == view.selected_photo { ">" } else { " " };
                let del_marker = if photo.marked_for_deletion { "[D]" } else { "   " };

                let dims = match (photo.width, photo.height) {
                    (Some(w), Some(h)) => format!("{}x{}", w, h),
                    _ => "?x?".to_string(),
                };

                let size = format_size(photo.size_bytes as u64);

                let style = if i == view.selected_photo {
                    if photo.marked_for_deletion {
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
                    }
                } else if photo.marked_for_deletion {
                    Style::default().fg(Color::Red)
                } else {
                    Style::default()
                };

                ListItem::new(format!(
                    "{} {} {} {} {}",
                    marker, del_marker, photo.filename, dims, size
                ))
                .style(style)
            })
            .collect();

        let title = format!(
            " {} ({}) [Space=toggle, a=auto] ",
            if group.group_type == "exact" { "Exact" } else { "Similar" },
            group.photos.len()
        );

        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(title),
        );

        let mut state = ListState::default();
        state.select(Some(view.selected_photo));
        frame.render_stateful_widget(list, inner_chunks[0], &mut state);

        // Show selected photo path
        if let Some(photo) = view.current_photo() {
            let path_text = Paragraph::new(photo.path.clone())
                .style(Style::default().fg(Color::DarkGray))
                .block(Block::default().borders(Borders::TOP));
            frame.render_widget(path_text, inner_chunks[1]);
        }
    } else {
        let msg = Paragraph::new("No duplicates found")
            .style(Style::default().fg(Color::DarkGray))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Photos "),
            );
        frame.render_widget(msg, inner_chunks[0]);
    }
}

fn render_preview(frame: &mut Frame, app: &mut App, area: Rect) {
    // Get current photo path
    let photo_path = match app.duplicates_view.as_ref() {
        Some(view) => match view.current_photo() {
            Some(photo) => PathBuf::from(&photo.path),
            None => {
                render_no_preview(frame, area, "No photo selected");
                return;
            }
        },
        None => {
            render_no_preview(frame, area, "No duplicates");
            return;
        }
    };

    // Split area for image and info
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(8), Constraint::Length(5)])
        .split(area);

    let image_area = chunks[0];
    let info_area = chunks[1];

    // Render image preview
    let thumbnail_size = app.config.preview.thumbnail_size;
    let rotation = app.db.get_photo_rotation(&photo_path).unwrap_or(0);

    if let Some(protocol) = app.image_preview.load_image(&photo_path, thumbnail_size, rotation) {
        let inner = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Green))
            .title(" Preview ");
        let inner_area = inner.inner(image_area);
        frame.render_widget(inner, image_area);

        let image = StatefulImage::new(None).resize(Resize::Fit(None));
        frame.render_stateful_widget(image, inner_area, protocol);
    } else if app.image_preview.is_loading_image(&photo_path) {
        render_no_preview(frame, image_area, "Loading...");
    } else {
        render_no_preview(frame, image_area, "Preview unavailable");
    }

    // Render photo info
    let view = app.duplicates_view.as_ref().unwrap();
    if let Some(photo) = view.current_photo() {
        let score = calculate_quality_score(photo);
        let dims = match (photo.width, photo.height) {
            (Some(w), Some(h)) => format!("{}x{}", w, h),
            _ => "unknown".to_string(),
        };
        let status = if photo.marked_for_deletion { "DELETE" } else { "KEEP" };
        let status_color = if photo.marked_for_deletion { Color::Red } else { Color::Green };

        let info_lines = vec![
            Line::from(vec![
                Span::raw("Status: "),
                Span::styled(status, Style::default().fg(status_color).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(format!("Size: {} | Dims: {}", format_size(photo.size_bytes as u64), dims)),
            Line::from(format!("Quality score: {}", score)),
        ];

        let info = Paragraph::new(info_lines)
            .block(Block::default().borders(Borders::ALL).title(" Info "));
        frame.render_widget(info, info_area);
    }
}

fn render_no_preview(frame: &mut Frame, area: Rect, message: &str) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(" Preview ");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let msg = Paragraph::new(message)
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);

    // Center vertically
    let y_offset = inner.height / 2;
    if y_offset > 0 && inner.height > 1 {
        let centered_area = Rect::new(inner.x, inner.y + y_offset, inner.width, 1);
        frame.render_widget(msg, centered_area);
    }
}

pub fn render_help(frame: &mut Frame, area: Rect) {
    let dialog_width = 55.min(area.width.saturating_sub(4));
    let dialog_height = 18.min(area.height.saturating_sub(4));

    let x = (area.width - dialog_width) / 2;
    let y = (area.height - dialog_height) / 2;

    let dialog_area = Rect::new(x, y, dialog_width, dialog_height);

    frame.render_widget(Clear, dialog_area);

    let help_text = vec![
        Line::from(Span::styled("Duplicates View", Style::default().add_modifier(Modifier::BOLD).fg(Color::Cyan))),
        Line::from(""),
        Line::from("  j/k/Up/Down      Move between photos"),
        Line::from("  J/K/Left/Right   Move between groups"),
        Line::from("  Mouse click      Select group or photo"),
        Line::from("  Space            Toggle deletion mark"),
        Line::from("  a                Auto-select (keep best)"),
        Line::from("  x                Move marked to trash"),
        Line::from("  X                Permanently delete"),
        Line::from("  Esc              Exit duplicates view"),
        Line::from("  ?                Toggle this help"),
        Line::from(""),
        Line::from(Span::styled("Legend", Style::default().add_modifier(Modifier::BOLD).fg(Color::Cyan))),
        Line::from("  =        Exact duplicate (SHA256)"),
        Line::from("  ~        Perceptual similar"),
        Line::from("  [D]      Marked for deletion"),
    ];

    let paragraph = Paragraph::new(help_text).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" Duplicates Help "),
    );

    frame.render_widget(paragraph, dialog_area);
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
