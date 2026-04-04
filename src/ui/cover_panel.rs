use ratatui::{
    Frame,
    layout::Rect,
    text::Span,
    widgets::{Block, BorderType, Borders, Paragraph},
};
use ratatui_image::StatefulImage;

use crate::app::App;
use crate::ui::theme::Theme;

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
