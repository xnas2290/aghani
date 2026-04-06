mod app;
mod audio;
mod config;
mod cover;
mod error;
mod events;
mod ipc;
mod library;
mod mpris;
mod ui;
use anyhow::Result;
use app::App;
use config::{Config, State};
mod lyrics;
use crossterm::{
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use events::handler::handle_events;
use library::cache::MetadataCache;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::sync::{mpsc, Arc, Mutex};
// use std::{env, io};
use std::io;
mod cli;

fn main() -> Result<()> {
    use cli::{ClearTarget, CliAction};

    match cli::parse_args() {
        CliAction::Help => {
            cli::print_help();
            return Ok(());
        }
        CliAction::ShowCaches => {
            cli::show_caches();
            return Ok(());
        }
        CliAction::ClearAllCaches => {
            cli::clear_caches(ClearTarget::All);
            return Ok(());
        }
        CliAction::ClearManualLyrics => {
            cli::clear_caches(ClearTarget::ManualLyrics);
            return Ok(());
        }
        CliAction::ClearAutoLyrics => {
            cli::clear_caches(ClearTarget::AutoLyrics);
            return Ok(());
        }
        CliAction::ClearMusicDataCache => {
            cli::clear_caches(ClearTarget::MusicData);
            return Ok(());
        }
        CliAction::Run(dir_override) => {
            // Continue with normal startup
            run(dir_override)?;
        }
    }

    Ok(())
}
fn check_dependencies() -> Result<()> {
    if std::process::Command::new("ffmpeg")
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_err()
    {
        eprintln!("Error: ffmpeg not found. Please install ffmpeg.");
        eprintln!("  Ubuntu/Debian: sudo apt install ffmpeg");
        eprintln!("  Arch:          sudo pacman -S ffmpeg");
        eprintln!("  macOS:         brew install ffmpeg");
        std::process::exit(1);
    }
    Ok(())
}
fn run(dir_override: Option<std::path::PathBuf>) -> Result<()> {
    let _ = check_dependencies();

    // Load config FIRST before using it
    let _config = Config::load()?;

    if let Ok(log) = std::fs::File::create("/tmp/aghani.log") {
        use std::os::unix::io::IntoRawFd;
        unsafe {
            libc::dup2(log.into_raw_fd(), 2);
        }
    }
    let config = Config::load()?;
    let mut state = State::load();

    // Single music_dir — from CLI override or config
    let music_dir = dir_override.unwrap_or_else(|| config.music_dir.clone());
    // Remove the or_else args check — CLI is handled before run() is called

    let lock_path = "/tmp/aghani.lock";
    ensure_single_instance(lock_path)?;

    if let Ok(log) = std::fs::File::create("/tmp/aghani.log") {
        use std::os::unix::io::IntoRawFd;
        unsafe {
            libc::dup2(log.into_raw_fd(), 2);
        }
    }

    if !music_dir.exists() {
        eprintln!("Directory does not exist: {:?}", music_dir);
        std::process::exit(1);
    }
    ui::theme::init_theme(config.colors.clone());
    let (_mpris_state, _mpris_rx) = mpris::start_mpris();
    // Terminal setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Metadata channel
    let cache_path = library::cache::default_cache_path();
    let cache = Arc::new(Mutex::new(MetadataCache::load(&cache_path)));
    let (meta_tx, meta_rx) = mpsc::channel();

    // IPC channel
    let (ipc_tx, ipc_rx) = mpsc::channel::<ipc::IpcCommand>();
    ipc::server::start_ipc(ipc_tx);

    // Create app
    // let mut app = App::new(&music_dir, meta_rx, &config)?;
    let mut app = App::new(&music_dir, meta_rx, &config)?;
    let (mpris_rx, mpris_update_tx) = mpris::start_mpris();
    app.mpris_rx = Some(mpris_rx);
    app.mpris_update_tx = Some(mpris_update_tx);
    app.ipc_rx = Some(ipc_rx);

    // Background metadata loader
    let tracks_paths: Vec<_> = app.tracks.iter().map(|t| t.path.clone()).collect();
    let cache_clone = Arc::clone(&cache);
    let cache_path_clone = cache_path.clone();
    // Set panic hook to clean up lock file on unexpected exit
    let lock_path_panic = lock_path.to_string();
    std::panic::set_hook(Box::new(move |info| {
        let _ = std::fs::remove_file(&lock_path_panic);
        eprintln!("Panic: {}", info);
    }));
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
                track.replaygain = cached.replaygain;
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
    state.volume = Some(app.player.volume);
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
    let _ = std::fs::remove_file(lock_path);
    Ok(())
}

fn ensure_single_instance(lock_path: &str) -> Result<()> {
    use std::io::Write;

    // Check if lock file exists
    if let Ok(content) = std::fs::read_to_string(lock_path) {
        if let Ok(pid) = content.trim().parse::<u32>() {
            // Check if that process is still alive
            let alive = std::path::Path::new(&format!("/proc/{}", pid)).exists();
            if alive {
                eprintln!("Aghani is already running (pid {})", pid);
                std::process::exit(0);
            }
            // Process is dead — stale lock, continue
        }
    }

    // Write our PID to the lock file
    let pid = std::process::id();
    let mut f = std::fs::File::create(lock_path)?;
    write!(f, "{}", pid)?;

    Ok(())
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    _config: &Config,
    state: &State,
) -> Result<()> {
    let mut was_overlay = false;
    let mut last_draw = std::time::Instant::now();
    let draw_interval = std::time::Duration::from_millis(500);

    std::thread::sleep(std::time::Duration::from_millis(100));
    app.drain_metadata();
    let _ = app.restore_session_from_state(state);

    terminal.draw(|f| {
        ui::layout::draw(f, app);
    })?;

    loop {
        let timeout = draw_interval
            .checked_sub(last_draw.elapsed())
            .unwrap_or(std::time::Duration::ZERO);

        // let is_overlay = app.save_mode || app.search_mode;

        // Overlay just opened or closed — clear and redraw immediately
        let is_overlay = app.save_mode
            || app.search_mode
            || app.confirm_dialog.is_some()
            || app.new_playlist_mode;
        if is_overlay != was_overlay {
            terminal.clear()?;
            terminal.draw(|f| {
                ui::layout::draw(f, app);
            })?;
            last_draw = std::time::Instant::now();
            was_overlay = is_overlay;
            continue;
        }
        if app.needs_full_redraw {
            terminal.clear()?;
            app.needs_full_redraw = false;
        }

        if crossterm::event::poll(timeout)? {
            if handle_events(app)? {
                break;
            }
            terminal.draw(|f| {
                ui::layout::draw(f, app);
            })?;
            last_draw = std::time::Instant::now();
        } else {
            terminal.draw(|f| {
                ui::layout::draw(f, app);
            })?;
            last_draw = std::time::Instant::now();
        }

        app.tick()?;
    }
    Ok(())
}
