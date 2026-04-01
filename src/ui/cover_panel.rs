use ratatui::{
    Frame,
    layout::Rect,
    text::Span,
    widgets::{Block, BorderType, Borders, Paragraph},
};

use crate::app::App;
use crate::ui::theme::Theme;

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        // .title(Span::styled(" 🖼  Cover ", Theme::title()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Theme::border());

    let inner = block.inner(area);
    f.render_widget(block, area);

    if let Some(ref lines) = app.cover_lines {
        // Clip to available area
        let available_h = inner.height as usize;
        let available_w = inner.width as usize;

        let clipped: Vec<_> = lines
            .iter()
            .take(available_h)
            .map(|line| {
                // Trim spans so they don't overflow the width
                let spans: Vec<_> = line.spans.iter().take(available_w).cloned().collect();
                ratatui::text::Line::from(spans)
            })
            .collect();

        let para = Paragraph::new(clipped);
        f.render_widget(para, inner);
    } else {
        let no_cover = Paragraph::new(Span::styled("  No cover art", Theme::dim()));
        f.render_widget(no_cover, inner);
    }
}
