//! Pipeline Status screen (spec §4.7).
//! MVP: managed folders + failures inbox. Workers panel is a placeholder
//! until the daemon driver lands and can publish live activity.

use clepho::db::managed_folders::ManagedFolder;
use clepho::db::pipeline_events::UnresolvedGroup;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};

#[allow(dead_code)]
pub struct PipelineStatusScreen {
    pub folders: Vec<ManagedFolder>,
    pub failures: Vec<UnresolvedGroup>,
    pub focus: Focus,
    pub folder_state: ListState,
    pub failure_state: ListState,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Folders,
    Failures,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum ScreenAction {
    None,
    Close,
    TogglePause(String),
    RetryGroup {
        stage: Option<String>,
        error_class: Option<String>,
    },
    ClearGroup {
        stage: Option<String>,
        error_class: Option<String>,
    },
}

#[allow(dead_code)]
impl PipelineStatusScreen {
    pub fn new(folders: Vec<ManagedFolder>, failures: Vec<UnresolvedGroup>) -> Self {
        let mut s = Self {
            folders,
            failures,
            focus: Focus::Folders,
            folder_state: ListState::default(),
            failure_state: ListState::default(),
        };
        if !s.folders.is_empty() {
            s.folder_state.select(Some(0));
        }
        if !s.failures.is_empty() {
            s.failure_state.select(Some(0));
        }
        s
    }

    pub fn render(&mut self, f: &mut Frame, area: Rect) {
        f.render_widget(Clear, area);
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(40),
                Constraint::Percentage(20),
                Constraint::Percentage(40),
            ])
            .split(area);

        let folder_items: Vec<ListItem> = self
            .folders
            .iter()
            .map(|fldr| {
                let badge = if fldr.paused { "P" } else { "*" };
                let cron = fldr.schedule_cron.as_deref().unwrap_or("-");
                ListItem::new(format!(
                    "{} {:60} cron: {:20} last: {}",
                    badge,
                    fldr.path,
                    cron,
                    fldr.last_run_at.as_deref().unwrap_or("-")
                ))
            })
            .collect();
        let folders_list = List::new(folder_items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Managed Folders "),
            )
            .highlight_symbol("> ");
        f.render_stateful_widget(folders_list, chunks[0], &mut self.folder_state);

