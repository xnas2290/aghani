use std::path::PathBuf;
use anyhow::Result;
use serde::{Deserialize, Serialize};

const APP_NAME: &str = "aghani";
const CONFIG_FILE: &str = "conf";

/// Returns ~/.config/aghani/conf
pub fn config_path() -> PathBuf {
    dirs_next::config_dir()
        .unwrap_or_else(|| PathBuf::from(".config"))
        .join(APP_NAME)
        .join(CONFIG_FILE)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeysConfig {
    pub quit:               String,
    pub play_pause:         String,
    pub next:               String,
    pub prev:               String,
    pub seek_forward:       String,
    pub seek_backward:      String,
    pub volume_up:          String,
    pub volume_down:        String,
    pub select_up:          String,
    pub select_down:        String,
    pub confirm:            String,
    pub tab_next:           String,
    pub tab_prev:           String,
    pub exit_scope:         String,
    pub save_to_playlist:   String,
    pub delete_from_playlist: String,
    pub add_to_favorites:   String,
    pub shuffle:            String,
    pub search:             String,
    pub go_to_playing:      String,
}

impl Default for KeysConfig {
    fn default() -> Self {
        KeysConfig {
            quit:                   "q".into(),
            play_pause:             "Space".into(),
            next:                   "n".into(),
            prev:                   "p".into(),
            seek_forward:           "Right".into(),
            seek_backward:          "Left".into(),
            volume_up:              "+".into(),
            volume_down:            "-".into(),
            select_up:              "k".into(),
            select_down:            "j".into(),
            confirm:                "Enter".into(),
            tab_next:               "Tab".into(),
            tab_prev:               "BackTab".into(),
            exit_scope:             "x".into(),
            save_to_playlist:       "s".into(),
            delete_from_playlist:   "d".into(),
            add_to_favorites:       "f".into(),
            shuffle:                "r".into(),
            search:                 "/".into(),
            go_to_playing:          "g".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColorsConfig {
    pub accent:           String,
    pub title:            String,
    pub normal:           String,
    pub dim:              String,
    pub highlight_fg:     String,
    pub highlight_bg:     String,
    pub border:           String,
    pub border_active:    String,
    pub progress:         String,
}

impl Default for ColorsConfig {
    fn default() -> Self {
        ColorsConfig {
            accent:         "Magenta".into(),
            title:          "Cyan".into(),
            normal:         "White".into(),
            dim:            "DarkGray".into(),
            highlight_fg:   "Black".into(),
            highlight_bg:   "Cyan".into(),
            border:         "DarkGray".into(),
            border_active:  "Cyan".into(),
            progress:       "Cyan".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StartupConfig {
    /// Which tab to open on startup: "Songs", "Albums", "Artists", "Playlists"
    pub tab: Option<String>,
    /// Default volume 0.0–2.0
    pub volume: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionConfig {
    /// Last played track path
    pub last_track: Option<PathBuf>,
    /// Last position in seconds
    pub last_position: Option<f64>,
    /// Last active tab
    pub last_tab: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Path to music directory
    pub music_dir: PathBuf,
    pub keys: KeysConfig,
    pub colors: ColorsConfig,
    pub startup: StartupConfig,
    pub session: SessionConfig,
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
            session: SessionConfig::default(),
        }
    }
}

impl Config {
    /// Load config from ~/.config/aghani/conf, creating it with defaults if missing
    pub fn load() -> Result<Self> {
        let path = config_path();

        if !path.exists() {
            let cfg = Config::default();
            cfg.save()?;
            return Ok(cfg);
        }

        let content = std::fs::read_to_string(&path)?;
        let cfg: Config = toml::from_str(&content)
            .unwrap_or_else(|e| {
                eprintln!("Warning: config parse error: {} — using defaults", e);
                Config::default()
            });

        Ok(cfg)
    }

    /// Save config to ~/.config/aghani/conf
    pub fn save(&self) -> Result<()> {
        let path = config_path();

        // Create directory if needed
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }

        let content = toml::to_string_pretty(self)?;

        // Write with helpful comments header
        let header = "# Aghani Music Player Configuration\n\
                      # This file is auto-generated. Edit freely.\n\
                      # Keys: use key names like \"q\", \"Space\", \"Enter\", \"Tab\",\n\
                      #       \"BackTab\", \"Left\", \"Right\", \"Up\", \"Down\", \"F1\"–\"F12\"\n\
                      # Colors: \"Black\", \"Red\", \"Green\", \"Yellow\", \"Blue\",\n\
                      #         \"Magenta\", \"Cyan\", \"White\", \"DarkGray\",\n\
                      #         or hex like \"#ff6600\"\n\n";

        std::fs::write(&path, format!("{}{}", header, content))?;
        Ok(())
    }

    /// Save session state (last track, position, tab)
    pub fn save_session(
        &mut self,
        last_track: Option<PathBuf>,
        last_position: Option<f64>,
        last_tab: Option<String>,
    ) -> Result<()> {
        self.session.last_track    = last_track;
        self.session.last_position = last_position;
        self.session.last_tab      = last_tab;
        self.save()
    }
}