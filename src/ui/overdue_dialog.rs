//! Overdue schedules dialog shown on startup.

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};
use std::collections::HashSet;

use crate::db::ScheduledTask;

/// State for the overdue schedules dialog.
pub struct OverdueDialog {
    /// List of overdue tasks.
    pub tasks: Vec<ScheduledTask>,
    /// Selected index.
    pub selected_index: usize,
    /// Selected task IDs for running.
    pub selected_tasks: HashSet<i64>,
}

impl OverdueDialog {
    pub fn new(tasks: Vec<ScheduledTask>) -> Self {
        Self {
            tasks,
            selected_index: 0,
            selected_tasks: HashSet::new(),
        }
    }

    /// Move selection down.
    pub fn move_down(&mut self) {
        if !self.tasks.is_empty() && self.selected_index < self.tasks.len() - 1 {
            self.selected_index += 1;
        }
    }

    /// Move selection up.
    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    /// Toggle selection of current task.
    pub fn toggle_selection(&mut self) {
        if let Some(task) = self.tasks.get(self.selected_index) {
            if self.selected_tasks.contains(&task.id) {
                self.selected_tasks.remove(&task.id);
            } else {
                self.selected_tasks.insert(task.id);
            }
        }
    }

    /// Select all tasks.
    pub fn select_all(&mut self) {
        for task in &self.tasks {
            self.selected_tasks.insert(task.id);
        }
    }

    /// Get currently selected task.
    pub fn selected_task(&self) -> Option<&ScheduledTask> {
        self.tasks.get(self.selected_index)
    }

    /// Get all selected task IDs (or all if none selected).
    pub fn tasks_to_run(&self) -> Vec<i64> {
        if self.selected_tasks.is_empty() {
            self.tasks.iter().map(|t| t.id).collect()
        } else {
            self.selected_tasks.iter().cloned().collect()
        }
    }

    /// Get all task IDs for cancellation.
    pub fn all_task_ids(&self) -> Vec<i64> {
        self.tasks.iter().map(|t| t.id).collect()
    }
}

pub fn render(frame: &mut Frame, dialog: &OverdueDialog, area: Rect) {
    // Center the dialog
    let dialog_width = 70.min(area.width.saturating_sub(4));
    let dialog_height = 18.min(area.height.saturating_sub(4));

    let x = (area.width - dialog_width) / 2;
    let y = (area.height - dialog_height) / 2;

    let dialog_area = Rect::new(x, y, dialog_width, dialog_height);

    // Clear background
    frame.render_widget(Clear, dialog_area);

    // Main layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // Task list
            Constraint::Length(3), // Help text
        ])
        .split(dialog_area);

    // Header
    let header = Paragraph::new(format!(" {} overdue scheduled tasks found", dialog.tasks.len()))
        .style(Style::default().fg(Color::Red))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Red))
                .title(" Overdue Tasks "),
        );
    frame.render_widget(header, chunks[0]);

    // Task list
    if dialog.tasks.is_empty() {
        let empty_msg = Paragraph::new("  No overdue tasks")
            .style(Style::default().fg(Color::DarkGray))
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(empty_msg, chunks[1]);
    } else {
        let items: Vec<ListItem> = dialog
            .tasks
            .iter()
            .enumerate()
            .map(|(i, task)| {
                let selected = dialog.selected_tasks.contains(&task.id);
                let marker = if selected { "[x]" } else { "[ ]" };

                let style = if i == dialog.selected_index {
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                } else if selected {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default()
                };

                // Format scheduled time
                let scheduled = if task.scheduled_at.len() >= 16 {
                    &task.scheduled_at[..16]
                } else {
                    &task.scheduled_at
                };

                ListItem::new(format!(
                    " {} {} | {} | {}",
                    marker,
                    task.task_type.display_name(),
                    scheduled,
                    truncate_path(&task.target_path, 30)
                )).style(style)
            })
            .collect();

        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Tasks "),
        );

        let mut state = ListState::default();
        state.select(Some(dialog.selected_index));
        frame.render_stateful_widget(list, chunks[1], &mut state);
    }

    // Help text
    let sel_count = dialog.selected_tasks.len();
    let sel_text = if sel_count > 0 {
        format!(" ({} selected)", sel_count)
    } else {
        String::new()
    };

    let help = Paragraph::new(format!(
        " j/k=nav  Space=toggle  a=all  Enter=run{}  c=cancel all  q=dismiss",
        sel_text
    ))
    .style(Style::default().fg(Color::DarkGray))
    .block(Block::default().borders(Borders::TOP));

    frame.render_widget(help, chunks[2]);
}

fn truncate_path(path: &str, max_len: usize) -> String {
    if path.len() <= max_len {
        path.to_string()
    } else {
        format!("...{}", &path[path.len() - max_len + 3..])
    }
}
