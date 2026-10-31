use crate::app::App;
use crate::library::{LibraryScope, LibraryTab, PlaylistScope};
use crate::ui::make_list_state;
use crate::ui::playlist_panel;
use crate::ui::theme::Theme;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    layout::Alignment,
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Tabs},
    Frame,
};
use ratatui_image::StatefulImage;
use unicode_width::UnicodeWidthStr;

pub fn handle_mouse_click(app: &mut App, area: Rect, x: u16, y: u16) -> bool {
    if x < area.x || x >= area.x + area.width || y < area.y || y >= area.y + area.height {
        return false;
    }

    let tab_y = area.y + 1;
    let mut tab_x = area.x + 1;
    if y == tab_y {
        for tab in LibraryTab::all() {
            let width = format!(" {} ", tab.label()).width() as u16;
            if x >= tab_x && x < tab_x + width {
                app.select_tab(tab.clone());
                return true;
            }
            tab_x = tab_x.saturating_add(width + 1);
        }
        return false;
    }

    let mut row = y.saturating_sub(area.y + 2) as usize;
    let image_item_height = match app.active_tab {
        LibraryTab::Albums if matches!(&app.album_scope, LibraryScope::All) => 7,
        LibraryTab::Artists => 7,
        LibraryTab::Playlists if matches!(&app.playlist_scope, PlaylistScope::All) => 7,
        _ => 0,
    };
    let (offset, length) = match app.active_tab {
        LibraryTab::Songs => (app.songs_offset, app.all_track_indices.len()),
        LibraryTab::Albums => match &app.album_scope {
            LibraryScope::Album(_) => (app.scoped_songs_offset, app.scoped_track_indices.len()),
            _ => (app.albums_offset, app.albums.len()),
        },
        LibraryTab::Artists => match &app.artist_scope {
            LibraryScope::Artist(_) => (app.artist_albums_offset, app.scoped_albums.len()),
            _ => (app.artists_offset, app.artists.len()),
        },
        LibraryTab::Playlists => match &app.playlist_scope {
            PlaylistScope::Open(pi) => (
                app.playlist_songs_offset,
                app.playlist_track_indices(*pi).len(),
            ),
            PlaylistScope::All => (app.playlist_list_offset, app.playlists.len()),
        },
    };

    if matches!(app.active_tab, LibraryTab::Albums)
        && matches!(&app.album_scope, LibraryScope::Album(_))
        || matches!(app.active_tab, LibraryTab::Artists)
            && matches!(&app.artist_scope, LibraryScope::Artist(_))
        || matches!(app.active_tab, LibraryTab::Playlists)
            && matches!(&app.playlist_scope, PlaylistScope::Open(_))
    {
        row = row.saturating_sub(1);
    }
    let grid_view = app.show_artwork
        && app.grid_view
        && matches!(
            app.active_tab,
            LibraryTab::Albums | LibraryTab::Artists | LibraryTab::Playlists
        )
        && !matches!(
            (&app.active_tab, &app.album_scope, &app.playlist_scope),
            (LibraryTab::Albums, LibraryScope::Album(_), _)
                | (LibraryTab::Playlists, _, PlaylistScope::Open(_))
        );
    let index = if grid_view {
        let columns = (area.width.saturating_sub(2) / 18).max(1) as usize;
        let column = x.saturating_sub(area.x + 1) as usize / 18;
        let grid_row = row / 8;
        (offset / columns) * columns + grid_row * columns + column
    } else {
        if image_item_height > 0 && app.show_artwork {
            row /= image_item_height;
        }
        offset + row
    };
    if index >= length {
        return false;
    }

    match app.active_tab {
        LibraryTab::Songs => app.selected_index = Some(index),
        LibraryTab::Albums => match &app.album_scope {
            LibraryScope::Album(_) => app.scoped_song_selected = Some(index),
            _ => app.album_selected = Some(index),
        },
        LibraryTab::Artists => match &app.artist_scope {
            LibraryScope::Artist(_) => app.artist_scoped_album_selected = Some(index),
            _ => app.artist_selected = Some(index),
        },
        LibraryTab::Playlists => match &app.playlist_scope {
            PlaylistScope::Open(_) => app.playlist_song_selected = Some(index),
            PlaylistScope::All => app.playlist_selected = Some(index),
        },
    }
    true
}

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Theme::border());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // tabs
            Constraint::Min(0),    // content
            Constraint::Length(1), // status line
        ])
        .split(inner);

    let tab_titles: Vec<Line> = LibraryTab::all()
        .iter()
        .map(|t| Line::from(Span::styled(format!(" {} ", t.label()), Theme::normal())))
        .collect();

    let tabs = Tabs::new(tab_titles)
        .select(app.active_tab.index())
        .highlight_style(Theme::highlight())
        .divider(Span::styled("│", Theme::dim()));
    f.render_widget(tabs, chunks[0]);

    match app.active_tab {
        LibraryTab::Songs => draw_songs(f, app, chunks[1]),
        LibraryTab::Albums => draw_albums(f, app, chunks[1]),
        LibraryTab::Artists => draw_artists(f, app, chunks[1]),
        LibraryTab::Playlists => playlist_panel::draw(f, app, chunks[1]),
    }

    if app.loading {
        let width = chunks[1].width.min(52);
        let height = 5.min(chunks[1].height);
        let x = chunks[1].x + chunks[1].width.saturating_sub(width) / 2;
        let y = chunks[1].y + chunks[1].height.saturating_sub(height) / 2;
        let loader_area = Rect::new(x, y, width, height);
        f.render_widget(Clear, loader_area);
        f.render_widget(
            Paragraph::new(format!(
                "Scanning music library…\n{} / {} tracks",
                app.loaded_count,
                app.tracks.len()
            ))
            .block(Block::default().borders(Borders::ALL).title(" Loading ")),
            loader_area,
        );
    }

    // let in_scope = match app.active_tab {
    //     LibraryTab::Albums => app.album_scope != LibraryScope::All,
    //     LibraryTab::Artists => app.artist_scope != LibraryScope::All,
    //     LibraryTab::Playlists => matches!(app.playlist_scope, PlaylistScope::Open(_)),
    //     _ => false,
    // };

    // let hint = if in_scope {
    //     "  [x] back  [Enter] play"
    // } else {
    //     "  [Enter] open  [Tab] switch"
    // };

    let sort_text = format!("Sort: {}", app.sort_label());
    let sort_width = sort_text
        .width()
        .min(chunks[2].width.saturating_sub(1) as usize) as u16;
    let status_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(0), Constraint::Length(sort_width)])
        .split(chunks[2]);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(status_text(app), Theme::accent()))),
        status_chunks[0],
    );
    f.render_widget(
        Paragraph::new(Span::styled(sort_text, Theme::dim())).alignment(Alignment::Right),
        status_chunks[1],
    );
}

