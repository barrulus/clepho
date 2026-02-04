//! In-app settings dialog for viewing and editing configuration.

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};

use crate::config::{Config, LlmProviderType};

/// Active section in the settings dialog
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsSection {
    LlmSettings,
    Prompts,
}

/// Which field is currently being edited
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditingField {
    None,
    Provider,
    Endpoint,
    Model,
    ApiKey,
    BatchConcurrency,
    CustomPrompt,
    BasePrompt,
}

/// Settings dialog state
pub struct SettingsDialog {
    /// Current section
    pub section: SettingsSection,
    /// Selected item index within current section
    pub selected: usize,
    /// Currently editing field
    pub editing: EditingField,
    /// Text buffer for editing
    pub edit_buffer: String,
    /// Cursor position in edit buffer
    pub cursor: usize,
    /// Whether changes have been made
    pub modified: bool,

    // LLM Settings (editable copies)
    pub provider: LlmProviderType,
    pub endpoint: String,
    pub model: String,
    pub api_key: Option<String>,
    pub batch_concurrency: usize,

    // Prompts
    pub custom_prompt: Option<String>,
    pub base_prompt: Option<String>,
}

impl SettingsDialog {
    pub fn new(config: &Config) -> Self {
        Self {
            section: SettingsSection::LlmSettings,
            selected: 0,
            editing: EditingField::None,
            edit_buffer: String::new(),
            cursor: 0,
            modified: false,

            provider: config.llm.provider,
            endpoint: config.llm.endpoint.clone(),
            model: config.llm.model.clone(),
            api_key: config.llm.api_key.clone(),
            batch_concurrency: config.llm.batch_concurrency,

            custom_prompt: config.llm.custom_prompt.clone(),
            base_prompt: config.llm.base_prompt.clone(),
        }
    }

    /// Get the number of items in the current section
    pub fn item_count(&self) -> usize {
        match self.section {
            SettingsSection::LlmSettings => 5, // provider, endpoint, model, api_key, batch_concurrency
            SettingsSection::Prompts => 2,     // custom_prompt, base_prompt
        }
    }

    /// Move to next section
    pub fn next_section(&mut self) {
        self.section = match self.section {
            SettingsSection::LlmSettings => SettingsSection::Prompts,
            SettingsSection::Prompts => SettingsSection::LlmSettings,
        };
        self.selected = 0;
    }

    /// Move to previous section
    pub fn prev_section(&mut self) {
        self.next_section(); // Only 2 sections, so same as next
    }

    /// Move selection down
    pub fn select_next(&mut self) {
        if self.selected < self.item_count().saturating_sub(1) {
            self.selected += 1;
        }
    }

