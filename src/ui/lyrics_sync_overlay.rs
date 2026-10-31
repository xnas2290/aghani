use crate::app::App;
use crate::lyrics::format_timestamp;
use crate::ui::theme::Theme;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame,
};

pub fn draw(f: &mut Frame, app: &App) {
    if !app.lyrics_sync_mode {
        return;
    }

    let area = centered_rect(f.area());
    f.render_widget(Clear, area);

    let block = Block::default()
        .title(Span::styled(" Sync Lyrics ", Theme::title()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Theme::border_active())
        .style(Theme::overlay_bg());
    let inner = block.inner(area);
    f.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // playback / progress
            Constraint::Min(1),    // lyric lines
            Constraint::Length(1), // key hint
        ])
        .split(inner);

    let total = app.lyrics_sync_lines.len();
    let selected = app.lyrics_sync_index.unwrap_or(0).min(total.saturating_sub(1));
    let playback_state = if app.player.is_paused() {
        "paused"
    } else {
        "playing"
    };

    let header = format!(
        " Line {}/{}   {}   {}",
        selected + 1,
        total.max(1),
        format_timestamp(app.player.elapsed()),
        playback_state,
    );
    f.render_widget(
        Paragraph::new(header)
            .alignment(Alignment::Center)
            .style(Theme::accent()),
        rows[0],
    );

    let mut items: Vec<ListItem> = Vec::with_capacity(total);
    for (index, line) in app.lyrics_sync_lines.iter().enumerate() {
        let is_selected = index == selected;
        let stamped = line.time.is_some();
        let marker = if is_selected {
            "▶"
        } else if stamped {
            "✓"
        } else {
            " "
        };
        let time = line
            .time
            .map(format_timestamp)
            .unwrap_or_else(|| "--:--.--".to_string());
        let prefix = format!(" {marker} [{time}] ");
        let available = (rows[1].width as usize).saturating_sub(prefix.chars().count());
        // Apply the same BiDi reordering used by the main lyrics panel so RTL
        // scripts (Arabic, Hebrew, etc.) render in the correct order.
        let text = crate::ui::prepare_text(&truncate(&line.text, available));

        let row_style = if is_selected {
            Theme::accent().add_modifier(Modifier::BOLD)
        } else if stamped {
            Theme::normal()
        } else {
            Theme::dim()
        };
        let time_style = if is_selected {
            row_style
        } else {
            Theme::dim()
        };

        items.push(ListItem::new(Line::from(vec![
            Span::styled(prefix, time_style),
            Span::styled(text, row_style),
        ])));
    }

    let height = rows[1].height as usize;
    let max_offset = total.saturating_sub(height);
    let offset = selected
        .saturating_sub(height / 2)
        .min(max_offset);

    let mut state = ListState::default();
    state.select(Some(selected));
    *state.offset_mut() = offset;

    f.render_stateful_widget(List::new(items), rows[1], &mut state);

    let hint = if selected + 1 >= total {
        "↓ stamp final line & save    Space play/pause    Esc save & exit"
    } else {
        "↓ stamp line & advance    Space play/pause    Esc save & exit"
    };
    f.render_widget(
        Paragraph::new(hint)
            .alignment(Alignment::Center)
            .style(Theme::dim()),
        rows[2],
    );
}

fn truncate(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let count = text.chars().count();
    if count <= width {
        return text.to_string();
    }
    if width == 1 {
        return "…".to_string();
    }
    let mut truncated: String = text.chars().take(width - 1).collect();
    truncated.push('…');
    truncated
}

fn centered_rect(area: Rect) -> Rect {
    let width = area
        .width
        .min(88)
        .saturating_sub(4)
        .max(20)
        .min(area.width);
    let height = area
        .height
        .min(30)
        .saturating_sub(2)
        .max(8)
        .min(area.height);

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
