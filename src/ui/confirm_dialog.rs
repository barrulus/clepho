//! Confirmation dialog for expensive tasks.

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::config::Action;

/// Focus area within the confirm dialog
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmFocus {
    PromptField,
    Buttons,
}

/// Dialog state for confirming an expensive task before execution
pub struct ConfirmDialog {
    /// The action pending confirmation
    pub action: Action,
    /// Description shown to user
    pub message: String,
    /// Whether this dialog has a prompt text field
    pub has_prompt_field: bool,
    /// The editable prompt text
    pub prompt_text: String,
    /// Cursor position in the prompt text
    pub prompt_cursor: usize,
    /// Which part of the dialog has focus
    pub focus: ConfirmFocus,
    /// The original prompt text (to detect modifications)
    pub original_prompt: String,
}

impl ConfirmDialog {
    pub fn new(action: Action, initial_prompt: Option<String>) -> Self {
        let message = match action {
            Action::Scan => "Scan directory for photos? This will index all images in the current directory.".to_string(),
            Action::DescribeWithLlm => "Generate AI description for this photo? This will send the image to your configured LLM.".to_string(),
            Action::BatchLlm => "Process all photos with AI? This will send all undescribed photos to your configured LLM.".to_string(),
            Action::DetectFaces => "Detect faces in photos? This will analyze images for face detection.".to_string(),
            Action::ClusterFaces => "Cluster similar faces? This will group detected faces by similarity.".to_string(),
            Action::ClipEmbedding => "Generate CLIP embeddings? This will create semantic embeddings for images in this directory.".to_string(),
            _ => format!("Execute {:?}?", action),
        };
        let has_prompt_field = matches!(action, Action::DescribeWithLlm | Action::BatchLlm);
        let prompt_text = initial_prompt.clone().unwrap_or_default();
        let prompt_cursor = prompt_text.len();
        let original_prompt = initial_prompt.unwrap_or_default();
        let focus = if has_prompt_field {
            ConfirmFocus::PromptField
        } else {
            ConfirmFocus::Buttons
        };
        Self { action, message, has_prompt_field, prompt_text, prompt_cursor, focus, original_prompt }
    }

    pub fn prompt_modified(&self) -> bool {
        self.prompt_text != self.original_prompt
    }

    pub fn toggle_focus(&mut self) {
        if self.has_prompt_field {
            self.focus = match self.focus {
                ConfirmFocus::PromptField => ConfirmFocus::Buttons,
                ConfirmFocus::Buttons => ConfirmFocus::PromptField,
            };
        }
    }

    // Text editing methods for prompt field
    pub fn handle_char(&mut self, c: char) {
        self.prompt_text.insert(self.prompt_cursor, c);
        self.prompt_cursor += 1;
    }

    pub fn backspace(&mut self) {
        if self.prompt_cursor > 0 {
            self.prompt_cursor -= 1;
            self.prompt_text.remove(self.prompt_cursor);
        }
    }

    pub fn delete(&mut self) {
        if self.prompt_cursor < self.prompt_text.len() {
            self.prompt_text.remove(self.prompt_cursor);
        }
    }

    pub fn move_cursor_left(&mut self) {
        if self.prompt_cursor > 0 {
            self.prompt_cursor -= 1;
        }
    }

    pub fn move_cursor_right(&mut self) {
        if self.prompt_cursor < self.prompt_text.len() {
            self.prompt_cursor += 1;
        }
    }

    pub fn move_cursor_home(&mut self) {
        self.prompt_cursor = 0;
    }

    pub fn move_cursor_end(&mut self) {
        self.prompt_cursor = self.prompt_text.len();
    }
}

