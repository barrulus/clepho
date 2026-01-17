use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState},
};

use crate::app::{App, DirEntry};

pub fn render_parent(frame: &mut Frame, app: &App, area: Rect) {
    let title = app
        .current_dir
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "/".to_string());

    let items: Vec<ListItem> = app
        .parent_entries
        .iter()
        .map(|entry| entry_to_list_item(entry, false))
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(title),
        )
        .highlight_style(Style::default().bg(Color::DarkGray));

    let mut state = ListState::default();
    state.select(Some(app.parent_selected_index));

    frame.render_stateful_widget(list, area, &mut state);
}

pub fn render_current(frame: &mut Frame, app: &App, area: Rect) {
    let title = app
        .current_dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| app.current_dir.to_string_lossy().to_string());

    let items: Vec<ListItem> = app
        .entries
        .iter()
        .map(|entry| entry_to_list_item(entry, true))
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Blue))
                .title(title),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Blue)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        );

    let mut state = ListState::default();
    state.select(Some(app.selected_index));

    frame.render_stateful_widget(list, area, &mut state);
}

fn entry_to_list_item(entry: &DirEntry, show_size: bool) -> ListItem<'static> {
    let icon = if entry.is_dir { "📁 " } else { "  " };
    let name = entry.name.clone();

    let text = if show_size && !entry.is_dir {
        format!("{}{} {}", icon, name, format_size(entry.size))
    } else {
        format!("{}{}", icon, name)
    };

    let style = if entry.is_dir {
        Style::default().fg(Color::Cyan)
    } else if is_image(&entry.name) {
        Style::default().fg(Color::Green)
    } else {
        Style::default()
    };

    ListItem::new(text).style(style)
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
