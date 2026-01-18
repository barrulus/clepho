mod browser;
mod dialogs;
pub mod duplicates;
pub mod export_dialog;
pub mod move_dialog;
pub mod people_dialog;
pub mod preview;
pub mod rename_dialog;
pub mod search_dialog;
mod status_bar;
mod task_list_dialog;
pub mod trash_dialog;

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

    // Render move dialog if in move mode
    if app.mode == AppMode::Moving {
        if let Some(ref dialog) = app.move_dialog {
            move_dialog::render(frame, dialog, area);
        }
    }

    // Render rename dialog if in rename mode
    if app.mode == AppMode::Renaming {
        if let Some(ref dialog) = app.rename_dialog {
            rename_dialog::render(frame, dialog, area);
        }
    }

    // Render export dialog if in export mode
    if app.mode == AppMode::Exporting {
        if let Some(ref dialog) = app.export_dialog {
            export_dialog::render(frame, dialog, area);
        }
    }

    // Render search dialog if in search mode
    if app.mode == AppMode::Searching {
        if let Some(ref dialog) = app.search_dialog {
            search_dialog::render(frame, dialog, area);
        }
    }

    // Render people dialog if in people management mode
    if app.mode == AppMode::PeopleManaging {
        if app.people_dialog.is_some() {
            people_dialog::render(frame, app, area);
        }
    }

    // Render task list dialog if in task list mode
    if app.mode == AppMode::TaskList {
        task_list_dialog::render(frame, app);
    }

    // Render trash dialog if in trash viewing mode
    if app.mode == AppMode::TrashViewing {
        if let Some(ref dialog) = app.trash_dialog {
            trash_dialog::render(frame, dialog, area);
        }
    }
}
