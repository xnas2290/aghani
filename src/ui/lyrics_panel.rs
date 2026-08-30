use crate::app::App;
use crate::ui::theme::Theme;
use ratatui::{
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};
use unicode_bidi::BidiInfo;
// fn prepare_text(text: &str) -> String {
//     if is_rtl(text) {
//         // Reverse the string for RTL display in LTR terminal
//         text.chars().rev().collect()
//     } else {
//         text.to_string()
//     }
// }

// fn is_rtl(text: &str) -> bool {
//     text.chars().any(|c| {
//         matches!(c as u32,
//             // Arabic
//             0x0600..=0x06FF |
//             // Arabic Supplement
//             0x0750..=0x077F |
//             // Arabic Extended-A
//             0x08A0..=0x08FF |
//             // Arabic Presentation Forms-A
//             0xFB50..=0xFDFF |
//             // Arabic Presentation Forms-B
//             0xFE70..=0xFEFF |
//             // Hebrew
//             0x0590..=0x05FF
//         )
//     })
// }

fn prepare_text(text: &str) -> String {
    let bidi = BidiInfo::new(text, None);
    if bidi.paragraphs.is_empty() {
        return text.to_string();
    }

    let para = &bidi.paragraphs[0];
    let line = 0..text.len();
    let reordered = bidi.reorder_line(para, line);
    reordered.to_string()
}
pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let has_manual = app
        .current_track()
        .map(|t| crate::lyrics::has_manual(&t.path))
        .unwrap_or(false);

    let title = if has_manual {
        "  Lyrics ✎ "
    } else {
        "  Lyrics "
    };
    let block = Block::default()
        .title(Span::styled(title, Theme::title()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Theme::border());

    let inner = block.inner(area);
    f.render_widget(block, area);

    if app.lyrics_loading {
        f.render_widget(
            Paragraph::new(Span::styled("  Fetching lyrics...", Theme::dim())),
            inner,
        );
        return;
    }

    if app.lyrics_error {
        f.render_widget(
            Paragraph::new(Span::styled("  No lyrics found", Theme::dim())),
            inner,
        );
        return;
    }

    let lyrics = match &app.lyrics {
        Some(l) => l,
        None => {
            f.render_widget(Paragraph::new(Span::styled("  —", Theme::dim())), inner);
            return;
        }
    };

    let elapsed = app.player.elapsed();
    let current = lyrics.current_line(elapsed).unwrap_or(0);

    let max_w = inner.width.saturating_sub(3) as usize;

    // Build flattened items and track where each original line starts
    let mut items: Vec<ListItem> = Vec::new();
    let mut current_flat_idx = 0usize; // index in flattened list for current line
    let mut flat_idx = 0usize;

    for (i, line) in lyrics.lines.iter().enumerate() {
        let is_current = i == current;
        let style = if is_current {
            Theme::accent().add_modifier(Modifier::BOLD)
        } else {
            Theme::dim()
        };
        // let display_text = prepare_text(&line.text);
        // let wrapped = wrap_text(&display_text, max_w);
        let wrapped = wrap_text(&line.text, max_w);

        if is_current {
            current_flat_idx = flat_idx;
        }

        for chunk in &wrapped {
            // Center each chunk within max_w
            let chunk_len = chunk.chars().count();
            let padding = if chunk_len < max_w {
                " ".repeat((max_w - chunk_len) / 2)
            } else {
                String::new()
            };

            items.push(ListItem::new(Line::from(Span::styled(
                format!("{}{}", padding, chunk),
                style,
            ))));
            flat_idx += 1;
        }
    }

    // Scroll to keep current line centered
    let height = inner.height as usize;
    let offset = if current_flat_idx < height / 2 {
        0
    } else {
        current_flat_idx - height / 2
    };

    let mut state = ListState::default();
    state.select(Some(current_flat_idx)); // ← use flattened index
    *state.offset_mut() = offset;

    f.render_stateful_widget(
        List::new(items).highlight_style(Theme::highlight()),
        inner,
        &mut state,
    );
}
fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![text.to_string()];
    }

    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_len = 0;

    // Split on whitespace, wrap, then apply BiDi to each line
    for word in text.split_whitespace() {
        let word_len = word.chars().count();
        if current_len == 0 {
            current.push_str(word);
            current_len = word_len;
        } else if current_len + 1 + word_len <= max_width {
            current.push(' ');
            current.push_str(word);
            current_len += 1 + word_len;
        } else {
            lines.push(prepare_text(&current)); // ← apply BiDi per line
            current = word.to_string();
            current_len = word_len;
        }
    }

    if !current.is_empty() {
        lines.push(prepare_text(&current));
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}
// fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
//     if max_width == 0 {
//         return vec![text.to_string()];
//     }

//     let mut lines = Vec::new();
//     let mut current = String::new();
//     let mut current_len = 0;

//     for word in text.split_whitespace() {
//         let word_len = word.chars().count();

//         if current_len == 0 {
//             // First word on line
//             current.push_str(word);
//             current_len = word_len;
//         } else if current_len + 1 + word_len <= max_width {
//             // Word fits on current line
//             current.push(' ');
//             current.push_str(word);
//             current_len += 1 + word_len;
//         } else {
//             // Start new line
//             lines.push(current.clone());
//             current = word.to_string();
//             current_len = word_len;
//         }
//     }

//     if !current.is_empty() {
//         lines.push(current);
//     }

//     if lines.is_empty() {
//         lines.push(String::new());
//     }

//     lines
// }