    /// Move selection up
    pub fn select_prev(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    /// Start editing the currently selected field
    pub fn start_edit(&mut self) {
        let field = self.get_current_field();
        self.editing = field;
        self.edit_buffer = self.get_field_value(field);
        self.cursor = self.edit_buffer.len();
    }

    /// Get the field at current selection (public version)
    pub fn get_current_field_public(&self) -> EditingField {
        self.get_current_field()
    }

    /// Get the field at current selection
    fn get_current_field(&self) -> EditingField {
        match self.section {
            SettingsSection::LlmSettings => match self.selected {
                0 => EditingField::Provider,
                1 => EditingField::Endpoint,
                2 => EditingField::Model,
                3 => EditingField::ApiKey,
                4 => EditingField::BatchConcurrency,
                _ => EditingField::None,
            },
            SettingsSection::Prompts => match self.selected {
                0 => EditingField::CustomPrompt,
                1 => EditingField::BasePrompt,
                _ => EditingField::None,
            },
        }
    }

    /// Get the current value of a field as string
    fn get_field_value(&self, field: EditingField) -> String {
        match field {
            EditingField::Provider => match self.provider {
                LlmProviderType::LmStudio => "lmstudio".to_string(),
                LlmProviderType::Ollama => "ollama".to_string(),
                LlmProviderType::OpenAI => "openai".to_string(),
                LlmProviderType::Anthropic => "anthropic".to_string(),
            },
            EditingField::Endpoint => self.endpoint.clone(),
            EditingField::Model => self.model.clone(),
            EditingField::ApiKey => self.api_key.clone().unwrap_or_default(),
            EditingField::BatchConcurrency => self.batch_concurrency.to_string(),
            EditingField::CustomPrompt => self.custom_prompt.clone().unwrap_or_default(),
            EditingField::BasePrompt => self.base_prompt.clone().unwrap_or_default(),
            EditingField::None => String::new(),
        }
    }

    /// Apply the edit buffer to the field
    pub fn apply_edit(&mut self) {
        match self.editing {
            EditingField::Provider => {
                self.provider = match self.edit_buffer.to_lowercase().as_str() {
                    "lmstudio" | "lm_studio" | "lm-studio" => LlmProviderType::LmStudio,
                    "ollama" => LlmProviderType::Ollama,
                    "openai" => LlmProviderType::OpenAI,
                    "anthropic" => LlmProviderType::Anthropic,
                    _ => self.provider, // Keep current if invalid
                };
                self.modified = true;
            }
            EditingField::Endpoint => {
                self.endpoint = self.edit_buffer.clone();
                self.modified = true;
            }
            EditingField::Model => {
                self.model = self.edit_buffer.clone();
                self.modified = true;
            }
            EditingField::ApiKey => {
                self.api_key = if self.edit_buffer.is_empty() {
                    None
                } else {
                    Some(self.edit_buffer.clone())
                };
                self.modified = true;
            }
            EditingField::BatchConcurrency => {
                if let Ok(val) = self.edit_buffer.parse::<usize>() {
                    self.batch_concurrency = val.max(1).min(32);
                    self.modified = true;
                }
            }
            EditingField::CustomPrompt => {
                self.custom_prompt = if self.edit_buffer.is_empty() {
                    None
                } else {
                    Some(self.edit_buffer.clone())
                };
                self.modified = true;
            }
            EditingField::BasePrompt => {
                self.base_prompt = if self.edit_buffer.is_empty() {
                    None
                } else {
                    Some(self.edit_buffer.clone())
                };
                self.modified = true;
            }
            EditingField::None => {}
        }
        self.editing = EditingField::None;
    }

    /// Cancel editing
    pub fn cancel_edit(&mut self) {
        self.editing = EditingField::None;
        self.edit_buffer.clear();
    }

    /// Cycle through provider options
    pub fn cycle_provider(&mut self) {
        self.provider = match self.provider {
            LlmProviderType::LmStudio => LlmProviderType::Ollama,
            LlmProviderType::Ollama => LlmProviderType::OpenAI,
            LlmProviderType::OpenAI => LlmProviderType::Anthropic,
            LlmProviderType::Anthropic => LlmProviderType::LmStudio,
        };
        self.modified = true;
    }

    /// Apply settings to config
    pub fn apply_to_config(&self, config: &mut Config) {
        config.llm.provider = self.provider;
        config.llm.endpoint = self.endpoint.clone();
        config.llm.model = self.model.clone();
        config.llm.api_key = self.api_key.clone();
        config.llm.batch_concurrency = self.batch_concurrency;
        config.llm.custom_prompt = self.custom_prompt.clone();
        config.llm.base_prompt = self.base_prompt.clone();
    }

    // Text editing methods
    pub fn handle_char(&mut self, c: char) {
        self.edit_buffer.insert(self.cursor, c);
        self.cursor += 1;
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.edit_buffer.remove(self.cursor);
        }
    }

    pub fn delete(&mut self) {
        if self.cursor < self.edit_buffer.len() {
            self.edit_buffer.remove(self.cursor);
        }
    }

    pub fn move_cursor_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn move_cursor_right(&mut self) {
        if self.cursor < self.edit_buffer.len() {
            self.cursor += 1;
        }
    }

    pub fn move_cursor_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_cursor_end(&mut self) {
        self.cursor = self.edit_buffer.len();
    }

    pub fn clear_field(&mut self) {
        self.edit_buffer.clear();
        self.cursor = 0;
    }
}

pub fn render(frame: &mut Frame, dialog: &SettingsDialog, area: Rect) {
    let dialog_width = 80.min(area.width.saturating_sub(4));
    let dialog_height = 30.min(area.height.saturating_sub(4));

    let x = (area.width - dialog_width) / 2;
    let y = (area.height - dialog_height) / 2;

    let dialog_area = Rect::new(x, y, dialog_width, dialog_height);

    frame.render_widget(Clear, dialog_area);

    // Main layout: tabs, content, help
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Tabs
            Constraint::Min(15),    // Content
            Constraint::Length(4),  // Help
        ])
        .margin(1)
        .split(dialog_area);

    // Outer border
    let modified_marker = if dialog.modified { " [modified]" } else { "" };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(format!(" Settings{} ", modified_marker));
    frame.render_widget(block, dialog_area);

    // Tabs
    render_tabs(frame, dialog, chunks[0]);

    // Content based on section
    match dialog.section {
        SettingsSection::LlmSettings => render_llm_settings(frame, dialog, chunks[1]),
        SettingsSection::Prompts => render_prompts(frame, dialog, chunks[1]),
    }

    // Help
    render_help(frame, dialog, chunks[2]);
}

fn render_tabs(frame: &mut Frame, dialog: &SettingsDialog, area: Rect) {
    let tabs = vec![
        ("LLM Settings", SettingsSection::LlmSettings),
        ("Prompts", SettingsSection::Prompts),
    ];

    let tab_spans: Vec<Span> = tabs
        .iter()
        .enumerate()
        .flat_map(|(i, (name, section))| {
            let style = if *section == dialog.section {
                Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let mut spans = vec![Span::styled(format!(" {} ", name), style)];
            if i < tabs.len() - 1 {
                spans.push(Span::raw(" | "));
            }
            spans
        })
        .collect();

    let tabs_widget = Paragraph::new(Line::from(tab_spans))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::BOTTOM));
    frame.render_widget(tabs_widget, area);
}

