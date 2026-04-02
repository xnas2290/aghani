mod app;
mod audio;
mod cover;
mod error;
mod events;
mod library;
mod ui;
use std::path::PathBuf;
use std::sync::mpsc;
use std::{env, io};

use anyhow::Result;
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

use app::App;
use events::handler::handle_events;

fn main() -> Result<()> {
    // Accept music directory as CLI argument, default to ~/Music
    let music_dir = env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| {
        dirs_next::audio_dir()
            .or_else(|| dirs_next::home_dir().map(|h| h.join("Music")))
            .unwrap_or_else(|| PathBuf::from("."))
    });

    if !music_dir.exists() {
        eprintln!(
            "Music directory {:?} does not exist.\nUsage: tmp <path-to-music-dir>",
            music_dir
        );
        std::process::exit(1);
    }

    // Terminal setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let (tx, rx) = mpsc::sync_channel(64);

    let mut app = App::new(&music_dir, rx)?;

    // Spawn background metadata loader AFTER app is created
    let tracks_paths: Vec<_> = app.tracks.iter().map(|t| t.path.clone()).collect();
    std::thread::spawn(move || {
        for (i, path) in tracks_paths.iter().enumerate() {
            let mut track = crate::library::track::Track::new(path.clone());
            if let Ok(cover) = crate::audio::metadata::enrich_track(&mut track) {
                if tx.send((i, track, cover)).is_err() {
                    break; // receiver dropped, stop loading
                }
            }
            // Yield to OS every 10 tracks so the main thread gets CPU time
            if i % 10 == 0 {
                std::thread::yield_now();
            }
        }
    });
    // let mut app = App::new(&music_dir)?;

    let result = run_loop(&mut terminal, &mut app);

    // Always restore terminal even on error
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }

    Ok(())
}

fn run_loop(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    loop {
        // Draw
        terminal.draw(|f| {
            ui::layout::draw(f, app);
        })?;

        // Update cover size from the actual rendered cover area
        // The cover panel inner is (40 - 2 borders = 38) wide, 22 - 2 = 20 tall
        // let size = terminal.size()?;
        // let cover_w = 38u32.min(size.width as u32);
        // let cover_h = 20u32.min(size.height as u32);
        // app.update_cover_size(cover_w, cover_h);

        // Handle input (blocks up to 50 ms)
        let should_quit = handle_events(app)?;
        if should_quit {
            break;
        }

        // Tick: auto-advance on track end, etc.
        app.tick()?;
        app.scroll_offset = app.scroll_offset.wrapping_add(1);
    }
    Ok(())
}