        let workers = Paragraph::new(
            "scan: idle  exif: idle  thumb: idle  llm: idle  faces: idle  index: idle\n\
             (live activity pending daemon driver)",
        )
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Workers "),
        )
        .wrap(Wrap { trim: true });
        f.render_widget(workers, chunks[1]);

        let failure_items: Vec<ListItem> = self
            .failures
            .iter()
            .map(|g| {
                ListItem::new(format!(
                    "{} / {} - {} ({} occurrences)",
                    g.stage.as_deref().unwrap_or("?"),
                    g.error_class.as_deref().unwrap_or("?"),
                    truncate(&g.example_message, 60),
                    g.count,
                ))
            })
            .collect();
        let failures_list = List::new(failure_items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Failures Inbox "),
            )
            .highlight_symbol("> ");
        f.render_stateful_widget(failures_list, chunks[2], &mut self.failure_state);
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> ScreenAction {
        use crossterm::event::KeyCode;
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                let len = self.active_len();
                let st = self.active_state();
                let i = st.selected().unwrap_or(0);
                if i + 1 < len {
                    st.select(Some(i + 1));
                }
                ScreenAction::None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                let st = self.active_state();
                let i = st.selected().unwrap_or(0);
                if i > 0 {
                    st.select(Some(i - 1));
                }
                ScreenAction::None
            }
            KeyCode::Tab => {
                self.focus = match self.focus {
                    Focus::Folders => Focus::Failures,
                    Focus::Failures => Focus::Folders,
                };
                ScreenAction::None
            }
            KeyCode::Char('p') if self.focus == Focus::Folders => {
                if let Some(i) = self.folder_state.selected() {
                    if let Some(f) = self.folders.get(i) {
                        return ScreenAction::TogglePause(f.path.clone());
                    }
                }
                ScreenAction::None
            }
            KeyCode::Char('r') if self.focus == Focus::Failures => {
                if let Some(i) = self.failure_state.selected() {
                    if let Some(g) = self.failures.get(i) {
                        return ScreenAction::RetryGroup {
                            stage: g.stage.clone(),
                            error_class: g.error_class.clone(),
                        };
                    }
                }
                ScreenAction::None
            }
            KeyCode::Char('c') if self.focus == Focus::Failures => {
                if let Some(i) = self.failure_state.selected() {
                    if let Some(g) = self.failures.get(i) {
                        return ScreenAction::ClearGroup {
                            stage: g.stage.clone(),
                            error_class: g.error_class.clone(),
                        };
                    }
                }
                ScreenAction::None
            }
            KeyCode::Esc | KeyCode::Char('q') => ScreenAction::Close,
            _ => ScreenAction::None,
        }
    }

    fn active_state(&mut self) -> &mut ListState {
        match self.focus {
            Focus::Folders => &mut self.folder_state,
            Focus::Failures => &mut self.failure_state,
        }
    }
    fn active_len(&self) -> usize {
        match self.focus {
            Focus::Folders => self.folders.len(),
            Focus::Failures => self.failures.len(),
        }
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.into()
    } else {
        let cut: String = s.chars().take(n).collect();
        format!("{}...", cut)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent::new(c, KeyModifiers::empty())
    }

    fn fixture() -> PipelineStatusScreen {
        let folders = vec![
            ManagedFolder {
                id: 1,
                path: "/photos/2024".into(),
                schedule_cron: Some("0 5 * * *".into()),
                paused: false,
                last_run_at: Some("2026-05-04T05:00:00Z".into()),
                faces_disabled: false,
            },
            ManagedFolder {
                id: 2,
                path: "/photos/2023".into(),
                schedule_cron: None,
                paused: true,
                last_run_at: None,
                faces_disabled: false,
            },
        ];
        let failures = vec![UnresolvedGroup {
            stage: Some("llm".into()),
            error_class: Some("llm_unreachable".into()),
            count: 8,
            example_message: "connection refused".into(),
        }];
        PipelineStatusScreen::new(folders, failures)
    }

    #[test]
    fn snapshot_renders_three_panels() {
        let backend = TestBackend::new(120, 30);
        let mut term = Terminal::new(backend).unwrap();
        let mut screen = fixture();
        term.draw(|f| screen.render(f, f.area())).unwrap();
        insta::assert_snapshot!(term.backend());
    }

    #[test]
    fn esc_closes() {
        let mut s = fixture();
        assert!(matches!(s.handle_key(key(KeyCode::Esc)), ScreenAction::Close));
    }

    #[test]
    fn q_closes() {
        let mut s = fixture();
        assert!(matches!(
            s.handle_key(key(KeyCode::Char('q'))),
            ScreenAction::Close
        ));
    }

    #[test]
    fn tab_switches_focus() {
        let mut s = fixture();
        assert_eq!(s.focus, Focus::Folders);
        s.handle_key(key(KeyCode::Tab));
        assert_eq!(s.focus, Focus::Failures);
        s.handle_key(key(KeyCode::Tab));
        assert_eq!(s.focus, Focus::Folders);
    }

    #[test]
    fn j_moves_down_within_bounds() {
        let mut s = fixture();
        s.handle_key(key(KeyCode::Char('j')));
        assert_eq!(s.folder_state.selected(), Some(1));
        s.handle_key(key(KeyCode::Char('j'))); // already at end
        assert_eq!(s.folder_state.selected(), Some(1));
    }

    #[test]
    fn k_moves_up_within_bounds() {
        let mut s = fixture();
        s.folder_state.select(Some(1));
        s.handle_key(key(KeyCode::Char('k')));
        assert_eq!(s.folder_state.selected(), Some(0));
        s.handle_key(key(KeyCode::Char('k'))); // already at top
        assert_eq!(s.folder_state.selected(), Some(0));
    }

    #[test]
    fn p_in_folders_emits_toggle_pause_with_path() {
        let mut s = fixture();
        match s.handle_key(key(KeyCode::Char('p'))) {
            ScreenAction::TogglePause(p) => assert_eq!(p, "/photos/2024"),
            other => panic!("expected TogglePause, got {:?}", other),
        }
    }

    #[test]
    fn r_in_failures_emits_retry_group() {
        let mut s = fixture();
        s.handle_key(key(KeyCode::Tab)); // → Failures
        match s.handle_key(key(KeyCode::Char('r'))) {
            ScreenAction::RetryGroup { stage, error_class } => {
                assert_eq!(stage.as_deref(), Some("llm"));
                assert_eq!(error_class.as_deref(), Some("llm_unreachable"));
            }
            other => panic!("expected RetryGroup, got {:?}", other),
        }
    }

    #[test]
    fn c_in_failures_emits_clear_group() {
        let mut s = fixture();
        s.handle_key(key(KeyCode::Tab));
        assert!(matches!(
            s.handle_key(key(KeyCode::Char('c'))),
            ScreenAction::ClearGroup { .. }
        ));
    }

    #[test]
    fn p_in_failures_does_nothing() {
        // p is folders-only
        let mut s = fixture();
        s.handle_key(key(KeyCode::Tab));
        assert!(matches!(
            s.handle_key(key(KeyCode::Char('p'))),
            ScreenAction::None
        ));
    }
}