fn render_llm_settings(frame: &mut Frame, dialog: &SettingsDialog, area: Rect) {
    let items = vec![
        format_setting_item(
            "Provider",
            &format!("{:?}", dialog.provider).to_lowercase(),
            dialog.selected == 0,
            dialog.editing == EditingField::Provider,
            &dialog.edit_buffer,
            dialog.cursor,
        ),
        format_setting_item(
            "Endpoint",
            &dialog.endpoint,
            dialog.selected == 1,
            dialog.editing == EditingField::Endpoint,
            &dialog.edit_buffer,
            dialog.cursor,
        ),
        format_setting_item(
            "Model",
            &dialog.model,
            dialog.selected == 2,
            dialog.editing == EditingField::Model,
            &dialog.edit_buffer,
            dialog.cursor,
        ),
        format_setting_item(
            "API Key",
            &mask_api_key(&dialog.api_key),
            dialog.selected == 3,
            dialog.editing == EditingField::ApiKey,
            &dialog.edit_buffer,
            dialog.cursor,
        ),
        format_setting_item(
            "Batch Concurrency",
            &dialog.batch_concurrency.to_string(),
            dialog.selected == 4,
            dialog.editing == EditingField::BatchConcurrency,
            &dialog.edit_buffer,
            dialog.cursor,
        ),
    ];

    let list_items: Vec<ListItem> = items.into_iter().map(ListItem::new).collect();

    let list = List::new(list_items).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Blue))
            .title(" LLM Configuration "),
    );

    let mut state = ListState::default();
    state.select(Some(dialog.selected));
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_prompts(frame: &mut Frame, dialog: &SettingsDialog, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    // Custom prompt
    let custom_style = if dialog.selected == 0 {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::Blue)
    };
    let custom_text = if dialog.editing == EditingField::CustomPrompt {
        format_edit_text(&dialog.edit_buffer, dialog.cursor)
    } else {
        dialog.custom_prompt.clone().unwrap_or_else(|| "(not set)".to_string())
    };
    let custom_widget = Paragraph::new(custom_text)
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(custom_style)
                .title(if dialog.selected == 0 { " > Custom Prompt " } else { " Custom Prompt " }),
        );
    frame.render_widget(custom_widget, chunks[0]);

    // Base prompt
    let base_style = if dialog.selected == 1 {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::Blue)
    };
    let base_text = if dialog.editing == EditingField::BasePrompt {
        format_edit_text(&dialog.edit_buffer, dialog.cursor)
    } else {
        dialog.base_prompt.clone().unwrap_or_else(|| "(using default)".to_string())
    };
    let base_widget = Paragraph::new(base_text)
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(base_style)
                .title(if dialog.selected == 1 { " > Base Prompt Override " } else { " Base Prompt Override " }),
        );
    frame.render_widget(base_widget, chunks[1]);
}

fn render_help(frame: &mut Frame, dialog: &SettingsDialog, area: Rect) {
    let help_text = if dialog.editing != EditingField::None {
        vec![
            Line::from("Enter=save | Esc=cancel | Ctrl+U=clear"),
            Line::from("Arrows=move cursor | Home/End=start/end"),
        ]
    } else {
        vec![
            Line::from("Tab=switch section | j/k=navigate | Enter=edit | Space=toggle"),
            Line::from("Ctrl+S=save config | Ctrl+R=reload config | Esc=close"),
        ]
    };

    let help = Paragraph::new(help_text)
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    frame.render_widget(help, area);
}

fn format_setting_item(
    label: &str,
    value: &str,
    selected: bool,
    editing: bool,
    edit_buffer: &str,
    cursor: usize,
) -> Line<'static> {
    let marker = if selected { "> " } else { "  " };
    let label_style = if selected {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };

    let value_text = if editing {
        format_edit_text(edit_buffer, cursor)
    } else {
        value.to_string()
    };

    let value_style = if editing {
        Style::default().fg(Color::Yellow)
    } else if selected {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    Line::from(vec![
        Span::raw(marker),
        Span::styled(format!("{:<20}", label), label_style),
        Span::styled(value_text, value_style),
    ])
}

fn format_edit_text(text: &str, cursor: usize) -> String {
    if cursor < text.len() {
        let (before, after) = text.split_at(cursor);
        let cursor_char = after.chars().next().unwrap_or(' ');
        let rest = &after[cursor_char.len_utf8()..];
        format!("{}[{}]{}", before, cursor_char, rest)
    } else {
        format!("{}[_]", text)
    }
}

fn mask_api_key(key: &Option<String>) -> String {
    match key {
        Some(k) if !k.is_empty() => {
            let len = k.len();
            if len <= 8 {
                "*".repeat(len)
            } else {
                format!("{}...{}", &k[..4], &k[len - 4..])
            }
        }
        _ => "(not set)".to_string(),
    }
}
