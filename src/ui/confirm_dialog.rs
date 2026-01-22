//! Confirmation dialog for expensive tasks.

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::config::Action;

/// Dialog state for confirming an expensive task before execution
pub struct ConfirmDialog {
    /// The action pending confirmation
    pub action: Action,
    /// Description shown to user
    pub message: String,
}

impl ConfirmDialog {
    pub fn new(action: Action) -> Self {
        let message = match action {
            Action::Scan => "Scan directory for photos? This will index all images in the current directory.".to_string(),
            Action::DescribeWithLlm => "Generate AI description for this photo? This will send the image to your configured LLM.".to_string(),
            Action::BatchLlm => "Process all photos with AI? This will send all undescribed photos to your configured LLM.".to_string(),
            Action::DetectFaces => "Detect faces in photos? This will analyze images for face detection.".to_string(),
            Action::ClusterFaces => "Cluster similar faces? This will group detected faces by similarity.".to_string(),
            Action::ClipEmbedding => "Generate CLIP embeddings? This will create semantic embeddings for images in this directory.".to_string(),
            _ => format!("Execute {:?}?", action),
        };
        Self { action, message }
    }
}

pub fn render(frame: &mut Frame, dialog: &ConfirmDialog, area: Rect) {
    let dialog_width = 60.min(area.width.saturating_sub(4));
    let dialog_height = 9;

    let x = (area.width - dialog_width) / 2;
    let y = (area.height - dialog_height) / 2;

    let dialog_area = Rect::new(x, y, dialog_width, dialog_height);

    frame.render_widget(Clear, dialog_area);

    // Layout: message area + button hints
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
