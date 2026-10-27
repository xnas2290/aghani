use crate::app::App;
use crate::lyrics::Lyrics;
use crate::ui::theme::Theme;
use ratatui::{
    layout::Rect,
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};
use unicode_bidi::BidiInfo;

pub fn handle_mouse_click(app: &mut App, area: Rect, x: u16, y: u16) -> anyhow::Result<bool> {
    if app.lyrics_loading || app.lyrics_error || x < area.x || y < area.y {
        return Ok(false);
    }

    let lyrics = match &app.lyrics {
        Some(lyrics) => lyrics,
        None => return Ok(false),
    };

    // Plain lyrics have no timestamps, so there is nothing to seek to.
    if !lyrics.synced {
        return Ok(false);
    }

    let inner = Block::default().borders(Borders::ALL).inner(area);
    if x >= inner.x + inner.width || y >= inner.y + inner.height {
        return Ok(false);
    }

    let max_w = inner.width.saturating_sub(3) as usize;
    let (starts, total) = flat_starts(lyrics, max_w);
    let height = inner.height as usize;
    let offset = visible_offset(app, lyrics, &starts, height, total);

    let clicked_flat_idx = offset + (y - inner.y) as usize;
    if clicked_flat_idx >= total {
        return Ok(false);
    }

    let line_index = starts.partition_point(|&start| start <= clicked_flat_idx) - 1;
    let time = lyrics.lines[line_index].time;

    // Clicking a line seeks to it without recentering the viewport. The
    // clicked line stays where it is; normal auto-follow resumes on the next
    // line.
    app.seek_to_lyric(time)?;
    app.lyrics_clicked_line = Some(line_index);
    app.lyrics_scroll = Some(offset);
    Ok(true)
}

/// Scroll the lyrics panel by one line. Plain lyrics can be scrolled freely,
/// while timed lyrics keep auto-following until the user scrolls manually.
pub fn handle_mouse_scroll(app: &mut App, area: Rect, up: bool) {
    if app.lyrics_loading || app.lyrics_error {
        return;
    }
    let lyrics = match &app.lyrics {
        Some(lyrics) => lyrics,
        None => return,
    };

    let inner = Block::default().borders(Borders::ALL).inner(area);
    let max_w = inner.width.saturating_sub(3) as usize;
    let (starts, total) = flat_starts(lyrics, max_w);
    let height = inner.height as usize;

    // Everything fits: keep auto-following so the current line stays marked.
    if total <= height {
        app.lyrics_scroll = None;
        app.lyrics_clicked_line = None;
        return;
    }

    let max_offset = total.saturating_sub(height);
    let base = visible_offset(app, lyrics, &starts, height, total);
    let next = if up {
        base.saturating_sub(1)
    } else {
        (base + 1).min(max_offset)
    };

    // Manual scrolling takes over from the one-shot click positioning.
    app.lyrics_clicked_line = None;
    app.lyrics_scroll = Some(next);
}

/// Flat index (accounting for wrapping) where each lyric line starts.
fn flat_starts(lyrics: &Lyrics, max_w: usize) -> (Vec<usize>, usize) {
    let mut starts = Vec::with_capacity(lyrics.lines.len());
    let mut total = 0usize;
    for line in &lyrics.lines {
        starts.push(total);
        total += wrap_text(&line.text, max_w).len();
    }
    (starts, total)
}

fn auto_offset(flat_idx: usize, height: usize) -> usize {
    if flat_idx < height / 2 {
        0
    } else {
        flat_idx - height / 2
    }
}

fn visible_offset(
    app: &App,
    lyrics: &Lyrics,
    starts: &[usize],
    height: usize,
    total: usize,
) -> usize {
    let max_offset = total.saturating_sub(height);
    let auto = if lyrics.synced {
        lyrics
            .current_line(app.player.elapsed())
            .and_then(|index| starts.get(index).copied())
            .map(|flat| auto_offset(flat, height))
            .unwrap_or(0)
    } else {
        0
    };

    app.lyrics_scroll.unwrap_or(auto).min(max_offset)
}
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

pub(crate) fn prepare_text(text: &str) -> String {
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

    // Plain lyrics have no timing, so nothing gets highlighted or followed.
    let current = if lyrics.synced {
        lyrics.current_line(app.player.elapsed())
    } else {
        None
    };

    let max_w = inner.width.saturating_sub(3) as usize;
    let (starts, total) = flat_starts(lyrics, max_w);
    let height = inner.height as usize;
    let offset = visible_offset(app, lyrics, &starts, height, total);

    // Build flattened items
    let mut items: Vec<ListItem> = Vec::new();

    for (i, line) in lyrics.lines.iter().enumerate() {
        let is_current = current == Some(i);
        let style = if is_current {
            Theme::accent().add_modifier(Modifier::BOLD)
        } else if lyrics.synced {
            Theme::dim()
        } else {
            Theme::normal()
        };
        // let display_text = prepare_text(&line.text);
        // let wrapped = wrap_text(&display_text, max_w);
        let wrapped = wrap_text(&line.text, max_w);

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
        }
    }

    // Only the current line's text is highlighted; there is no row/background
    // selection bar.
    let mut state = ListState::default();
    *state.offset_mut() = offset;

    f.render_stateful_widget(List::new(items), inner, &mut state);
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
