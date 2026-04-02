use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Tabs},
};
use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::library::{LibraryScope, LibraryTab};
use crate::ui::playlist_panel;
use crate::ui::theme::Theme;

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

    let in_scope = match app.active_tab {
        LibraryTab::Albums => app.album_scope != LibraryScope::All,
        LibraryTab::Artists => app.artist_scope != LibraryScope::All,
        _ => false,
    };
    let hint = if in_scope {
        "  [x] back  [Enter] play"
    } else {
        "  [Enter] open  [Tab] switch"
    };
    let status_line = Line::from(vec![
        Span::styled(app.scope_status(), Theme::accent()),
        Span::styled(hint, Theme::dim()),
    ]);
    f.render_widget(Paragraph::new(status_line), chunks[2]);
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
            let right_text = format!("{} [{}]", t.duration_str(), t.extension().to_uppercase());
            let right_width = right_text.width();
            let total_offset = 6;
            let max_title_width = (area.width as usize).saturating_sub(right_width + total_offset);

            let display_title = if t.title.width() > max_title_width {
                let mut s = String::new();
                let mut w = 0;
                for c in t.title.chars() {
                    let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
                    if w + cw + 1 > max_title_width {
                        s.push('…');
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

    let mut state = ListState::default();
    state.select(app.selected_index);
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

        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    album_name.to_uppercase(),
                    Theme::accent().add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("  [{}]", total_time), Theme::dim()),
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

                let right_text = format!("{} [{}]", t.duration_str(), t.extension().to_uppercase());
                let padding = " ".repeat(
                    (sub_chunks[1].width as usize)
                        .saturating_sub(t.title.width() + right_text.width() + 6),
                );

                ListItem::new(Line::from(vec![
                    Span::styled(if is_playing { "▶ " } else { "  " }, style),
                    Span::styled(t.title.clone(), style),
                    Span::raw(padding),
                    Span::styled(right_text, if is_selected { style } else { Theme::dim() }),
                ]))
                .style(style)
            })
            .collect();

        let mut state = ListState::default();
        state.select(app.scoped_song_selected);
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
                    "  1 song ".to_string()
                } else {
                    format!("  {} songs", count)
                };

                let right_width = right_info.width();
                let total_offset = 6;
                let max_name_width =
                    (area.width as usize).saturating_sub(right_width + total_offset);

                let display_name = if album.width() > max_name_width {
                    let mut s = String::new();
                    let mut w = 0;
                    for c in album.chars() {
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
                    album.clone()
                };

                let padding = " ".repeat(
                    (area.width as usize)
                        .saturating_sub(display_name.width() + right_width + total_offset),
                );

                ListItem::new(Line::from(vec![
                    Span::styled(if is_active { "▶ " } else { "  " }, style),
                    Span::styled(display_name, style),
                    Span::raw(padding),
                    Span::styled(right_info, if is_selected { style } else { Theme::dim() }),
                ]))
                .style(style)
            })
            .collect();

        let mut state = ListState::default();
        state.select(app.album_selected);
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

        let mut state = ListState::default();
        state.select(app.artist_scoped_album_selected);
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

        let mut state = ListState::default();
        state.select(app.artist_selected);
        f.render_stateful_widget(List::new(items).highlight_symbol("› "), area, &mut state);
    }
}
