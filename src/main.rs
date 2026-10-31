mod app;
mod audio;
mod config;
#[cfg(feature = "tui")]
mod cover;
#[cfg(feature = "gui")]
#[path = "ui/dynamic_theme.rs"]
mod dynamic_theme;
mod error;
#[cfg(feature = "tui")]
mod events;
mod file_manager;
#[cfg(feature = "gui")]
mod gui;
mod ipc;
mod library;
mod mpris;
#[cfg(feature = "tui")]
mod ui;
use anyhow::Result;
#[cfg(feature = "tui")]
use app::App;
#[cfg(feature = "tui")]
use config::{Config, State};
mod lyrics;
#[cfg(feature = "tui")]
use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
#[cfg(feature = "tui")]
use events::handler::handle_events;
#[cfg(feature = "tui")]
use ratatui::{backend::CrosstermBackend, Terminal};
#[cfg(feature = "tui")]
use std::sync::mpsc;
// use std::{env, io};
#[cfg(feature = "tui")]
use std::io;
mod cli;

#[cfg(all(feature = "gui", feature = "tui"))]
compile_error!("Enable exactly one interface feature: `tui` or `gui`.");

#[cfg(not(any(feature = "gui", feature = "tui")))]
compile_error!("Enable an interface feature: `tui` or `gui`.");

#[cfg(feature = "tui")]
fn main() -> Result<()> {
    tui_main()
}

#[cfg(feature = "gui")]
fn main() -> Result<()> {
    gui::run()?;
    Ok(())
}

#[cfg(feature = "tui")]
fn tui_main() -> Result<()> {
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
    // Looking the executable up in PATH avoids spawning ffmpeg on every
    // startup just to print `-version`. Playback will report a missing binary
    // if the lookup is wrong.
    let found = std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| {
                let candidate = dir.join("ffmpeg");
                candidate.is_file()
            })
        })
        .unwrap_or(false);

    if !found {
        eprintln!("Error: ffmpeg not found. Please install ffmpeg.");
        eprintln!("  Ubuntu/Debian: sudo apt install ffmpeg");
        eprintln!("  Arch:          sudo pacman -S ffmpeg");
        eprintln!("  macOS:         brew install ffmpeg");
        std::process::exit(1);
    }
    Ok(())
}
#[cfg(feature = "tui")]
fn run(dir_override: Option<std::path::PathBuf>) -> Result<()> {
    let pid = std::process::id();
    std::panic::set_hook(Box::new(move |info| {
        // Kill any ffmpeg children on panic
        let _ = std::process::Command::new("pkill")
            .args(["-P", &pid.to_string()])
            .status();
        eprintln!("Panic: {}", info);
    }));
    let _ = check_dependencies();

    let lock_path = "/tmp/aghani.lock";
    // Do this before redirecting stderr so a second instance's warning is
    // visible in the terminal that started it.
    ensure_single_instance(lock_path)?;

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

    if !music_dir.exists() {
        eprintln!("Directory does not exist: {:?}", music_dir);
        std::process::exit(1);
    }
    ui::theme::init_theme(config.colors.clone());
    // Terminal setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // IPC channel
    let (ipc_tx, ipc_rx) = mpsc::channel::<ipc::IpcCommand>();
    ipc::server::start_ipc(ipc_tx);

    // Create app and start the shared background metadata loader.
    let mut app = App::new(&music_dir, &config)?;
    let (mpris_rx, mpris_update_tx) = mpris::start_mpris();
    app.mpris_rx = Some(mpris_rx);
    app.mpris_update_tx = Some(mpris_update_tx);
    app.ipc_rx = Some(ipc_rx);

    // Set panic hook to clean up lock file on unexpected exit
    let lock_path_panic = lock_path.to_string();
    std::panic::set_hook(Box::new(move |info| {
        let _ = std::fs::remove_file(&lock_path_panic);
        eprintln!("Panic: {}", info);
    }));
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
    app.player.stop(); // kill ffmpeg process
    state.last_tab = Some(format!("{:?}", app.active_tab));
    state.volume = Some(app.player.volume);
    state.history = app.history.clone();
    state.pending_history = app.pending_history.clone();
    state.play_counts = app.play_counts.clone();
    state.artwork_overrides = app.artwork_overrides.clone();
    state.pinned_playlists = app.pinned_playlists.clone();
    let _ = state.save();

    // Cleanup IPC socket
    let _ = std::fs::remove_file(ipc::server::SOCKET_PATH);

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;
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
            // A PID can be reused by an unrelated process. Only treat the
            // lock as active when its command is actually Aghani.
            let alive = std::fs::read_to_string(format!("/proc/{}/cmdline", pid))
                .ok()
                .and_then(|cmdline| cmdline.split('\0').next().map(str::to_owned))
                .and_then(|command| {
                    std::path::Path::new(&command)
                        .file_name()
                        .map(|name| name == "aghani")
                })
                .unwrap_or(false);
            if alive {
                eprintln!("Warning: Aghani is already running (pid {}). Exiting.", pid);
                std::process::exit(0);
            }

            // Process is dead — stale lock, continue
            println!("Removing stale Aghani lock (pid {})", pid);
        }
    }

    // Write our PID to the lock file
    let pid = std::process::id();
    let mut f = std::fs::File::create(lock_path)?;
    write!(f, "{}", pid)?;

    Ok(())
}

#[cfg(feature = "tui")]
fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    _config: &Config,
    state: &State,
) -> Result<()> {
    let mut was_overlay = false;
    // Redraw at a modest idle cadence; input is still handled immediately and
    // triggers an immediate repaint below.
    let draw_interval = std::time::Duration::from_millis(250);

    app.drain_metadata();
    let _ = app.restore_session_from_state(state);

    terminal.draw(|f| {
        ui::layout::draw(f, app);
    })?;
    let mut last_draw = std::time::Instant::now();

    loop {
        let timeout = draw_interval
            .checked_sub(last_draw.elapsed())
            .unwrap_or(std::time::Duration::ZERO);

        // let is_overlay = app.save_mode || app.search_mode;

        // Overlay just opened or closed — clear and redraw immediately
        let is_overlay = app.save_mode
            || app.search_mode
            || app.confirm_dialog.is_some()
            || app.new_playlist_mode
            || app.artwork_path_mode
            || app.metadata_edit_mode
            || app.lyrics_sync_mode;
        if is_overlay != was_overlay {
            terminal.clear()?;
            terminal.draw(|f| {
                ui::layout::draw(f, app);
            })?;
            last_draw = std::time::Instant::now();
            was_overlay = is_overlay;
            continue;
        }
        let mut force_draw = false;
        if app.needs_full_redraw {
            terminal.clear()?;
            app.needs_full_redraw = false;
            force_draw = true;
        }

        let had_event = crossterm::event::poll(timeout.min(std::time::Duration::from_millis(50)))?;
        if had_event {
            if handle_events(app)? {
                break;
            }
            // External editors and terminal hand-offs invalidate the previous
            // frame, so repaint from scratch before drawing again.
            if app.needs_full_redraw {
                terminal.clear()?;
                app.needs_full_redraw = false;
            }
            force_draw = true;
        }

        app.tick()?;

        if force_draw || last_draw.elapsed() >= draw_interval {
            terminal.draw(|f| {
                ui::layout::draw(f, app);
            })?;
            last_draw = std::time::Instant::now();
        }
    }
    Ok(())
}
