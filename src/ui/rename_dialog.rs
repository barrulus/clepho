use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};
use std::path::PathBuf;

/// State for the batch rename dialog
pub struct RenameDialog {
    /// Files to be renamed
    pub files: Vec<PathBuf>,
    /// Pattern input
    pub pattern: String,
    /// Current cursor position in pattern
    pub cursor: usize,
    /// Preview of new names
    pub preview: Vec<(String, String)>, // (old_name, new_name)
    /// Error message if any
    pub error: Option<String>,
    /// Counter start value
    pub counter_start: u32,
}

impl RenameDialog {
    pub fn new(files: Vec<PathBuf>) -> Self {
        let mut dialog = Self {
            files,
            pattern: "{name}.{ext}".to_string(),
            cursor: 11, // End of default pattern
            preview: Vec::new(),
            error: None,
            counter_start: 1,
        };
        dialog.update_preview();
        dialog
    }

    pub fn handle_char(&mut self, c: char) {
        self.pattern.insert(self.cursor, c);
        self.cursor += 1;
        self.update_preview();
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.pattern.remove(self.cursor);
            self.update_preview();
        }
    }

    pub fn delete(&mut self) {
        if self.cursor < self.pattern.len() {
            self.pattern.remove(self.cursor);
            self.update_preview();
        }
    }

    pub fn move_cursor_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn move_cursor_right(&mut self) {
        if self.cursor < self.pattern.len() {
            self.cursor += 1;
        }
    }

    pub fn move_cursor_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_cursor_end(&mut self) {
        self.cursor = self.pattern.len();
    }

    pub fn update_preview(&mut self) {
        self.preview.clear();
        self.error = None;

        let mut counter = self.counter_start;

        for file_path in &self.files {
            let old_name = file_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            match self.apply_pattern(file_path, counter) {
                Ok(new_name) => {
                    self.preview.push((old_name, new_name));
                    counter += 1;
                }
                Err(e) => {
                    self.error = Some(e);
                    break;
                }
            }
        }

        // Check for conflicts
        if self.error.is_none() {
            self.check_conflicts();
        }
    }

    fn apply_pattern(&self, file_path: &PathBuf, counter: u32) -> Result<String, String> {
        let name = file_path
            .file_stem()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        let ext = file_path
            .extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_default();

        // Get metadata for date/time
        let (date, time) = if let Ok(metadata) = std::fs::metadata(file_path) {
            if let Ok(modified) = metadata.modified() {
                let duration = modified
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default();
                let secs = duration.as_secs();
                // Simple date/time formatting
                let days = secs / 86400;
                let years = 1970 + days / 365;
                let remaining_days = days % 365;
                let months = remaining_days / 30 + 1;
                let day = remaining_days % 30 + 1;
                let hours = (secs % 86400) / 3600;
                let minutes = (secs % 3600) / 60;
                let seconds = secs % 60;
                (
                    format!("{:04}-{:02}-{:02}", years, months, day),
                    format!("{:02}-{:02}-{:02}", hours, minutes, seconds),
                )
            } else {
                ("unknown".to_string(), "unknown".to_string())
            }
        } else {
            ("unknown".to_string(), "unknown".to_string())
        };

        // Apply pattern substitutions
        let mut result = self.pattern.clone();

        result = result.replace("{name}", &name);
        result = result.replace("{ext}", &ext);
        result = result.replace("{date}", &date);
        result = result.replace("{time}", &time);
        result = result.replace("{counter}", &format!("{:03}", counter));
        result = result.replace("{c}", &format!("{}", counter));

        // Validate result
        if result.is_empty() {
            return Err("Pattern results in empty filename".to_string());
        }

        if result.contains('/') || result.contains('\\') {
            return Err("Filename cannot contain path separators".to_string());
        }

        Ok(result)
    }

    fn check_conflicts(&mut self) {
        let new_names: Vec<&String> = self.preview.iter().map(|(_, new)| new).collect();

        // Check for duplicates in new names
        for (i, name) in new_names.iter().enumerate() {
            for (j, other) in new_names.iter().enumerate() {
                if i != j && name == other {
                    self.error = Some(format!("Conflict: multiple files would have name '{}'", name));
                    return;
                }
            }
        }

        // Check for conflicts with existing files (that aren't being renamed)
        for (old_name, new_name) in &self.preview {
            if old_name != new_name {
                for file_path in &self.files {
                    if let Some(parent) = file_path.parent() {
                        let new_path = parent.join(new_name);
                        if new_path.exists() {
                            // Check if it's one of the files we're renaming
                            let is_being_renamed = self.preview.iter().any(|(old, _)| {
                                file_path.file_name().map(|n| n.to_string_lossy().to_string())
                                    == Some(old.clone())
                            });
                            if !is_being_renamed {
                                self.error = Some(format!("File '{}' already exists", new_name));
                                return;
                            }
                        }
                    }
                }
            }
        }
    }

    /// Execute the rename operation
    pub fn execute(&self) -> Result<(usize, usize), String> {
        if self.error.is_some() {
            return Err(self.error.clone().unwrap());
        }

        let mut success = 0;
        let mut failed = 0;

        for (file_path, (_, new_name)) in self.files.iter().zip(self.preview.iter()) {
            if let Some(parent) = file_path.parent() {
                let new_path = parent.join(new_name);

                if file_path == &new_path {
                    // No change needed
                    success += 1;
                    continue;
                }

                match std::fs::rename(file_path, &new_path) {
                    Ok(_) => success += 1,
                    Err(_) => failed += 1,
                }
            } else {
                failed += 1;
            }
        }

        Ok((success, failed))
    }
}

