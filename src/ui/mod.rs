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

pub(crate) fn prepare_text(text: &str) -> String {
    let bidi = unicode_bidi::BidiInfo::new(text, None);
    if bidi.paragraphs.is_empty() {
        return text.to_string();
    }

    let paragraph = &bidi.paragraphs[0];
    bidi.reorder_line(paragraph, 0..text.len()).to_string()
}

pub fn make_list_state(selected: Option<usize>, offset: usize) -> ratatui::widgets::ListState {
    let mut state = ratatui::widgets::ListState::default();
    state.select(selected);
    *state.offset_mut() = offset;
    state
}

#[cfg(test)]
mod tests {
    use super::prepare_text;

    #[test]
    fn prepare_text_reorders_right_to_left_text() {
        assert_eq!(prepare_text("hello"), "hello");
        assert_eq!(prepare_text("مرحبا"), "ابحرم");
    }
}
