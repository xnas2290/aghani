use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
};

use crate::app::App;
use crate::ui::confirm_overlay;
use crate::ui::{
    cover_panel,
    library_panel,
    lyrics_panel,
    player_panel,
    save_overlay,
    search_overlay, // statusbar,
};

pub fn draw(f: &mut Frame, app: &mut App) {
    let size = f.area();

    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0)]) //, Constraint::Length(1)])
        .split(size);

    let main = root[0];
    // let status_area = root[1];

    match app.layout_mode.as_str() {
        "minimal" => draw_minimal(f, app, main),
        "compact" => draw_compact(f, app, main),
        "default_lyrics" => draw_default_lyrics(f, app, main),
        "compact_lyrics" => draw_compact_lyrics(f, app, main), // Reuse compact for now
        "minimal_lyrics" => draw_minimal_lyrics(f, app, main), // Reuse wide for now
        _ => draw_default(f, app, main),
    }

    // statusbar::draw(f, app, status_area);
    save_overlay::draw(f, app);
    search_overlay::draw(f, app);
    confirm_overlay::draw(f, app);
}

fn draw_default(f: &mut Frame, app: &mut App, area: Rect) {
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(app.cover_width), Constraint::Min(0)])
        .split(area);

    let left_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(app.cover_height), Constraint::Min(0)])
        .split(columns[0]);

    app.list_height = columns[1].height.saturating_sub(4) as usize;
    app.scoped_list_height = app.list_height.saturating_sub(1);

    if app.show_cover {
        cover_panel::draw(f, app, left_rows[0]);
    }
    if app.show_player {
        player_panel::draw(f, app, left_rows[1]);
    }
    library_panel::draw(f, app, columns[1]);
}

fn draw_default_lyrics(f: &mut Frame, app: &mut App, area: Rect) {
    // 1. Horizontal split: Left (fixed), Middle (flexible), Right (larger)
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(app.cover_width), // Left sidebar
            Constraint::Min(20),                 // Library (minimum width)
            Constraint::Percentage(30),          // Lyrics (increased size)
        ])
        .split(area);

    // 2. Vertical split for the Left column (Cover & Player)
    let left_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(app.cover_height), Constraint::Min(0)])
        .split(columns[0]);

    // 3. Update list heights based on the Middle column
    app.list_height = columns[1].height.saturating_sub(4) as usize;
    app.scoped_list_height = app.list_height.saturating_sub(1);

    // 4. Draw Panels
    if app.show_cover {
        cover_panel::draw(f, app, left_rows[0]);
    }
    if app.show_player {
        player_panel::draw(f, app, left_rows[1]);
    }

    library_panel::draw(f, app, columns[1]);

    // This container is now bigger based on the Constraint above
    // if app.show_lyrics {
    lyrics_panel::draw(f, app, columns[2]);
    // }
}

fn draw_compact(f: &mut Frame, app: &mut App, area: Rect) {
    // No cover, smaller player
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(app.player_height - 5),
        ])
        .split(area);

    app.list_height = rows[0].height.saturating_sub(4) as usize;
    app.scoped_list_height = app.list_height.saturating_sub(1);

    if app.show_player {
        player_panel::draw_compact(f, app, rows[1]);
    }
    library_panel::draw(f, app, rows[0]);
}

fn draw_compact_lyrics(f: &mut Frame, app: &mut App, area: Rect) {
    // 1. Horizontal split: Main Content (Library/Player) vs Lyrics
    let main_columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),         // Main content takes remaining space
            Constraint::Percentage(50), // Lyrics sidebar
        ])
        .split(area);

    // 2. Vertical split for the Left column (Library on top, Player on bottom)
    let left_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(app.player_height.saturating_sub(5)),
        ])
        .split(main_columns[0]);

    // 3. Update list heights based on the new Library area (left_rows[0])
    app.list_height = left_rows[0].height.saturating_sub(4) as usize;
    app.scoped_list_height = app.list_height.saturating_sub(1);

    // 4. Draw Panels
    library_panel::draw(f, app, left_rows[0]);

    if app.show_player {
        player_panel::draw_compact(f, app, left_rows[1]);
    }

    lyrics_panel::draw(f, app, main_columns[1]);
}

fn draw_minimal(f: &mut Frame, app: &mut App, area: Rect) {
    // Full screen library only
    app.list_height = area.height.saturating_sub(4) as usize;
    app.scoped_list_height = app.list_height.saturating_sub(1);
    library_panel::draw(f, app, area);
}

fn draw_minimal_lyrics(f: &mut Frame, app: &mut App, area: Rect) {
    // 1. Simple Horizontal split
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),         // Library
            Constraint::Percentage(50), // Lyrics
        ])
        .split(area);

    // 2. Update list heights based on the Library column (columns[0])
    app.list_height = columns[0].height.saturating_sub(4) as usize;
    app.scoped_list_height = app.list_height.saturating_sub(1);

    // 3. Draw Panels
    library_panel::draw(f, app, columns[0]);
    lyrics_panel::draw(f, app, columns[1]);
}
