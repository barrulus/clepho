//! Force-reprocess dialog (spec §4.4). Lets the user choose which stages to
//! clear `<stage>_done_at` for, with a pre-flight count of values that will
//! be preserved by the provenance contract.

use clepho::pipeline::reprocess::{count_confirmed_values, count_user_values};
use clepho::pipeline::stages::StageId;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use rusqlite::Connection;

#[allow(dead_code)]
pub struct ReprocessDialog {
    pub folder: String,
    pub photo_count: i64,
    pub stages: [(StageId, bool); 6],
    /// 0..6 = stage rows; 6 = Cancel; 7 = Confirm.
    pub cursor: usize,
    pub preserved_user_values: i64,
    pub preserved_confirmed_values: i64,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum ReprocessOutcome {
    Pending,
    Cancel,
    Confirm(Vec<StageId>),
}

#[allow(dead_code)]
impl ReprocessDialog {
    pub fn new(conn: &Connection, folder: String) -> anyhow::Result<Self> {
        let prefix = format!("{}/%", folder.trim_end_matches('/'));
        let photo_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM photos WHERE path LIKE ?1",
            rusqlite::params![prefix],
            |r| r.get(0),
        )?;

        let preserved_user_values = count_user_values(conn, &folder)?;
        let preserved_confirmed_values = count_confirmed_values(conn, &folder)?;

        // LLM is the most common reprocess case (rerun with new model).
        let stages = [
            (StageId::Scan, false),
            (StageId::Exif, false),
            (StageId::Thumb, false),
            (StageId::Llm, true),
            (StageId::Faces, false),
            (StageId::Index, false),
        ];

        Ok(Self {
            folder,
            photo_count,
            stages,
            cursor: 0,
            preserved_user_values,
            preserved_confirmed_values,
        })
    }

    pub fn render(&self, f: &mut Frame, area: Rect) {
        f.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" Reprocess folder: {} ", self.folder));

        let mut lines: Vec<Line> = vec![
            Line::from(format!("Photos in folder: {}", self.photo_count)),
            Line::from(""),
            Line::from("Select stages to reset (space to toggle):"),
        ];
        for (i, (stage, sel)) in self.stages.iter().enumerate() {
            let cursor = if self.cursor == i { "> " } else { "  " };
            let mark = if *sel { "[x]" } else { "[ ]" };
            lines.push(Line::from(format!("{}{} {}", cursor, mark, stage.name())));
        }
        lines.push(Line::from(""));
        lines.push(Line::from(format!(
            "Will preserve: {} user-edited values, {} confirmed-AI values.",
            self.preserved_user_values, self.preserved_confirmed_values
        )));
        lines.push(Line::from(""));
        let cancel_cur = if self.cursor == 6 { "> " } else { "  " };
        let confirm_cur = if self.cursor == 7 { "> " } else { "  " };
        lines.push(Line::from(format!(
            "{}[Esc] Cancel    {}[Enter] Run",
            cancel_cur, confirm_cur
        )));

        let p = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false });
        f.render_widget(p, area);
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) -> ReprocessOutcome {
        use crossterm::event::KeyCode;
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.cursor = (self.cursor + 1).min(7);
                ReprocessOutcome::Pending
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
                ReprocessOutcome::Pending
            }
            KeyCode::Char(' ') if self.cursor < 6 => {
                self.stages[self.cursor].1 = !self.stages[self.cursor].1;
                ReprocessOutcome::Pending
            }
            KeyCode::Esc | KeyCode::Char('q') => ReprocessOutcome::Cancel,
            KeyCode::Enter => {
                let selected: Vec<StageId> = self
                    .stages
                    .iter()
                    .filter(|(_, s)| *s)
                    .map(|(id, _)| *id)
                    .collect();
                if selected.is_empty() {
                    ReprocessOutcome::Cancel
                } else {
                    ReprocessOutcome::Confirm(selected)
                }
            }
            _ => ReprocessOutcome::Pending,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clepho::db::apply_v2_schema;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent::new(c, KeyModifiers::empty())
    }

    fn fresh_dialog() -> ReprocessDialog {
        let c = Connection::open_in_memory().unwrap();
        apply_v2_schema(&c).unwrap();
        ReprocessDialog::new(&c, "/photos".into()).unwrap()
    }

    #[test]
    fn defaults_select_llm_only() {
        let d = fresh_dialog();
        assert_eq!(d.cursor, 0);
        let selected: Vec<_> = d
            .stages
            .iter()
            .filter(|(_, s)| *s)
            .map(|(id, _)| *id)
            .collect();
        assert_eq!(selected, vec![StageId::Llm]);
    }

    #[test]
    fn space_toggles_current_stage() {
        let mut d = fresh_dialog();
        d.handle_key(key(KeyCode::Char(' ')));
        assert!(d.stages[0].1, "scan should be selected after toggle");
        d.handle_key(key(KeyCode::Char(' ')));
        assert!(!d.stages[0].1);
    }

    #[test]
    fn cursor_navigation_clamped() {
        let mut d = fresh_dialog();
        for _ in 0..20 {
            d.handle_key(key(KeyCode::Char('j')));
        }
        assert_eq!(d.cursor, 7);
        for _ in 0..20 {
            d.handle_key(key(KeyCode::Char('k')));
        }
        assert_eq!(d.cursor, 0);
    }

    #[test]
    fn esc_cancels() {
        let mut d = fresh_dialog();
        assert!(matches!(
            d.handle_key(key(KeyCode::Esc)),
            ReprocessOutcome::Cancel
        ));
    }

    #[test]
    fn enter_with_selection_confirms_with_stages() {
        let mut d = fresh_dialog();
        // default has Llm selected
        match d.handle_key(key(KeyCode::Enter)) {
            ReprocessOutcome::Confirm(stages) => assert_eq!(stages, vec![StageId::Llm]),
            other => panic!("expected Confirm(Llm), got {:?}", other),
        }
    }

    #[test]
    fn enter_with_no_selection_falls_through_to_cancel() {
        let mut d = fresh_dialog();
        // Toggle off the default Llm selection
        d.handle_key(key(KeyCode::Char('j'))); // exif
        d.handle_key(key(KeyCode::Char('j'))); // thumb
        d.handle_key(key(KeyCode::Char('j'))); // llm
        d.handle_key(key(KeyCode::Char(' ')));
        assert!(matches!(
            d.handle_key(key(KeyCode::Enter)),
            ReprocessOutcome::Cancel
        ));
    }

    #[test]
    fn space_in_action_row_does_nothing() {
        let mut d = fresh_dialog();
        d.cursor = 6; // Cancel row
        d.handle_key(key(KeyCode::Char(' ')));
        // No stage was toggled by the space because cursor >= 6
        let selected: Vec<_> = d
            .stages
            .iter()
            .filter(|(_, s)| *s)
            .map(|(id, _)| *id)
            .collect();
        assert_eq!(selected, vec![StageId::Llm]);
    }
}
