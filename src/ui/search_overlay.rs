use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph},
};

use crate::app::{App, SearchResult};
use crate::ui::theme::Theme;

pub fn draw(f: &mut Frame, app: &App) {
    if !app.search_mode {
        return;
    }

    let area = centered_rect(60, 70, f.area());
    f.render_widget(Clear, area);
    // Fill with explicit background to prevent ghost characters
    let bg = Block::default().style(Theme::normal());
    f.render_widget(bg, area);

    let block = Block::default()
        .title(Span::styled(" 🔍 Search ", Theme::title()))
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Theme::border_active())
        .style(Theme::overlay_bg());

    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // query input line
            Constraint::Length(1), // divider
            Constraint::Min(0),    // results
            Constraint::Length(1), // hint
        ])
        .split(inner);

    // ── Query line ────────────────────────────────────────────────────────
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" / ", Theme::accent()),
            Span::styled(app.search_query.clone(), Theme::normal()),
            Span::styled("█", Theme::accent()), // cursor
        ])),
        chunks[0],
    );

    // ── Divider ───────────────────────────────────────────────────────────
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "─".repeat(inner.width as usize),
            Theme::dim(),
        ))),
        chunks[1],
    );

    // ── Results ───────────────────────────────────────────────────────────
    if app.search_query.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                "  Start typing to search…",
                Theme::dim(),
            ))),
            chunks[2],
        );
    } else if app.search_results.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled("  No results found", Theme::dim()))),
            chunks[2],
        );
    } else {
        let items: Vec<ListItem> = app
            .search_results
            .iter()
            .map(|r| match r {
                SearchResult::Track(ti) => {
                    let t = &app.tracks[*ti];
                    let is_playing = app.current_index == Some(*ti);
                    let icon = if is_playing { "▶ " } else { "  " };
                    ListItem::new(Line::from(vec![
                        Span::styled(icon, Theme::accent()),
                        Span::styled("♪ ", Theme::dim()),
                        Span::styled(t.title.clone(), Theme::normal()),
                        Span::styled(format!("  {}", t.artist), Theme::dim()),
                    ]))
                }
                SearchResult::Album(album) => ListItem::new(Line::from(vec![
                    Span::styled("  ", Theme::dim()),
                    Span::styled("◉ ", Theme::accent()),
                    Span::styled(album.clone(), Theme::normal()),
                    Span::styled("  album", Theme::dim()),
                ])),
                SearchResult::Artist(artist) => ListItem::new(Line::from(vec![
                    Span::styled("  ", Theme::dim()),
                    Span::styled("● ", Theme::accent()),
                    Span::styled(artist.clone(), Theme::normal()),
                    Span::styled("  artist", Theme::dim()),
                ])),
                SearchResult::PlaylistTrack {
                    playlist_idx,
                    track_idx,
                } => {
                    let t = &app.tracks[*track_idx];
                    let pl = &app.playlists[*playlist_idx];
                    ListItem::new(Line::from(vec![
                        Span::styled("  ", Theme::dim()),
                        Span::styled("≡ ", Theme::accent()),
                        Span::styled(t.title.clone(), Theme::normal()),
                        Span::styled(format!("  in {}", pl.name), Theme::dim()),
                    ]))
                }
            })
            .collect();

        let mut state = ListState::default();
        state.select(app.search_selected);

        f.render_stateful_widget(
            List::new(items)
                .highlight_style(Theme::highlight())
                .highlight_symbol("› "),
            chunks[2],
            &mut state,
        );
    }

    // ── Hint ──────────────────────────────────────────────────────────────
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " [Enter] open  [j/k] navigate  [Esc] close",
            Theme::dim(),
        ))),
        chunks[3],
    );
}

fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area)[1];

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vert)[1]
}
