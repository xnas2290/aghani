use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::app::App;
use crate::ui::theme::Theme;
pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    if app.loading {
        let msg = format!(
            " Loading library... {}/{} ",
            app.loaded_count,
            app.tracks.len()
        );
        let bar = Paragraph::new(Line::from(Span::styled(msg, Theme::accent())));
        f.render_widget(bar, area);
        return;
    }
    let hints = vec![
        ("Space", "Play/Pause"),
        ("n/p", "Next/Prev"),
        ("j/k", "Navigate"),
        ("Enter", "Select"),
        ("+/-", "Volume"),
        ("q", "Quit"),
        ("Tab", "Switch tab"),
        ("x", "Back"),
        ("s", "Save to playlist"),
        ("d", "Remove from playlist"),
    ];

    let mut spans = Vec::new();
    for (key, desc) in hints {
        spans.push(Span::styled(format!(" [{}]", key), Theme::accent()));
        spans.push(Span::styled(format!(" {} ", desc), Theme::dim()));
    }

    let bar = Paragraph::new(Line::from(spans));
    f.render_widget(bar, area);
}
