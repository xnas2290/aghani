mod app;
mod audio;
mod cover;
mod error;
mod events;
mod library;
mod ui;

use anyhow::Result;
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};
use std::{env, io};

use app::App;
use events::handler::handle_events;
use library::cache::MetadataCache;

fn main() -> Result<()> {
    let log_file = std::fs::File::create("/tmp/tui-player.log").expect("failed to create log file");
    use std::os::unix::io::IntoRawFd;
    unsafe {
        libc::dup2(log_file.into_raw_fd(), 2); // fd 2 = stderr
    }
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

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let cache_path = music_dir.join(".tui-player-cache.json");
    let cache = Arc::new(Mutex::new(MetadataCache::load(&cache_path)));

    // UNBOUNDED channel — sender never blocks
    let (tx, rx) = mpsc::channel();
    let mut app = App::new(&music_dir, rx)?;

    let tracks_paths: Vec<_> = app.tracks.iter().map(|t| t.path.clone()).collect();
    let cache_clone = Arc::clone(&cache);
    let cache_path_clone = cache_path.clone();

    std::thread::spawn(move || {
        let mut cache = cache_clone.lock().unwrap();
        let mut dirty = false;

        for (i, path) in tracks_paths.iter().enumerate() {
            let mut track = crate::library::track::Track::new(path.clone());

            if let Some(cached) = cache.get_valid(path) {
                track.title = cached.title.clone();
                track.artist = cached.artist.clone();
                track.album = cached.album.clone();
                track.duration = std::time::Duration::from_secs_f64(cached.duration_secs);
                track.has_cover = cached.has_cover;
                track.bitrate = cached.bitrate;
                track.sample_rate = cached.sample_rate;
                track.channels = cached.channels;
            } else {
                if crate::audio::metadata::enrich_track(&mut track).is_ok() {
                    cache.insert(path.clone(), &track);
                    dirty = true;
                }
            }

            // With unbounded channel this never blocks
            if tx.send((i, track, None::<Vec<u8>>)).is_err() {
                break;
            }
        }

        if dirty {
            let _ = cache.save(&cache_path_clone);
        }
    });

    let result = run_loop(&mut terminal, &mut app);

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
    let mut was_overlay = false;
    loop {
        let is_overlay = app.save_mode || app.search_mode;

        // Force full redraw when overlay opens or closes
        if is_overlay != was_overlay {
            terminal.clear()?;
        }
        was_overlay = is_overlay;

        terminal.draw(|f| {
            ui::layout::draw(f, app);
        })?;

        let should_quit = handle_events(app)?;
        if should_quit {
            break;
        }

        app.tick()?;
    }
    Ok(())
}
