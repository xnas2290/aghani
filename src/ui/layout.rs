use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout}, //, Rect},
};

use crate::app::App;
use crate::ui::{
    cover_panel, library_panel, player_panel, save_overlay, search_overlay, statusbar,
};
// use crate::ui::search_overlay;

pub fn draw(f: &mut Frame, app: &mut App) {
    let size = f.area();

    // Top-level: main area + status bar
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),    // main content
            Constraint::Length(1), // status bar
        ])
        .split(size);

    let main = root[0];
    let status_area = root[1];

    // Main: left column (cover + player) | right column (library)
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(40), // left: cover + player
            Constraint::Min(0),     // right: library list
        ])
        .split(main);

    let left = columns[0];
    let right = columns[1];

    // Left column: cover on top, player info below
    let left_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(22), // cover art (≈ 20 half-block rows + border)
            Constraint::Min(0),     // player panel
        ])
        .split(left);
    // After splitting layout, add:
    app.list_height = right.height.saturating_sub(4) as usize;
    app.scoped_list_height = app.list_height.saturating_sub(1);
    cover_panel::draw(f, app, left_rows[0]);
    player_panel::draw(f, app, left_rows[1]);
    library_panel::draw(f, app, right);
    statusbar::draw(f, app, status_area);
    save_overlay::draw(f, app);
    search_overlay::draw(f, app);
}
