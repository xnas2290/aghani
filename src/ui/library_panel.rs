use crate::app::App;
use crate::library::{LibraryScope, LibraryTab, PlaylistScope};
use crate::ui::make_list_state;
use crate::ui::playlist_panel;
use crate::ui::theme::Theme;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, Paragraph, Tabs},
};
use unicode_width::UnicodeWidthStr;

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
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

    let status_line = Line::from(vec![
        Span::styled(status_text(app), Theme::accent()),
        // Span::styled(hint, Theme::dim()),
    ]);

    f.render_widget(Paragraph::new(status_line), chunks[2]);
}

fn status_text(app: &App) -> String {
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
                app.selected_index.map(|i| i + 1).unwrap_or(0),
                app.playlists.len()
            ),
        },
    }
}
fn draw_songs(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .all_track_indices
        .iter()
        .enumerate()
        .map(|(i, &real_idx)| {
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

            let icon = if is_playing { "▶ " } else { "  " };
            let right_text = if t.extension().len() == 3 {
                format!("{}  [{}]", t.duration_str(), t.extension().to_uppercase()) // Add an extra space if extension is 3 chars
            } else {
                format!("{} [{}]", t.duration_str(), t.extension().to_uppercase())
            };
            let right_width = right_text.width();
            let total_offset = 6; // Space for icon, highlight symbol, and padding
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
                Span::styled(icon, style),
                Span::styled(display_title, style),
                Span::raw(" ".repeat(padding_len)),
                Span::styled(right_text, if is_selected { style } else { Theme::dim() }),
            ]))
            .style(style)
        })
        .collect();

    let mut state = make_list_state(app.selected_index, app.songs_offset);
    f.render_stateful_widget(List::new(items).highlight_symbol("› "), area, &mut state);
}

fn draw_albums(f: &mut Frame, app: &App, area: Rect) {
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

        let items: Vec<ListItem> = app
            .scoped_track_indices
            .iter()
            .enumerate()
            .map(|(i, &idx)| {
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

                let icon = if is_playing { "▶ " } else { "  " };
                let right_text = format!("{} {}", t.duration_str(), t.extension().to_uppercase());
                let right_width = right_text.width();
                let total_offset = 6; // Space for icon, highlight symbol, and padding
                let available_width = sub_chunks[1].width as usize;
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
                    Span::styled(icon, style),
                    Span::styled(display_title, style),
                    Span::raw(" ".repeat(padding_len)),
                    Span::styled(right_text, if is_selected { style } else { Theme::dim() }),
                ]))
                .style(style)
            })
            .collect();

        let mut state = make_list_state(app.scoped_song_selected, app.scoped_songs_offset);
        f.render_stateful_widget(
            List::new(items).highlight_symbol("› "),
            sub_chunks[1],
            &mut state,
        );
    } else {
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
                let total_offset = 6; // Space for icon, highlight symbol, and padding
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

                ListItem::new(Line::from(vec![
                    Span::styled(if is_active { "▶ " } else { "  " }, style),
                    Span::styled(display_name, style),
                    Span::raw(" ".repeat(padding_len)),
                    Span::styled(right_info, if is_selected { style } else { Theme::dim() }),
                ]))
                .style(style)
            })
            .collect();

        let mut state = make_list_state(app.album_selected, app.albums_offset);
        f.render_stateful_widget(List::new(items).highlight_symbol("› "), area, &mut state);
    }
}

fn draw_artists(f: &mut Frame, app: &App, area: Rect) {
    if let LibraryScope::Artist(artist_name) = &app.artist_scope {
        let sub_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(0)])
            .split(area);

        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    artist_name.to_uppercase(),
                    Theme::accent().add_modifier(Modifier::BOLD),
                ),
            ])),
            sub_chunks[0],
        );

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

                ListItem::new(Line::from(vec![
                    Span::styled("  ", style),
                    Span::styled(album.clone(), style),
                    Span::raw(padding),
                    Span::styled(right_info, if is_selected { style } else { Theme::dim() }),
                ]))
                .style(style)
            })
            .collect();

        let mut state = make_list_state(app.artist_scoped_album_selected, app.artist_albums_offset);
        f.render_stateful_widget(
            List::new(items).highlight_symbol("› "),
            sub_chunks[1],
            &mut state,
        );
    } else {
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

                ListItem::new(Line::from(vec![
                    Span::styled("  ", style),
                    Span::styled(display_name, style),
                    Span::raw(padding),
                    Span::styled(right_info, if is_selected { style } else { Theme::dim() }),
                ]))
                .style(style)
            })
            .collect();

        let mut state = make_list_state(app.artist_selected, app.artists_offset);
        f.render_stateful_widget(List::new(items).highlight_symbol("› "), area, &mut state);
    }
}
