use std::path::PathBuf;

pub enum CliAction {
    Run(Option<PathBuf>), // normal launch, optional music dir
    Help,
    ShowCaches,
    ClearAllCaches,
    ClearManualLyrics,
    ClearAutoLyrics,
    ClearMusicDataCache,
}

pub fn parse_args() -> CliAction {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if args.is_empty() {
        return CliAction::Run(None);
    }

    match args[0].as_str() {
        "--help" | "-h" => CliAction::Help,
        "--caches" => CliAction::ShowCaches,
        "--clear-caches" => CliAction::ClearAllCaches,
        "--clear-manual-lyrics" => CliAction::ClearManualLyrics,
        "--clear-auto-lyrics" => CliAction::ClearAutoLyrics,
        "--clear-music-data-cache" => CliAction::ClearMusicDataCache,
        "--dir" | "-d" => {
            if let Some(dir) = args.get(1) {
                CliAction::Run(Some(PathBuf::from(dir)))
            } else {
                eprintln!("Error: --dir requires a path argument");
                std::process::exit(1);
            }
        }
        "--version" | "-V" => {
            println!("aghani {}", env!("CARGO_PKG_VERSION"));
            std::process::exit(0);
        }
        arg if !arg.starts_with('-') => {
            // Positional argument = music dir (backward compat)
            CliAction::Run(Some(PathBuf::from(&args[0])))
        }
        unknown => {
            eprintln!("Unknown argument: {}\nRun with --help for usage.", unknown);
            std::process::exit(1);
        }
    }
}

pub fn print_help() {
    println!(
        r#"
aghani — A terminal music player

USAGE:
    aghani [OPTIONS] [MUSIC_DIR]

ARGUMENTS:
    MUSIC_DIR              Path to music directory (overrides config)

OPTIONS:
    -d, --dir <PATH>       Play music from directory
        --help             Show this help message
        --caches           Show all cache locations and sizes
        --clear-caches     Clear all caches
        --clear-manual-lyrics
                           Clear manually added lyrics
        --clear-auto-lyrics
                           Clear auto-fetched lyrics cache
        --clear-music-data-cache
                           Clear music metadata cache (tags, duration, etc.)

CONFIG:
    ~/.config/aghani/conf

KEYBINDS:
    Space          Play / Pause
    n / p          Next / Previous track
    j / k          Navigate list
    Enter          Select / Open
    Tab            Switch library tab
    /              Search
    s              Save to playlist
    f              Add to favorites
    r              Toggle shuffle
    Shift+R        Toggle repeat
    > / <          Seek forward / backward 5s
    + / -          Volume up / down
    g              Go to playing track
    t              Go to the top
    b              Go to the bottom
    L              Edit lyrics (opens $EDITOR)
    Shift+L        Toggle manual/fetched lyrics
    1-4            Switch layout
    x              Go back (exit scope)
    q              Quit

LAYOUTS:
    1  Default  (cover + player + library)
    2  Compact  (player + library)
    3  Minimal  (library only)
    4  Clean     (cover + player)

ENVIRONMENT:
    EDITOR / VISUAL    Editor for manual lyrics (default: nano)

MORE INFO:
    https://github.com/xnas2290/aghani
"#
    );
}

pub fn show_caches() {
    use crate::lyrics::{auto_lyrics_cache_dir, manual_lyrics_cache_dir};

    let config_dir = dirs_next::config_dir().unwrap_or_default().join("aghani");
    let _cache_base = dirs_next::cache_dir().unwrap_or_default().join("aghani");

    println!("aghani cache locations:\n");

    print_cache_entry("Config", &config_dir.join("conf"));
    print_cache_entry("Music metadata cache", &_cache_base.join("metadata.json"));
    print_cache_entry("Auto lyrics", &auto_lyrics_cache_dir());
    print_cache_entry("Manual lyrics", &manual_lyrics_cache_dir());
    print_cache_entry(
        "IPC status",
        &std::path::PathBuf::from("/tmp/aghani-status"),
    );
    print_cache_entry(
        "IPC status JSON",
        &std::path::PathBuf::from("/tmp/aghani-status.json"),
    );
    print_cache_entry("Log", &std::path::PathBuf::from("/tmp/aghani.log"));
}

fn print_cache_entry(label: &str, path: &std::path::Path) {
    let size = dir_size(path);
    let exists = path.exists();
    println!(
        "  {:<30} {}\n  {:<30} {}\n",
        label,
        if exists {
            "✓ exists"
        } else {
            "✗ not found"
        },
        "",
        path.display(),
    );
    if exists && size > 0 {
        println!("  {:<30} {}\n", "", format_size(size));
    }
}

fn dir_size(path: &std::path::Path) -> u64 {
    if path.is_file() {
        return path.metadata().map(|m| m.len()).unwrap_or(0);
    }
    walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .map(|e| e.metadata().map(|m| m.len()).unwrap_or(0))
        .sum()
}

fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

pub fn clear_caches(which: ClearTarget) {
    use crate::lyrics::{auto_lyrics_cache_dir, cache_dir};

    match which {
        ClearTarget::All => {
            clear_dir(&auto_lyrics_cache_dir(), "auto lyrics");
            clear_dir(&cache_dir().join("manual"), "manual lyrics");
            // Music data cache is in music dir — warn user
            println!("Note: music metadata cache (.aghani-cache.json) is in your music directory.");
            println!("Run --clear-music-data-cache to remove it.");
        }
        ClearTarget::ManualLyrics => {
            clear_dir(&cache_dir().join("manual"), "manual lyrics");
        }
        ClearTarget::AutoLyrics => {
            clear_dir(&auto_lyrics_cache_dir(), "auto lyrics");
        }
        ClearTarget::MusicData => {
            let count = find_and_remove(&cache_dir(), "metadata.json");
            println!("Removed {} music data cache file(s)", count);
        }
    }
}

pub enum ClearTarget {
    All,
    ManualLyrics,
    AutoLyrics,
    MusicData,
}

fn clear_dir(path: &std::path::Path, label: &str) {
    if !path.exists() {
        println!("No {} cache found", label);
        return;
    }
    match std::fs::remove_dir_all(path) {
        Ok(_) => println!("Cleared {} cache: {}", label, path.display()),
        Err(e) => eprintln!("Failed to clear {}: {}", label, e),
    }
}

fn find_and_remove(base: &std::path::Path, filename: &str) -> usize {
    let mut count = 0;
    if let Ok(entries) = walkdir::WalkDir::new(base)
        .max_depth(5)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy() == filename)
        .map(|e| e.path().to_path_buf())
        .collect::<Vec<_>>()
        .into_iter()
        .map(|p| std::fs::remove_file(&p))
        .collect::<Vec<_>>()
        .into_iter()
        .try_fold(0usize, |acc, r| r.map(|_| acc + 1))
    {
        count = entries;
    }
    count
}
