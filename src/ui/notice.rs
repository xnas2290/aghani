use crate::app::App;
use crate::ui::theme::Theme;
use ratatui::{
    layout::{Alignment, Rect},
    text::Span,
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame,
};

/// Small transient message shown near the bottom of the screen.
pub fn draw(f: &mut Frame, app: &App) {
    let Some((message, _)) = &app.notice else {
        return;
    };

    let screen = f.area();
    if screen.width < 4 || screen.height < 3 {
        return;
    }

    let width = (message.chars().count() as u16)
        .saturating_add(6)
        .min(screen.width.saturating_sub(2))
        .max(8)
        .min(screen.width);
    let height = 3;
    let x = screen.width.saturating_sub(width) / 2;
    let y = screen.height.saturating_sub(height + 1);
    let area = Rect::new(x, y, width, height);

    f.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Theme::accent())
        .style(Theme::overlay_bg());
    let inner = block.inner(area);
    f.render_widget(block, area);
    f.render_widget(
        Paragraph::new(Span::styled(message.as_str(), Theme::normal()))
            .alignment(Alignment::Center),
        inner,
    );
}
