//! Shown once on first startup when the existing DB has the legacy schema (gen<2).
//! User must explicitly confirm to drop tables and apply v2.

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResetChoice {
    Pending,
    Confirmed,
    Declined,
}

#[allow(dead_code)]
pub struct ResetDbDialog {
    pub choice: ResetChoice,
}

impl Default for ResetDbDialog {
    fn default() -> Self {
        Self {
            choice: ResetChoice::Pending,
        }
    }
}

#[allow(dead_code)]
impl ResetDbDialog {
    pub fn render(&self, f: &mut Frame, area: Rect) {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" Database schema upgrade required ")
            .style(Style::default().fg(Color::Yellow));

        let body = "Clepho v2 uses a new schema. Your existing database will be DROPPED \
                    and recreated empty. Photos must be rescanned. There is no automatic data \
                    migration in v2.\n\n\
                    [y] Drop and reset    [n] Quit";

        let p = Paragraph::new(body).block(block).wrap(Wrap { trim: true });
        f.render_widget(Clear, area);
        f.render_widget(p, area);
    }

    pub fn handle_key(&mut self, key: crossterm::event::KeyEvent) {
        use crossterm::event::KeyCode;
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => self.choice = ResetChoice::Confirmed,
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.choice = ResetChoice::Declined
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(c: KeyCode) -> KeyEvent {
        KeyEvent::new(c, KeyModifiers::empty())
    }

    #[test]
    fn default_is_pending() {
        let d = ResetDbDialog::default();
        assert_eq!(d.choice, ResetChoice::Pending);
    }

    #[test]
    fn y_confirms() {
        let mut d = ResetDbDialog::default();
        d.handle_key(key(KeyCode::Char('y')));
        assert_eq!(d.choice, ResetChoice::Confirmed);
    }

    #[test]
    fn capital_y_also_confirms() {
        let mut d = ResetDbDialog::default();
        d.handle_key(key(KeyCode::Char('Y')));
        assert_eq!(d.choice, ResetChoice::Confirmed);
    }

    #[test]
    fn n_declines() {
        let mut d = ResetDbDialog::default();
        d.handle_key(key(KeyCode::Char('n')));
        assert_eq!(d.choice, ResetChoice::Declined);
    }

    #[test]
    fn esc_declines() {
        let mut d = ResetDbDialog::default();
        d.handle_key(key(KeyCode::Esc));
        assert_eq!(d.choice, ResetChoice::Declined);
    }

    #[test]
    fn other_keys_ignored() {
        let mut d = ResetDbDialog::default();
        d.handle_key(key(KeyCode::Char('q')));
        d.handle_key(key(KeyCode::Enter));
        assert_eq!(d.choice, ResetChoice::Pending);
    }
}
