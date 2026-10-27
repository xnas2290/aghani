use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    Frame,
};

use crate::app::App;
use crate::ui::confirm_overlay;
use crate::ui::{
    cover_panel,
    library_panel,
    lyrics_panel,
    lyrics_sync_overlay,
    metadata_overlay,
    notice,
    player_panel,
    save_overlay,
    search_overlay, // statusbar,
};

fn update_library_list_heights(app: &mut App, width: u16, height: u16) {
    let base_height = height.saturating_sub(4) as usize;
    let image_list = app.show_artwork
        && match app.active_tab {
            crate::library::LibraryTab::Albums => {
                matches!(&app.album_scope, crate::library::LibraryScope::All)
            }
            crate::library::LibraryTab::Artists => true,
            crate::library::LibraryTab::Playlists => {
                matches!(&app.playlist_scope, crate::library::PlaylistScope::All)
            }
            crate::library::LibraryTab::Songs => false,
        };

    if image_list {
        if app.grid_view {
            let columns = (width as usize / 18).max(1);
            app.list_height = columns * (base_height / 8).max(1);
            app.scoped_list_height = columns * (base_height.saturating_sub(1) / 8).max(1);
        } else {
            app.list_height = (base_height / 7).max(1);
            app.scoped_list_height = (base_height.saturating_sub(1) / 7).max(1);
        }
    } else {
        app.list_height = base_height;
        app.scoped_list_height = app.list_height.saturating_sub(1);
    }
}

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
        "clean" => draw_default_clean(f, app, main),
        "default_lyrics" => draw_default_lyrics(f, app, main),
        "compact_lyrics" => draw_compact_lyrics(f, app, main),
        "minimal_lyrics" => draw_minimal_lyrics(f, app, main),
        "clean_lyrics" => draw_default_lyrics_clean(f, app, main),
        _ => draw_default(f, app, main),
    }

    // statusbar::draw(f, app, status_area);
    save_overlay::draw(f, app);
    search_overlay::draw(f, app);
    metadata_overlay::draw(f, app);
    confirm_overlay::draw(f, app);
    lyrics_sync_overlay::draw(f, app);
    notice::draw(f, app);
}

pub fn library_area(app: &App, area: Rect) -> Option<Rect> {
    match app.layout_mode.as_str() {
        "default" => {
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(app.cover_width), Constraint::Min(0)])
                .split(area);
            Some(columns[1])
        }
        "default_lyrics" => {
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(app.cover_width),
                    Constraint::Min(20),
                    Constraint::Percentage(30),
                ])
                .split(area);
            Some(columns[1])
        }
        "compact" => {
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(0),
                    Constraint::Length(app.player_height - 5),
                ])
                .split(area);
            Some(rows[0])
        }
        "compact_lyrics" => {
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(0), Constraint::Percentage(50)])
                .split(area);
            let rows = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(0),
                    Constraint::Length(app.player_height.saturating_sub(5)),
                ])
                .split(columns[0]);
            Some(rows[0])
        }
        "minimal" => Some(area),
        "minimal_lyrics" => {
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(0), Constraint::Percentage(50)])
                .split(area);
            Some(columns[0])
        }
        _ => None,
    }
}

pub fn lyrics_area(app: &App, area: Rect) -> Option<Rect> {
    match app.layout_mode.as_str() {
        "default_lyrics" => {
            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(app.cover_width),
                    Constraint::Min(20),
                    Constraint::Percentage(30),
                ])
                .split(area);
            Some(columns[2])
        }
        "compact_lyrics" | "minimal_lyrics" => Some(
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(0), Constraint::Percentage(50)])
                .split(area)[1],
        ),
        "clean_lyrics" => Some(
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(app.cover_width), Constraint::Min(0)])
                .split(area)[1],
        ),
        _ => None,
    }
}

pub fn cover_area(app: &App, area: Rect) -> Option<Rect> {
    if !app.show_cover {
        return None;
    }

    match app.layout_mode.as_str() {
        "default" | "clean_lyrics" => {
            let left = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Length(app.cover_width), Constraint::Min(0)])
                .split(area)[0];
            Some(
                Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(app.cover_height), Constraint::Min(0)])
                    .split(left)[0],
            )
        }
        "default_lyrics" => {
            let left = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(app.cover_width),
                    Constraint::Min(20),
                    Constraint::Percentage(30),
                ])
                .split(area)[0];
            Some(
                Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(app.cover_height), Constraint::Min(0)])
                    .split(left)[0],
            )
        }
        "clean" => {
            let center = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(30),
                    Constraint::Percentage(40),
                    Constraint::Percentage(30),
                ])
                .split(area)[1];
            let top = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(app.cover_height), Constraint::Min(0)])
                .split(center)[0];
            Some(
                Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Min(0),
                        Constraint::Length(app.cover_width),
                        Constraint::Min(0),
                    ])
                    .split(top)[1],
            )
        }
        _ => None,
    }
}