pub fn render_artwork_rows(
    f: &mut Frame,
    app: &mut App,
    area: Rect,
    keys: &[String],
    offset: usize,
) {
    if !app.show_artwork {
        return;
    }
    const ROW_HEIGHT: u16 = 7;
    const IMAGE_WIDTH: u16 = 12;
    const IMAGE_HEIGHT: u16 = 6;
    // Image protocols can leave terminal graphics behind when a list scrolls.
    // Clear only image rows; the label row below each image must remain intact.
    let slots = (area.height.saturating_add(ROW_HEIGHT - 1)) / ROW_HEIGHT;
    for slot in 0..slots {
        let y = area.y.saturating_add(slot * ROW_HEIGHT);
        if y >= area.bottom() {
            break;
        }
        f.render_widget(
            Clear,
            Rect::new(
                area.x,
                y,
                IMAGE_WIDTH.min(area.width),
                IMAGE_HEIGHT.min(area.bottom().saturating_sub(y)),
            ),
        );
    }
    for (visible, key) in keys.iter().skip(offset).enumerate() {
        let y = area.y.saturating_add(visible as u16 * ROW_HEIGHT);
        if y >= area.bottom() {
            break;
        }
        let height = IMAGE_HEIGHT.min(area.bottom().saturating_sub(y));
        ensure_artwork_protocol(app, key);
        if let Some(protocol) = app.artwork_protocols.get_mut(key) {
            f.render_stateful_widget(
                StatefulImage::new(),
                Rect::new(area.x, y, IMAGE_WIDTH.min(area.width), height),
                protocol,
            );
        }
    }
}

