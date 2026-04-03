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
