// use crate::config::ColorsConfig;
// use ratatui::style::{Color, Modifier, Style};
// use std::sync::OnceLock;

// static COLORS: OnceLock<ColorsConfig> = OnceLock::new();

// pub fn init_theme(colors: ColorsConfig) {
//     let _ = COLORS.set(colors);
// }

// fn colors() -> &'static ColorsConfig {
//     COLORS.get_or_init(ColorsConfig::default)
// }

// fn parse_color(s: &str) -> Color {
//     match s {
//         "Black" => Color::Black,
//         "Red" => Color::Red,
//         "Green" => Color::Green,
//         "Yellow" => Color::Yellow,
//         "Blue" => Color::Blue,
//         "Magenta" => Color::Magenta,
//         "Cyan" => Color::Cyan,
//         "White" => Color::White,
//         "DarkGray" => Color::DarkGray,
//         hex if hex.starts_with('#') && hex.len() == 7 => {
//             let r = u8::from_str_radix(&hex[1..3], 16).unwrap_or(255);
//             let g = u8::from_str_radix(&hex[3..5], 16).unwrap_or(255);
//             let b = u8::from_str_radix(&hex[5..7], 16).unwrap_or(255);
//             Color::Rgb(r, g, b)
//         }
//         _ => Color::White,
//     }
// }

// pub struct Theme;

// impl Theme {
//     pub fn title() -> Style {
//         Style::default().fg(parse_color(&colors().title))
//     }
//     pub fn highlight() -> Style {
//         Style::default()
//             .fg(parse_color(&colors().highlight_fg))
//             .bg(parse_color(&colors().highlight_bg))
//             .add_modifier(Modifier::BOLD)
//     }
//     pub fn normal() -> Style {
//         Style::default().fg(parse_color(&colors().normal))
//     }
//     pub fn dim() -> Style {
//         Style::default().fg(parse_color(&colors().dim))
//     }
//     pub fn accent() -> Style {
//         Style::default().fg(parse_color(&colors().accent))
//     }
//     pub fn progress_filled() -> Style {
//         Style::default().fg(parse_color(&colors().progress))
//     }
//     // pub fn progress_empty() -> Style {
//     //     Style::default().fg(parse_color(&colors().dim))
//     // }
//     pub fn border() -> Style {
//         Style::default().fg(parse_color(&colors().border))
//     }
//     pub fn border_active() -> Style {
//         Style::default().fg(parse_color(&colors().border_active))
//     }
//     pub fn overlay_bg() -> Style {
//         Style::default()
//             .fg(parse_color(&colors().normal))
//             .bg(Color::Black)
//     }
// }
use crate::config::ColorsConfig;
use crate::ui::dynamic_theme::DynamicColors;
use ratatui::style::{Color, Modifier, Style};
use std::sync::OnceLock;

static STATIC_COLORS: OnceLock<ColorsConfig> = OnceLock::new();

// Dynamic colors updated per-track
static DYNAMIC: std::sync::RwLock<Option<DynamicColors>> = std::sync::RwLock::new(None);

static USE_DYNAMIC: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn init_theme(colors: ColorsConfig) {
    USE_DYNAMIC.store(colors.dynamic_theme, std::sync::atomic::Ordering::Relaxed);
    let _ = STATIC_COLORS.set(colors);
}

pub fn set_dynamic_colors(dc: DynamicColors) {
    if let Ok(mut guard) = DYNAMIC.write() {
        *guard = Some(dc);
    }
}

pub fn clear_dynamic_colors() {
    if let Ok(mut guard) = DYNAMIC.write() {
        *guard = None;
    }
}

fn is_dynamic() -> bool {
    USE_DYNAMIC.load(std::sync::atomic::Ordering::Relaxed)
}

fn dynamic() -> Option<DynamicColors> {
    DYNAMIC.read().ok()?.clone()
}

fn static_colors() -> &'static ColorsConfig {
    STATIC_COLORS.get_or_init(ColorsConfig::default)
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
    pub fn accent() -> Style {
        if is_dynamic() {
            if let Some(dc) = dynamic() {
                return Style::default().fg(dc.primary);
            }
        }
        Style::default().fg(parse_color(&static_colors().accent))
    }

    pub fn title() -> Style {
        if is_dynamic() {
            if let Some(dc) = dynamic() {
                return Style::default().fg(dc.primary).add_modifier(Modifier::BOLD);
            }
        }
        Style::default().fg(parse_color(&static_colors().title))
    }

    pub fn highlight() -> Style {
        if is_dynamic() {
            if let Some(dc) = dynamic() {
                return Style::default()
                    .fg(dc.background)
                    .bg(dc.primary)
                    .add_modifier(Modifier::BOLD);
            }
        }
        Style::default()
            .fg(parse_color(&static_colors().highlight_fg))
            .bg(parse_color(&static_colors().highlight_bg))
            .add_modifier(Modifier::BOLD)
    }

    pub fn normal() -> Style {
        Style::default().fg(parse_color(&static_colors().normal))
    }

    pub fn dim() -> Style {
        Style::default().fg(parse_color(&static_colors().dim))
    }

    pub fn progress_filled() -> Style {
        if is_dynamic() {
            if let Some(dc) = dynamic() {
                return Style::default().fg(dc.secondary);
            }
        }
        Style::default().fg(parse_color(&static_colors().progress))
    }

    // pub fn progress_empty() -> Style {
    //     Style::default().fg(parse_color(&static_colors().dim))
    // }

    pub fn border() -> Style {
        Style::default().fg(parse_color(&static_colors().border))
    }

    pub fn border_active() -> Style {
        if is_dynamic() {
            if let Some(dc) = dynamic() {
                return Style::default().fg(dc.primary);
            }
        }
        Style::default().fg(parse_color(&static_colors().border_active))
    }

    pub fn overlay_bg() -> Style {
        Style::default()
            .fg(parse_color(&static_colors().normal))
            .bg(Color::Black)
    }
}