fn ensure_artwork_protocol(app: &mut App, key: &str) {
    // Only visible rows reach this function, so this is also where artwork
    // loading is started. Placeholder art keeps the layout stable meanwhile.
    app.ensure_artwork_requested(key);
    if !app.artwork_protocols.contains_key(key) {
        let placeholder = crate::library::artwork::placeholder();
        app.artwork_protocols.insert(
            key.to_string(),
            app.picker.new_resize_protocol(placeholder.image()),
        );
    }
}

pub fn draw_image_grid(
    f: &mut Frame,
    app: &mut App,
    area: Rect,
    keys: &[String],
    labels: &[String],
    selected: Option<usize>,
    offset: usize,
) {
    const TILE_WIDTH: u16 = 18;
    const TILE_HEIGHT: u16 = 8;
    const IMAGE_WIDTH: u16 = 16;
    const IMAGE_HEIGHT: u16 = 7;
    let columns = (area.width / TILE_WIDTH).max(1) as usize;
    let rows = (area.height / TILE_HEIGHT).max(1) as usize;
    let capacity = columns * rows;
    let start = (offset / columns) * columns;

    f.render_widget(Clear, area);
    for (slot, index) in (start..keys.len()).take(capacity).enumerate() {
        let column = (slot % columns) as u16;
        let row = (slot / columns) as u16;
        let x = area.x + column * TILE_WIDTH;
        let y = area.y + row * TILE_HEIGHT;
        let tile_width = TILE_WIDTH.min(area.right().saturating_sub(x));
        let image_width = IMAGE_WIDTH.min(tile_width);
        let image_height = IMAGE_HEIGHT.min(area.bottom().saturating_sub(y));
        if image_width == 0 || image_height == 0 {
            continue;
        }

        let style = if selected == Some(index) {
            Theme::highlight()
        } else {
            Theme::normal()
        };
        f.render_widget(
            Block::default().style(style),
            Rect::new(
                x,
                y,
                tile_width,
                TILE_HEIGHT.min(area.bottom().saturating_sub(y)),
            ),
        );
        ensure_artwork_protocol(app, &keys[index]);
        if let Some(protocol) = app.artwork_protocols.get_mut(&keys[index]) {
            f.render_stateful_widget(
                StatefulImage::new(),
                Rect::new(x, y, image_width, image_height),
                protocol,
            );
        }
        let label_area = Rect::new(
            x,
            y.saturating_add(IMAGE_HEIGHT),
            tile_width,
            area.bottom()
                .saturating_sub(y.saturating_add(IMAGE_HEIGHT))
                .min(1),
        );
        if label_area.height > 0 {
            f.render_widget(
                Paragraph::new(crate::ui::prepare_text(&labels[index])).style(style),
                label_area,
            );
        }
    }
}

