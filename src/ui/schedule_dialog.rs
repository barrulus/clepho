//! Schedule dialog for creating scheduled tasks.

use chrono::{Local, NaiveDate, NaiveDateTime, NaiveTime, Timelike};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};
use std::path::PathBuf;

use crate::db::ScheduledTaskType;

/// Which field is currently being edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleField {
    TaskType,
    Date,
    Hour,
    HoursToggle,
    HoursStart,
    HoursEnd,
}

/// State for the schedule dialog.
pub struct ScheduleDialog {
    /// Files to schedule (from selection).
    pub files: Vec<PathBuf>,
    /// Current directory (for scan tasks).
    pub current_dir: PathBuf,
    /// Selected task type.
    pub task_type: ScheduledTaskType,
    /// Scheduled date.
    pub date: NaiveDate,
    /// Scheduled hour (0-23).
    pub hour: u8,
    /// Whether to use hours of operation.
    pub use_hours: bool,
    /// Hours of operation start.
    pub hours_start: u8,
    /// Hours of operation end.
    pub hours_end: u8,
    /// Current field being edited.
    pub field: ScheduleField,
}

impl ScheduleDialog {
    pub fn new(files: Vec<PathBuf>, current_dir: PathBuf) -> Self {
        let now = Local::now();
        Self {
            files,
            current_dir,
            task_type: ScheduledTaskType::Scan,
            date: now.date_naive(),
            hour: (now.hour() + 1) as u8 % 24, // Default to next hour
            use_hours: false,
            hours_start: 9,
            hours_end: 17,
            field: ScheduleField::TaskType,
        }
    }

    /// Move to next field.
    pub fn next_field(&mut self) {
        self.field = match self.field {
            ScheduleField::TaskType => ScheduleField::Date,
            ScheduleField::Date => ScheduleField::Hour,
            ScheduleField::Hour => ScheduleField::HoursToggle,
            ScheduleField::HoursToggle => {
                if self.use_hours {
                    ScheduleField::HoursStart
                } else {
                    ScheduleField::TaskType
                }
            }
            ScheduleField::HoursStart => ScheduleField::HoursEnd,
            ScheduleField::HoursEnd => ScheduleField::TaskType,
        };
    }

    /// Move to previous field.
    pub fn prev_field(&mut self) {
        self.field = match self.field {
            ScheduleField::TaskType => {
                if self.use_hours {
                    ScheduleField::HoursEnd
                } else {
                    ScheduleField::HoursToggle
                }
            }
            ScheduleField::Date => ScheduleField::TaskType,
            ScheduleField::Hour => ScheduleField::Date,
            ScheduleField::HoursToggle => ScheduleField::Hour,
            ScheduleField::HoursStart => ScheduleField::HoursToggle,
            ScheduleField::HoursEnd => ScheduleField::HoursStart,
        };
    }

    /// Increment current field value.
    pub fn increment(&mut self) {
        match self.field {
            ScheduleField::TaskType => {
                self.task_type = match self.task_type {
                    ScheduledTaskType::Scan => ScheduledTaskType::LlmBatch,
                    ScheduledTaskType::LlmBatch => ScheduledTaskType::FaceDetection,
                    ScheduledTaskType::FaceDetection => ScheduledTaskType::Scan,
                };
            }
            ScheduleField::Date => {
                if let Some(next) = self.date.succ_opt() {
                    self.date = next;
                }
            }
            ScheduleField::Hour => {
                self.hour = (self.hour + 1) % 24;
            }
            ScheduleField::HoursToggle => {
                self.use_hours = !self.use_hours;
            }
            ScheduleField::HoursStart => {
                self.hours_start = (self.hours_start + 1) % 24;
            }
            ScheduleField::HoursEnd => {
                self.hours_end = (self.hours_end + 1) % 24;
            }
        }
    }

