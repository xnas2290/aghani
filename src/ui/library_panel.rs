use ratatui::{
    Frame,
    layout::Rect,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::app::App;
use crate::ui::theme::Theme;

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let width = area.width as usize;

    let items: Vec<ListItem> = app
        .tracks
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let is_playing = app.current_index == Some(i);
            let icon = if is_playing { " ▶ " } else { "  " };

            let style = if is_playing {
                Theme::accent()
            } else {
                Theme::normal()
            };

            let file_type = t.path.extension().and_then(|e| e.to_str()).unwrap_or("???");
            let duration = t.duration_str();

            let right = format!("{} [{}]", duration, file_type);
            let right_width = UnicodeWidthStr::width(right.as_str());
            let icon_width = UnicodeWidthStr::width(icon);

            let gap = 4;
            let available = width.saturating_sub(icon_width + right_width + gap);

            let chars: Vec<char> = t.title.chars().collect();

            let title_display =
                if is_playing && UnicodeWidthStr::width(t.title.as_str()) > available {
                    let len = chars.len();
                    let offset = app.scroll_offset % len;

                    let mut result = String::new();
                    let mut current_width = 0;

                    for i in 0..len {
                        let c = chars[(offset + i) % len];
                        let w = UnicodeWidthChar::width(c).unwrap_or(1);

                        if current_width + w > available {
                            break;
                        }

                        result.push(c);
                        current_width += w;
                    }

                    let padding = available.saturating_sub(current_width);
                    result.push_str(&" ".repeat(padding));

                    result
                } else {
                    let mut result = String::new();
                    let mut current_width = 0;

                    for c in chars {
                        let w = UnicodeWidthChar::width(c).unwrap_or(1);
                        if current_width + w > available {
                            break;
                        }
                        result.push(c);
                        current_width += w;
                    }

                    let padding = available.saturating_sub(current_width);
                    result.push_str(&" ".repeat(padding));

                    result
                };

            let line = Line::from(vec![
                Span::styled(icon.to_string(), style),
                Span::styled(title_display, style),
                Span::raw(" "),
                Span::styled(right, Theme::dim()),
            ]);

            ListItem::new(line)
        })
        .collect();

    let mut state = ListState::default();
    state.select(app.selected_index);

    // ✅ Set title as "current_index / total"
    let title_text = match app.current_index {
        Some(idx) => format!("{} / {}", idx + 1, app.tracks.len()), // +1 for 1-based indexing
        None => format!("0 / {}", app.tracks.len()),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Theme::border())
        .title(title_text);

    let list = List::new(items)
        .block(block)
        .highlight_style(Theme::highlight())
        .highlight_symbol("");

    f.render_stateful_widget(list, area, &mut state);
}