fn status_text(app: &App) -> String {
    if app.loading {
        return format!(
            "Loading metadata… {}/{} tracks",
            app.loaded_count,
            app.tracks.len()
        );
    }
    if app.artwork_pending_count() > 0 {
        return format!(
            "Preparing artwork cache… {} image(s) in background",
            app.artwork_pending_count()
        );
    }
    match app.active_tab {
        LibraryTab::Songs => {
            if let Some(current) = app.current_index {
                if let Some(pos) = app.all_track_indices.iter().position(|&i| i == current) {
                    return format!(
                        "{}  {}/{}",
                        app.selected_index.map(|i| i + 1).unwrap_or(0),
                        pos + 1,
                        app.all_track_indices.len()
                    );
                }
            }
            format!("0/{}", app.all_track_indices.len())
        }

        LibraryTab::Albums => match &app.album_scope {
            LibraryScope::Album(_) => {
                if let Some(current) = app.current_index {
                    if let Some(pos) = app.scoped_track_indices.iter().position(|&i| i == current) {
                        return format!(
                            "{}  {}/{}",
                            app.scoped_song_selected.map(|i| i + 1).unwrap_or(0),
                            pos + 1,
                            app.scoped_track_indices.len()
                        );
                    }
                }
                format!("0/{}", app.scoped_track_indices.len())
            }
            LibraryScope::All => format!(
                "{}  {} albums",
                app.album_selected.map(|i| i + 1).unwrap_or(0),
                app.albums.len()
            ),
            _ => String::new(),
        },

        LibraryTab::Artists => match &app.artist_scope {
            LibraryScope::Artist(_) => {
                // you're browsing albums here, so just show album count
                format!(
                    "{}/{}",
                    app.artist_scoped_album_selected.map(|i| i + 1).unwrap_or(0),
                    app.scoped_albums.len()
                )
            }
            LibraryScope::All => format!(
                "{}  {} artists",
                app.artist_selected.map(|i| i + 1).unwrap_or(0),
                app.artists.len()
            ),
            _ => String::new(),
        },

        LibraryTab::Playlists => match &app.playlist_scope {
            PlaylistScope::Open(pi) => {
                let indices = app.playlist_track_indices(*pi);

                if let Some(current) = app.current_index {
                    if let Some(pos) = indices.iter().position(|&i| i == current) {
                        return format!(
                            "{}  {}/{}",
                            app.playlist_song_selected.map(|i| i + 1).unwrap_or(0),
                            pos + 1,
                            indices.len()
                        );
                    }
                }

                format!("0/{}", indices.len())
            }

            PlaylistScope::All => format!(
                "{}  {} playlists",
                app.playlist_selected.map(|i| i + 1).unwrap_or(0),
                app.playlists.len()
            ),
        },
    }
}

fn draw_songs(f: &mut Frame, app: &App, area: Rect) {
    let indices = &app.all_track_indices;
    let start = visible_start(indices.len(), app.selected_index, app.songs_offset, area.height);
    let end = (start + area.height as usize).min(indices.len());
    let items: Vec<ListItem> = (start..end)
        .map(|i| {
            let real_idx = indices[i];
            let t = &app.tracks[real_idx];
            let is_playing = app.current_index == Some(real_idx);
            let is_selected = app.selected_index == Some(i);

            let style = if is_selected {
                Theme::highlight()
            } else if is_playing {
                Theme::accent()
            } else {
                Theme::normal()
            };

            let is_favorite = app.is_favorite(&t.path);
            let favorite_icon = if is_favorite { " ♥ " } else { "   " };
            let right_text = if t.extension().len() == 3 {
                format!("{}  [{}]", t.duration_str(), t.extension().to_uppercase())
            // Add an extra space if extension is 3 chars
            } else {
                format!("{} [{}]", t.duration_str(), t.extension().to_uppercase())
            };
            let right_width = right_text.width();
            let total_offset = 4; // Space for heart and padding
            let max_title_width = (area.width as usize).saturating_sub(right_width + total_offset);

            let display_title = if t.title.width() > max_title_width {
                let mut s = String::new();
                let mut w = 2;
                for c in t.title.chars() {
                    let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
                    if w + cw > max_title_width {
                        if s.len() < t.title.len() {
                            s.push('…');
                        }
                        break;
                    }
                    s.push(c);
                    w += cw;
                }
                s
            } else {
                t.title.clone()
            };

            let padding_len = (area.width as usize)
                .saturating_sub(display_title.width() + right_width + total_offset);

            ListItem::new(Line::from(vec![
                Span::styled(favorite_icon, style),
                Span::styled(crate::ui::prepare_text(&display_title), style),
                Span::raw(" ".repeat(padding_len)),
                Span::styled(right_text, if is_selected { style } else { Theme::dim() }),
            ]))
            .style(style)
        })
        .collect();

    let relative = relative_selection(app.selected_index, start, items.len());
    let mut state = make_list_state(relative, 0);
    f.render_stateful_widget(List::new(items).highlight_symbol(""), area, &mut state);
}

