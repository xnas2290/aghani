use std::path::Path;

use anyhow::Result;
use walkdir::WalkDir;

use crate::library::track::Track;

const SUPPORTED_EXTENSIONS: &[&str] = &["mp3", "flac", "ogg", "wav", "m4a", "aac", "opus"];

pub fn scan_directory(dir: &Path) -> Result<Vec<Track>> {
    let mut tracks = Vec::new();

    for entry in WalkDir::new(dir)
        .follow_links(true)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        // WalkDir already knows the type from readdir; avoid an extra stat per
        // entry (which is expensive on large libraries).
        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.path();
        let is_supported = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| {
                SUPPORTED_EXTENSIONS
                    .iter()
                    .any(|supported| ext.eq_ignore_ascii_case(supported))
            });

        if is_supported {
            tracks.push(Track::new(path.to_path_buf()));
        }
    }

    tracks.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(tracks)
}
