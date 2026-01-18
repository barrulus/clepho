use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

pub fn render_help(frame: &mut Frame, area: Rect) {
    // Center the help dialog
    let dialog_width = 60.min(area.width.saturating_sub(4));
    let dialog_height = 33.min(area.height.saturating_sub(4));

    let x = (area.width - dialog_width) / 2;
    let y = (area.height - dialog_height) / 2;

    let dialog_area = Rect::new(x, y, dialog_width, dialog_height);

    // Clear the area behind the dialog
    frame.render_widget(Clear, dialog_area);

    let help_text = vec![
        Line::from(Span::styled("Navigation", Style::default().add_modifier(Modifier::BOLD).fg(Color::Cyan))),
        Line::from(""),
        Line::from("  j / ↓      Move down"),
        Line::from("  k / ↑      Move up"),
        Line::from("  h / ← / ⌫  Go to parent directory"),
        Line::from("  l / → / ↵  Enter directory"),
        Line::from("  gg         Go to top"),
        Line::from("  G          Go to bottom"),
        Line::from("  Ctrl+d     Page down"),
        Line::from("  Ctrl+u     Page up"),
        Line::from("  ~          Go to home directory"),
        Line::from(""),
        Line::from(Span::styled("Selection", Style::default().add_modifier(Modifier::BOLD).fg(Color::Cyan))),
        Line::from(""),
        Line::from("  Space      Toggle file selection"),
        Line::from("  V          Enter visual mode (range select)"),
        Line::from("  Esc        Cancel running task / clear selection"),
        Line::from(""),
        Line::from(Span::styled("Actions", Style::default().add_modifier(Modifier::BOLD).fg(Color::Cyan))),
        Line::from(""),
        Line::from("  s          Scan current directory for photos"),
        Line::from("  d          Find duplicate photos"),
        Line::from("  D          Describe image with AI (LLM)"),
        Line::from("  P          Batch process all photos with AI"),
        Line::from("  F          Detect faces in photos"),
        Line::from("  C          Cluster similar faces together"),
        Line::from("  T          View/manage running tasks"),
        Line::from("  t          View/manage trash"),
        Line::from("  m          Move selected/current file(s)"),
        Line::from("  R          Rename selected/current file(s)"),
        Line::from("  E          Export photo database"),
        Line::from("  /          Semantic search photos"),
        Line::from("  p          Manage people/faces"),
        Line::from("  ?          Show this help"),
        Line::from("  q          Quit"),
        Line::from(""),
        Line::from(Span::styled("Press any key to close", Style::default().fg(Color::DarkGray))),
    ];

    let paragraph = Paragraph::new(help_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(" Help ")
                .title_style(Style::default().add_modifier(Modifier::BOLD)),
        )
        .wrap(Wrap { trim: false });

    frame.render_widget(paragraph, dialog_area);
}
