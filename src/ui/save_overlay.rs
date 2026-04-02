use crate::app::App;
// use crate::library::playlist::Playlist;
use crate::ui::theme::Theme;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph},
};

pub fn draw(f: &mut Frame, app: &App) {
    if !app.save_mode {
        return;
    }

    // Centered popup: 40% width, auto height
    let area = centered_rect(40, 50, f.area());

    // Clear background
    f.render_widget(Clear, area);
    // Fill with explicit background to prevent ghost characters
    let bg = Block::default().style(Theme::normal());
    f.render_widget(bg, area);
    let block = Block::default()
        .title(Span::styled(" Save to Playlist ", Theme::title()))
        .borders(Borders::ALL)
        .border_type(BorderType::Double)
        .border_style(Theme::border_active())
        .style(Theme::overlay_bg());

    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.save_candidates.is_empty() {
        let msg = Paragraph::new(Span::styled("  Already in all playlists", Theme::dim()));
        f.render_widget(msg, inner);
        return;
    }

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(inner);

    let items: Vec<ListItem> = app
        .save_candidates
        .iter()
        .map(|&pi| {
            let pl = &app.playlists[pi];
            let is_fav = pl.name.to_lowercase() == "favorites";
            let icon = if is_fav { "♥ " } else { "≡ " };
            ListItem::new(Line::from(vec![
                Span::styled(icon, Theme::accent()),
                Span::styled(pl.name.clone(), Theme::normal()),
                Span::styled(format!("  {} songs", pl.track_paths.len()), Theme::dim()),
            ]))
        }) // fn draw_empty(f: &mut Frame, msg: &str, area: Rect) {
        //     f.render_widget(
        //         Paragraph::new(Span::styled(format!("  {}", msg), Theme::dim())),
        //         area,
        //     );
        // }
        .collect();

    let mut state = ListState::default();
    state.select(Some(app.save_selected));

    let list = List::new(items)
        .highlight_style(Theme::highlight())
        .highlight_symbol("› ");
    f.render_stateful_widget(list, chunks[0], &mut state);

    let hint = Paragraph::new(Line::from(Span::styled(
        " [Enter] add  [Esc] cancel",
        Theme::dim(),
    )))
    .alignment(Alignment::Center);
    f.render_widget(hint, chunks[1]);
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