pub fn render(frame: &mut Frame, dialog: &RenameDialog, area: Rect) {
    // Center the dialog
    let dialog_width = 80.min(area.width.saturating_sub(4));
    let dialog_height = 28.min(area.height.saturating_sub(4));

    let x = (area.width - dialog_width) / 2;
    let y = (area.height - dialog_height) / 2;

    let dialog_area = Rect::new(x, y, dialog_width, dialog_height);

    // Clear the area behind the dialog
    frame.render_widget(Clear, dialog_area);

    // Main layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Length(3),  // Pattern input
            Constraint::Length(3),  // Variables help
            Constraint::Min(10),    // Preview
            Constraint::Length(2),  // Error/status
            Constraint::Length(2),  // Footer
        ])
        .split(dialog_area);

    // Draw border
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta))
        .title(" Batch Rename ")
        .title_style(Style::default().add_modifier(Modifier::BOLD));
    frame.render_widget(block, dialog_area);

    // Header
    let header = Paragraph::new(format!("Renaming {} file(s)", dialog.files.len()))
        .style(Style::default().fg(Color::Magenta));
    frame.render_widget(header, chunks[0]);

    // Pattern input with cursor
    let pattern_display = format!("{}_", &dialog.pattern[..dialog.cursor]);
    let pattern_after = &dialog.pattern[dialog.cursor..];
    let input = Paragraph::new(Line::from(vec![
        Span::raw(&pattern_display[..pattern_display.len() - 1]),
        Span::styled("|", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        Span::raw(pattern_after),
    ]))
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Pattern ")
            .border_style(Style::default().fg(Color::Yellow)),
    );
    frame.render_widget(input, chunks[1]);

    // Variables help
    let help = Paragraph::new(
        "Variables: {name} {ext} {date} {time} {counter} {c}",
    )
    .style(Style::default().fg(Color::DarkGray))
    .wrap(Wrap { trim: true });
    frame.render_widget(help, chunks[2]);

    // Preview list
    let preview_items: Vec<ListItem> = dialog
        .preview
        .iter()
        .take(10) // Limit preview to first 10 files
        .map(|(old, new)| {
            let style = if old == new {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default().fg(Color::Green)
            };
            ListItem::new(Line::from(vec![
                Span::styled(old, Style::default().fg(Color::Red)),
                Span::raw(" -> "),
                Span::styled(new, style),
            ]))
        })
        .collect();

    let more_text = if dialog.files.len() > 10 {
        format!(" Preview (showing 10 of {}) ", dialog.files.len())
    } else {
        " Preview ".to_string()
    };

    let preview_list = List::new(preview_items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(more_text)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(preview_list, chunks[3]);

    // Error or status
    let status = if let Some(ref error) = dialog.error {
        Paragraph::new(error.as_str()).style(Style::default().fg(Color::Red))
    } else {
        Paragraph::new("Ready to rename").style(Style::default().fg(Color::Green))
    };
    frame.render_widget(status, chunks[4]);

    // Footer
    let footer = Paragraph::new("Enter: confirm | Esc: cancel | Arrows: move cursor")
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(footer, chunks[5]);
}
