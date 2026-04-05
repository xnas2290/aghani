use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{List, ListItem, Paragraph},
};

use crate::app::App;
use crate::library::PlaylistScope;
use crate::ui::theme::Theme;
// use unicode_width::UnicodeWidthChar;
use crate::ui::make_list_state;
use unicode_width::UnicodeWidthStr;
pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    match &app.playlist_scope {
        PlaylistScope::All => draw_playlist_list(f, app, area),
        PlaylistScope::Open(pi) => draw_playlist_songs(f, app, *pi, area),
    }
}

fn draw_playlist_list(f: &mut Frame, app: &App, area: Rect) {
    let items: Vec<ListItem> = app
        .playlists
        .iter()
        .map(|pl| {
            let count = pl.track_paths.len();
            let is_fav = pl.name.to_lowercase() == "favorites";
            let icon = if is_fav { "  ♥ " } else { "  ≡ " };

            let count_text = format!(" {} songs ", count);
            let total_offset = 4; // spacing from icon + title
            let name_width = pl.name.width();
            let count_width = count_text.width();
            let padding_len = (area.width as usize)
                .saturating_sub(icon.width() + name_width + count_width + total_offset);

            ListItem::new(Line::from(vec![
                Span::styled(icon, Theme::accent()),
                Span::styled(pl.name.clone(), Theme::normal()),
                Span::raw(" ".repeat(padding_len)),
                Span::styled(count_text, Theme::dim()),
            ]))
        })
        .collect();

    let mut state = make_list_state(app.playlist_selected, app.playlist_list_offset); // ListState::default();
    state.select(app.playlist_selected);

    let list = List::new(items)
        .highlight_style(Theme::highlight())
        .highlight_symbol("› ");

    f.render_stateful_widget(list, area, &mut state);
}

fn draw_playlist_songs(f: &mut Frame, app: &App, pi: usize, area: Rect) {
    let playlist = &app.playlists[pi];
    let indices = app.playlist_track_indices(pi);

    // Split the area into header and list
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // header
            Constraint::Min(0),    // list
        ])
        .split(area);

    // Calculate total duration for the playlist
    let total_secs: u64 = indices
        .iter()
        .map(|&idx| app.tracks[idx].duration.as_secs())
        .sum();
    let total_time = if total_secs >= 3600 {
        format!(
            "{:02}:{:02}:{:02}",
            total_secs / 3600,
            (total_secs % 3600) / 60,
            total_secs % 60
        )
    } else {
        format!("{:02}:{:02}", total_secs / 60, total_secs % 60)
    };
    // Calculate available width for playlist name
    let right_text = format!("[{}]", total_time);
    let right_width = right_text.width();
    let total_offset = 4; // Space for padding before and after playlist name
    let available_width = chunks[0].width as usize;
    let max_name_width = available_width.saturating_sub(right_width + total_offset);

    let display_name = if playlist.name.width() > max_name_width {
        let mut s = String::new();
        let mut w = 2;
        for c in playlist.name.chars() {
            let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
            if w + cw > max_name_width {
                if s.len() < playlist.name.len() {
                    s.push('…');
                }
                break;
            }
            s.push(c);
            w += cw;
        }
        s
    } else {
        playlist.name.clone()
    };

    let padding_len =
        available_width.saturating_sub(display_name.width() + right_width + total_offset);

    // Header with total duration at the end
    let header = Paragraph::new(Line::from(vec![
        Span::raw("  "),
        Span::styled(display_name.to_uppercase(), Theme::accent()),
        Span::raw(" ".repeat(padding_len)),
        Span::styled(right_text, Theme::normal()),
    ]));
    f.render_widget(header, chunks[0]);

    // List items
    let items: Vec<ListItem> = indices
        .iter()
        .enumerate()
        .map(|(i, &real_idx)| {
            let t = &app.tracks[real_idx];
            let is_playing = app.current_index == Some(real_idx);
            let is_selected = app.playlist_song_selected == Some(i);

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
            let total_offset = 6; // spacing

            // Truncate title if needed
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

    let mut state = make_list_state(app.playlist_song_selected, app.playlist_songs_offset);
    state.select(app.playlist_song_selected);

    let list = List::new(items)
        .highlight_style(Theme::highlight())
        .highlight_symbol("› ");

    f.render_stateful_widget(list, chunks[1], &mut state);
}
