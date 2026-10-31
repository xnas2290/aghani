use crate::app::App;
use crate::events::keys::{map_key_with_config, Action};
use anyhow::Result;
use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
// use std::time::Duration;

pub fn handle_events(app: &mut App) -> Result<bool> {
    if !crossterm::event::poll(std::time::Duration::from_millis(0))? {
        return Ok(false);
    }
    match event::read()? {
        Event::Mouse(mouse) => {
            match mouse.kind {
                MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                    let size = crossterm::terminal::size()?;
                    let terminal_area = ratatui::layout::Rect::new(0, 0, size.0, size.1);
                    let compact = matches!(app.layout_mode.as_str(), "compact" | "compact_lyrics");
                    let volume_area = crate::ui::layout::player_area(app, terminal_area)
                        .is_some_and(|area| {
                            crate::ui::player_panel::is_volume_area(
                                area,
                                mouse.column,
                                mouse.row,
                                compact,
                            )
                        });
                    if volume_area {
                        match mouse.kind {
                            MouseEventKind::ScrollUp => app.volume_up(),
                            MouseEventKind::ScrollDown => app.volume_down(),
                            _ => unreachable!(),
                        }
                    } else if let Some(area) = crate::ui::layout::lyrics_area(app, terminal_area)
                        .filter(|area| {
                            area.contains(ratatui::layout::Position::new(mouse.column, mouse.row))
                        })
                    {
                        let up = matches!(mouse.kind, MouseEventKind::ScrollUp);
                        crate::ui::lyrics_panel::handle_mouse_scroll(app, area, up);
                    } else if crate::ui::layout::cover_area(app, terminal_area).is_some_and(
                        |area| {
                            area.contains(ratatui::layout::Position::new(mouse.column, mouse.row))
                        },
                    ) {
                        match mouse.kind {
                            MouseEventKind::ScrollUp => app.prev_track()?,
                            MouseEventKind::ScrollDown => app.next_track()?,
                            _ => unreachable!(),
                        }
                    } else {
                        let columns = crate::ui::layout::library_area(app, terminal_area)
                            .map(|area| (area.width.saturating_sub(2) / 18).max(1) as usize)
                            .unwrap_or(1);
                        let up = matches!(mouse.kind, MouseEventKind::ScrollUp);
                        app.scroll_list(up, columns);
                    }
                }
                MouseEventKind::Down(MouseButton::Left) => {
                    let size = crossterm::terminal::size()?;
                    let terminal_area = ratatui::layout::Rect::new(0, 0, size.0, size.1);
                    let compact = matches!(app.layout_mode.as_str(), "compact" | "compact_lyrics");

                    // Clicking the album art toggles between the normal view and
                    // the lyrics view.
                    if let Some(area) = crate::ui::layout::cover_area(app, terminal_area) {
                        if crate::ui::cover_panel::handle_mouse_click(
                            app,
                            area,
                            mouse.column,
                            mouse.row,
                        ) {
                            return Ok(false);
                        }
                    }

                    let mut handled = crate::ui::layout::player_area(app, terminal_area)
                        .map(|area| {
                            crate::ui::player_panel::handle_mouse_click(
                                app,
                                area,
                                mouse.column,
                                mouse.row,
                                compact,
                            )
                        })
                        .transpose()?
                        .unwrap_or(false);
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
                        handled |= clicked_item;
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

            // ── Priority 1: lyric synchronization ────────────────────────────────
            if app.lyrics_sync_mode {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Esc => app.finish_lyrics_sync(false)?,
                        KeyCode::Down => app.stamp_lyrics_sync_line()?,
                        KeyCode::Char(' ') => app.toggle_play()?,
                        _ => {}
                    }
                }
                return Ok(false);
            }

            // ── Priority 2: metadata editor ──────────────────────────────────────
            if app.metadata_edit_mode {
                match key.code {
                    KeyCode::Esc => app.cancel_metadata_edit(),
                    KeyCode::Enter => app.save_metadata_edit()?,
                    KeyCode::Up | KeyCode::BackTab => app.metadata_edit_prev_field(),
                    KeyCode::Down | KeyCode::Tab => app.metadata_edit_next_field(),
                    KeyCode::Left => app.metadata_edit_cursor_left(),
                    KeyCode::Right => app.metadata_edit_cursor_right(),
                    KeyCode::Home => app.metadata_edit_cursor_home(),
                    KeyCode::End => app.metadata_edit_cursor_end(),
                    KeyCode::Backspace => app.metadata_edit_backspace(),
                    KeyCode::Delete => app.metadata_edit_delete(),
                    KeyCode::Char(c)
                        if !key
                            .modifiers
                            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                    {
                        app.metadata_edit_insert_char(c)
                    }
                    _ => {}
                }
                return Ok(false);
            }

            // ── Priority 2: new playlist input ───────────────────────────────────
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

            if app.artwork_path_mode {
                match key.code {
                    KeyCode::Esc => app.cancel_artwork_path(),
                    KeyCode::Enter => app.confirm_artwork_path()?,
                    KeyCode::Backspace => {
                        app.artwork_path_input.pop();
                    }
                    KeyCode::Char(c) => app.artwork_path_input.push(c),
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
            // The single-quote key starts lyric synchronization.
            if key.kind == KeyEventKind::Press
                && matches!(key.code, KeyCode::Char('\''))
                && !key
                    .modifiers
                    .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT)
            {
                app.start_lyrics_sync();
                return Ok(false);
            }

            if matches!(key.code, KeyCode::Char('p'))
                && app.active_tab == crate::library::LibraryTab::Playlists
                && matches!(&app.playlist_scope, crate::library::PlaylistScope::All)
            {
                app.toggle_pin_selected_playlist();
                return Ok(false);
            }

            if matches!(key.code, KeyCode::Left | KeyCode::Right)
                && matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat)
            {
                app.update_seek(key.code == KeyCode::Right);
                return Ok(false);
            }

            if app.layout_mode.ends_with("_lyrics")
                && matches!(key.code, KeyCode::Up | KeyCode::Down)
            {
                if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                    app.step_synced_lyrics(key.code == KeyCode::Down)?;
                }
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
                Action::SetArtwork => app.start_artwork_path(),
                Action::ToggleArtwork => app.toggle_artwork(),
                Action::ToggleGrid => app.toggle_grid_view(),
                Action::CycleSort => app.cycle_sort(),
                Action::EditMetadata => app.start_metadata_edit(),
                Action::OpenInFileManager => app.open_selected_in_file_manager()?,

                Action::None => {}
            }
        }
        _ => {}
    }
    Ok(false)
}
