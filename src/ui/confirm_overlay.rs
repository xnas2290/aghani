use crate::app::App;
use crate::ui::theme::Theme;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
};

pub fn draw(f: &mut Frame, app: &App) {
    if app.confirm_dialog.is_none() && !app.new_playlist_mode {
        return;
    }

    // Use fixed dimensions: 40 chars wide, 7 lines high
    // This makes the "box" feel much smaller and professional
    let area = centered_rect_fixed(40, 7, f.area());
    f.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded) // Rounded looks "smaller/softer" than Double
        .border_style(Theme::border_active())
        .style(Theme::overlay_bg());

    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.new_playlist_mode {
        draw_new_playlist(f, app, inner);
    } else if let Some(ref dialog) = app.confirm_dialog {
        draw_confirm(f, dialog.message.as_str(), inner);
    }
}

fn draw_new_playlist(f: &mut Frame, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // Title
            Constraint::Length(3), // Input (Min 3 for borders)
            Constraint::Length(1), // Hint
        ])
        .split(area);

    f.render_widget(
        Paragraph::new("New Playlist")
            .alignment(Alignment::Center)
            .style(Theme::title()),
        chunks[0],
    );

    // Placeholder logic: If name is empty, show dimmed "Name..."
    let display_text = if app.new_playlist_name.is_empty() {
        Span::styled(" Name...", Theme::dim())
    } else {
        Span::styled(format!(" {}█", app.new_playlist_name), Theme::normal())
    };

    f.render_widget(
        Paragraph::new(Line::from(vec![display_text])).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Theme::dim()),
        ),
        chunks[1],
    );

    f.render_widget(
        Paragraph::new("Enter ↵ / Esc ⎋")
            .alignment(Alignment::Center)
            .style(Theme::dim()),
        chunks[2],
    );
}

fn draw_confirm(f: &mut Frame, message: &str, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),    // Message
            Constraint::Length(1), // Actions
        ])
        .split(area);

    f.render_widget(
        Paragraph::new(message)
            .alignment(Alignment::Center)
            .wrap(ratatui::widgets::Wrap { trim: true }),
        chunks[0],
    );

    f.render_widget(
        Paragraph::new("y: confirm • n: cancel")
            .alignment(Alignment::Center)
            .style(Theme::dim()),
        chunks[1],
    );
}

// Fixed-size centering helper
fn centered_rect_fixed(width: u16, height: u16, area: Rect) -> Rect {
    let horizontal_pad = area.width.saturating_sub(width) / 2;
    let vertical_pad = area.height.saturating_sub(height) / 2;

    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(vertical_pad),
            Constraint::Length(height),
            Constraint::Length(vertical_pad),
        ])
        .split(area)[1];

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(horizontal_pad),
            Constraint::Length(width),
            Constraint::Length(horizontal_pad),
        ])
        .split(vert)[1]
}
// fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
//     let vert = Layout::default()
//         .direction(Direction::Vertical)
//         .constraints([
//             Constraint::Percentage((100 - percent_y) / 2),
//             Constraint::Percentage(percent_y),
//             Constraint::Percentage((100 - percent_y) / 2),
//         ])
//         .split(area)[1];

//     Layout::default()
//         .direction(Direction::Horizontal)
//         .constraints([
//             Constraint::Percentage((100 - percent_x) / 2),
//             Constraint::Percentage(percent_x),
//             Constraint::Percentage((100 - percent_x) / 2),
//         ])
//         .split(vert)[1]
// }
