use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};

use crate::db::SearchResult;

/// State for the semantic search dialog
pub struct SearchDialog {
    /// Search query input
    pub query: String,
    /// Cursor position
    pub cursor: usize,
    /// Search results
    pub results: Vec<SearchResult>,
    /// Selected result index
    pub selected_index: usize,
    /// Status message
    pub status: Option<String>,
    /// Is currently searching
    pub searching: bool,
}

impl SearchDialog {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            cursor: 0,
            results: Vec::new(),
            selected_index: 0,
            status: None,
            searching: false,
        }
    }

    pub fn handle_char(&mut self, c: char) {
        self.query.insert(self.cursor, c);
        self.cursor += 1;
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.query.remove(self.cursor);
        }
    }

    pub fn move_cursor_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn move_cursor_right(&mut self) {
        if self.cursor < self.query.len() {
            self.cursor += 1;
        }
    }

    pub fn clear(&mut self) {
        self.query.clear();
        self.cursor = 0;
        self.results.clear();
        self.selected_index = 0;
    }

    pub fn move_selection_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn move_selection_down(&mut self) {
        if !self.results.is_empty() && self.selected_index < self.results.len() - 1 {
            self.selected_index += 1;
        }
    }

    pub fn set_results(&mut self, results: Vec<SearchResult>) {
        self.results = results;
        self.selected_index = 0;
        self.searching = false;
        if self.results.is_empty() {
            self.status = Some("No results found".to_string());
        } else {
            self.status = Some(format!("Found {} results", self.results.len()));
        }
    }

    pub fn selected_result(&self) -> Option<&SearchResult> {
        self.results.get(self.selected_index)
    }
}

impl Default for SearchDialog {
    fn default() -> Self {
        Self::new()
    }
}

pub fn render(frame: &mut Frame, dialog: &SearchDialog, area: Rect) {
    // Center the dialog
    let dialog_width = 80.min(area.width.saturating_sub(4));
    let dialog_height = 25.min(area.height.saturating_sub(4));

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
            Constraint::Length(3), // Search input
            Constraint::Min(10),   // Results list
            Constraint::Length(2), // Status
            Constraint::Length(2), // Footer
        ])
        .split(dialog_area);

    // Draw border
    let title = if dialog.searching {
        " Semantic Search (searching...) "
    } else {
        " Semantic Search "
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(title)
        .title_style(Style::default().add_modifier(Modifier::BOLD));
    frame.render_widget(block, dialog_area);

    // Search input
    let input_text = format!(
        "{}|{}",
        &dialog.query[..dialog.cursor],
        &dialog.query[dialog.cursor..]
    );
    let input = Paragraph::new(input_text)
        .style(Style::default().fg(Color::Yellow))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Query ")
                .border_style(Style::default().fg(Color::Yellow)),
        );
    frame.render_widget(input, chunks[0]);

    // Results list
    let items: Vec<ListItem> = dialog
        .results
        .iter()
        .map(|result| {
            let similarity_pct = (result.similarity * 100.0) as u32;
            let desc = result
                .description
                .as_ref()
                .map(|d| {
                    if d.len() > 50 {
                        format!("{}...", &d[..50])
                    } else {
                        d.clone()
                    }
                })
                .unwrap_or_else(|| "(no description)".to_string());

            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(
                        format!("[{}%] ", similarity_pct),
                        Style::default().fg(Color::Green),
                    ),
                    Span::styled(&result.filename, Style::default().fg(Color::White)),
                ]),
                Line::from(Span::styled(
                    format!("  {}", desc),
                    Style::default().fg(Color::DarkGray),
                )),
            ])
        })
        .collect();

    let results_title = if dialog.results.is_empty() {
        " Results ".to_string()
    } else {
        format!(" Results ({}) ", dialog.results.len())
    };

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(results_title)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Cyan)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        );

    let mut state = ListState::default();
    if !dialog.results.is_empty() {
        state.select(Some(dialog.selected_index));
    }
    frame.render_stateful_widget(list, chunks[1], &mut state);

    // Status
    let status_text = dialog.status.as_deref().unwrap_or("");
    let status = Paragraph::new(status_text).style(Style::default().fg(Color::DarkGray));
    frame.render_widget(status, chunks[2]);

    // Footer
    let footer = Paragraph::new(
        "Enter: search | ↑↓: select | Ctrl+O: open | Esc: close",
    )
    .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(footer, chunks[3]);
}
