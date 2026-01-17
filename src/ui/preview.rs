use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
};
use std::fs;

use crate::app::App;

pub fn render(frame: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::DarkGray))
        .title("Preview");

    // Clone entry to avoid borrow conflicts with get_llm_description
    let selected = app.selected_entry().cloned();

    match selected {
        Some(ref entry) if entry.is_dir => {
            render_directory_preview(frame, &entry.path, block, area);
        }
        Some(ref entry) if is_image(&entry.name) => {
            let description = app.get_llm_description();
            render_image_preview(frame, entry, description.as_deref(), block, area);
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
            .take(50) // Limit preview entries
            .map(|entry| {
                let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
                let name = entry.file_name().to_string_lossy().to_string();
                let icon = if is_dir { "📁 " } else { "  " };
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

fn render_image_preview(frame: &mut Frame, entry: &crate::app::DirEntry, llm_description: Option<&str>, block: Block, area: Rect) {
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

    // Try to get image dimensions
    if let Ok(reader) = image::ImageReader::open(&entry.path) {
        if let Ok((width, height)) = reader.into_dimensions() {
            info_lines.push(Line::from(vec![
                Span::styled("Dimensions: ", Style::default().fg(Color::DarkGray)),
                Span::raw(format!("{}x{}", width, height)),
            ]));
        }
    }

    // Try to get EXIF data
    if let Ok(file) = std::fs::File::open(&entry.path) {
        let mut bufreader = std::io::BufReader::new(&file);
        if let Ok(exif) = exif::Reader::new().read_from_container(&mut bufreader) {
            // Camera info
            if let Some(field) = exif.get_field(exif::Tag::Make, exif::In::PRIMARY) {
                info_lines.push(Line::from(vec![
                    Span::styled("Camera: ", Style::default().fg(Color::DarkGray)),
                    Span::raw(field.display_value().to_string()),
                ]));
            }

            if let Some(field) = exif.get_field(exif::Tag::Model, exif::In::PRIMARY) {
                info_lines.push(Line::from(vec![
                    Span::styled("Model: ", Style::default().fg(Color::DarkGray)),
                    Span::raw(field.display_value().to_string()),
                ]));
            }

            // Date taken
            if let Some(field) = exif.get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY) {
                info_lines.push(Line::from(vec![
                    Span::styled("Taken: ", Style::default().fg(Color::DarkGray)),
                    Span::raw(field.display_value().to_string()),
                ]));
            }

            // Exposure settings
            if let Some(field) = exif.get_field(exif::Tag::FNumber, exif::In::PRIMARY) {
                info_lines.push(Line::from(vec![
                    Span::styled("Aperture: ", Style::default().fg(Color::DarkGray)),
                    Span::raw(format!("f/{}", field.display_value())),
                ]));
            }

            if let Some(field) = exif.get_field(exif::Tag::ExposureTime, exif::In::PRIMARY) {
                info_lines.push(Line::from(vec![
                    Span::styled("Shutter: ", Style::default().fg(Color::DarkGray)),
                    Span::raw(format!("{}s", field.display_value())),
                ]));
            }

            if let Some(field) = exif.get_field(exif::Tag::PhotographicSensitivity, exif::In::PRIMARY) {
                info_lines.push(Line::from(vec![
                    Span::styled("ISO: ", Style::default().fg(Color::DarkGray)),
                    Span::raw(field.display_value().to_string()),
                ]));
            }

            if let Some(field) = exif.get_field(exif::Tag::FocalLength, exif::In::PRIMARY) {
                info_lines.push(Line::from(vec![
                    Span::styled("Focal: ", Style::default().fg(Color::DarkGray)),
                    Span::raw(format!("{}mm", field.display_value())),
                ]));
            }
        }
    }

    // Modified time
    if let Some(modified) = entry.modified {
        if let Ok(duration) = modified.duration_since(std::time::UNIX_EPOCH) {
            let secs = duration.as_secs();
            let datetime = format_timestamp(secs);
            info_lines.push(Line::from(vec![
                Span::styled("Modified: ", Style::default().fg(Color::DarkGray)),
                Span::raw(datetime),
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
        // Wrap description text
        for line in description.lines() {
            info_lines.push(Line::from(line.to_string()));
        }
        info_lines.push(Line::from(""));
        info_lines.push(Line::from(Span::styled(
            "[D] to regenerate",
            Style::default().fg(Color::DarkGray),
        )));
    } else {
        info_lines.push(Line::from(""));
        info_lines.push(Line::from(Span::styled(
            "[D] to describe with AI",
            Style::default().fg(Color::DarkGray),
        )));
    }

    let text = Text::from(info_lines);
    let paragraph = Paragraph::new(text).block(block).wrap(Wrap { trim: true });
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

fn format_timestamp(secs: u64) -> String {
    // Simple timestamp formatting (could use chrono for better formatting)
    let days = secs / 86400;
    let years = 1970 + days / 365;
    let remaining_days = days % 365;
    let months = remaining_days / 30;
    let day = remaining_days % 30;
    format!("{}-{:02}-{:02}", years, months + 1, day + 1)
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
