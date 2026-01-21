//! Dialog for editing photo descriptions.

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use std::path::PathBuf;

/// Dialog state for editing a photo's description
pub struct EditDescriptionDialog {
    /// Path of the photo being edited
    pub photo_path: PathBuf,
    /// Original description (if any)
    pub original: Option<String>,
    /// Current text being edited
    pub text: String,
    /// Cursor position in text
    pub cursor: usize,
    /// Scroll offset for long text
    pub scroll: u16,
}

impl EditDescriptionDialog {
    pub fn new(photo_path: PathBuf, description: Option<String>) -> Self {
        let text = description.clone().unwrap_or_default();
        let cursor = text.len();
        Self {
            photo_path,
            original: description,
            text,
            cursor,
            scroll: 0,
        }
    }

    pub fn handle_char(&mut self, c: char) {
        self.text.insert(self.cursor, c);
        self.cursor += 1;
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.text.remove(self.cursor);
        }
    }

    pub fn delete(&mut self) {
        if self.cursor < self.text.len() {
            self.text.remove(self.cursor);
        }
    }

    pub fn move_cursor_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn move_cursor_right(&mut self) {
        if self.cursor < self.text.len() {
            self.cursor += 1;
        }
    }

    pub fn move_cursor_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_cursor_end(&mut self) {
        self.cursor = self.text.len();
    }

    pub fn move_cursor_word_left(&mut self) {
        // Skip whitespace first
        while self.cursor > 0 && self.text.chars().nth(self.cursor - 1) == Some(' ') {
            self.cursor -= 1;
        }
        // Then skip to start of word
        while self.cursor > 0 && self.text.chars().nth(self.cursor - 1) != Some(' ') {
            self.cursor -= 1;
        }
    }

    pub fn move_cursor_word_right(&mut self) {
        let len = self.text.len();
        // Skip current word
        while self.cursor < len && self.text.chars().nth(self.cursor) != Some(' ') {
            self.cursor += 1;
        }
        // Skip whitespace
        while self.cursor < len && self.text.chars().nth(self.cursor) == Some(' ') {
            self.cursor += 1;
        }
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }

    pub fn revert(&mut self) {
        self.text = self.original.clone().unwrap_or_default();
        self.cursor = self.text.len();
    }

    pub fn is_modified(&self) -> bool {
        self.original.as_deref() != Some(&self.text) && !(self.original.is_none() && self.text.is_empty())
    }

    pub fn get_text(&self) -> &str {
        &self.text
    }
}

pub fn render(frame: &mut Frame, dialog: &EditDescriptionDialog, area: Rect) {
    let dialog_width = 70.min(area.width.saturating_sub(4));
    let dialog_height = 20.min(area.height.saturating_sub(4));

    let x = (area.width - dialog_width) / 2;
    let y = (area.height - dialog_height) / 2;

    let dialog_area = Rect::new(x, y, dialog_width, dialog_height);

    frame.render_widget(Clear, dialog_area);

    // Layout: filename, text area, help
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Filename
            Constraint::Min(8),     // Text area
            Constraint::Length(4),  // Help
        ])
        .margin(1)
        .split(dialog_area);

    // Outer border
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Edit Description ");
    frame.render_widget(block, dialog_area);

    // Filename
    let filename = dialog.photo_path.file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Unknown".to_string());
    let modified_marker = if dialog.is_modified() { " [modified]" } else { "" };
    let filename_widget = Paragraph::new(format!("{}{}", filename, modified_marker))
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(filename_widget, chunks[0]);

    // Text area with cursor
    let text_with_cursor = if dialog.cursor < dialog.text.len() {
        let (before, after) = dialog.text.split_at(dialog.cursor);
        let cursor_char = after.chars().next().unwrap_or(' ');
        let rest = &after[cursor_char.len_utf8()..];
        format!(
            "{}{}{}",
            before,
            cursor_char,
            rest
        )
    } else {
        format!("{}_", dialog.text)
    };

    // Create spans with cursor highlighting
    let display_text = if dialog.cursor < dialog.text.len() {
        let (before, after) = dialog.text.split_at(dialog.cursor);
        let cursor_char = after.chars().next().unwrap_or(' ');
        let rest = &after[cursor_char.len_utf8()..];
        Line::from(vec![
            Span::raw(before),
            Span::styled(
                cursor_char.to_string(),
                Style::default().bg(Color::White).fg(Color::Black),
            ),
            Span::raw(rest),
        ])
    } else {
        Line::from(vec![
            Span::raw(&dialog.text),
            Span::styled(" ", Style::default().bg(Color::White)),
        ])
    };

    let text_widget = Paragraph::new(vec![display_text])
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Green))
                .title(" Description (Ctrl+Enter to save) "),
        );
    frame.render_widget(text_widget, chunks[1]);

    // Help text
    let help_text = vec![
        Line::from("Enter=newline | Ctrl+Enter=save | Esc=cancel"),
        Line::from("Ctrl+U=clear | Ctrl+R=revert | Arrows=move cursor"),
    ];
    let help = Paragraph::new(help_text)
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    frame.render_widget(help, chunks[2]);
}
