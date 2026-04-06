use serde::Deserialize;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct LrcLine {
    pub time: Duration,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct Lyrics {
    pub lines: Vec<LrcLine>,
    #[allow(dead_code)]
    pub synced: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LrcLibResponse {
    synced_lyrics: Option<String>,
    plain_lyrics: Option<String>,
}

impl Lyrics {
    /// Optimized binary search for the current line
    pub fn current_line(&self, elapsed: Duration) -> Option<usize> {
        if self.lines.is_empty() {
            return None;
        }

        // binary search for the first line that is GREATER than elapsed
        // then step back one.
        match self.lines.partition_point(|line| line.time <= elapsed) {
            0 => Some(0),
            i => Some(i - 1),
        }
    }
}
// ── Cache directories ─────────────────────────────────────────────────────────

pub fn cache_dir() -> PathBuf {
    dirs_next::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("aghani")
        .join("lyrics")
}

pub fn auto_lyrics_cache_dir() -> PathBuf {
    let dir = cache_dir().join("auto");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn manual_lyrics_cache_dir() -> PathBuf {
    let dir = cache_dir().join("manual");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

// ── Manual lyrics path — stored in cache, NOT next to audio file ──────────────

pub fn manual_lyrics_path(track_path: &std::path::Path) -> PathBuf {
    let filename = format!("{}.lrc", sanitize_path(track_path));
    manual_lyrics_cache_dir().join(filename)
}

fn sanitize_path(path: &std::path::Path) -> String {
    path.to_string_lossy()
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' })
        .collect()
}

pub fn load_manual(track_path: &std::path::Path) -> Option<Lyrics> {
    let content = std::fs::read_to_string(manual_lyrics_path(track_path)).ok()?;
    parse_lrc(&content)
}

pub fn has_manual(track_path: &std::path::Path) -> bool {
    manual_lyrics_path(track_path).exists()
}

// ── Auto lyrics cache path ────────────────────────────────────────────────────

pub fn cache_path(artist: &str, title: &str) -> PathBuf {
    let filename = format!("{}-{}.lrc", sanitize(artist), sanitize(title));
    auto_lyrics_cache_dir().join(filename)
}








// ── Improved Encoding ─────────────────────────────────────────────────────────

fn url_encode(s: &str) -> String {
    // Using a more robust encoding approach for UTF-8
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | '~') {
                c.to_string()
            } else {
                c.encode_utf8(&mut [0; 4])
                    .as_bytes()
                    .iter()
                    .map(|b| format!("%{:02X}", b))
                    .collect()
            }
        })
        .collect()
}

// ── Fetch Logic with Fallbacks ────────────────────────────────────────────────

pub fn fetch_lyrics(
    title: &str,
    artist: &str,
    album: &str,
    duration: f64,
    track_path: &std::path::Path,
) -> Option<Lyrics> {
    if let Some(lyrics) = load_manual(track_path) {
        return Some(lyrics);
    }
    let cache = cache_path(artist, title);

    if cache.exists() {
        let content = std::fs::read_to_string(&cache).ok()?;
        if content == "NOT_FOUND" {
            return None;
        }
        return parse_lrc(&content);
    }

    // Try Source 1: LRCLIB Direct Get (Fastest)
    if let Some(lrc) = lrclib_get(artist, title, album, duration) {
        let _ = std::fs::write(&cache, &lrc);
        return parse_lrc(&lrc);
    }

    // Try Source 2: LRCLIB Search (Better for special characters/mismatched metadata)
    if let Some(lrc) = lrclib_search(artist, title) {
        let _ = std::fs::write(&cache, &lrc);
        return parse_lrc(&lrc);
    }

    // Cache negative result
    let _ = std::fs::write(&cache, "NOT_FOUND");
    None
}

/// Exact match lookup
fn lrclib_get(artist: &str, title: &str, album: &str, dur: f64) -> Option<String> {
    let url = format!(
        "https://lrclib.net/api/get?artist_name={}&track_name={}&album_name={}&duration={}",
        url_encode(artist),
        url_encode(title),
        url_encode(album),
        dur as u32
    );

    request_lrc_content(&url)
}

/// Broader search lookup - useful when titles have (feat. X) or special symbols
fn lrclib_search(artist: &str, title: &str) -> Option<String> {
    let query = url_encode(&format!("{} {}", artist, title));
    let url = format!("https://lrclib.net/api/search?q={}", query);

    let resp = ureq::get(&url).call().ok()?;
    let results: Vec<LrcLibResponse> = resp.into_json().ok()?;

    // Pick the first result that has lyrics
    results
        .into_iter()
        .find_map(|r| r.synced_lyrics.or(r.plain_lyrics))
}

fn request_lrc_content(url: &str) -> Option<String> {
    let resp = ureq::get(url).timeout(Duration::from_secs(5)).call().ok()?;

    let data: LrcLibResponse = resp.into_json().ok()?;
    data.synced_lyrics.or(data.plain_lyrics)
}

// ── Improved Parser ───────────────────────────────────────────────────────────

pub fn parse_lrc(content: &str) -> Option<Lyrics> {
    let mut lines = Vec::new();
    let mut synced = false;

    for line in content.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if line.starts_with('[') && line.contains(']') {
            let parts: Vec<&str> = line.splitn(2, ']').collect();
            if parts.len() == 2 {
                let timestamp = &parts[0][1..]; // strip '['
                let text = parts[1].trim();

                if let Some(dur) = parse_timestamp(timestamp) {
                    synced = true;
                    if !text.is_empty() {
                        lines.push(LrcLine {
                            time: dur,
                            text: text.to_string(),
                        });
                    }
                    continue;
                }
            }
        }

        // If not a timestamped line but we haven't found any timestamps yet,
        // treat as plain lyrics
        if !synced {
            lines.push(LrcLine {
                time: Duration::ZERO,
                text: line.to_string(),
            });
        }
    }

    if lines.is_empty() {
        return None;
    }
    lines.sort_by_key(|l| l.time);
    Some(Lyrics { lines, synced })
}

fn parse_timestamp(s: &str) -> Option<Duration> {
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 2 {
        return None;
    }

    let min: f64 = parts[0].parse().ok()?;
    let sec: f64 = parts[1].parse().ok()?;
    Some(Duration::from_secs_f64(min * 60.0 + sec))
}



fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}