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
}

impl Track {
    // pub fn from_path(path: PathBuf) -> Self {
    //     let mut track = Self::new(path.clone());

    //     // open file
    //     if let Ok(file) = File::open(&path) {
    //         let mss = MediaSourceStream::new(Box::new(file), Default::default());

    //         let mut hint = Hint::new();
    //         if let Some(ext) = path.extension() {
    //             hint.with_extension(&ext.to_string_lossy());
    //         }

    //         if let Ok(probed) = get_probe().format(
    //             &hint,
    //             mss,
    //             &FormatOptions::default(),
    //             &MetadataOptions::default(),
    //         ) {
    //             let format = probed.format;

    //             if let Some(t) = format
    //                 .tracks()
    //                 .iter()
    //                 .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
    //             {
    //                 let params = &t.codec_params;

    //                 // sample rate
    //                 track.sample_rate = params.sample_rate;

    //                 // duration from frames + sample rate
    //                 if let (Some(frames), Some(rate)) = (params.n_frames, params.sample_rate) {
    //                     let secs = frames as f64 / rate as f64;
    //                     track.duration = Duration::from_secs_f64(secs);
    //                 }

    //                 // approximate bitrate
    //                 if let Ok(md) = std::fs::metadata(&path) {
    //                     let size_bits = md.len() * 8;
    //                     if track.duration.as_secs() > 0 {
    //                         let approx =
    //                             (size_bits as f64 / track.duration.as_secs() as f64) / 1000.0;
    //                         track.bitrate = Some(approx as u32);
    //                     }
    //                 }
    //             }
    //         }
    //     }

    //     track
    // }

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
