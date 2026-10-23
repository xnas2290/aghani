use crate::app::App;
use crate::events::keys::{map_key_with_config, Action};
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
// use std::time::Duration;

pub fn handle_events(app: &mut App) -> Result<bool> {
    if !crossterm::event::poll(std::time::Duration::from_millis(0))? {
        return Ok(false);
    }
    match event::read()? {
        Event::Mouse(mouse) => {
            match mouse.kind {
                MouseEventKind::ScrollUp => app.select_prev(),
                MouseEventKind::ScrollDown => app.select_next(),
                MouseEventKind::Down(MouseButton::Left) => {
                    let size = crossterm::terminal::size()?;
                    let terminal_area = ratatui::layout::Rect::new(0, 0, size.0, size.1);
                    let mut handled = false;
                    if let Some(area) = crate::ui::layout::library_area(app, terminal_area) {
                        let double_click = app.is_double_mouse_click(mouse.column, mouse.row);
                        let clicked_item = crate::ui::library_panel::handle_mouse_click(
                            app,
                            area,
                            mouse.column,
                            mouse.row,
                        );
                        if double_click && clicked_item && mouse.row != area.y + 1 {
                            app.activate_selected()?;
                        }
                        handled = clicked_item;
                    }
                    if !handled {
                        if let Some(area) = crate::ui::layout::lyrics_area(app, terminal_area) {
                        crate::ui::lyrics_panel::handle_mouse_click(
                            app,
                            area,
                            mouse.column,
                            mouse.row,
                        )?;
                        }
                    }
                }
                _ => {}
            }
            return Ok(false);
        }
        Event::Key(key) => {
        if key.kind == KeyEventKind::Release {
            // Crossterm sends release events separately from the press/repeat
            // events. Commit the accumulated target when seeking ends.
            if matches!(key.code, KeyCode::Left | KeyCode::Right) {
                app.finish_seek()?;
            }
            return Ok(false);
        }

        // ── Priority 1: new playlist input ───────────────────────────────────
        if app.new_playlist_mode {
            match key.code {
                KeyCode::Esc => app.new_playlist_cancel(),
                KeyCode::Enter => app.new_playlist_confirm()?,
                KeyCode::Backspace => app.new_playlist_backspace(),
                KeyCode::Char(c) => app.new_playlist_type_char(c),
                _ => {}
            }
            return Ok(false);
        }

        // ── Priority 2: confirm dialog ────────────────────────────────────────
        if app.confirm_dialog.is_some() {
            match key.code {
                KeyCode::Enter | KeyCode::Char('y') => app.confirm_dialog_confirm()?,
                KeyCode::Esc | KeyCode::Char('n') => app.confirm_dialog_cancel(),
                _ => {}
            }
            return Ok(false);
        }

        // ── Priority 3: save overlay ──────────────────────────────────────────
        if app.save_mode {
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => app.cancel_save(),
                KeyCode::Enter => app.confirm_save()?,
                KeyCode::Char('j') | KeyCode::Down => {
                    if app.save_selected + 1 < app.save_candidates.len() {
                        app.save_selected += 1;
                    }
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    if app.save_selected > 0 {
                        app.save_selected -= 1;
                    }
                }
                _ => {}
            }
            return Ok(false);
        }

        // ── Priority 4: search overlay ────────────────────────────────────────
        if app.search_mode {
            match key.code {
                KeyCode::Esc => app.exit_search(),
                KeyCode::Enter => app.search_confirm()?,
                KeyCode::Down => app.search_select_next(),
                KeyCode::Up => app.search_select_prev(),
                KeyCode::Backspace => app.search_backspace(),
                KeyCode::Char(c) => app.search_type_char(c),
                _ => {}
            }
            return Ok(false);
        }

        // ── Normal input ──────────────────────────────────────────────────────
        if matches!(key.code, KeyCode::Left | KeyCode::Right)
            && matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
        {
            app.update_seek(key.code == KeyCode::Right);
            return Ok(false);
        }

        let action = map_key_with_config(key, &app.keys);
        match action {
            Action::Quit => return Ok(true),
            Action::TogglePlay => app.toggle_play()?,
            Action::Next => app.next_track()?,
            Action::Prev => app.prev_track()?,
            Action::VolumeUp => app.volume_up(),
            Action::VolumeDown => app.volume_down(),
            Action::SelectDown => app.select_next(),
            Action::SelectUp => app.select_prev(),
            Action::ConfirmSelect => app.play_selected()?,
            Action::TabNext => app.tab_next(),
            Action::TabPrev => app.tab_prev(),
            Action::ExitScope => app.exit_scope(),
            Action::AddToFavorites => app.add_to_favorites()?,
            Action::SaveToPlaylist => app.open_save_mode(),
            Action::DeleteFromPlaylist => app.request_delete_playlist(),
            Action::NewPlaylist => app.start_new_playlist(),
            Action::DeletePlaylist => app.request_delete_playlist(),
            Action::OpenSearch => app.enter_search(),
            Action::GoToPlaying => app.go_to_playing(),
            Action::ToggleShuffle => app.toggle_shuffle(),
            Action::ToggleRepeat => app.toggle_repeat(),
            Action::SeekForward => app.seek_forward()?,
            Action::SeekBackward => app.seek_backward()?,
            Action::ShowLyrics => app.toggle_lyrics(),
            Action::CancelDialog => app.confirm_dialog_cancel(),
            Action::ConfirmDialog => app.confirm_dialog_confirm()?,
            Action::SetLayout(i) => app.set_layout(i),
            Action::EditLyrics => app.edit_lyrics()?,
            Action::GotoTop => app.go_to_top(),
            Action::GotoBottom => app.go_to_bottom(),
            Action::MoveToTop => app.move_selected_song(false)?,
            Action::MoveToBottom => app.move_selected_song(true)?,

            Action::None => {}
        }
        }
        _ => {}
    }
    Ok(false)
}