pub fn render(frame: &mut Frame, dialog: &ConfirmDialog, area: Rect) {
    let dialog_width = 60.min(area.width.saturating_sub(4));
    let dialog_height = if dialog.has_prompt_field { 15 } else { 9 };

    let x = (area.width - dialog_width) / 2;
    let y = (area.height - dialog_height) / 2;

    let dialog_area = Rect::new(x, y, dialog_width, dialog_height);

    frame.render_widget(Clear, dialog_area);

    if dialog.has_prompt_field {
        // Layout: message + prompt label + prompt input + help + buttons
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Message
                Constraint::Length(1), // Prompt label
                Constraint::Length(3), // Prompt input
                Constraint::Length(1), // Help text
                Constraint::Length(3), // Buttons
            ])
            .margin(1)
            .split(dialog_area);

        // Outer border
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow))
            .title(" Confirm Task ");
        frame.render_widget(block, dialog_area);

        // Message
        let message = Paragraph::new(dialog.message.as_str())
            .wrap(ratatui::widgets::Wrap { trim: true })
            .alignment(Alignment::Center);
        frame.render_widget(message, chunks[0]);

        // Prompt label
        let label = Paragraph::new("LLM Prompt (per-folder):")
            .style(Style::default().fg(Color::Cyan));
        frame.render_widget(label, chunks[1]);

        // Prompt input field
        let input_style = if dialog.focus == ConfirmFocus::PromptField {
            Style::default().fg(Color::White).bg(Color::DarkGray)
        } else {
            Style::default().fg(Color::Gray)
        };
        let input_block = Block::default()
            .borders(Borders::ALL)
            .border_style(if dialog.focus == ConfirmFocus::PromptField {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::DarkGray)
            });

        // Show the prompt text with cursor
        let inner = input_block.inner(chunks[2]);
        frame.render_widget(input_block, chunks[2]);

        // Visible portion of text that fits in the input area
        let available_width = inner.width as usize;
        let text = &dialog.prompt_text;
        let cursor = dialog.prompt_cursor;

        // Calculate scroll offset so cursor is always visible
        let scroll_offset = if cursor >= available_width {
            cursor - available_width + 1
        } else {
            0
        };
        let visible_end = (scroll_offset + available_width).min(text.len());
        let visible_text = if scroll_offset < text.len() {
            &text[scroll_offset..visible_end]
        } else {
            ""
        };

        let input = Paragraph::new(visible_text).style(input_style);
        frame.render_widget(input, inner);

        // Show cursor when focused
        if dialog.focus == ConfirmFocus::PromptField {
            let cursor_x = inner.x + (cursor - scroll_offset) as u16;
            let cursor_y = inner.y;
            frame.set_cursor_position(Position::new(cursor_x, cursor_y));
        }

        // Help text
        let help = Paragraph::new("Tab: switch focus")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center);
        frame.render_widget(help, chunks[3]);

        // Button hints
        let button_style = if dialog.focus == ConfirmFocus::Buttons {
            Style::default().add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let buttons = Line::from(vec![
            Span::styled("  [Enter/y] ", button_style.fg(Color::Green)),
            Span::styled("Yes", button_style),
            Span::raw("    "),
            Span::styled("[Esc/n] ", button_style.fg(Color::Red)),
            Span::styled("No", button_style),
        ]);
        let button_widget = Paragraph::new(buttons).alignment(Alignment::Center);
        frame.render_widget(button_widget, chunks[4]);
    } else {
        // Original layout for non-prompt dialogs
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(3),    // Message
                Constraint::Length(3), // Buttons
            ])
            .margin(1)
            .split(dialog_area);

        // Outer border
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow))
            .title(" Confirm Task ");
        frame.render_widget(block, dialog_area);

        // Message
        let message = Paragraph::new(dialog.message.as_str())
            .wrap(ratatui::widgets::Wrap { trim: true })
            .alignment(Alignment::Center);
        frame.render_widget(message, chunks[0]);

        // Button hints
        let buttons = Line::from(vec![
            Span::styled("  [Enter/y] ", Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
            Span::raw("Yes"),
            Span::raw("    "),
            Span::styled("[Esc/n] ", Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)),
            Span::raw("No"),
        ]);
        let button_widget = Paragraph::new(buttons).alignment(Alignment::Center);
        frame.render_widget(button_widget, chunks[1]);
    }
}
