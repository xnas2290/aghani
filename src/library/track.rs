// use std::fs::File;
use std::path::PathBuf;
use std::time::Duration;

// use symphonia::core::{
//     codecs::CODEC_TYPE_NULL, formats::FormatOptions, io::MediaSourceStream, meta::MetadataOptions,
//     probe::Hint,
// };
// use symphonia::default::get_probe;

#[derive(Debug, Clone)]
pub struct Track {
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: Duration,
    pub has_cover: bool,
    pub bitrate: Option<u32>,     // approximate kbps
    pub sample_rate: Option<u32>, // Hz
    pub channels: Option<u8>,     // number of audio channels
    pub replaygain: Option<f32>,  // in dB, positive or negative
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
            artist: "Unknown Artist".to_string(),
            album: "Unknown Album".to_string(),
            duration: Duration::ZERO,
            has_cover: false,
            bitrate: None,
            sample_rate: None,
            channels: None,
            replaygain: None,
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

    pub fn bitrate_str(&self) -> String {
        self.bitrate
            .map(|b| format!("{} kbps", b))
            .unwrap_or_else(|| "—".into())
    }

    pub fn sample_rate_str(&self) -> String {
        self.sample_rate
            .map(|s| format!("{} Hz", s))
            .unwrap_or_else(|| "—".into())
    }
}
