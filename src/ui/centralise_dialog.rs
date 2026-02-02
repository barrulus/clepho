//! Dialog for centralising files into a managed library.

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
};
use std::path::PathBuf;

use crate::centralise::{CentralisePreview, CentraliseResult, PlannedOperation};
use crate::config::CentraliseOperation;

/// Dialog state for file centralisation
pub struct CentraliseDialog {
    /// Library root path
    pub library_path: PathBuf,
    /// Operation mode (copy or move)
    pub operation: CentraliseOperation,
    /// Preview of planned operations
    pub preview: Option<CentralisePreview>,
    /// Result after execution
    pub result: Option<CentraliseResult>,
    /// Currently selected item in preview list
    pub selected_index: usize,
    /// Scroll offset for the list (reserved for future scrolling implementation)
    pub _scroll_offset: usize,
    /// Current mode
    pub mode: CentraliseDialogMode,
    /// Source files to centralise
    pub source_files: Vec<PathBuf>,
    /// Error message if any
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CentraliseDialogMode {
    /// Configuring options before preview
    Configure,
    /// Showing preview of operations
    Preview,
    /// Executing operations
    Executing,
    /// Showing results
    Results,
}

impl CentraliseDialog {
    pub fn new(library_path: PathBuf, operation: CentraliseOperation, source_files: Vec<PathBuf>) -> Self {
        Self {
            library_path,
            operation,
            preview: None,
            result: None,
            selected_index: 0,
            _scroll_offset: 0,
            mode: CentraliseDialogMode::Configure,
            source_files,
            error: None,
        }
    }

    /// Toggle between copy and move operation
    pub fn toggle_operation(&mut self) {
        self.operation = match self.operation {
            CentraliseOperation::Copy => CentraliseOperation::Move,
            CentraliseOperation::Move => CentraliseOperation::Copy,
        };
    }

    /// Move selection down in the preview list
    pub fn move_down(&mut self) {
        let max_idx = self.preview.as_ref()
            .map(|p| p.operations.len() + p.skipped.len())
            .unwrap_or(0);
        if self.selected_index < max_idx.saturating_sub(1) {
            self.selected_index += 1;
        }
    }

    /// Move selection up in the preview list
    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    /// Page down in the list
    pub fn page_down(&mut self, visible_rows: usize) {
        let max_idx = self.preview.as_ref()
            .map(|p| p.operations.len() + p.skipped.len())
            .unwrap_or(0);
        self.selected_index = (self.selected_index + visible_rows).min(max_idx.saturating_sub(1));
    }

    /// Page up in the list
    pub fn page_up(&mut self, visible_rows: usize) {
        self.selected_index = self.selected_index.saturating_sub(visible_rows);
    }

    /// Get the currently selected planned operation
    #[allow(dead_code)]
    pub fn selected_operation(&self) -> Option<&PlannedOperation> {
        self.preview.as_ref()?.operations.get(self.selected_index)
    }
}

/// Render the centralise dialog
pub fn render(frame: &mut Frame, dialog: &CentraliseDialog, area: Rect) {
    let dialog_width = 80.min(area.width.saturating_sub(4));
    let dialog_height = 30.min(area.height.saturating_sub(4));

    let x = (area.width - dialog_width) / 2;
    let y = (area.height - dialog_height) / 2;

    let dialog_area = Rect::new(x, y, dialog_width, dialog_height);

    frame.render_widget(Clear, dialog_area);

    match dialog.mode {
        CentraliseDialogMode::Configure => render_configure(frame, dialog, dialog_area),
        CentraliseDialogMode::Preview => render_preview(frame, dialog, dialog_area),
        CentraliseDialogMode::Executing => render_executing(frame, dialog, dialog_area),
        CentraliseDialogMode::Results => render_results(frame, dialog, dialog_area),
    }
}

fn render_configure(frame: &mut Frame, dialog: &CentraliseDialog, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Centralise Files ");
    frame.render_widget(block, area);

    let inner = Rect::new(
        area.x + 2,
        area.y + 1,
        area.width.saturating_sub(4),
        area.height.saturating_sub(2),
    );

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Title
            Constraint::Length(3),  // Library path
            Constraint::Length(3),  // Operation mode
            Constraint::Length(3),  // File count
            Constraint::Min(4),     // Spacer
            Constraint::Length(2),  // Error
            Constraint::Length(2),  // Help
        ])
        .split(inner);

    // Title
    let title = Paragraph::new("Organize photos into a managed library")
        .style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center);
    frame.render_widget(title, chunks[0]);

    // Library path
    let lib_text = format!("Library: {}", dialog.library_path.display());
    let lib_para = Paragraph::new(lib_text)
        .style(Style::default().fg(Color::Yellow));
    frame.render_widget(lib_para, chunks[1]);

    // Operation mode
    let op_text = match dialog.operation {
        CentraliseOperation::Copy => "[C] Operation: COPY (keeps originals)",
        CentraliseOperation::Move => "[C] Operation: MOVE (removes originals)",
    };
    let op_para = Paragraph::new(op_text)
        .style(Style::default().fg(Color::Cyan));
    frame.render_widget(op_para, chunks[2]);

    // File count
    let count_text = format!("Files to process: {}", dialog.source_files.len());
    let count_para = Paragraph::new(count_text)
        .style(Style::default().fg(Color::White));
    frame.render_widget(count_para, chunks[3]);

    // Error message
    if let Some(ref err) = dialog.error {
        let err_para = Paragraph::new(format!("Error: {}", err))
            .style(Style::default().fg(Color::Red));
        frame.render_widget(err_para, chunks[5]);
    }

    // Help text
    let help = Paragraph::new("Enter: Preview | c: Toggle Copy/Move | Esc: Cancel")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    frame.render_widget(help, chunks[6]);
}

