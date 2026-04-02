use ratatui::style::{Color, Modifier, Style};

pub struct Theme;

impl Theme {
    pub fn title() -> Style {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    }

    pub fn highlight() -> Style {
        Style::default()
            .fg(Color::Black)
            .bg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    }

    pub fn normal() -> Style {
        Style::default().fg(Color::White)
    }

    pub fn dim() -> Style {
        Style::default().fg(Color::DarkGray)
    }

    pub fn accent() -> Style {
        Style::default().fg(Color::Magenta)
    }

    pub fn progress_filled() -> Style {
        Style::default().fg(Color::Cyan)
    }

    // pub fn progress_empty() -> Style {
    //     Style::default().fg(Color::DarkGray)
    // }

    pub fn border() -> Style {
        Style::default().fg(Color::DarkGray)
    }

    pub fn border_active() -> Style {
        Style::default().fg(Color::Cyan)
    }
    pub fn overlay_bg() -> Style {
        Style::default().fg(Color::White).bg(Color::Black) // explicit black background kills ghost chars
    }
}
