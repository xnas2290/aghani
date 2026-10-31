use crate::app::App;
use crate::ui::theme::Theme;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph},
    Frame,
};

const FIELD_LABELS: [&str; 4] = ["Title", "Artist", "Album", "Artwork"];
const LABEL_WIDTH: u16 = 10;

pub fn draw(f: &mut Frame, app: &App) {
    if !app.metadata_edit_mode {
        return;
    }

    let area = centered_rect_fixed(66, 12, f.area());
    f.render_widget(Clear, area);

    let block = Block::default()
        .title(Span::styled(" Edit Metadata & Artwork ", Theme::title()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Theme::border_active())
        .style(Theme::overlay_bg());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // spacer
            Constraint::Length(1), // title
            Constraint::Length(1), // artist
            Constraint::Length(1), // album
            Constraint::Length(1), // artwork URL
            Constraint::Min(1),    // error / spacer
            Constraint::Length(1), // artwork hint
            Constraint::Length(1), // key hint
        ])
        .split(inner);

    for field in 0..FIELD_LABELS.len() {
        draw_field(f, app, rows[field + 1], field);
    }

    if let Some(error) = &app.metadata_edit_error {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("  {error}"),
                Theme::accent(),
            )))
            .alignment(Alignment::Left),
            rows[5],
        );
    }

    f.render_widget(
        Paragraph::new("Artwork: paste an image URL (http/https); blank keeps current")
            .alignment(Alignment::Center)
            .style(Theme::dim()),
        rows[6],
    );

    f.render_widget(
        Paragraph::new("↑/↓ field  ←/→ cursor  Enter save  Esc cancel")
            .alignment(Alignment::Center)
            .style(Theme::dim()),
        rows[7],
    );
}

fn draw_field(f: &mut Frame, app: &App, area: Rect, field: usize) {
    let selected = app.metadata_edit_field == field;
    let label_style = if selected {
        Theme::highlight().add_modifier(Modifier::BOLD)
    } else {
        Theme::dim()
    };
    let value_style = if selected {
        Theme::highlight()
    } else {
        Theme::normal()
    };

    let value = &app.metadata_edit_values[field];
    let chars: Vec<char> = value.chars().collect();
    let cursor = app.metadata_edit_cursors[field].min(chars.len());
    let value_width = area.width.saturating_sub(LABEL_WIDTH).max(1) as usize;
    let start = if cursor >= value_width {
        cursor + 1 - value_width
    } else {
        0
    };
    let end = (start + value_width).min(chars.len());

    let mut rendered = String::new();
    for (offset, ch) in chars[start..end].iter().enumerate() {
        if selected && start + offset == cursor {
            rendered.push('█');
        }
        rendered.push(*ch);
    }
    if selected && cursor >= end {
        rendered.push('█');
    }

    let used = LABEL_WIDTH as usize + rendered.chars().count();
    let padding = " ".repeat(area.width.saturating_sub(used as u16) as usize);

    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(
                format!("  {:<width$}", FIELD_LABELS[field], width = 8),
                label_style,
            ),
            Span::styled(rendered, value_style),
            Span::styled(padding, value_style),
        ])),
        area,
    );
}

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
