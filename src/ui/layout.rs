use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
};

use crate::app::App;
use crate::ui::confirm_overlay;
use crate::ui::{
    cover_panel,
    library_panel,
    player_panel,
    save_overlay,
    search_overlay, // statusbar,
};

// At end of draw(), after search_overlay:

// pub fn draw(f: &mut Frame, app: &mut App) {
//     let size = f.area();

//     // Top-level: main area + status bar
//     let root = Layout::default()
//         .direction(Direction::Vertical)
//         .constraints([
//             Constraint::Min(0),    // main content
//             Constraint::Length(1), // status bar
//         ])
//         .split(size);

//     let main = root[0];
//     let status_area = root[1];

//     // Main: left column (cover + player) | right column (library)
//     let columns = Layout::default()
//         .direction(Direction::Horizontal)
//         .constraints([
//             Constraint::Length(40), // left: cover + player
//             Constraint::Min(0),     // right: library list
//         ])
//         .split(main);

//     let left = columns[0];
//     let right = columns[1];

//     // Left column: cover on top, player info below
//     let left_rows = Layout::default()
//         .direction(Direction::Vertical)
//         .constraints([
//             Constraint::Length(22), // cover art (≈ 20 half-block rows + border)
//             Constraint::Min(0),     // player panel
//         ])
//         .split(left);
//     // After splitting layout, add:
//     app.list_height = right.height.saturating_sub(4) as usize;
//     app.scoped_list_height = app.list_height.saturating_sub(1);
//     cover_panel::draw(f, app, left_rows[0]);
//     player_panel::draw(f, app, left_rows[1]);
//     library_panel::draw(f, app, right);
//     statusbar::draw(f, app, status_area);
//     save_overlay::draw(f, app);
//     search_overlay::draw(f, app);
// }

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
        "wide" => draw_wide(f, app, main),
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

fn draw_wide(f: &mut Frame, app: &mut App, area: Rect) {
    // Cover on left, library in middle, player on right
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(app.cover_width),
            Constraint::Min(0),
            Constraint::Length(app.cover_width),
        ])
        .split(area);

    app.list_height = columns[1].height.saturating_sub(4) as usize;
    app.scoped_list_height = app.list_height.saturating_sub(1);

    if app.show_cover {
        cover_panel::draw(f, app, columns[0]);
    }
    library_panel::draw(f, app, columns[1]);
    if app.show_player {
        player_panel::draw(f, app, columns[2]);
    }
}

fn draw_minimal(f: &mut Frame, app: &mut App, area: Rect) {
    // Full screen library only
    app.list_height = area.height.saturating_sub(4) as usize;
    app.scoped_list_height = app.list_height.saturating_sub(1);
    library_panel::draw(f, app, area);
}
