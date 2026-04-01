use anyhow::Result;
use crossterm::event::{self, Event, KeyEventKind};
use std::time::Duration;

use crate::app::App;
use crate::events::keys::{map_key, Action};

pub fn handle_events(app: &mut App) -> Result<bool> {
    if event::poll(Duration::from_millis(50))? {
        if let Event::Key(key) = event::read()? {
            // Ignore key release on Windows
            if key.kind == KeyEventKind::Release {
                return Ok(false);
            }

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
                Action::None => {}
            }
        }
    }
    Ok(false)
}