/// First row index to build for a viewport. Corrects a stale offset when the
/// selection moved or the terminal was resized, mirroring `scroll_offset`.
pub(crate) fn visible_start(
    len: usize,
    selected: Option<usize>,
    offset: usize,
    height: u16,
) -> usize {
    if len == 0 || height == 0 {
        return 0;
    }
    let height = height as usize;
    let mut start = offset.min(len - 1);
    if let Some(selected) = selected {
        let selected = selected.min(len - 1);
        if selected < start {
            start = selected;
        } else if selected >= start + height {
            start = selected + 1 - height;
        }
    }
    start
}

pub(crate) fn relative_selection(
    selected: Option<usize>,
    start: usize,
    visible_len: usize,
) -> Option<usize> {
    selected
        .and_then(|selected| selected.checked_sub(start))
        .filter(|&relative| relative < visible_len)
}

fn draw_albums(f: &mut Frame, app: &mut App, area: Rect) {
    if let LibraryScope::Album(album_name) = &app.album_scope {
        let sub_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(area);

        let total_secs: u64 = app
            .scoped_track_indices
            .iter()
            .map(|&idx| app.tracks[idx].duration.as_secs())
            .sum();
        let total_time = format!("{:02}:{:02}", total_secs / 60, total_secs % 60);

        // Calculate available width for album name
        let right_text = format!("[{}]  ", total_time);
        let right_width = right_text.width();
        let total_offset = 4; // Space for padding before and after album name
        let available_width = sub_chunks[0].width as usize;
        let max_name_width = available_width.saturating_sub(right_width + total_offset);

        let display_name = if album_name.width() > max_name_width {
            let mut s = String::new();
            let mut w = 2;
            for c in album_name.chars() {
                let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
                if w + cw > max_name_width {
                    if s.len() < album_name.len() {
                        s.push('…');
                    }
                    break;
                }
                s.push(c);
                w += cw;
            }
            s
        } else {
            album_name.clone()
        };

        let padding_len =
            available_width.saturating_sub(display_name.width() + right_width + total_offset);

        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    display_name.to_uppercase(),
                    Theme::accent().add_modifier(Modifier::BOLD),
                ),
                Span::raw(" ".repeat(padding_len)),
                Span::styled(right_text, Theme::normal()),
            ])),
            sub_chunks[0],
        );

        let indices = &app.scoped_track_indices;
        let list_area = sub_chunks[1];
        let start = visible_start(
            indices.len(),
            app.scoped_song_selected,
            app.scoped_songs_offset,
            list_area.height,
        );
        let end = (start + list_area.height as usize).min(indices.len());
        let items: Vec<ListItem> = (start..end)
            .map(|i| {
                let idx = indices[i];
                let t = &app.tracks[idx];
                let is_playing = app.current_index == Some(idx);
                let is_selected = app.scoped_song_selected == Some(i);

                let style = if is_selected {
                    Theme::highlight()
                } else if is_playing {
                    Theme::accent()
                } else {
                    Theme::normal()
                };

                let favorite_icon = if app.is_favorite(&t.path) {
                    "♥ "
                } else {
                    "  "
                };
                let right_text = format!(" {} {}", t.duration_str(), t.extension().to_uppercase());
                let right_width = right_text.width();
                let total_offset = 4; // Space for heart and padding
                let available_width = list_area.width as usize;
                let max_title_width = available_width.saturating_sub(right_width + total_offset);

                let display_title = if t.title.width() > max_title_width {
                    let mut s = String::new();
                    let mut w = 2;
                    for c in t.title.chars() {
                        let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
                        if w + cw > max_title_width {
                            if s.len() < t.title.len() {
                                s.push('…');
                            }
                            break;
                        }
                        s.push(c);
                        w += cw;
                    }
                    s
                } else {
                    t.title.clone()
                };

                let padding_len = available_width
                    .saturating_sub(display_title.width() + right_width + total_offset);

                ListItem::new(Line::from(vec![
                    Span::styled(favorite_icon, style),
                    Span::styled(crate::ui::prepare_text(&display_title), style),
                    Span::raw(" ".repeat(padding_len)),
                    Span::styled(right_text, if is_selected { style } else { Theme::dim() }),
                ]))
                .style(style)
            })
            .collect();

        let relative = relative_selection(app.scoped_song_selected, start, items.len());
        let mut state = make_list_state(relative, 0);
        f.render_stateful_widget(
            List::new(items).highlight_symbol(""),
            list_area,
            &mut state,
        );
    } else {
        if app.grid_view && app.show_artwork {
            let keys: Vec<String> = app
                .albums
                .iter()
                .map(|album| format!("album:{album}"))
                .collect();
            let labels = app.albums.clone();
            draw_image_grid(
                f,
                app,
                area,
                &keys,
                &labels,
                app.album_selected,
                app.albums_offset,
            );
            return;
        }
        let items: Vec<ListItem> = app
            .albums
            .iter()
            .enumerate()
            .map(|(i, album)| {
                let is_active = app
                    .current_track()
                    .map(|t| &t.album == album)
                    .unwrap_or(false);
                let is_selected = app.album_selected == Some(i);

                let style = if is_selected {
                    Theme::highlight()
                } else if is_active {
                    Theme::accent()
                } else {
                    Theme::normal()
                };

                let count = app.album_track_count(album);
                let right_info = if count == 1 {
                    " 1 song  ".to_string()
                } else {
                    format!("{} songs ", count)
                };

                let right_width = right_info.width();
                let total_offset = 2; // Padding
                let max_name_width =
                    (area.width as usize).saturating_sub(right_width + total_offset);

                let display_name = if album.width() > max_name_width {
                    let mut s = String::new();
                    let mut w = 2;
                    for c in album.chars() {
                        let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
                        if w + cw > max_name_width {
                            if s.len() < album.len() {
                                s.push('…');
                            }
                            break;
                        }
                        s.push(c);
                        w += cw;
                    }
                    s
                } else {
                    album.clone()
                };

                let padding_len = (area.width as usize)
                    .saturating_sub(display_name.width() + right_width + total_offset);

                let text_line = Line::from(vec![
                    Span::styled(crate::ui::prepare_text(&display_name), style),
                    Span::raw(" ".repeat(padding_len)),
                    Span::styled(right_info, if is_selected { style } else { Theme::dim() }),
                ]);
                let lines = if app.show_artwork {
                    let mut lines = vec![Line::from("            "); 6];
                    lines.push(text_line);
                    lines
                } else {
                    vec![text_line]
                };
                ListItem::new(lines).style(style)
            })
            .collect();

        let mut state = make_list_state(app.album_selected, app.albums_offset);
        let keys: Vec<String> = app
            .albums
            .iter()
            .map(|album| format!("album:{album}"))
            .collect();
        f.render_stateful_widget(List::new(items).highlight_symbol(""), area, &mut state);
        render_artwork_rows(f, app, area, &keys, app.albums_offset);
    }
}

