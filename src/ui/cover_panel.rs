use ratatui::{
    layout::{Position, Rect},
    text::Span,
    widgets::{Block, BorderType, Borders, Paragraph},
    Frame,
};
use ratatui_image::StatefulImage;

use crate::app::App;
use crate::ui::theme::Theme;

/// Clicking the album art switches to view 2 with lyrics, or back to
/// view 1 without lyrics when already there.
pub fn handle_mouse_click(app: &mut App, area: Rect, x: u16, y: u16) -> bool {
    if !area.contains(Position::new(x, y)) {
        return false;
    }

    app.toggle_cover_lyrics_view();
    true
}

pub fn draw(f: &mut Frame, app: &mut App, area: Rect) {
    let block = Block::default()
        // .title(Span::styled(" 🖼  Cover ", Theme::title()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Theme::accent());

    let inner = block.inner(area);
    f.render_widget(block, area);

    if let Some(ref mut proto) = app.cover_image {
        f.render_stateful_widget(StatefulImage::new(), inner, proto);
    } else {
        let no_cover = Paragraph::new(Span::styled("  No cover art", Theme::accent()));
        f.render_widget(no_cover, inner);
    }
}
