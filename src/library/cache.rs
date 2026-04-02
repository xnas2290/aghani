use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::library::track::Track;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CachedTrack {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_secs: f64,
    pub has_cover: bool,
    pub bitrate: Option<u32>,
    pub sample_rate: Option<u32>,
    pub channels: Option<u8>,
    pub modified: u64, // file mtime as secs since epoch
}

#[derive(Serialize, Deserialize, Debug, Default)]
pub struct MetadataCache {
    pub entries: HashMap<PathBuf, CachedTrack>,
}

impl MetadataCache {
    pub fn load(cache_path: &Path) -> Self {
        std::fs::read_to_string(cache_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, cache_path: &Path) -> Result<()> {
        let json = serde_json::to_string(self)?;
        std::fs::write(cache_path, json)?;
        Ok(())
    }

    pub fn get_valid(&self, path: &Path) -> Option<&CachedTrack> {
        let cached = self.entries.get(path)?;
        let mtime = mtime(path)?;
        if cached.modified == mtime {
            Some(cached)
        } else {
            None // file changed, cache stale
        }
    }

    pub fn insert(&mut self, path: PathBuf, track: &Track) {
        let modified = mtime(&path).unwrap_or(0);
        self.entries.insert(path, CachedTrack {
            title: track.title.clone(),
            artist: track.artist.clone(),
            album: track.album.clone(),
            duration_secs: track.duration.as_secs_f64(),
            has_cover: track.has_cover,
            bitrate: track.bitrate,
            sample_rate: track.sample_rate,
            channels: track.channels,
            modified,
        });
    }
}

pub fn mtime(path: &Path) -> Option<u64> {
    path.metadata().ok()?
        .modified().ok()?
        .duration_since(SystemTime::UNIX_EPOCH).ok()
        .map(|d| d.as_secs())
}