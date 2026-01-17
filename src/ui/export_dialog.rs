use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};
use std::path::PathBuf;

use crate::export::ExportFormat;

/// State for the export dialog
pub struct ExportDialog {
    /// Selected format
    pub format: ExportFormat,
    /// Output path
    pub output_path: PathBuf,
    /// Available formats
    formats: Vec<ExportFormat>,
    /// Selected format index
    selected_index: usize,
}

impl ExportDialog {
    pub fn new(default_dir: PathBuf) -> Self {
        let formats = vec![ExportFormat::Json, ExportFormat::Csv, ExportFormat::Html];

        Self {
            format: ExportFormat::Json,
            output_path: default_dir.join("clepho_export.json"),
            formats,
            selected_index: 0,
        }
    }

    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
            self.update_format();
        }
    }

    pub fn move_down(&mut self) {
        if self.selected_index < self.formats.len() - 1 {
            self.selected_index += 1;
            self.update_format();
        }
    }

    fn update_format(&mut self) {
        self.format = self.formats[self.selected_index];
        // Update output path extension
        let stem = self.output_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "clepho_export".to_string());

        if let Some(parent) = self.output_path.parent() {
            self.output_path = parent.join(format!("{}.{}", stem, self.format.extension()));
        }
    }

    pub fn selected_format(&self) -> ExportFormat {
        self.format
    }

    pub fn output_path(&self) -> &PathBuf {
        &self.output_path
    }
}

pub fn render(frame: &mut Frame, dialog: &ExportDialog, area: Rect) {
    // Center the dialog
    let dialog_width = 60.min(area.width.saturating_sub(4));
    let dialog_height = 15.min(area.height.saturating_sub(4));

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
            Constraint::Length(2), // Header
            Constraint::Length(5), // Format selection
            Constraint::Length(3), // Output path
            Constraint::Length(2), // Footer
        ])
        .split(dialog_area);

    // Draw border
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green))
        .title(" Export Photos ")
        .title_style(Style::default().add_modifier(Modifier::BOLD));
    frame.render_widget(block, dialog_area);

    // Header
    let header = Paragraph::new("Select export format:")
        .style(Style::default().fg(Color::Green));
    frame.render_widget(header, chunks[0]);

    // Format selection
    let items: Vec<ListItem> = dialog
        .formats
        .iter()
        .map(|f| {
            let desc = match f {
                ExportFormat::Json => "JSON - Full metadata export",
                ExportFormat::Csv => "CSV  - Spreadsheet compatible",
                ExportFormat::Html => "HTML - Visual gallery report",
            };
            ListItem::new(desc)
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title(" Format "))
        .highlight_style(
            Style::default()
                .bg(Color::Green)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        );

    let mut state = ListState::default();
    state.select(Some(dialog.selected_index));
    frame.render_stateful_widget(list, chunks[1], &mut state);

    // Output path
    let output = Paragraph::new(format!("Output: {}", dialog.output_path.display()))
        .style(Style::default().fg(Color::DarkGray))
        .block(Block::default().borders(Borders::ALL).title(" Output File "));
    frame.render_widget(output, chunks[2]);

    // Footer
    let footer = Paragraph::new("j/k: select | Enter: export | Esc: cancel")
        .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(footer, chunks[3]);
}
