use ratatui::{
    prelude::*,
    widgets::Paragraph,
};

use crate::app::{App, AppMode, ScanProgress};

pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    // If scanning, show scan progress
    if app.mode == AppMode::Scanning {
        render_scan_progress(frame, app, area);
        return;
    }

    // If there's a status message, show it
    if let Some(ref message) = app.status_message {
        let line = Line::from(vec![
            Span::styled(
                format!(" {} ", message),
                Style::default().fg(Color::Yellow).bg(Color::DarkGray),
            ),
        ]);
        let paragraph = Paragraph::new(line);
        frame.render_widget(paragraph, area);
        return;
    }

    // Normal status bar
    let path = app.current_dir.to_string_lossy();

    let item_count = app.entries.len();
    let dir_count = app.entries.iter().filter(|e| e.is_dir).count();
    let file_count = item_count - dir_count;

    let position = if item_count > 0 {
        format!("{}/{}", app.selected_index + 1, item_count)
    } else {
        "0/0".to_string()
    };

    let left_text = Span::styled(
        format!(" {} ", path),
        Style::default().fg(Color::White).bg(Color::DarkGray),
    );

    let middle_text = Span::styled(
        format!(" {} dirs, {} files ", dir_count, file_count),
        Style::default().fg(Color::Gray),
    );

    let right_text = Span::styled(
        format!(" {} | s scan | ? help | q quit ", position),
        Style::default().fg(Color::White).bg(Color::DarkGray),
    );

    // Calculate spacing
    let left_width = path.len() + 2;
    let middle_width = format!(" {} dirs, {} files ", dir_count, file_count).len();
    let right_width = format!(" {} | s scan | ? help | q quit ", position).len();
    let total_used = left_width + middle_width + right_width;
    let available = area.width as usize;

    let spacing = if available > total_used {
        " ".repeat((available - total_used) / 2)
    } else {
        String::new()
    };

    let line = Line::from(vec![
        left_text,
        Span::raw(&spacing),
        middle_text,
        Span::raw(&spacing),
        right_text,
    ]);

    let paragraph = Paragraph::new(line);
    frame.render_widget(paragraph, area);
}

fn render_scan_progress(frame: &mut Frame, app: &App, area: Rect) {
    let progress_text = match &app.scan_progress {
        Some(ScanProgress::Started { total_files }) => {
            format!("Starting scan... {} files found", total_files)
        }
        Some(ScanProgress::Scanning { current, total, path }) => {
            let filename = std::path::Path::new(path)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.clone());
            format!("Scanning {}/{}: {}", current, total, filename)
        }
        Some(ScanProgress::Completed { scanned, new, updated }) => {
            format!("Complete: {} scanned, {} new, {} updated", scanned, new, updated)
        }
        Some(ScanProgress::Error { message }) => {
            format!("Error: {}", message)
        }
        None => "Scanning...".to_string(),
    };

    let line = Line::from(vec![
        Span::styled(
            format!(" {} ", progress_text),
            Style::default().fg(Color::Cyan).bg(Color::DarkGray),
        ),
        Span::styled(
            " [ESC to cancel] ",
            Style::default().fg(Color::Yellow).bg(Color::DarkGray),
        ),
    ]);

    let paragraph = Paragraph::new(line);
    frame.render_widget(paragraph, area);
}
