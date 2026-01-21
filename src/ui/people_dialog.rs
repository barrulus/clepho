use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
};
use ratatui_image::{Resize, StatefulImage};

use crate::app::App;
use crate::db::{BoundingBox, FaceWithPhoto, Person};

/// A simplified face entry for display
#[derive(Clone)]
pub struct FaceEntry {
    pub face_id: i64,
    pub photo_filename: String,
    pub photo_path: String,
    pub bbox: BoundingBox,
}

impl From<FaceWithPhoto> for FaceEntry {
    fn from(f: FaceWithPhoto) -> Self {
        Self {
            face_id: f.face.id,
            photo_filename: f.photo_filename,
            photo_path: f.photo_path,
            bbox: f.face.bbox,
        }
    }
}

/// View mode for the people dialog
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeopleViewMode {
    /// View named people
    People,
    /// View unassigned faces
    Faces,
}

/// Input mode for naming
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    /// Normal navigation
    Normal,
    /// Entering a name for a person/face
    Naming,
}

/// Active pane in the dialog (for keyboard navigation)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PeopleActivePane {
    /// List pane (left side)
    #[default]
    List,
    /// Preview pane (right side, only in Faces view)
    Preview,
}

/// State for the people management dialog
pub struct PeopleDialog {
    /// Current view mode
    pub view_mode: PeopleViewMode,
    /// Input mode
    pub input_mode: InputMode,
    /// Active pane for navigation
    pub active_pane: PeopleActivePane,
    /// Named people
    pub people: Vec<Person>,
    /// Unassigned faces
    pub faces: Vec<FaceEntry>,
    /// Selected index in current list
    pub selected_index: usize,
    /// Name input for naming faces
    pub name_input: String,
    /// Cursor position in name input
    pub cursor: usize,
    /// Status message
    pub status: Option<String>,
}

impl PeopleDialog {
    pub fn new(people: Vec<Person>, faces: Vec<FaceWithPhoto>) -> Self {
        let face_entries: Vec<FaceEntry> = faces.into_iter().map(|f| f.into()).collect();
        Self {
            view_mode: if people.is_empty() && !face_entries.is_empty() {
                PeopleViewMode::Faces
            } else {
                PeopleViewMode::People
            },
            input_mode: InputMode::Normal,
            active_pane: PeopleActivePane::List,
            people,
            faces: face_entries,
            selected_index: 0,
            name_input: String::new(),
            cursor: 0,
            status: None,
        }
    }

    pub fn toggle_view_mode(&mut self) {
        self.view_mode = match self.view_mode {
            PeopleViewMode::People => PeopleViewMode::Faces,
            PeopleViewMode::Faces => PeopleViewMode::People,
        };
        self.selected_index = 0;
        self.active_pane = PeopleActivePane::List;
    }

    /// Move focus to the right (list -> preview)
    pub fn move_right(&mut self) -> bool {
        // Preview only available in Faces view
        if self.view_mode == PeopleViewMode::Faces && self.active_pane == PeopleActivePane::List {
            self.active_pane = PeopleActivePane::Preview;
            true
        } else {
            false
        }
    }

    /// Move focus to the left (preview -> list)
    pub fn move_left(&mut self) -> bool {
        if self.active_pane == PeopleActivePane::Preview {
            self.active_pane = PeopleActivePane::List;
            true
        } else {
            false
        }
    }

