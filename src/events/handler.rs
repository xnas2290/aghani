use crate::app::App;
use crate::events::keys::{Action, map_key};
use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use std::time::Duration;

pub fn handle_events(app: &mut App) -> Result<bool> {
    if event::poll(Duration::from_millis(50))? {
        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Release {
                return Ok(false);
            }

            // Save mode captures ALL input — nothing else runs
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
                return Ok(false); // consume event, don't fall through
            }

            // Normal input
            let action = map_key(key);
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
                Action::DeleteFromPlaylist => app.delete_from_playlist()?,
                Action::None => {}
            }
        }
    }
    Ok(false)
}
