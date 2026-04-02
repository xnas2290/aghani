use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct Track {
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: Duration,
    pub has_cover: bool,
    pub bitrate: Option<u32>,     // kbps
    pub sample_rate: Option<u32>, // Hz
    // pub channels: Option<u8>,     // number of audio channels
}

impl Track {
    pub fn new(path: PathBuf) -> Self {
        let title = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        Track {
            path,
            title,
            artist: String::from("Unknown Artist"),
            album: String::from("Unknown Album"),
            duration: Duration::ZERO,
            has_cover: false,
            bitrate: None,
            sample_rate: None,
            // channels: None,
        }
    }

    pub fn duration_str(&self) -> String {
        let secs = self.duration.as_secs();
        let m = secs / 60;
        let s = secs % 60;
        format!("{:02}:{:02}", m, s)
    }
    pub fn extension(&self) -> String {
        self.path
            .extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string()
    }
}
