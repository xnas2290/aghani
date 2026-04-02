use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{List, ListItem, ListState},
};

use crate::app::App;
use crate::library::PlaylistScope;
use crate::ui::theme::Theme;

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
            let icon = if is_fav { "♥ " } else { "≡ " };
            ListItem::new(Line::from(vec![
                Span::styled(icon, Theme::accent()),
                Span::styled(pl.name.clone(), Theme::normal()),
                Span::styled(format!("  {} songs", count), Theme::dim()),
            ]))
        })
        .collect();

    let mut state = ListState::default();
    state.select(app.playlist_selected);

    let list = List::new(items)
        .highlight_style(Theme::highlight())
        .highlight_symbol("› ");
    f.render_stateful_widget(list, area, &mut state);
}

fn draw_playlist_songs(f: &mut Frame, app: &App, pi: usize, area: Rect) {
    let indices = app.playlist_track_indices(pi);
    let items: Vec<ListItem> = indices
        .iter()
        .map(|&real_idx| {
            let t = &app.tracks[real_idx];
            let is_playing = app.current_index == Some(real_idx);
            let icon = if is_playing { "▶ " } else { "  " };
            let style = if is_playing {
                Theme::accent()
            } else {
                Theme::normal()
            };
            ListItem::new(Line::from(vec![
                Span::styled(icon.to_string(), style),
                Span::styled(t.title.clone(), style),
                Span::styled(
                    format!("  {} — {}", t.artist, t.duration_str()),
                    Theme::dim(),
                ),
            ]))
        })
        .collect();

    let mut state = ListState::default();
    state.select(app.playlist_song_selected);

    let list = List::new(items)
        .highlight_style(Theme::highlight())
        .highlight_symbol("› ");
    f.render_stateful_widget(list, area, &mut state);
}
