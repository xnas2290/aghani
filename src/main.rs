mod app;
mod audio;
mod config;
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
use std::sync::{Arc, Mutex, mpsc};
use std::{env, io};

use app::App;
use config::Config;
use events::handler::handle_events;
use library::cache::MetadataCache;

fn main() -> Result<()> {
    // Redirect stderr so ALSA messages don't bleed into TUI
    if let Ok(log) = std::fs::File::create("/tmp/aghani.log") {
        use std::os::unix::io::IntoRawFd;
        unsafe {
            libc::dup2(log.into_raw_fd(), 2);
        }
    }

    // Load config — creates default if missing
    let mut config = Config::load()?;

    // CLI arg overrides config music_dir
    let music_dir = env::args()
        .nth(1)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| config.music_dir.clone());

    if !music_dir.exists() {
        eprintln!(
            "Music directory {:?} does not exist.\n\
             Set it in ~/.config/aghani/conf or pass as argument: aghani <path>",
            music_dir
        );
        std::process::exit(1);
    }

    // Update config with resolved music dir
    config.music_dir = music_dir.clone();
    ui::theme::init_theme(config.colors.clone());
    // Terminal setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Cache + channel
    let cache_path = music_dir.join(".aghani-cache.json");
    let cache = Arc::new(Mutex::new(MetadataCache::load(&cache_path)));
    let (tx, rx) = mpsc::channel();
    let mut app = App::new(&music_dir, rx, &config)?;

    // Background metadata loader
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
            if tx.send((i, track, None::<Vec<u8>>)).is_err() {
                break;
            }
        }

        if dirty {
            let _ = cache.save(&cache_path_clone);
        }
    });

    let result = run_loop(&mut terminal, &mut app, &mut config);

    // Save session before exit
    let last_track = app.current_track().map(|t| t.path.clone());
    let last_position = if last_track.is_some() {
        Some(app.player.elapsed().as_secs_f64())
    } else {
        None
    };
    let last_tab = Some(format!("{:?}", app.active_tab));
    let _ = config.save_session(last_track, last_position, last_tab);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }

    Ok(())
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    _config: &mut Config,
) -> Result<()> {
    let mut was_overlay = false;
    std::thread::sleep(std::time::Duration::from_millis(100));
    app.drain_metadata(); // process any instantly-available cache hits
    let _ = app.restore_session(&_config);
    loop {
        let is_overlay = app.save_mode || app.search_mode;
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
