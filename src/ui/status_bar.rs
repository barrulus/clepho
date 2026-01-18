use ratatui::{
    prelude::*,
    widgets::Paragraph,
};

use crate::app::App;

pub fn render(frame: &mut Frame, app: &App, area: Rect) {
    // If there's a status message, show it prominently
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

    // Build status bar content
    let path = app.current_dir.to_string_lossy();

    let item_count = app.entries.len();
    let dir_count = app.entries.iter().filter(|e| e.is_dir).count();
    let file_count = item_count - dir_count;

    let position = if item_count > 0 {
        format!("{}/{}", app.selected_index + 1, item_count)
    } else {
        "0/0".to_string()
    };

    // Build running task indicators
    let running_tasks = app.task_manager.running_tasks();
    let task_indicators: String = if running_tasks.is_empty() {
        String::new()
    } else {
        let indicators: Vec<String> = running_tasks
            .iter()
            .map(|task| {
                if let Some(ref progress) = task.progress {
                    format!("[{}:{}%]", task.task_type.short_name(), progress.percent())
                } else {
                    format!("[{}:...]", task.task_type.short_name())
                }
            })
            .collect();
        indicators.join(" ")
    };

    // Build the status bar line
    let mut spans = Vec::new();

    // Left: path
    spans.push(Span::styled(
        format!(" {} ", path),
        Style::default().fg(Color::White).bg(Color::DarkGray),
    ));

    // Middle: dir/file count
    spans.push(Span::styled(
        format!(" {} dirs, {} files ", dir_count, file_count),
        Style::default().fg(Color::Gray),
    ));

    // Task indicators (if any)
    if !task_indicators.is_empty() {
        spans.push(Span::styled(
            format!(" {} ", task_indicators),
            Style::default().fg(Color::Cyan),
        ));
    }

    // Calculate remaining space and add spacing
    let content_len: usize = spans.iter().map(|s| s.content.len()).sum();
    let help_text = if running_tasks.is_empty() {
        format!(" {} | s:scan ?:help q:quit ", position)
    } else {
        format!(" {} | T:tasks ?:help q:quit ", position)
    };
    let help_len = help_text.len();

    let available = area.width as usize;
    if available > content_len + help_len {
        let spacing = " ".repeat(available - content_len - help_len);
        spans.push(Span::raw(spacing));
    }

    // Right: help hints
    spans.push(Span::styled(
        help_text,
        Style::default().fg(Color::White).bg(Color::DarkGray),
    ));

    let line = Line::from(spans);
    let paragraph = Paragraph::new(line);
    frame.render_widget(paragraph, area);
}
