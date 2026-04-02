use std::path::{Path, PathBuf};
use anyhow::Result;

#[derive(Debug, Clone)]
pub struct Playlist {
    pub name: String,
    pub path: Option<PathBuf>,
    pub track_paths: Vec<PathBuf>, // always absolute canonical paths
}

fn canonicalize(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

impl Playlist {
    pub fn new(name: String, path: Option<PathBuf>) -> Self {
        Playlist { name, path, track_paths: vec![] }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let name = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let content = std::fs::read_to_string(path)?;
        let base = path.parent().unwrap_or(Path::new("."));

        let track_paths = content
            .lines()
            .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
            .map(|l| {
                let p = Path::new(l.trim());
                let full = if p.is_absolute() { p.to_path_buf() } else { base.join(p) };
                canonicalize(&full) // always store absolute canonical
            })
            .collect();

        Ok(Playlist { name, path: Some(path.to_path_buf()), track_paths })
    }

    pub fn save(&self) -> Result<()> {
        let path = match &self.path {
            Some(p) => p,
            None => return Ok(()),
        };
        let base = path.parent().unwrap_or(Path::new("."));
        let mut lines = vec!["#EXTM3U".to_string()];
        for track_path in &self.track_paths {
            // Save as relative path so m3u is portable
            let rel = pathdiff::diff_paths(track_path, base)
                .unwrap_or_else(|| track_path.clone());
            lines.push(rel.to_string_lossy().to_string());
        }
        std::fs::write(path, lines.join("\n"))?;
        Ok(())
    }

    pub fn add_track(&mut self, path: PathBuf) -> bool {
        let canonical = canonicalize(&path);
        if self.track_paths.contains(&canonical) {
            return false;
        }
        self.track_paths.push(canonical);
        true
    }

    pub fn remove_track(&mut self, idx: usize) {
        if idx < self.track_paths.len() {
            self.track_paths.remove(idx);
        }
    }
}