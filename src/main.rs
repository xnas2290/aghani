mod app;
mod audio;
mod config;
mod cover;
mod error;
mod events;
mod ipc;
mod library;
mod ui;

use anyhow::Result;
use app::App;
use config::{Config, State};
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use events::handler::handle_events;
use library::cache::MetadataCache;
use ratatui::{Terminal, backend::CrosstermBackend};
use std::sync::{Arc, Mutex, mpsc};
use std::{env, io};

fn main() -> Result<()> {
    // Redirect stderr so ALSA/ffmpeg noise doesn't bleed into TUI
    if let Ok(log) = std::fs::File::create("/tmp/aghani.log") {
        use std::os::unix::io::IntoRawFd;
        unsafe {
            libc::dup2(log.into_raw_fd(), 2);
        }
    }

    let config = Config::load()?;
    let mut state = State::load();

    let music_dir = env::args()
        .nth(1)
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| config.music_dir.clone());

    if !music_dir.exists() {
        eprintln!("Directory does not exist: {:?}", music_dir);
        std::process::exit(1);
    }

    ui::theme::init_theme(config.colors.clone());

    // Terminal setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Metadata channel
    let cache_path = music_dir.join(".aghani-cache.json");
    let cache = Arc::new(Mutex::new(MetadataCache::load(&cache_path)));
    let (meta_tx, meta_rx) = mpsc::channel();

    // IPC channel
    let (ipc_tx, ipc_rx) = mpsc::channel::<ipc::IpcCommand>();
    ipc::server::start_ipc(ipc_tx);

    // Create app
    let mut app = App::new(&music_dir, meta_rx, &config)?;
    app.ipc_rx = Some(ipc_rx);

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
            if meta_tx.send((i, track, None::<Vec<u8>>)).is_err() {
                break;
            }
        }
        if dirty {
            let _ = cache.save(&cache_path_clone);
        }
    });

    let result = run_loop(&mut terminal, &mut app, &config, &state);
    //clear icp socket on exit
    // Clear IPC status files on clean exit
    let _ = std::fs::write("/tmp/aghani-status", "");
    let _ = std::fs::write("/tmp/aghani-status.json", r#"{"status":"stopped"}"#);
    let _ = std::fs::remove_file(ipc::server::SOCKET_PATH);
    // Save session on exit
    state.last_track = app.current_track().map(|t| t.path.clone());
    state.last_position = if state.last_track.is_some() {
        Some(app.player.elapsed().as_secs_f64())
    } else {
        None
    };
    state.last_tab = Some(format!("{:?}", app.active_tab));
    let _ = state.save();

    // Cleanup IPC socket
    let _ = std::fs::remove_file(ipc::server::SOCKET_PATH);

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
    _config: &Config,
    state: &State,
) -> Result<()> {
    let mut was_overlay = false;

    std::thread::sleep(std::time::Duration::from_millis(100));
    app.drain_metadata();
    let _ = app.restore_session_from_state(state);

    loop {
        let is_overlay = app.save_mode || app.search_mode;
        if is_overlay != was_overlay {
            terminal.clear()?;
        }
        was_overlay = is_overlay;

        terminal.draw(|f| {
            ui::layout::draw(f, app);
        })?;

        if handle_events(app)? {
            break;
        }
        app.tick()?;
    }
    Ok(())
}
