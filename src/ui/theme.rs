use crate::config::ColorsConfig;
use ratatui::style::{Color, Modifier, Style};
use std::sync::OnceLock;

static COLORS: OnceLock<ColorsConfig> = OnceLock::new();

pub fn init_theme(colors: ColorsConfig) {
    let _ = COLORS.set(colors);
}

fn colors() -> &'static ColorsConfig {
    COLORS.get_or_init(ColorsConfig::default)
}

fn parse_color(s: &str) -> Color {
    match s {
        "Black" => Color::Black,
        "Red" => Color::Red,
        "Green" => Color::Green,
        "Yellow" => Color::Yellow,
        "Blue" => Color::Blue,
        "Magenta" => Color::Magenta,
        "Cyan" => Color::Cyan,
        "White" => Color::White,
        "DarkGray" => Color::DarkGray,
        hex if hex.starts_with('#') && hex.len() == 7 => {
            let r = u8::from_str_radix(&hex[1..3], 16).unwrap_or(255);
            let g = u8::from_str_radix(&hex[3..5], 16).unwrap_or(255);
            let b = u8::from_str_radix(&hex[5..7], 16).unwrap_or(255);
            Color::Rgb(r, g, b)
        }
        _ => Color::White,
    }
}

pub struct Theme;

impl Theme {
    pub fn title() -> Style {
        Style::default().fg(parse_color(&colors().title))
    }
    pub fn highlight() -> Style {
        Style::default()
            .fg(parse_color(&colors().highlight_fg))
            .bg(parse_color(&colors().highlight_bg))
            .add_modifier(Modifier::BOLD)
    }
    pub fn normal() -> Style {
        Style::default().fg(parse_color(&colors().normal))
    }
    pub fn dim() -> Style {
        Style::default().fg(parse_color(&colors().dim))
    }
    pub fn accent() -> Style {
        Style::default().fg(parse_color(&colors().accent))
    }
    pub fn progress_filled() -> Style {
        Style::default().fg(parse_color(&colors().progress))
    }
    // pub fn progress_empty() -> Style {
    //     Style::default().fg(parse_color(&colors().dim))
    // }
    pub fn border() -> Style {
        Style::default().fg(parse_color(&colors().border))
    }
    pub fn border_active() -> Style {
        Style::default().fg(parse_color(&colors().border_active))
    }
    pub fn overlay_bg() -> Style {
        Style::default()
            .fg(parse_color(&colors().normal))
            .bg(Color::Black)
    }
}
