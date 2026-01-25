mod browser;
pub mod centralise_dialog;
pub mod changes_dialog;
pub mod confirm_dialog;
mod dialogs;
pub mod duplicates;
pub mod edit_dialog;
pub mod export_dialog;
pub mod gallery;
pub mod move_dialog;
pub mod tag_dialog;
pub mod slideshow;
pub mod overdue_dialog;
pub mod people_dialog;
pub mod preview;
pub mod rename_dialog;
pub mod schedule_dialog;
pub mod search_dialog;
mod status_bar;
mod task_list_dialog;
pub mod trash_dialog;

use ratatui::prelude::*;
use ratatui::widgets::Clear;

use crate::app::{App, AppMode};

pub fn render(frame: &mut Frame, app: &mut App) {
    let area = frame.area();

    // If a full screen clear was requested (e.g., after exiting gallery/slideshow),
    // clear the entire screen first to remove any terminal graphics artifacts.
    // This is necessary because terminal image protocols (Sixel/Kitty) render
    // graphics independently of the text buffer.
    if app.clear_on_next_render {
        frame.render_widget(Clear, area);
        app.clear_on_next_render = false;
    }

    // Handle duplicates view mode
    if app.mode == AppMode::Duplicates || app.mode == AppMode::DuplicatesHelp {
        duplicates::render(frame, app, area);
        if app.mode == AppMode::DuplicatesHelp {
            duplicates::render_help(frame, area);
        }
        return;
    }

    // Handle gallery view mode
    if app.mode == AppMode::Gallery || app.mode == AppMode::GalleryHelp {
        gallery::render(frame, app, area);
        if app.mode == AppMode::GalleryHelp {
            gallery::render_help(frame, area);
        }
        return;
    }

    // Handle slideshow mode
    if app.mode == AppMode::Slideshow || app.mode == AppMode::SlideshowHelp {
        slideshow::render(frame, app, area);
        if app.mode == AppMode::SlideshowHelp {
            slideshow::render_help(frame, area);
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

    // Render edit description dialog if in edit mode
    if app.mode == AppMode::EditingDescription {
        if let Some(ref dialog) = app.edit_dialog {
            edit_dialog::render(frame, dialog, area);
        }
    }

    // Render changes dialog if in changes viewing mode
    if app.mode == AppMode::ChangesViewing {
        if let Some(ref dialog) = app.changes_dialog {
            changes_dialog::render(frame, dialog, area);
        }
    }

    // Render schedule dialog if in scheduling mode
    if app.mode == AppMode::Scheduling {
        if let Some(ref dialog) = app.schedule_dialog {
            schedule_dialog::render(frame, dialog, area);
        }
    }

    // Render overdue dialog if in overdue dialog mode
    if app.mode == AppMode::OverdueDialog {
        if let Some(ref dialog) = app.overdue_dialog {
            overdue_dialog::render(frame, dialog, area);
        }
    }

    // Render tag dialog if in tagging mode
    if app.mode == AppMode::Tagging {
        if let Some(ref dialog) = app.tag_dialog {
            tag_dialog::render(frame, dialog, area);
        }
    }

    // Render centralise dialog if in centralising mode
    if app.mode == AppMode::Centralising {
        if let Some(ref dialog) = app.centralise_dialog {
            centralise_dialog::render(frame, dialog, area);
        }
    }

    // Render confirm dialog if in confirming mode
    if app.mode == AppMode::Confirming {
        if let Some(ref dialog) = app.confirm_dialog {
            confirm_dialog::render(frame, dialog, area);
        }
    }
}