    pub fn move_up(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    pub fn move_down(&mut self) {
        let max_index = match self.view_mode {
            PeopleViewMode::People => self.people.len().saturating_sub(1),
            PeopleViewMode::Faces => self.faces.len().saturating_sub(1),
        };
        if self.selected_index < max_index {
            self.selected_index += 1;
        }
    }

    pub fn enter_naming_mode(&mut self) {
        // Pre-fill with existing name if renaming a person
        if self.view_mode == PeopleViewMode::People {
            if let Some(person) = self.people.get(self.selected_index) {
                self.name_input = person.name.clone();
            }
        } else {
            // For faces, start with empty name
            self.name_input.clear();
        }
        self.cursor = self.name_input.len();
        self.input_mode = InputMode::Naming;
    }

    pub fn exit_naming_mode(&mut self) {
        self.input_mode = InputMode::Normal;
        self.name_input.clear();
        self.cursor = 0;
    }

    pub fn handle_char(&mut self, c: char) {
        self.name_input.insert(self.cursor, c);
        self.cursor += 1;
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.name_input.remove(self.cursor);
        }
    }

    pub fn move_cursor_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn move_cursor_right(&mut self) {
        if self.cursor < self.name_input.len() {
            self.cursor += 1;
        }
    }

    /// Get the currently selected face ID (for naming)
    pub fn selected_face_id(&self) -> Option<i64> {
        if self.view_mode == PeopleViewMode::Faces {
            self.faces.get(self.selected_index).map(|f| f.face_id)
        } else {
            None
        }
    }

    /// Get the currently selected person ID (for renaming or viewing)
    pub fn selected_person_id(&self) -> Option<i64> {
        if self.view_mode == PeopleViewMode::People {
            self.people.get(self.selected_index).map(|p| p.id)
        } else {
            None
        }
    }

    /// Get the entered name
    pub fn get_name(&self) -> &str {
        &self.name_input
    }

    /// Update data after database changes
    pub fn update_data(&mut self, people: Vec<Person>, faces: Vec<FaceWithPhoto>) {
        self.people = people;
        self.faces = faces.into_iter().map(|f| f.into()).collect();
        // Adjust selected index if needed
        let max_index = match self.view_mode {
            PeopleViewMode::People => self.people.len().saturating_sub(1),
            PeopleViewMode::Faces => self.faces.len().saturating_sub(1),
        };
        if self.selected_index > max_index {
            self.selected_index = max_index;
        }
    }

    /// Check if the current list is empty
    pub fn is_empty(&self) -> bool {
        match self.view_mode {
            PeopleViewMode::People => self.people.is_empty(),
            PeopleViewMode::Faces => self.faces.is_empty(),
        }
    }

    /// Get the currently selected face entry (for preview)
    pub fn selected_face(&self) -> Option<&FaceEntry> {
        if self.view_mode == PeopleViewMode::Faces {
            self.faces.get(self.selected_index)
        } else {
            None
        }
    }
}

