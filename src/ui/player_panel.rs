use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect}, // Import Alignment from layout
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Gauge, Paragraph},
};

use crate::app::App;
use crate::ui::theme::Theme;

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    // Outer block with rounded border
    let block = Block::default()
        .title(Span::styled(" ♪ Now Playing ", Theme::normal()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Theme::border());

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Layout for the panel content
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // title
            Constraint::Length(1), // artist / album
            Constraint::Length(1), // spacer
            Constraint::Length(1), // progress bar
            Constraint::Length(1), // time
            Constraint::Length(1), // spacer
            Constraint::Length(1), // controls hint
        ])
        .split(inner);

    // Track info
    if let Some(track) = app.current_track() {
        // Track title (centered and bold)
        let title = Paragraph::new(Line::from(vec![
            Span::styled("", Theme::dim()), // Add spacing before the title for centering
            Span::styled(&track.title, Theme::title().add_modifier(Modifier::BOLD)),
            Span::styled("", Theme::dim()), // Add spacing after the title for centering
        ]))
        .alignment(Alignment::Center); // Use the correct Alignment from layout
        f.render_widget(title, chunks[0]);

        // Artist and album (dimmed)
        let sub = Paragraph::new(Line::from(vec![Span::styled(
            format!("  {} — {}", track.artist, track.album),
            Theme::dim(),
        )]));
        f.render_widget(sub, chunks[1]);

        // Progress bar (remove text)
        let total = track.duration.as_secs_f64().max(1.0);
        let elapsed = app.player.elapsed().as_secs_f64();
        let ratio = (elapsed / total).clamp(0.0, 1.0);

        let gauge = Gauge::default()
            .gauge_style(Theme::progress_filled())
            .ratio(ratio)
            .use_unicode(true);
        f.render_widget(gauge, chunks[3]);

        // Time label
        let elapsed_str = fmt_dur(elapsed as u64);
        let total_str = track.duration_str();
        let time = Paragraph::new(Line::from(vec![Span::styled(
            format!("  {} / {}", elapsed_str, total_str),
            Theme::dim(),
        )]));
        f.render_widget(time, chunks[4]);
    } else {
        let idle = Paragraph::new(Line::from(Span::styled(
            "  No track loaded — pick one from the library",
            Theme::dim(),
        )));
        f.render_widget(idle, chunks[0]);
    }

    // Controls and volume
    let status_icon = if app.player.is_paused() { "⏸" } else { "▶" };
    let status_text = if app.player.is_paused() {
        "Paused "
    } else {
        "Playing"
    };
    let vol_pct = (app.player.volume * 100.0) as u32;

    // Add space between play/pause and volume
    let controls = Paragraph::new(Line::from(vec![
        Span::styled(
            format!("  {} {} ", status_icon, status_text),
            Theme::accent(),
        ),
        Span::styled("   ", Theme::dim()), // Space between play/pause and volume
        Span::styled(format!("Vol: {}%", vol_pct), Theme::dim()),
    ]));
    f.render_widget(controls, chunks[6]);
}

fn fmt_dur(secs: u64) -> String {
    format!("{:02}:{:02}", secs / 60, secs % 60)
}
