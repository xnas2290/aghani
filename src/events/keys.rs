use crossterm::event::{KeyCode, KeyEvent}; //, KeyModifiers};

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
    OpenSearch,
    ExitScope, // for drill-down views, go back to parent scope
    GoToPlaying,
    None,
    ToggleRepeat,
}

use crate::config::KeysConfig;

pub fn map_key_with_config(event: KeyEvent, keys: &KeysConfig) -> Action {
    let key_str = key_to_string(event);
    match key_str.as_str() {
        k if k == keys.quit => Action::Quit,
        k if k == keys.play_pause => Action::TogglePlay,
        k if k == keys.next => Action::Next,
        k if k == keys.prev => Action::Prev,
        k if k == keys.seek_forward => Action::SeekForward,
        k if k == keys.seek_backward => Action::SeekBackward,
        k if k == keys.volume_up => Action::VolumeUp,
        k if k == keys.volume_down => Action::VolumeDown,
        k if k == keys.select_up => Action::SelectUp,
        k if k == keys.select_down => Action::SelectDown,
        k if k == keys.confirm => Action::ConfirmSelect,
        k if k == keys.tab_next => Action::TabNext,
        k if k == keys.tab_prev => Action::TabPrev,
        k if k == keys.exit_scope => Action::ExitScope,
        k if k == keys.save_to_playlist => Action::SaveToPlaylist,
        k if k == keys.delete_from_playlist => Action::DeleteFromPlaylist,
        k if k == keys.add_to_favorites => Action::AddToFavorites,
        k if k == keys.shuffle => Action::ToggleShuffle,
        k if k == keys.search => Action::OpenSearch,
        k if k == keys.go_to_playing => Action::GoToPlaying,
        k if k == keys.toggle_repeat => Action::ToggleRepeat,

        _ => Action::None,
    }
}

fn key_to_string(event: KeyEvent) -> String {
    match event.code {
        KeyCode::Char(' ') => "Space".into(),
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Enter => "Enter".into(),
        KeyCode::Tab => "Tab".into(),
        KeyCode::BackTab => "BackTab".into(),
        KeyCode::Left => "Left".into(),
        KeyCode::Right => "Right".into(),
        KeyCode::Up => "Up".into(),
        KeyCode::Down => "Down".into(),
        KeyCode::Backspace => "Backspace".into(),
        KeyCode::Esc => "Esc".into(),
        KeyCode::F(n) => format!("F{}", n),
        _ => String::new(),
    }
}