    /// Decrement current field value.
    pub fn decrement(&mut self) {
        match self.field {
            ScheduleField::TaskType => {
                self.task_type = match self.task_type {
                    ScheduledTaskType::Scan => ScheduledTaskType::FaceDetection,
                    ScheduledTaskType::LlmBatch => ScheduledTaskType::Scan,
                    ScheduledTaskType::FaceDetection => ScheduledTaskType::LlmBatch,
                };
            }
            ScheduleField::Date => {
                if let Some(prev) = self.date.pred_opt() {
                    self.date = prev;
                }
            }
            ScheduleField::Hour => {
                self.hour = if self.hour == 0 { 23 } else { self.hour - 1 };
            }
            ScheduleField::HoursToggle => {
                self.use_hours = !self.use_hours;
            }
            ScheduleField::HoursStart => {
                self.hours_start = if self.hours_start == 0 { 23 } else { self.hours_start - 1 };
            }
            ScheduleField::HoursEnd => {
                self.hours_end = if self.hours_end == 0 { 23 } else { self.hours_end - 1 };
            }
        }
    }

    /// Get the scheduled datetime as ISO string.
    pub fn scheduled_at(&self) -> String {
        let time = NaiveTime::from_hms_opt(self.hour as u32, 0, 0)
            .unwrap_or_else(|| NaiveTime::from_hms_opt(0, 0, 0).unwrap());
        let datetime = NaiveDateTime::new(self.date, time);
        datetime.format("%Y-%m-%dT%H:%M:%S").to_string()
    }

    /// Get target path based on task type.
    pub fn target_path(&self) -> String {
        self.current_dir.to_string_lossy().to_string()
    }

    /// Get hours of operation if enabled.
    pub fn hours_of_operation(&self) -> Option<(u8, u8)> {
        if self.use_hours {
            Some((self.hours_start, self.hours_end))
        } else {
            None
        }
    }
}

pub fn render(frame: &mut Frame, dialog: &ScheduleDialog, area: Rect) {
    // Center the dialog
    let dialog_width = 60.min(area.width.saturating_sub(4));
    let dialog_height = 16.min(area.height.saturating_sub(4));

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
            Constraint::Min(0),    // Content
            Constraint::Length(3), // Help text
        ])
        .split(dialog_area);

    // Header
    let file_count = if dialog.files.is_empty() {
        "current directory".to_string()
    } else {
        format!("{} files", dialog.files.len())
    };

    let header = Paragraph::new(format!(" Schedule {} for: {}", dialog.task_type.display_name(), file_count))
        .style(Style::default().fg(Color::Yellow))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow))
                .title(" Schedule Task "),
        );
    frame.render_widget(header, chunks[0]);

    // Content - field list
    let field_style = |f: ScheduleField| {
        if dialog.field == f {
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        }
    };

    let marker = |f: ScheduleField| {
        if dialog.field == f { ">" } else { " " }
    };

    let mut items = vec![
        ListItem::new(format!(
            "{} Task Type: {}",
            marker(ScheduleField::TaskType),
            dialog.task_type.display_name()
        )).style(field_style(ScheduleField::TaskType)),

        ListItem::new(format!(
            "{} Date: {}",
            marker(ScheduleField::Date),
            dialog.date.format("%Y-%m-%d")
        )).style(field_style(ScheduleField::Date)),

        ListItem::new(format!(
            "{} Time: {:02}:00",
            marker(ScheduleField::Hour),
            dialog.hour
        )).style(field_style(ScheduleField::Hour)),

        ListItem::new(format!(
            "{} Hours of Operation: {}",
            marker(ScheduleField::HoursToggle),
            if dialog.use_hours { "Yes" } else { "No" }
        )).style(field_style(ScheduleField::HoursToggle)),
    ];

    if dialog.use_hours {
        items.push(
            ListItem::new(format!(
                "{}   Start Hour: {:02}:00",
                marker(ScheduleField::HoursStart),
                dialog.hours_start
            )).style(field_style(ScheduleField::HoursStart))
        );
        items.push(
            ListItem::new(format!(
                "{}   End Hour: {:02}:00",
                marker(ScheduleField::HoursEnd),
                dialog.hours_end
            )).style(field_style(ScheduleField::HoursEnd))
        );
    }

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Settings "),
    );

    let mut state = ListState::default();
    frame.render_stateful_widget(list, chunks[1], &mut state);

    // Help text
    let help = Paragraph::new(" Tab/j/k=nav  +/-=change  Enter=schedule  n=run now  q=cancel")
        .style(Style::default().fg(Color::DarkGray))
        .block(Block::default().borders(Borders::TOP));

    frame.render_widget(help, chunks[2]);
}