pub fn render(frame: &mut Frame, app: &mut App, area: Rect) {
    // Extract all needed data from dialog first to avoid borrow conflicts
    let (view_mode, input_mode, people_len, faces_len, name_input, cursor, status, _selected_index) = {
        let dialog = match app.people_dialog.as_ref() {
            Some(d) => d,
            None => return,
        };
        (
            dialog.view_mode,
            dialog.input_mode,
            dialog.people.len(),
            dialog.faces.len(),
            dialog.name_input.clone(),
            dialog.cursor,
            dialog.status.clone(),
            dialog.selected_index,
        )
    };

    // Calculate dialog size - wider when in Faces view to accommodate preview
    let base_width = if view_mode == PeopleViewMode::Faces { 100 } else { 70 };
    let dialog_width = base_width.min(area.width.saturating_sub(4));
    let dialog_height = 30.min(area.height.saturating_sub(4));

    let x = (area.width - dialog_width) / 2;
    let y = (area.height - dialog_height) / 2;

    let dialog_area = Rect::new(x, y, dialog_width, dialog_height);

    // Clear the area behind the dialog
    frame.render_widget(Clear, dialog_area);

    // Draw border
    let title = " People & Faces ";
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta))
        .title(title)
        .title_style(Style::default().add_modifier(Modifier::BOLD));

    // Get inner area before rendering (block is consumed by render)
    let inner_area = block.inner(dialog_area);
    frame.render_widget(block, dialog_area);

    // Main layout
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Tab bar / mode indicator
            Constraint::Min(10),   // List (or list+preview)
            Constraint::Length(3), // Name input (if in naming mode)
            Constraint::Length(1), // Status
            Constraint::Length(1), // Footer
        ])
        .split(inner_area);

    // Tab bar
    let people_style = if view_mode == PeopleViewMode::People {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD).add_modifier(Modifier::UNDERLINED)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let faces_style = if view_mode == PeopleViewMode::Faces {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD).add_modifier(Modifier::UNDERLINED)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let tab_text = Line::from(vec![
        Span::raw(" "),
        Span::styled(format!("People ({})", people_len), people_style),
        Span::raw("  |  "),
        Span::styled(format!("Faces ({})", faces_len), faces_style),
        Span::raw("   [Tab to switch]"),
    ]);
    let tabs = Paragraph::new(tab_text);
    frame.render_widget(tabs, chunks[0]);

    // List content (with preview for Faces view)
    match view_mode {
        PeopleViewMode::People => {
            // Re-borrow dialog for people list (immutable is fine here)
            if let Some(ref dialog) = app.people_dialog {
                render_people_list(frame, dialog, chunks[1]);
            }
        }
        PeopleViewMode::Faces => {
            render_faces_with_preview(frame, app, chunks[1]);
        }
    }

    // Name input (only visible in naming mode)
    if input_mode == InputMode::Naming {
        let input_text = format!(
            "{}|{}",
            &name_input[..cursor],
            &name_input[cursor..]
        );
        let input = Paragraph::new(input_text)
            .style(Style::default().fg(Color::Yellow))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Enter name ")
                    .border_style(Style::default().fg(Color::Yellow)),
            );
        frame.render_widget(input, chunks[2]);
    }

    // Status
    let status_text = status.as_deref().unwrap_or("");
    let status_widget = Paragraph::new(status_text).style(Style::default().fg(Color::DarkGray));
    frame.render_widget(status_widget, chunks[3]);

    // Footer
    let footer_text = if input_mode == InputMode::Naming {
        "Enter: confirm | Esc: cancel"
    } else {
        "↑↓: navigate | Tab: switch view | n: name | Enter: view photos | Esc: close"
    };
    let footer = Paragraph::new(footer_text).style(Style::default().fg(Color::DarkGray));
    frame.render_widget(footer, chunks[4]);
}

fn render_people_list(frame: &mut Frame, dialog: &PeopleDialog, area: Rect) {
    if dialog.people.is_empty() {
        let empty = Paragraph::new("No named people yet.\nSwitch to Faces view (Tab) to name detected faces.")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" People ")
                    .border_style(Style::default().fg(Color::DarkGray)),
            );
        frame.render_widget(empty, area);
        return;
    }

    let items: Vec<ListItem> = dialog
        .people
        .iter()
        .map(|person| {
            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(&person.name, Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                ]),
                Line::from(Span::styled(
                    format!("  {} photos", person.face_count),
                    Style::default().fg(Color::DarkGray),
                )),
            ])
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" People ")
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Magenta)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        );

    let mut state = ListState::default();
    if !dialog.people.is_empty() {
        state.select(Some(dialog.selected_index));
    }
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_faces_with_preview(frame: &mut Frame, app: &mut App, area: Rect) {
    let (faces_empty, active_pane, selected_index, faces_data) = match app.people_dialog.as_ref() {
        Some(d) => (
            d.faces.is_empty(),
            d.active_pane,
            d.selected_index,
            d.faces.iter().map(|f| (f.photo_filename.clone(), f.face_id)).collect::<Vec<_>>(),
        ),
        None => return,
    };

    if faces_empty {
        let empty = Paragraph::new("No unassigned faces.\nRun face detection first (F key in browser).")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Unassigned Faces ")
                    .border_style(Style::default().fg(Color::DarkGray)),
            );
        frame.render_widget(empty, area);
        return;
    }

    // Split area: list on left, preview on right
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50), // Face list
            Constraint::Percentage(50), // Face preview
        ])
        .split(area);

    // Determine border colors based on active pane
    let list_border_color = if active_pane == PeopleActivePane::List {
        Color::Yellow
    } else {
        Color::DarkGray
    };
    let preview_border_color = if active_pane == PeopleActivePane::Preview {
        Color::Yellow
    } else {
        Color::DarkGray
    };

    // Render face list
    let items: Vec<ListItem> = faces_data
        .iter()
        .map(|(filename, face_id)| {
            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(filename, Style::default().fg(Color::Yellow)),
                ]),
                Line::from(Span::styled(
                    format!("  Face #{}", face_id),
                    Style::default().fg(Color::DarkGray),
                )),
            ])
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Unassigned Faces ")
                .border_style(Style::default().fg(list_border_color)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Yellow)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        );

    let mut state = ListState::default();
    state.select(Some(selected_index));
    frame.render_stateful_widget(list, chunks[0], &mut state);

    // Render face preview
    render_face_preview(frame, app, chunks[1], preview_border_color);
}

