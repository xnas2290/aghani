use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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
    pub edit_lyrics: String,
    pub go_to_top: String,
    pub go_to_bottom: String,
    pub set_artwork: String,
    pub toggle_artwork: String,
    pub edit_metadata: String,
    pub open_in_file_manager: String,
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
            edit_lyrics: "L".into(),
            go_to_top: "Home".into(),
            go_to_bottom: "End".into(),
            set_artwork: "I".into(),
            toggle_artwork: "i".into(),
            edit_metadata: "e".into(),
            open_in_file_manager: "F".into(),
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
            show_cover: false,
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
#[serde(default)]
pub struct GuiConfig {
    pub dark_mode: bool,
    pub lyrics_font_size: f32,
    /// Last width of the lyrics side panel, restored on reopen and restart.
    pub lyrics_panel_width: f32,
    pub theme: String,
    pub muted_volume: Option<f32>,
    pub hide_single_track_albums: bool,
    pub show_waveform: bool,
}

impl Default for GuiConfig {
    fn default() -> Self {
        Self {
            dark_mode: true,
            lyrics_font_size: 20.0,
            lyrics_panel_width: 400.0,
            theme: "classic_dark".into(),
            muted_volume: None,
            hide_single_track_albums: false,
            show_waveform: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub music_dir: PathBuf,
    pub keys: KeysConfig,
    pub colors: ColorsConfig,
    pub startup: StartupConfig,
    pub layout: LayoutConfig,
    #[serde(default)]
    pub gui: GuiConfig,
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
            gui: GuiConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct State {
    pub last_track: Option<PathBuf>,
    pub last_position: Option<f64>,
    pub last_tab: Option<String>,
    pub volume: Option<f32>,
    pub history: Vec<PathBuf>,
    /// Plays made from the History playlist that have not been committed yet.
    pub pending_history: Vec<PathBuf>,
    pub play_counts: HashMap<PathBuf, u64>,
    pub artwork_overrides: HashMap<String, PathBuf>,
    pub pinned_playlists: Vec<String>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_without_gui_preferences_uses_gui_defaults() {
        let mut value = toml::Value::try_from(Config::default()).unwrap();
        value.as_table_mut().unwrap().remove("gui");
        let serialized = toml::to_string(&value).unwrap();
        let config: Config = toml::from_str(&serialized).unwrap();

        assert!(config.gui.dark_mode);
        assert_eq!(config.gui.lyrics_font_size, 20.0);
        assert_eq!(config.gui.theme, "classic_dark");
        assert_eq!(config.gui.muted_volume, None);
        assert!(!config.gui.hide_single_track_albums);
        assert!(config.gui.show_waveform);
    }

    #[test]
    fn older_gui_preferences_default_to_classic_dark_theme() {
        let mut value = toml::Value::try_from(Config::default()).unwrap();
        value
            .as_table_mut()
            .unwrap()
            .get_mut("gui")
            .unwrap()
            .as_table_mut()
            .unwrap()
            .remove("theme");
        let serialized = toml::to_string(&value).unwrap();
        let config: Config = toml::from_str(&serialized).unwrap();

        assert_eq!(config.gui.theme, "classic_dark");
    }

    #[test]
    fn older_gui_preferences_default_to_showing_single_track_albums() {
        let mut value = toml::Value::try_from(Config::default()).unwrap();
        value
            .as_table_mut()
            .unwrap()
            .get_mut("gui")
            .unwrap()
            .as_table_mut()
            .unwrap()
            .remove("hide_single_track_albums");
        value
            .as_table_mut()
            .unwrap()
            .get_mut("gui")
            .unwrap()
            .as_table_mut()
            .unwrap()
            .remove("show_waveform");
        let serialized = toml::to_string(&value).unwrap();
        let config: Config = toml::from_str(&serialized).unwrap();

        assert!(!config.gui.hide_single_track_albums);
        assert!(config.gui.show_waveform);
    }
}
