mod browser;
mod dialogs;
pub mod duplicates;
mod preview;
mod status_bar;

use ratatui::prelude::*;

use crate::app::{App, AppMode};

pub fn render(frame: &mut Frame, app: &mut App) {
    let area = frame.area();

    // Handle duplicates view mode
    if app.mode == AppMode::Duplicates || app.mode == AppMode::DuplicatesHelp {
        if let Some(ref view) = app.duplicates_view {
            duplicates::render(frame, view, area);
            if app.mode == AppMode::DuplicatesHelp {
                duplicates::render_help(frame, area);
            }
        }
        return;
    }

    // Main layout: content area + status bar
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);

    // Three-column layout for the browser
    let browser_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(20), // Parent directory
            Constraint::Percentage(40), // Current directory
            Constraint::Percentage(40), // Preview
        ])
        .split(main_chunks[0]);

    // Render the three columns
    browser::render_parent(frame, app, browser_chunks[0]);
    browser::render_current(frame, app, browser_chunks[1]);
    preview::render(frame, app, browser_chunks[2]);

    // Render status bar
    status_bar::render(frame, app, main_chunks[1]);

    // Render help overlay if in help mode
    if app.mode == AppMode::Help {
        dialogs::render_help(frame, area);
    }
}