fn render_face_preview(frame: &mut Frame, app: &mut App, area: Rect, border_color: Color) {
    let preview_block = Block::default()
        .borders(Borders::ALL)
        .title(" Face Preview ")
        .border_style(Style::default().fg(border_color));

    // Get selected face info before borrowing app mutably
    let face_info = app.people_dialog.as_ref().and_then(|d| {
        d.selected_face().map(|f| {
            (
                std::path::PathBuf::from(&f.photo_path),
                f.bbox.clone(),
                f.face_id,
            )
        })
    });

    let (path, bbox, face_id) = match face_info {
        Some(info) => info,
        None => {
            let empty = Paragraph::new("No face selected")
                .style(Style::default().fg(Color::DarkGray))
                .alignment(Alignment::Center)
                .block(preview_block);
            frame.render_widget(empty, area);
            return;
        }
    };

    let inner_area = preview_block.inner(area);
    frame.render_widget(preview_block, area);

    // Check if image preview is enabled and available
    if !app.config.preview.image_preview || !app.image_preview.is_available() {
        let info = Paragraph::new(format!(
            "Face #{}\n\nPosition: {}x{}\nSize: {}x{}\n\n(Image preview not available)",
            face_id, bbox.x, bbox.y, bbox.width, bbox.height
        ))
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
        frame.render_widget(info, inner_area);
        return;
    }

    // Split preview area: image on top, info below
    let preview_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(5),    // Image
            Constraint::Length(3), // Info
        ])
        .split(inner_area);

    // Load and render the face crop
    let thumbnail_size = app.config.preview.thumbnail_size;

    // Create a unique cache key for this face crop
    let face_cache_key = std::path::PathBuf::from(format!(
        "{}#face_{}",
        path.display(),
        face_id
    ));

    // Try to load the face crop (or start async loading)
    if let Some(protocol) = app.image_preview.load_face_crop(&path, &bbox, face_id, thumbnail_size) {
        let image = StatefulImage::new(None).resize(Resize::Fit(None));
        frame.render_stateful_widget(image, preview_chunks[0], protocol);
    } else if app.image_preview.is_loading_face(&face_cache_key) {
        let loading = Paragraph::new("Loading face...")
            .style(Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC))
            .alignment(Alignment::Center);
        frame.render_widget(loading, preview_chunks[0]);
    } else {
        let loading = Paragraph::new("Preparing preview...")
            .style(Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC))
            .alignment(Alignment::Center);
        frame.render_widget(loading, preview_chunks[0]);
    }

    // Face info
    let info_text = format!("Face #{} | {}x{} px", face_id, bbox.width, bbox.height);
    let info = Paragraph::new(info_text)
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    frame.render_widget(info, preview_chunks[1]);
}
