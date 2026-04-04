use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const APP_NAME: &str = "aghani";
const CONFIG_FILE: &str = "conf";
const STATE_FILE: &str = "state";

pub fn config_path() -> PathBuf {
    dirs_next::config_dir()
        .unwrap_or_else(|| PathBuf::from(".config"))
        .join(APP_NAME)
        .join(CONFIG_FILE)
}

pub fn state_path() -> PathBuf {
    dirs_next::config_dir()
        .unwrap_or_else(|| PathBuf::from(".config"))
        .join(APP_NAME)
        .join(STATE_FILE)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct KeysConfig {
    pub quit: String,
    pub play_pause: String,
    pub next: String,
    pub prev: String,
    pub seek_forward: String,
    pub seek_backward: String,
    pub volume_up: String,
    pub volume_down: String,
    pub select_up: String,
    pub select_down: String,
    pub confirm: String,
    pub tab_next: String,
    pub tab_prev: String,
    pub exit_scope: String,
    pub save_to_playlist: String,
    pub delete_from_playlist: String,
    pub add_to_favorites: String,
    pub shuffle: String,
    pub search: String,
    pub go_to_playing: String,
    pub toggle_repeat: String,
    pub new_playlist: String,
    pub delete_playlist: String,
    pub set_layout_1: String,
    pub set_layout_2: String,
    pub set_layout_3: String,
    pub set_layout_4: String,
    pub show_lyrics: String,
}

impl Default for KeysConfig {
    fn default() -> Self {
        KeysConfig {
            quit: "q".into(),
            play_pause: "Space".into(),
            next: "n".into(),
            prev: "p".into(),
            seek_forward: "Right".into(),
            seek_backward: "Left".into(),
            volume_up: "=".into(),
            volume_down: "-".into(),
            select_up: "k".into(),
            select_down: "j".into(),
            confirm: "Enter".into(),
            tab_next: "Tab".into(),
            tab_prev: "BackTab".into(),
            exit_scope: "x".into(),
            save_to_playlist: "s".into(),
            delete_from_playlist: "d".into(),
            add_to_favorites: "f".into(),
            shuffle: "r".into(),
            search: "/".into(),
            go_to_playing: "g".into(),
            toggle_repeat: "R".into(),
            new_playlist: "A".into(),
            delete_playlist: "D".into(),
            set_layout_1: "1".into(),
            set_layout_2: "2".into(),
            set_layout_3: "3".into(),
            set_layout_4: "4".into(),
            show_lyrics: "l".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorsConfig {
    pub accent: String,
    pub title: String,
    pub normal: String,
    pub dim: String,
    pub highlight_fg: String,
    pub highlight_bg: String,
    pub border: String,
    pub border_active: String,
    pub progress: String,
    pub dynamic_theme: bool,         // extract colors from album art
    pub dynamic_theme_style: String, // "vibrant", "muted", "dark", "light"
}

impl Default for ColorsConfig {
    fn default() -> Self {
        ColorsConfig {
            accent: "Magenta".into(),
            title: "Cyan".into(),
            normal: "White".into(),
            dim: "DarkGray".into(),
            highlight_fg: "Black".into(),
            highlight_bg: "Cyan".into(),
            border: "DarkGray".into(),
            border_active: "Cyan".into(),
            progress: "Cyan".into(),
            dynamic_theme: false,
            dynamic_theme_style: "vibrant".into(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayoutConfig {
    /// Show cover art panel: true/false
    pub show_cover: bool,
    /// Show player panel: true/false  
    pub show_player: bool,
    /// Show lyrics panel: true/false
    // pub show_lyrics: bool,
    /// Cover panel width (columns)
    pub cover_width: u16,
    /// Cover panel height (rows)
    pub cover_height: u16,
    /// Player panel height (rows)
    pub player_height: u16,
    /// Layout mode: "default", "compact", "wide", "minimal"
    pub mode: String,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        LayoutConfig {
            show_cover: true,
            show_player: true,
            // show_lyrics: false,
            cover_width: 40,
            cover_height: 22,
            player_height: 12,
            mode: "default".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StartupConfig {
    pub tab: Option<String>,
    pub volume: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub music_dir: PathBuf,
    pub keys: KeysConfig,
    pub colors: ColorsConfig,
    pub startup: StartupConfig,
    pub layout: LayoutConfig,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            music_dir: dirs_next::audio_dir()
                .or_else(|| dirs_next::home_dir().map(|h| h.join("Music")))
                .unwrap_or_else(|| PathBuf::from(".")),
            keys: KeysConfig::default(),
            colors: ColorsConfig::default(),
            startup: StartupConfig::default(),
            layout: LayoutConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct State {
    pub last_track: Option<PathBuf>,
    pub last_position: Option<f64>,
    pub last_tab: Option<String>,
    pub volume: Option<f32>,
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = config_path();
        if !path.exists() {
            let cfg = Config::default();
            cfg.save()?;
            return Ok(cfg);
        }
        let content = std::fs::read_to_string(&path)?;
        let cfg: Config = toml::from_str(&content)?;
        Ok(cfg)
    }

    pub fn save(&self) -> Result<()> {
        let path = config_path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let content = toml::to_string_pretty(self)?;
        let header = "# Aghani Configuration\n# Edit keys and colors here. This file is NOT auto-overwritten.\n\n";
        std::fs::write(&path, format!("{}{}", header, content))?;
        Ok(())
    }
}

impl State {
    pub fn load() -> Self {
        let path = state_path();
        if let Ok(content) = std::fs::read_to_string(path) {
            toml::from_str(&content).unwrap_or_default()
        } else {
            State::default()
        }
    }

    pub fn save(&self) -> Result<()> {
        let path = state_path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}
