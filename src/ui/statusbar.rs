use ratatui::{
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use crate::ui::theme::Theme;

pub fn draw(f: &mut Frame, area: Rect) {
    let hints = vec![
        ("Space", "Play/Pause"),
        ("n/p", "Next/Prev"),
        ("j/k", "Navigate"),
        ("Enter", "Select"),
        ("+/-", "Volume"),
        ("q", "Quit"),
    ];

    let mut spans = Vec::new();
    for (key, desc) in hints {
        spans.push(Span::styled(format!(" [{}]", key), Theme::accent()));
        spans.push(Span::styled(format!(" {} ", desc), Theme::dim()));
    }

    let bar = Paragraph::new(Line::from(spans));
    f.render_widget(bar, area);
}
