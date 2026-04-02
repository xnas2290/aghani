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
    TabNext,
    TabPrev,
    SaveToPlaylist,
    DeleteFromPlaylist,
    AddToFavorites,
    SeekForward,
    SeekBackward,
    ToggleShuffle,

    ExitScope, // for drill-down views, go back to parent scope
    None,
}

pub fn map_key(event: KeyEvent) -> Action {
    match (event.code, event.modifiers) {
        // Quit
        (KeyCode::Char('q'), _) | (KeyCode::Char('c'), KeyModifiers::CONTROL) => Action::Quit,

        // Playback toggle
        (KeyCode::Char(' '), _) => Action::TogglePlay,

        // Next / Previous track
        (KeyCode::Char('n'), _) => Action::Next,
        (KeyCode::Char('p'), _) => Action::Prev,

        // Volume control (only + and -)
        (KeyCode::Char('+'), _) | (KeyCode::Char('='), _) => Action::VolumeUp,
        (KeyCode::Char('-'), _) => Action::VolumeDown,

        // List selection (arrow keys or j/k)
        (KeyCode::Up, _) | (KeyCode::Char('k'), _) => Action::SelectUp,
        (KeyCode::Down, _) | (KeyCode::Char('j'), _) => Action::SelectDown,

        // Confirm selection
        (KeyCode::Enter, _) => Action::ConfirmSelect,
        // Tab navigation
        (KeyCode::Tab, _) => Action::TabNext,
        (KeyCode::BackTab, _) => Action::TabPrev,
        // Exit Scope (e.g. from album view back to all songs)
        (KeyCode::Char('x'), _) => Action::ExitScope,
        (KeyCode::Char('s'), _) => Action::SaveToPlaylist,
        (KeyCode::Char('d'), _) => Action::DeleteFromPlaylist,
        (KeyCode::Char('f'), _) => Action::AddToFavorites,
        (KeyCode::Right, _) => Action::SeekForward,
        (KeyCode::Left, _) => Action::SeekBackward,
        (KeyCode::Char('r'), _) => Action::ToggleShuffle,
        // Anything else
        _ => Action::None,
    }
}
