//! Changes dialog for displaying detected file changes.

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Tabs},
};
use std::collections::HashSet;
use std::path::PathBuf;

use crate::scanner::ChangeDetectionResult;

/// Tab selection for the changes dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangesTab {
    New,
    Modified,
}

/// State for the changes dialog.
pub struct ChangesDialog {
    /// The detected changes.
    pub changes: ChangeDetectionResult,
    /// Current tab.
    pub tab: ChangesTab,
    /// Selected index in current tab's file list.
    pub selected_index: usize,
    /// Selected files for rescanning.
    pub selected_files: HashSet<PathBuf>,
}

impl ChangesDialog {
    pub fn new(changes: ChangeDetectionResult) -> Self {
        Self {
            changes,
            tab: ChangesTab::New,
            selected_index: 0,
            selected_files: HashSet::new(),
        }
    }

    /// Get the current file list based on selected tab.
    fn current_files(&self) -> &[PathBuf] {
        match self.tab {
            ChangesTab::New => &self.changes.new_files,
            ChangesTab::Modified => &self.changes.modified_files,
        }
    }

    /// Move selection down.
    pub fn move_down(&mut self) {
        let files = self.current_files();
        if !files.is_empty() && self.selected_index < files.len() - 1 {
            self.selected_index += 1;
        }
    }

    /// Move selection up.
    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    /// Switch between tabs.
    pub fn switch_tab(&mut self) {
        self.tab = match self.tab {
            ChangesTab::New => ChangesTab::Modified,
            ChangesTab::Modified => ChangesTab::New,
        };
        self.selected_index = 0;
    }

    /// Toggle selection of current file.
    pub fn toggle_selection(&mut self) {
        let path = match self.tab {
            ChangesTab::New => self.changes.new_files.get(self.selected_index).cloned(),
            ChangesTab::Modified => self.changes.modified_files.get(self.selected_index).cloned(),
        };

        if let Some(path) = path {
            if self.selected_files.contains(&path) {
                self.selected_files.remove(&path);
            } else {
                self.selected_files.insert(path);
            }
        }
    }

    /// Select all files in both tabs.
    pub fn select_all(&mut self) {
        for path in &self.changes.new_files {
            self.selected_files.insert(path.clone());
        }
        for path in &self.changes.modified_files {
            self.selected_files.insert(path.clone());
        }
    }

    /// Get all selected files (or all files if none selected).
    pub fn files_to_rescan(&self) -> Vec<PathBuf> {
        if self.selected_files.is_empty() {
            // Return all changed files
            let mut all = self.changes.new_files.clone();
            all.extend(self.changes.modified_files.clone());
            all
        } else {
            self.selected_files.iter().cloned().collect()
        }
    }

    /// Check if a path is selected.
    pub fn is_selected(&self, path: &PathBuf) -> bool {
        self.selected_files.contains(path)
    }

    /// Get count of selected files.
    pub fn selection_count(&self) -> usize {
        self.selected_files.len()
    }
}

pub fn render(frame: &mut Frame, dialog: &ChangesDialog, area: Rect) {
    // Center the dialog
    let dialog_width = 70.min(area.width.saturating_sub(4));
    let dialog_height = 20.min(area.height.saturating_sub(4));

    let x = (area.width - dialog_width) / 2;
    let y = (area.height - dialog_height) / 2;

    let dialog_area = Rect::new(x, y, dialog_width, dialog_height);

    // Clear background
    frame.render_widget(Clear, dialog_area);

    // Main layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Tabs
            Constraint::Min(0),    // File list
            Constraint::Length(3), // Help text
        ])
        .split(dialog_area);

    // Tab bar
    let new_count = dialog.changes.new_files.len();
    let mod_count = dialog.changes.modified_files.len();

    let tab_titles = vec![
        Line::from(format!(" New ({}) ", new_count)),
        Line::from(format!(" Modified ({}) ", mod_count)),
    ];

    let selected_tab = match dialog.tab {
        ChangesTab::New => 0,
        ChangesTab::Modified => 1,
    };

    let tabs = Tabs::new(tab_titles)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow))
                .title(" File Changes "),
        )
        .select(selected_tab)
        .style(Style::default().fg(Color::White))
        .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));

    frame.render_widget(tabs, chunks[0]);

    // File list
    let files = match dialog.tab {
        ChangesTab::New => &dialog.changes.new_files,
        ChangesTab::Modified => &dialog.changes.modified_files,
    };

    if files.is_empty() {
        let empty_msg = Paragraph::new("  No files in this category")
            .style(Style::default().fg(Color::DarkGray))
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(empty_msg, chunks[1]);
    } else {
        let items: Vec<ListItem> = files
            .iter()
            .enumerate()
            .map(|(i, path)| {
                let filename = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();

                let selected = dialog.selected_files.contains(path);
                let marker = if selected { "[x]" } else { "[ ]" };

                let style = if i == dialog.selected_index {
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                } else if selected {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default()
                };

                ListItem::new(format!(" {} {}", marker, filename)).style(style)
            })
            .collect();

        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(match dialog.tab {
                    ChangesTab::New => " New Files ",
                    ChangesTab::Modified => " Modified Files ",
                }),
        );

        let mut state = ListState::default();
        state.select(Some(dialog.selected_index));
        frame.render_stateful_widget(list, chunks[1], &mut state);
    }

    // Help text
    let sel_count = dialog.selection_count();
    let sel_text = if sel_count > 0 {
        format!(" ({} selected)", sel_count)
    } else {
        String::new()
    };

    let help_text = format!(
        " Tab=switch  j/k=nav  Space=toggle  a=all  Enter=rescan{}  q=close",
        sel_text
    );

    let help = Paragraph::new(help_text)
        .style(Style::default().fg(Color::DarkGray))
        .block(Block::default().borders(Borders::TOP));

    frame.render_widget(help, chunks[2]);
}
