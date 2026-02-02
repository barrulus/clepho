//! Dialog for managing tags on photos.

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};
use std::path::PathBuf;

use crate::db::UserTag;

/// Dialog state for tagging a photo
pub struct TagDialog {
    /// Path of the photo being tagged
    pub photo_path: PathBuf,
    /// Photo ID in database
    pub photo_id: i64,
    /// Current tags on this photo
    pub current_tags: Vec<UserTag>,
    /// All available tags
    pub all_tags: Vec<UserTag>,
    /// Suggestions based on input
    pub suggestions: Vec<UserTag>,
    /// Input text for new tag
    pub input: String,
    /// Selected index in the tag list (current tags view)
    pub selected_index: usize,
    /// Mode: viewing current tags or adding new
    pub mode: TagDialogMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagDialogMode {
    /// Viewing current tags, can delete
    ViewTags,
    /// Adding new tag with input
    AddTag,
}

impl TagDialog {
    pub fn new(photo_path: PathBuf, photo_id: i64, current_tags: Vec<UserTag>, all_tags: Vec<UserTag>) -> Self {
        Self {
            photo_path,
            photo_id,
            current_tags,
            all_tags,
            suggestions: Vec::new(),
            input: String::new(),
            selected_index: 0,
            mode: TagDialogMode::ViewTags,
        }
    }

    /// Handle character input for tag name
    pub fn handle_char(&mut self, c: char) {
        self.input.push(c);
        self.update_suggestions();
    }

    /// Handle backspace
    pub fn backspace(&mut self) {
        self.input.pop();
        self.update_suggestions();
    }

    /// Update suggestions based on current input
    pub fn update_suggestions(&mut self) {
        if self.input.is_empty() {
            self.suggestions = self.all_tags.clone();
        } else {
            let lower = self.input.to_lowercase();
            self.suggestions = self.all_tags
                .iter()
                .filter(|t| t.name.to_lowercase().contains(&lower))
                .cloned()
                .collect();
        }
        // Reset selection
        self.selected_index = 0;
    }

    /// Get the currently selected tag in add mode
    pub fn selected_suggestion(&self) -> Option<&UserTag> {
        self.suggestions.get(self.selected_index)
    }

    /// Get the currently selected tag in view mode
    pub fn selected_current_tag(&self) -> Option<&UserTag> {
        self.current_tags.get(self.selected_index)
    }

    /// Move selection down
    pub fn move_down(&mut self) {
        let max_idx = match self.mode {
            TagDialogMode::ViewTags => self.current_tags.len(),
            TagDialogMode::AddTag => self.suggestions.len(),
        };
        if self.selected_index < max_idx.saturating_sub(1) {
            self.selected_index += 1;
        }
    }

    /// Move selection up
    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    /// Switch to add mode
    pub fn enter_add_mode(&mut self) {
        self.mode = TagDialogMode::AddTag;
        self.input.clear();
        self.update_suggestions();
    }

    /// Switch to view mode
    pub fn enter_view_mode(&mut self) {
        self.mode = TagDialogMode::ViewTags;
        self.selected_index = 0;
    }

    /// Clear input
    #[allow(dead_code)]
    pub fn clear_input(&mut self) {
        self.input.clear();
        self.update_suggestions();
    }
}

pub fn render(frame: &mut Frame, dialog: &TagDialog, area: Rect) {
    let dialog_width = 50.min(area.width.saturating_sub(4));
    let dialog_height = 20.min(area.height.saturating_sub(4));

    let x = (area.width - dialog_width) / 2;
    let y = (area.height - dialog_height) / 2;

    let dialog_area = Rect::new(x, y, dialog_width, dialog_height);

    frame.render_widget(Clear, dialog_area);

    // Outer block
    let title = format!(" Tags: {} ", dialog.photo_path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default());
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(title);
    frame.render_widget(block, dialog_area);

    // Inner layout
    let inner = Rect::new(
        dialog_area.x + 1,
        dialog_area.y + 1,
        dialog_area.width.saturating_sub(2),
        dialog_area.height.saturating_sub(2),
    );

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Input / mode indicator
            Constraint::Min(8),     // Tag list
            Constraint::Length(2),  // Help
        ])
        .split(inner);

    match dialog.mode {
        TagDialogMode::ViewTags => render_view_mode(frame, dialog, chunks),
        TagDialogMode::AddTag => render_add_mode(frame, dialog, chunks),
    }
}

fn render_view_mode(frame: &mut Frame, dialog: &TagDialog, chunks: std::rc::Rc<[Rect]>) {
    // Mode indicator
    let mode_text = Paragraph::new("Current tags (a=add, d=delete, Esc=close)")
        .style(Style::default().fg(Color::Yellow));
    frame.render_widget(mode_text, chunks[0]);

    // Current tags list
    if dialog.current_tags.is_empty() {
        let empty = Paragraph::new("No tags assigned")
            .style(Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC))
            .block(Block::default().borders(Borders::ALL).title(" Tags "));
        frame.render_widget(empty, chunks[1]);
    } else {
        let items: Vec<ListItem> = dialog.current_tags
            .iter()
            .enumerate()
            .map(|(i, tag)| {
                let style = if i == dialog.selected_index {
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(format!("  {} ", tag.name)).style(style)
            })
            .collect();

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title(" Tags "))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

        let mut state = ListState::default();
        state.select(Some(dialog.selected_index));
        frame.render_stateful_widget(list, chunks[1], &mut state);
    }

    // Help text
    let help = Paragraph::new("j/k:navigate | a:add tag | d:remove tag | Esc:close")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    frame.render_widget(help, chunks[2]);
}

fn render_add_mode(frame: &mut Frame, dialog: &TagDialog, chunks: std::rc::Rc<[Rect]>) {
    // Input field (placeholder text computed but using dialog.input directly below)
    let _input_text = if dialog.input.is_empty() {
        "Type tag name (Enter=select/create, Esc=cancel)"
    } else {
        &dialog.input
    };
    let input_style = if dialog.input.is_empty() {
        Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC)
    } else {
        Style::default().fg(Color::White)
    };
    let input = Paragraph::new(format!("> {}_", if dialog.input.is_empty() { "" } else { &dialog.input }))
        .style(input_style)
        .block(Block::default().borders(Borders::ALL).title(" Add Tag "));
    frame.render_widget(input, chunks[0]);

    // Suggestions list
    if dialog.suggestions.is_empty() && !dialog.input.is_empty() {
        let create_msg = format!("Press Enter to create tag: \"{}\"", dialog.input);
        let msg = Paragraph::new(create_msg)
            .style(Style::default().fg(Color::Yellow))
            .block(Block::default().borders(Borders::ALL).title(" Suggestions "));
        frame.render_widget(msg, chunks[1]);
    } else {
        let items: Vec<ListItem> = dialog.suggestions
            .iter()
            .enumerate()
            .map(|(i, tag)| {
                let style = if i == dialog.selected_index {
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                ListItem::new(format!("  {} ", tag.name)).style(style)
            })
            .collect();

        let list = List::new(items)
            .block(Block::default().borders(Borders::ALL).title(" Suggestions "))
            .highlight_style(Style::default().add_modifier(Modifier::REVERSED));

        let mut state = ListState::default();
        state.select(Some(dialog.selected_index));
        frame.render_stateful_widget(list, chunks[1], &mut state);
    }

    // Help text
    let help = Paragraph::new("j/k:select | Enter:add | Esc:cancel")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    frame.render_widget(help, chunks[2]);
}
