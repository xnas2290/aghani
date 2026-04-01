use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Quit,
    TogglePlay,
    Next,
    Prev,
    VolumeUp,
    VolumeDown,
    SelectUp,
    SelectDown,
    ConfirmSelect,
    None,
}

pub fn map_key(event: KeyEvent) -> Action {
    match (event.code, event.modifiers) {
        // Quit
        (KeyCode::Char('q'), _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => Action::Quit,

        // Playback toggle
        (KeyCode::Char(' '), _) => Action::TogglePlay,

        // Next / Previous track
        (KeyCode::Char('n'), _) | (KeyCode::Right, _) => Action::Next,
        (KeyCode::Char('p'), _) | (KeyCode::Left, _) => Action::Prev,

        // Volume control (only + and -)
        (KeyCode::Char('+'), _) | (KeyCode::Char('='), _) => Action::VolumeUp,
        (KeyCode::Char('-'), _) => Action::VolumeDown,

        // List selection (arrow keys or j/k)
        (KeyCode::Up, _) | (KeyCode::Char('k'), _) => Action::SelectUp,
        (KeyCode::Down, _) | (KeyCode::Char('j'), _) => Action::SelectDown,

        // Confirm selection
        (KeyCode::Enter, _) => Action::ConfirmSelect,

        // Anything else
        _ => Action::None,
    }
}
