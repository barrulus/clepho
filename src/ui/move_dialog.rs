use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};
use std::fs;
use std::path::PathBuf;

/// State for the move file dialog
pub struct MoveDialog {
    /// Current directory being browsed
    pub current_dir: PathBuf,
    /// Directory entries
    pub entries: Vec<PathBuf>,
    /// Selected index in the directory listing
    pub selected_index: usize,
    /// Files to be moved
    pub files_to_move: Vec<PathBuf>,
    /// User input for quick path entry
    pub input: String,
    /// Whether input mode is active
    pub input_mode: bool,
}

impl MoveDialog {
    pub fn new(start_dir: PathBuf, files_to_move: Vec<PathBuf>) -> Self {
        let mut dialog = Self {
            current_dir: start_dir.clone(),
            entries: Vec::new(),
            selected_index: 0,
            files_to_move,
            input: String::new(),
            input_mode: false,
        };
        dialog.load_directory(&start_dir);
        dialog
    }

    pub fn load_directory(&mut self, path: &PathBuf) {
        self.current_dir = path.clone();
        self.entries.clear();
        self.selected_index = 0;

        // Add parent directory option
        if let Some(parent) = path.parent() {
            self.entries.push(parent.to_path_buf());
        }

        // Read directory entries, only directories
        if let Ok(read_dir) = fs::read_dir(path) {
            let mut dirs: Vec<PathBuf> = read_dir
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
                .map(|e| e.path())
                .collect();
            dirs.sort();
            self.entries.extend(dirs);
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

    pub fn enter_selected(&mut self) {
        if let Some(path) = self.entries.get(self.selected_index) {
            let path = path.clone();
            self.load_directory(&path);
        }
    }

    pub fn go_parent(&mut self) {
        if let Some(parent) = self.current_dir.parent() {
            let parent = parent.to_path_buf();
            self.load_directory(&parent);
        }
    }

    pub fn toggle_input_mode(&mut self) {
        self.input_mode = !self.input_mode;
        if self.input_mode {
            self.input = self.current_dir.to_string_lossy().to_string();
        }
    }

    pub fn handle_input(&mut self, c: char) {
        if self.input_mode {
            self.input.push(c);
        }
    }

    pub fn backspace(&mut self) {
        if self.input_mode {
            self.input.pop();
        }
    }

    pub fn confirm_input(&mut self) {
        if self.input_mode {
            let path = PathBuf::from(&self.input);
            if path.is_dir() {
                self.load_directory(&path);
            }
            self.input_mode = false;
        }
    }

    /// Get the target directory for the move operation
    pub fn target_dir(&self) -> &PathBuf {
        &self.current_dir
    }
}

pub fn render(frame: &mut Frame, dialog: &MoveDialog, area: Rect) {
    // Center the dialog
    let dialog_width = 70.min(area.width.saturating_sub(4));
    let dialog_height = 25.min(area.height.saturating_sub(4));

    let x = (area.width - dialog_width) / 2;
    let y = (area.height - dialog_height) / 2;

    let dialog_area = Rect::new(x, y, dialog_width, dialog_height);

    // Clear the area behind the dialog
    frame.render_widget(Clear, dialog_area);

    // Main layout: header, directory list, path input, footer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3), // Header with file count
            Constraint::Min(10),   // Directory listing
            Constraint::Length(3), // Path input
            Constraint::Length(2), // Footer with instructions
        ])
        .split(dialog_area);

    // Draw border
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(" Move Files ")
        .title_style(Style::default().add_modifier(Modifier::BOLD));
    frame.render_widget(block, dialog_area);

    // Header: show file count
    let header = Paragraph::new(format!(
        "Moving {} file(s) to:",
        dialog.files_to_move.len()
    ))
    .style(Style::default().fg(Color::Yellow));
    frame.render_widget(header, chunks[0]);

    // Directory listing
    let items: Vec<ListItem> = dialog
        .entries
        .iter()
        .enumerate()
        .map(|(i, path)| {
            let name = if i == 0 && path.parent().is_some() && path != &dialog.current_dir {
                "..".to_string()
            } else {
                path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.to_string_lossy().to_string())
            };
            ListItem::new(format!("/ {}", name)).style(Style::default().fg(Color::Cyan))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(format!(" {} ", dialog.current_dir.display())),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Yellow)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        );

    let mut state = ListState::default();
    state.select(Some(dialog.selected_index));
    frame.render_stateful_widget(list, chunks[1], &mut state);

    // Path input
    let input_style = if dialog.input_mode {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let input_text = if dialog.input_mode {
        format!("> {}_", dialog.input)
    } else {
        format!("  {} (press / to edit)", dialog.current_dir.display())
    };
    let input = Paragraph::new(input_text)
        .style(input_style)
        .block(Block::default().borders(Borders::ALL).title(" Path "));
    frame.render_widget(input, chunks[2]);

    // Footer with instructions
    let footer = Paragraph::new(
        "j/k: navigate | Enter: open dir | /: edit path | m: confirm move | Esc: cancel",
    )
    .style(Style::default().fg(Color::DarkGray));
    frame.render_widget(footer, chunks[3]);
}