fn render_preview(frame: &mut Frame, dialog: &CentraliseDialog, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Green))
        .title(" Preview - Dry Run ");
    frame.render_widget(block, area);

    let inner = Rect::new(
        area.x + 1,
        area.y + 1,
        area.width.saturating_sub(2),
        area.height.saturating_sub(2),
    );

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),  // Summary
            Constraint::Min(10),    // File list
            Constraint::Length(3),  // Selected item detail
            Constraint::Length(2),  // Help
        ])
        .split(inner);

    // Summary
    if let Some(ref preview) = dialog.preview {
        let op_str = match dialog.operation {
            CentraliseOperation::Copy => "copy",
            CentraliseOperation::Move => "move",
        };
        let summary = format!(
            "Will {} {} files ({:.2} MB) | {} skipped",
            op_str,
            preview.operations.len(),
            preview.total_bytes as f64 / (1024.0 * 1024.0),
            preview.skipped.len()
        );
        let summary_para = Paragraph::new(summary)
            .style(Style::default().fg(Color::Yellow));
        frame.render_widget(summary_para, chunks[0]);

        // File list
        let list_height = chunks[1].height.saturating_sub(2) as usize;
        let total_items = preview.operations.len() + preview.skipped.len();

        let mut items: Vec<ListItem> = Vec::new();

        // Operations
        for (i, op) in preview.operations.iter().enumerate() {
            let style = if i == dialog.selected_index {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Green)
            };

            let src_name = op.source.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let dest_name = op.destination.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            let text = format!("  {} -> {}", src_name, dest_name);
            items.push(ListItem::new(text).style(style));
        }

        // Skipped
        for (i, (path, reason)) in preview.skipped.iter().enumerate() {
            let idx = preview.operations.len() + i;
            let style = if idx == dialog.selected_index {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            let name = path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let text = format!("  [SKIP] {} - {}", name, reason);
            items.push(ListItem::new(text).style(style));
        }

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title(" Files "));

        let mut state = ListState::default();
        state.select(Some(dialog.selected_index));
        frame.render_stateful_widget(list, chunks[1], &mut state);

        // Scrollbar
        if total_items > list_height {
            let scrollbar = Scrollbar::default()
                .orientation(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("▲"))
                .end_symbol(Some("▼"));
            let mut scrollbar_state = ScrollbarState::new(total_items)
                .position(dialog.selected_index);
            frame.render_stateful_widget(
                scrollbar,
                chunks[1].inner(Margin { vertical: 1, horizontal: 0 }),
                &mut scrollbar_state,
            );
        }

        // Selected item detail
        if let Some(op) = preview.operations.get(dialog.selected_index) {
            let detail = format!(
                "From: {}\nTo:   {}",
                op.source.display(),
                op.destination.display()
            );
            let detail_para = Paragraph::new(detail)
                .style(Style::default().fg(Color::White))
                .block(Block::default().borders(Borders::TOP));
            frame.render_widget(detail_para, chunks[2]);
        }
    }

    // Help text
    let help = Paragraph::new("Enter: Execute | j/k: Navigate | Esc: Back")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    frame.render_widget(help, chunks[3]);
}

fn render_executing(frame: &mut Frame, _dialog: &CentraliseDialog, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(" Executing... ");
    frame.render_widget(block, area);

    let inner = Rect::new(
        area.x + 2,
        area.y + area.height / 2,
        area.width.saturating_sub(4),
        3,
    );

    let text = Paragraph::new("Processing files...")
        .style(Style::default().fg(Color::Yellow))
        .alignment(Alignment::Center);
    frame.render_widget(text, inner);
}

fn render_results(frame: &mut Frame, dialog: &CentraliseDialog, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Results ");
    frame.render_widget(block, area);

    let inner = Rect::new(
        area.x + 2,
        area.y + 1,
        area.width.saturating_sub(4),
        area.height.saturating_sub(2),
    );

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),  // Summary
            Constraint::Min(8),     // Details
            Constraint::Length(2),  // Help
        ])
        .split(inner);

    if let Some(ref result) = dialog.result {
        // Summary
        let summary = format!(
            "Succeeded: {} | Failed: {} | Skipped: {}",
            result.succeeded.len(),
            result.failed.len(),
            result.skipped.len()
        );
        let color = if result.failed.is_empty() {
            Color::Green
        } else {
            Color::Yellow
        };
        let summary_para = Paragraph::new(summary)
            .style(Style::default().fg(color).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center);
        frame.render_widget(summary_para, chunks[0]);

        // Details - show failures if any
        let mut lines = Vec::new();

        if !result.failed.is_empty() {
            lines.push(Line::from(Span::styled(
                "Failed:",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            )));
            for (path, err) in &result.failed {
                let name = path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                lines.push(Line::from(Span::styled(
                    format!("  {} - {}", name, err),
                    Style::default().fg(Color::Red),
                )));
            }
            lines.push(Line::from(""));
        }

        if !result.succeeded.is_empty() && result.succeeded.len() <= 10 {
            lines.push(Line::from(Span::styled(
                "Succeeded:",
                Style::default().fg(Color::Green),
            )));
            for op in &result.succeeded {
                let name = op.destination.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                lines.push(Line::from(Span::styled(
                    format!("  {}", name),
                    Style::default().fg(Color::Green),
                )));
            }
        }

        let details = Paragraph::new(lines)
            .block(Block::default().borders(Borders::TOP));
        frame.render_widget(details, chunks[1]);
    }

    // Help text
    let help = Paragraph::new("Enter/Esc: Close")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    frame.render_widget(help, chunks[2]);
}
