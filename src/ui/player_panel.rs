use crate::app::App;
use crate::app::RepeatMode;
use crate::ui::theme::Theme;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph},
};

pub fn draw(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(Span::styled("  Now Playing ", Theme::title()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Theme::border_active());

    let inner = block.inner(area);
    f.render_widget(block, area);
    let repeat_span = match app.repeat {
        RepeatMode::Off => Span::styled(" ", Theme::dim()),
        RepeatMode::All => Span::styled(" ", Theme::accent()),
        RepeatMode::One => Span::styled("1 ", Theme::accent().add_modifier(Modifier::BOLD)),
    };
    // ── Root Layout Calculation ───────────────────────────────────────
    // Content is 9 lines high. Min(0) constraints act as spacers for Y-centering.
    let root_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),    // Top Spacer (dynamic)
            Constraint::Length(9), // Main Content Height
            Constraint::Min(0),    // Middle Spacer (dynamic)
            Constraint::Length(2), // Progress Group (Time + Bar) - Anchored bottom
        ])
        .split(inner);

    let content_area = root_layout[1];
    let footer_area = root_layout[3];

    // Split the 9-line content stack
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // 3: Spacer
            Constraint::Length(1), // 0: Title
            Constraint::Length(1), // 1: Artist
            Constraint::Length(1), // 2: Album
            Constraint::Length(1), // 4: Type
            Constraint::Length(1), // 5: Bitrate
            Constraint::Length(1), // 6: Sample Rate
            Constraint::Length(1), // 7: Spacer
            Constraint::Length(1), // 8: Status row (Play / Shuffle / Vol)
        ])
        .split(content_area);

    // Split footer for Time and Bar
    let footer_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // 0: Time
            Constraint::Length(1), // 1: Progress Bar
        ])
        .split(footer_area);

    if let Some(track) = app.current_track() {
        let max_w = inner.width.saturating_sub(12) as usize;
        // f.render_widget(
        //     Paragraph::new(Line::from(vec![Span::styled(
        //         app.current_index.map(|i| i + 1).unwrap_or(0).to_string(),
        //         Theme::dim(),
        //     )])),
        //     rows[0],
        // );
        // ── Metadata ──────────────────────────────────────────────────
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  Title   ", Theme::dim()),
                Span::styled(
                    truncate(&track.title, max_w),
                    Theme::title().add_modifier(Modifier::BOLD),
                ),
            ])),
            rows[0],
        );

        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  Artist  ", Theme::dim()),
                Span::styled(truncate(&track.artist, max_w), Theme::normal()),
            ])),
            rows[1],
        );

        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  Album   ", Theme::dim()),
                Span::styled(truncate(&track.album, max_w), Theme::normal()),
            ])),
            rows[2],
        );

        // ── Technical Info (Type, Bitrate, SR on separate lines) ─────────
        let ext = track
            .path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("?")
            .to_lowercase();
        let bitrate = track.bitrate_str();
        let sr = track
            .sample_rate_str()
            .trim()
            .split_whitespace() // split "44.1 kHz" → ["44.1", "kHz"]
            .next()
            .and_then(|s| s.parse::<f64>().ok())
            .map(|sr| {
                if sr > 1000.0 {
                    // treat as Hz
                    format!("{:.1} kHz", sr / 1000.0)
                } else {
                    // already in kHz
                    format!("{:.1} kHz", sr)
                }
            })
            .unwrap_or_else(|| "—".into());

        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  Type: ", Theme::dim()),
                Span::styled(ext, Theme::accent()),
            ])),
            rows[4],
        );

        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  Bit Rate: ", Theme::dim()),
                Span::styled(bitrate, Theme::normal()),
            ])),
            rows[5],
        );

        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  Sample Rate: ", Theme::dim()),
                Span::styled(sr, Theme::normal()),
            ])),
            rows[6],
        );

        // ── Status Row (Playback, Shuffle, Volume) ────────────────────
        let play_icon = if app.player.is_paused() {
            " PAUSED "
        } else {
            " PLAYING"
        };

        let vol_pct = (app.player.volume * 100.0) as u32;

        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("  {} ", play_icon), Theme::accent()),
                repeat_span, // ← add this
                if app.shuffle {
                    Span::styled("    ", Theme::accent())

                    // Theme::accent().add_modifier(Modifier::BOLD)
                } else {
                    // Theme::dim()
                    Span::styled("    ", Theme::dim())
                },
                // Span::styled("│ ", Theme::dim()),
                Span::styled("  ", Theme::dim()),
                Span::styled(make_progress_bar(vol_pct, 10), Theme::progress_filled()),
                Span::styled(format!(" {}%", vol_pct), Theme::dim()),
            ])),
            rows[8],
        );

        // ── Footer (Time + Song Progress Bar) ─────────────────────────
        let elapsed = app.player.elapsed().as_secs();
        let total = track.duration.as_secs();
        let remaining = total.saturating_sub(elapsed);
        let ratio = if total > 0 {
            (elapsed as f64 / total as f64).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let progress_pct = (ratio * 100.0) as u32;

        // Split the time row into Left (Elapsed/Total) and Right (Remaining)
        let time_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(footer_rows[0]);

        // Left: Elapsed / Total
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(format!("  {}", fmt_dur(elapsed)), Theme::accent()),
                Span::styled(format!(" / {}", fmt_dur(total)), Theme::dim()),
            ])),
            time_chunks[0],
        );

        // Right: Remaining (Aligned to the Right)
        f.render_widget(
            Paragraph::new(Line::from(vec![Span::styled(
                format!("-{}  ", fmt_dur(remaining)),
                Theme::dim(),
            )]))
            .alignment(Alignment::Right),
            time_chunks[1],
        );

        // Render the Song Progress Bar (Full Width)
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  ", Theme::dim()),
                Span::styled(
                    make_progress_bar(progress_pct, inner.width.saturating_sub(4) as usize),
                    Theme::progress_filled(),
                ),
            ])),
            footer_rows[1],
        );
    } else {
        f.render_widget(
            Paragraph::new("No track loaded")
                .alignment(Alignment::Center)
                .style(Theme::dim()),
            rows[3],
        );
    }
}

