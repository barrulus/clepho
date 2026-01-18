//! Task list dialog for viewing and managing running background tasks.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Gauge};

use crate::app::App;
use crate::tasks::BackgroundTask;

/// Render the task list dialog.
pub fn render(frame: &mut Frame, app: &App) {
    let area = frame.area();

    // Calculate dialog size - centered, not too wide
    let dialog_width = 60.min(area.width.saturating_sub(4));
    let dialog_height = 14.min(area.height.saturating_sub(4));

    let dialog_x = (area.width.saturating_sub(dialog_width)) / 2;
    let dialog_y = (area.height.saturating_sub(dialog_height)) / 2;

    let dialog_area = Rect::new(dialog_x, dialog_y, dialog_width, dialog_height);

    // Clear the area behind the dialog
    frame.render_widget(Clear, dialog_area);

    // Render dialog border
    let block = Block::default()
        .title(" Running Tasks ")
        .borders(Borders::ALL)
        .style(Style::default().bg(Color::Black));
    frame.render_widget(block, dialog_area);

    // Get running tasks
    let running_tasks = app.task_manager.running_tasks();
    let inner = Rect::new(
        dialog_area.x + 1,
        dialog_area.y + 1,
        dialog_area.width.saturating_sub(2),
        dialog_area.height.saturating_sub(2),
    );

    if running_tasks.is_empty() {
        // Show message when no tasks running
        let text = Paragraph::new("No tasks running\n\nPress Esc or T to close")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(text, inner);
    } else {
        // Render each task
        let task_height = 3;
        let max_tasks = (inner.height as usize) / task_height;

        for (idx, task) in running_tasks.iter().take(max_tasks).enumerate() {
            let task_area = Rect::new(
                inner.x,
                inner.y + (idx as u16 * task_height as u16),
                inner.width,
                task_height as u16,
            );
            render_task(frame, task, idx, task_area);
        }

        // Render help at the bottom
        let help_y = dialog_area.y + dialog_area.height - 2;
        if help_y < area.height {
            let help_area = Rect::new(dialog_area.x + 1, help_y, dialog_area.width - 2, 1);
            let help_text = Paragraph::new("1-9:cancel task  c:cancel all  Esc:close")
                .style(Style::default().fg(Color::DarkGray))
                .alignment(Alignment::Center);
            frame.render_widget(help_text, help_area);
        }
    }
}

/// Render a single task row with progress bar.
fn render_task(frame: &mut Frame, task: &BackgroundTask, index: usize, area: Rect) {
    if area.height < 2 {
        return;
    }

    // First line: task number, type, and elapsed time
    let elapsed = task.elapsed();
    let elapsed_str = format!("{}s", elapsed.as_secs());

    let header = format!(
        "[{}] {} ({})",
        index + 1,
        task.task_type.display_name(),
        elapsed_str
    );
    let header_text = Paragraph::new(header)
        .style(Style::default().fg(Color::Cyan));
    let header_area = Rect::new(area.x, area.y, area.width, 1);
    frame.render_widget(header_text, header_area);

    // Second line: progress bar or current item
    if area.height >= 2 {
        let progress_area = Rect::new(area.x, area.y + 1, area.width, 1);

        if let Some(ref progress) = task.progress {
            let label = if let Some(ref item) = progress.current_item {
                // Truncate item name if too long
                let max_len = (area.width as usize).saturating_sub(12);
                if item.len() > max_len {
                    format!("{:.width$}...", item, width = max_len.saturating_sub(3))
                } else {
                    item.clone()
                }
            } else {
                format!("{}/{}", progress.current, progress.total)
            };

            let ratio = if progress.total > 0 {
                progress.current as f64 / progress.total as f64
            } else {
                0.0
            };

            let gauge = Gauge::default()
                .ratio(ratio.min(1.0))
                .label(label)
                .gauge_style(Style::default().fg(Color::Green).bg(Color::DarkGray));
            frame.render_widget(gauge, progress_area);
        } else {
            let status = Paragraph::new("Starting...")
                .style(Style::default().fg(Color::Yellow));
            frame.render_widget(status, progress_area);
        }
    }
}
