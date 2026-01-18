use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};

use crate::db::trash::TrashedPhoto;

/// State for the trash viewing dialog
pub struct TrashDialog {
    /// List of trashed photos
    pub entries: Vec<TrashedPhoto>,
    /// Selected index
    pub selected_index: usize,
    /// Total trash size in bytes
    pub total_size: u64,
    /// Max allowed trash size in bytes
    pub max_size: u64,
}

impl TrashDialog {
    pub fn new(entries: Vec<TrashedPhoto>, total_size: u64, max_size: u64) -> Self {
        Self {
            entries,
            selected_index: 0,
            total_size,
            max_size,
        }
    }

    pub fn move_down(&mut self) {
        if !self.entries.is_empty() && self.selected_index < self.entries.len() - 1 {
            self.selected_index += 1;
        }
    }

    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn selected_entry(&self) -> Option<&TrashedPhoto> {
        self.entries.get(self.selected_index)
    }

    pub fn refresh(&mut self, entries: Vec<TrashedPhoto>, total_size: u64) {
        self.entries = entries;
        self.total_size = total_size;
        // Adjust selected index if needed
        if self.selected_index >= self.entries.len() && !self.entries.is_empty() {
            self.selected_index = self.entries.len() - 1;
        }
    }
}

pub fn render(frame: &mut Frame, dialog: &TrashDialog, area: Rect) {
    // Center the dialog
    let dialog_width = 80.min(area.width.saturating_sub(4));
    let dialog_height = 24.min(area.height.saturating_sub(4));

    let x = (area.width - dialog_width) / 2;
    let y = (area.height - dialog_height) / 2;

    let dialog_area = Rect::new(x, y, dialog_width, dialog_height);

    // Clear background
    frame.render_widget(Clear, dialog_area);

    // Split into list and help areas
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header with stats
            Constraint::Min(0),     // File list
            Constraint::Length(4),  // Help text
        ])
        .split(dialog_area);

    // Header with trash statistics
    let size_text = format_size(dialog.total_size);
    let max_text = format_size(dialog.max_size);
    let usage_pct = if dialog.max_size > 0 {
        (dialog.total_size as f64 / dialog.max_size as f64 * 100.0) as u32
    } else {
        0
    };

    let header_text = format!(
        " {} files | {} / {} ({}%)",
        dialog.entries.len(),
        size_text,
        max_text,
        usage_pct
    );

    let header = Paragraph::new(header_text)
        .style(Style::default().fg(Color::Yellow))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow))
                .title(" Trash "),
        );
    frame.render_widget(header, chunks[0]);

    // File list
    if dialog.entries.is_empty() {
        let empty_msg = Paragraph::new("  Trash is empty")
            .style(Style::default().fg(Color::DarkGray))
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(empty_msg, chunks[1]);
    } else {
        let items: Vec<ListItem> = dialog
            .entries
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let marker = if i == dialog.selected_index { ">" } else { " " };
                let size = format_size(entry.size_bytes as u64);
                let date = format_date(&entry.trashed_at);

                let style = if i == dialog.selected_index {
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                ListItem::new(format!(
                    "{} {} | {} | {}",
                    marker, entry.filename, size, date
                ))
                .style(style)
            })
            .collect();

        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Trashed Files "),
        );

        let mut state = ListState::default();
        state.select(Some(dialog.selected_index));
        frame.render_stateful_widget(list, chunks[1], &mut state);
    }

    // Help text
    let help_text = vec![
        Line::from(Span::styled(
            "  j/k=Navigate  Enter/r=Restore  d=Delete permanently  c=Cleanup old  q=Close",
            Style::default().fg(Color::DarkGray),
        )),
        Line::from(""),
        if let Some(entry) = dialog.selected_entry() {
            Line::from(Span::styled(
                format!("  Original: {}", entry.original_path),
                Style::default().fg(Color::Blue),
            ))
        } else {
            Line::from("")
        },
    ];

    let help = Paragraph::new(help_text)
        .block(Block::default().borders(Borders::TOP));
    frame.render_widget(help, chunks[2]);
}

fn format_size(size: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if size >= GB {
        format!("{:.1}G", size as f64 / GB as f64)
    } else if size >= MB {
        format!("{:.1}M", size as f64 / MB as f64)
    } else if size >= KB {
        format!("{:.1}K", size as f64 / KB as f64)
    } else {
        format!("{}B", size)
    }
}

fn format_date(date_str: &str) -> String {
    // Just extract the date part from ISO format
    if date_str.len() >= 10 {
        date_str[..10].to_string()
    } else {
        date_str.to_string()
    }
}