pub fn draw_compact(f: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title(Span::styled("  Now Playing ", Theme::title()))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Theme::border_active());

    let inner = block.inner(area);
    f.render_widget(block, area);

    // ── Define the 5 rows ─────────────────────────────
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // 0: Title(L) • Artist(C) • Album(R)
            Constraint::Length(1), // 1: Tech Row
            Constraint::Length(1), // 2: Controls(L) • Volume(R)
            Constraint::Min(0),    // 3: Spacer
            Constraint::Length(1), // 4: Time Row (Left/Right)
            Constraint::Length(1), // 5: Progress Bar
        ])
        .split(inner);

    if let Some(track) = app.current_track() {
        let max_w = (inner.width / 3).saturating_sub(5) as usize;

        // ── Line 1: Title(L) • Artist(C) • Album(R) ───
        // Left: Title
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" Title ", Theme::dim()),
                Span::styled(
                    truncate(&track.title, max_w),
                    Theme::title().add_modifier(Modifier::BOLD),
                ),
            ]))
            .alignment(Alignment::Left),
            layout[0],
        );
        // Center: Artist
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" Artist ", Theme::dim()),
                Span::styled(truncate(&track.artist, max_w), Theme::normal()),
            ]))
            .alignment(Alignment::Center),
            layout[0],
        );
        // Right: Album
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" Album ", Theme::dim()),
                Span::styled(truncate(&track.album, max_w - 1), Theme::normal()),
                Span::raw(" "),
            ]))
            .alignment(Alignment::Right),
            layout[0],
        );

        // ── Line 2: Tech Info ─────────────────────────
        let ext = track
            .path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("?")
            .to_lowercase();
        let bitrate = track.bitrate_str();
        let sr = track
            .sample_rate_str()
            .trim()
            .split_whitespace()
            .next()
            .and_then(|s| s.parse::<f64>().ok())
            .map(|sr| {
                if sr > 1000.0 {
                    format!("{:.1} kHz", sr / 1000.0)
                } else {
                    format!("{:.1} kHz", sr)
                }
            })
            .unwrap_or_else(|| "—".into());

        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" Type: ", Theme::dim()),
                Span::styled(ext, Theme::accent()),
                Span::styled(" • Bit rate: ", Theme::dim()),
                Span::styled(bitrate, Theme::normal()),
                Span::styled(" • Sample rate: ", Theme::dim()),
                Span::styled(sr, Theme::normal()),
            ])),
            layout[1],
        );

        // ── Line 3: Controls(L) • Volume(R) ───────────
        let play_icon = if app.player.is_paused() {
            " ▶ PAUSED "
        } else {
            " ⏸ PLAYING"
        };
        let repeat_span = match app.repeat {
            RepeatMode::Off => Span::styled("  ", Theme::dim()),
            RepeatMode::All => Span::styled("  ", Theme::accent()),
            RepeatMode::One => Span::styled(" 1 ", Theme::accent().add_modifier(Modifier::BOLD)),
        };
        let shuffle_span = if app.shuffle {
            Span::styled("  ", Theme::accent())
        } else {
            Span::styled("  ", Theme::dim())
        };
        let vol_pct = (app.player.volume * 100.0) as u32;

        // Left: Controls
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(play_icon, Theme::accent()),
                repeat_span,
                shuffle_span,
            ]))
            .alignment(Alignment::Left),
            layout[2],
        );
        // Right: Volume
        f.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("  ", Theme::dim()),
                Span::styled(make_progress_bar(vol_pct, 10), Theme::progress_filled()),
                Span::styled(format!(" {}% ", vol_pct), Theme::dim()),
            ]))
            .alignment(Alignment::Right),
            layout[2],
        );

        // ── Line 4: Time Row ──────────────────────────
        let elapsed = app.player.elapsed().as_secs();
        let total = track.duration.as_secs();
        let remaining = total.saturating_sub(elapsed);

        f.render_widget(
            Paragraph::new(format!(" {} / {}", fmt_dur(elapsed), fmt_dur(total))),
            layout[4],
        );
        f.render_widget(
            Paragraph::new(format!("-{} ", fmt_dur(remaining)))
                .alignment(Alignment::Right)
                .style(Theme::dim()),
            layout[4],
        );

        // ── Line 5: Smooth Progress Bar ───────────────
        let ratio = if total > 0 {
            (elapsed as f64 / total as f64).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let progress_pct = (ratio * 100.0) as u32;

        f.render_widget(
            Paragraph::new(make_progress_bar(progress_pct, inner.width as usize))
                .style(Theme::progress_filled()),
            layout[5],
        );
    } else {
        f.render_widget(
            Paragraph::new("No track")
                .alignment(Alignment::Center)
                .style(Theme::dim()),
            inner,
        );
    }
}

/// Helper for block-style bars
fn make_progress_bar(pct: u32, width: usize) -> String {
    let fragments = ["", "▏", "▎", "▍", "▌", "▋", "▊", "▉"];
    let empty_char = '·'; // "Smaller" looking pad

    // Calculate total 1/8th steps
    // We cap pct at 100 to prevent overflow glitches
    let pct = pct.min(100);
    let total_steps = (pct as usize * width * 8) / 100;

    let full_blocks = total_steps / 8;
    let fraction_idx = total_steps % 8;

    let mut bar = String::new();
    let mut current_width = 0;

    // 1. Add the full blocks
    for _ in 0..full_blocks {
        if current_width < width {
            bar.push('█');
            current_width += 1;
        }
    }

    // 2. Add the smooth fractional transition
    if current_width < width && fraction_idx > 0 {
        bar.push_str(fragments[fraction_idx]);
        current_width += 1;
    }

    // 3. Fill the exact remaining width with "small" pads
    while current_width < width {
        bar.push(empty_char);
        current_width += 1;
    }

    bar
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{}…", cut)
    }
}

fn fmt_dur(secs: u64) -> String {
    format!("{:02}:{:02}", secs / 60, secs % 60)
}