pub fn player_area(app: &App, area: Rect) -> Option<Rect> {
    if !app.show_player {
        return None;
    }

    match app.layout_mode.as_str() {
        "default" | "default_lyrics" | "clean_lyrics" => {
            let width = if app.layout_mode == "default_lyrics" {
                Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Length(app.cover_width),
                        Constraint::Min(20),
                        Constraint::Percentage(30),
                    ])
                    .split(area)[0]
            } else {
                Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([Constraint::Length(app.cover_width), Constraint::Min(0)])
                    .split(area)[0]
            };
            Some(
                Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(app.cover_height), Constraint::Min(0)])
                    .split(width)[1],
            )
        }
        "compact" => Some(
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(0),
                    Constraint::Length(app.player_height - 5),
                ])
                .split(area)[1],
        ),
        "compact_lyrics" => {
            let main = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(0), Constraint::Percentage(50)])
                .split(area)[0];
            Some(
                Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Min(0),
                        Constraint::Length(app.player_height.saturating_sub(5)),
                    ])
                    .split(main)[1],
            )
        }
        "clean" => {
            let center = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(30),
                    Constraint::Percentage(40),
                    Constraint::Percentage(30),
                ])
                .split(area)[1];
            Some(
                Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(app.cover_height), Constraint::Min(0)])
                    .split(center)[1],
            )
        }
        _ => None,
    }
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

    update_library_list_heights(app, columns[1].width, columns[1].height);

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
    update_library_list_heights(app, columns[1].width, columns[1].height);

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

    update_library_list_heights(app, rows[0].width, rows[0].height);

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
    update_library_list_heights(app, left_rows[0].width, left_rows[0].height);

    // 4. Draw Panels
    library_panel::draw(f, app, left_rows[0]);

    if app.show_player {
        player_panel::draw_compact(f, app, left_rows[1]);
    }

    lyrics_panel::draw(f, app, main_columns[1]);
}

fn draw_minimal(f: &mut Frame, app: &mut App, area: Rect) {
    // Full screen library only
    update_library_list_heights(app, area.width, area.height);
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
    update_library_list_heights(app, columns[0].width, columns[0].height);

    // 3. Draw Panels
    library_panel::draw(f, app, columns[0]);
    lyrics_panel::draw(f, app, columns[1]);
}

fn draw_default_clean(f: &mut Frame, app: &mut App, area: Rect) {
    // 1. Center column (40%)
    let main_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Percentage(40),
            Constraint::Percentage(30),
        ])
        .split(area);

    let center_column = main_layout[1];

    // 2. Vertical split (rows have SAME width)
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(app.cover_height), Constraint::Min(0)])
        .split(center_column);

    // 3. Create a centered area INSIDE the top row (for the image only)
    let centered_image = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(app.cover_width), // image width only
            Constraint::Min(0),
        ])
        .split(rows[0])[1];

    // ---- DRAW ----

    if app.show_cover {
        cover_panel::draw(f, app, centered_image);
    }

    if app.show_player {
        player_panel::draw(f, app, rows[1]);
    }
}

fn draw_default_lyrics_clean(f: &mut Frame, app: &mut App, area: Rect) {
    // Exact same layout as default: Fixed sidebar width, flexible lyrics
    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Length(app.cover_width), Constraint::Min(0)])
        .split(area);

    let left_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(app.cover_height), Constraint::Min(0)])
        .split(columns[0]);

    // Centering the image inside its row while keeping the container fixed
    let centered_image_area = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Min(0),
            Constraint::Length(app.cover_width),
            Constraint::Min(0),
        ])
        .split(left_rows[0]);

    if app.show_cover {
        cover_panel::draw(f, app, centered_image_area[1]);
    }
    if app.show_player {
        // Player uses the full sidebar width (columns[0])
        player_panel::draw(f, app, left_rows[1]);
    }

    // Swapped library_panel for lyrics_panel
    lyrics_panel::draw(f, app, columns[1]);
}
