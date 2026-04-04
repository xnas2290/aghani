use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct LrcLine {
    pub time:  Duration,
    pub text:  String,
}

#[derive(Debug, Clone)]
pub struct Lyrics {
    pub lines: Vec<LrcLine>,
    pub synced: bool,
}

impl Lyrics {
    /// Returns the current line index based on playback position
    pub fn current_line(&self, elapsed: Duration) -> Option<usize> {
        if self.lines.is_empty() { return None; }
        // Find the last line whose timestamp <= elapsed
        let mut idx = 0;
        for (i, line) in self.lines.iter().enumerate() {
            if line.time <= elapsed {
                idx = i;
            } else {
                break;
            }
        }
        Some(idx)
    }
}

// ── Cache path ────────────────────────────────────────────────────────────────

pub fn cache_path(artist: &str, title: &str) -> PathBuf {
    let cache_dir = dirs_next::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("aghani")
        .join("lyrics");
    let _ = std::fs::create_dir_all(&cache_dir);
    let filename = format!(
        "{}-{}.lrc",
        sanitize(artist),
        sanitize(title)
    );
    cache_dir.join(filename)
}

fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' })
        .collect::<String>()
        .to_lowercase()
}

// ── LRCLIB fetch ──────────────────────────────────────────────────────────────

pub fn fetch_lyrics(
    title: &str,
    artist: &str,
    album: &str,
    duration_secs: f64,
) -> Option<Lyrics> {
    let cache = cache_path(artist, title);

    // Check cache first
    if cache.exists() {
        let content = std::fs::read_to_string(&cache).ok()?;
        if content.trim() == "NOT_FOUND" {
            return None; // cached negative result
        }
        return parse_lrc(&content);
    }

    // Fetch from LRCLIB
    let url = format!(
        "https://lrclib.net/api/get?artist_name={}&track_name={}&album_name={}&duration={}",
        urlenccode(artist),
        urlenccode(title),
        urlenccode(album),
        duration_secs as u32,
    );

    let response = ureq::get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .call()
        .ok()?;

    if response.status() == 404 {
        // Cache the negative result so we don't hit the API repeatedly
        let _ = std::fs::write(&cache, "NOT_FOUND");
        return None;
    }

    let body = response.into_string().ok()?;
    let json: serde_json::Value = serde_json::from_str(&body).ok()?;

    // Prefer synced lyrics, fall back to plain
    let lrc_content = json["syncedLyrics"]
        .as_str()
        .or_else(|| json["plainLyrics"].as_str())?;

    if lrc_content.is_empty() {
        let _ = std::fs::write(&cache, "NOT_FOUND");
        return None;
    }

    // Cache the result
    let _ = std::fs::write(&cache, lrc_content);

    parse_lrc(lrc_content)
}

fn urlenccode(s: &str) -> String {
    s.chars().map(|c| match c {
        'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
        ' ' => "+".to_string(),
        c => format!("%{:02X}", c as u32),
    }).collect()
}

// ── LRC parser ────────────────────────────────────────────────────────────────

pub fn parse_lrc(content: &str) -> Option<Lyrics> {
    let mut lines: Vec<LrcLine> = Vec::new();
    let mut synced = false;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }

        // Try to parse timestamp [mm:ss.xx]
        if line.starts_with('[') {
            if let Some(close) = line.find(']') {
                let tag = &line[1..close];
                let text = line[close + 1..].trim().to_string();

                if let Some(dur) = parse_timestamp(tag) {
                    synced = true;
                    if !text.is_empty() {
                        lines.push(LrcLine { time: dur, text });
                    }
                    continue;
                }
            }
        }

        // Plain lyric line
        if !line.starts_with('[') {
            lines.push(LrcLine {
                time: Duration::ZERO,
                text: line.to_string(),
            });
        }
    }

    if lines.is_empty() { return None; }

    lines.sort_by_key(|l| l.time);

    Some(Lyrics { lines, synced })
}

fn parse_timestamp(s: &str) -> Option<Duration> {
    // Format: mm:ss.xx or mm:ss
    let parts: Vec<&str> = s.splitn(2, ':').collect();
    if parts.len() != 2 { return None; }

    let mins: f64 = parts[0].parse().ok()?;
    let secs: f64 = parts[1].parse().ok()?;

    Some(Duration::from_secs_f64(mins * 60.0 + secs))
}