fn draw_artists(f: &mut Frame, app: &mut App, area: Rect) {
    if let LibraryScope::Artist(artist_name) = &app.artist_scope {
        let sub_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(area);

        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    crate::ui::prepare_text(&artist_name.to_uppercase()),
                    Theme::accent().add_modifier(Modifier::BOLD),
                ),
            ])),
            sub_chunks[0],
        );

        if app.grid_view && app.show_artwork {
            let keys: Vec<String> = app
                .scoped_albums
                .iter()
                .map(|album| format!("album:{album}"))
                .collect();
            let labels = app.scoped_albums.clone();
            draw_image_grid(
                f,
                app,
                sub_chunks[1],
                &keys,
                &labels,
                app.artist_scoped_album_selected,
                app.artist_albums_offset,
            );
            return;
        }

        let items: Vec<ListItem> = app
            .scoped_albums
            .iter()
            .enumerate()
            .map(|(i, album)| {
                let is_selected = app.artist_scoped_album_selected == Some(i);
                let style = if is_selected {
                    Theme::highlight()
                } else {
                    Theme::normal()
                };

                let count = app.album_track_count(album);
                let right_info = if count == 1 {
                    "1 song".to_string()
                } else {
                    format!("{} songs", count)
                };
                let padding = " ".repeat(
                    (sub_chunks[1].width as usize)
                        .saturating_sub(album.width() + right_info.width() + 6),
                );

                let text_line = Line::from(vec![
                    Span::styled("  ", style),
                    Span::styled(crate::ui::prepare_text(album), style),
                    Span::raw(padding),
                    Span::styled(right_info, if is_selected { style } else { Theme::dim() }),
                ]);
                let lines = if app.show_artwork {
                    let mut lines = vec![Line::from("            "); 6];
                    lines.push(text_line);
                    lines
                } else {
                    vec![text_line]
                };
                ListItem::new(lines).style(style)
            })
            .collect();

        let mut state = make_list_state(app.artist_scoped_album_selected, app.artist_albums_offset);
        let keys: Vec<String> = app
            .scoped_albums
            .iter()
            .map(|album| format!("album:{album}"))
            .collect();
        f.render_stateful_widget(
            List::new(items).highlight_symbol(""),
            sub_chunks[1],
            &mut state,
        );
        render_artwork_rows(f, app, sub_chunks[1], &keys, app.artist_albums_offset);
    } else {
        if app.grid_view && app.show_artwork {
            let keys: Vec<String> = app
                .artists
                .iter()
                .map(|artist| format!("artist:{artist}"))
                .collect();
            let labels = app.artists.clone();
            draw_image_grid(
                f,
                app,
                area,
                &keys,
                &labels,
                app.artist_selected,
                app.artists_offset,
            );
            return;
        }
        let items: Vec<ListItem> = app
            .artists
            .iter()
            .enumerate()
            .map(|(i, artist)| {
                let is_selected = app.artist_selected == Some(i);
                let style = if is_selected {
                    Theme::highlight()
                } else {
                    Theme::normal()
                };

                let count = app.artist_album_counts.get(artist).copied().unwrap_or(0);
                let right_info = if count == 1 {
                    "  1 album ".to_string()
                } else {
                    format!("  {} albums", count)
                };

                let right_width = right_info.width();
                let total_offset = 6;
                let max_name_width =
                    (area.width as usize).saturating_sub(right_width + total_offset);

                let display_name = if artist.width() > max_name_width {
                    let mut s = String::new();
                    let mut w = 0;
                    for c in artist.chars() {
                        let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
                        if w + cw + 1 > max_name_width {
                            s.push('…');
                            break;
                        }
                        s.push(c);
                        w += cw;
                    }
                    s
                } else {
                    artist.clone()
                };

                let padding = " ".repeat(
                    (area.width as usize)
                        .saturating_sub(display_name.width() + right_width + total_offset),
                );

                let text_line = Line::from(vec![
                    Span::styled("  ", style),
                    Span::styled(crate::ui::prepare_text(&display_name), style),
                    Span::raw(padding),
                    Span::styled(right_info, if is_selected { style } else { Theme::dim() }),
                ]);
                let lines = if app.show_artwork {
                    let mut lines = vec![Line::from("            "); 6];
                    lines.push(text_line);
                    lines
                } else {
                    vec![text_line]
                };
                ListItem::new(lines).style(style)
            })
            .collect();

        let mut state = make_list_state(app.artist_selected, app.artists_offset);
        let keys: Vec<String> = app
            .artists
            .iter()
            .map(|artist| format!("artist:{artist}"))
            .collect();
        f.render_stateful_widget(List::new(items).highlight_symbol(""), area, &mut state);
        render_artwork_rows(f, app, area, &keys, app.artists_offset);
    }
}

#[cfg(test)]
mod tests {
    use super::{relative_selection, visible_start};

    #[test]
    fn visible_start_keeps_selection_in_view() {
        assert_eq!(visible_start(100, Some(50), 0, 10), 41);
        assert_eq!(visible_start(100, Some(5), 20, 10), 5);
        assert_eq!(visible_start(100, Some(95), 0, 10), 86);
        assert_eq!(visible_start(0, None, 0, 10), 0);
    }

    #[test]
    fn relative_selection_is_none_outside_viewport() {
        assert_eq!(relative_selection(Some(45), 41, 10), Some(4));
        assert_eq!(relative_selection(Some(40), 41, 10), None);
        assert_eq!(relative_selection(None, 41, 10), None);
    }
}
