pub mod confirm_overlay;
pub mod cover_panel;
pub mod dynamic_theme;
pub mod layout;
pub mod library_panel;
pub mod lyrics_panel;
pub mod lyrics_sync_overlay;
pub mod metadata_overlay;
pub mod notice;
pub mod player_panel;
pub mod playlist_panel;
pub mod save_overlay;
pub mod search_overlay;
pub mod statusbar;
pub mod theme;

pub fn make_list_state(selected: Option<usize>, offset: usize) -> ratatui::widgets::ListState {
    let mut state = ratatui::widgets::ListState::default();
    state.select(selected);
    *state.offset_mut() = offset;
    state
}
