use crate::library::track::Track;
use anyhow::Result;
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

/// A lyric line while it is being synchronized. `time` stays `None` until the
/// user stamps the line during a sync session.
#[derive(Debug, Clone)]
pub struct SyncLine {
    pub text: String,
    pub time: Option<Duration>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LrcLibResponse {
    synced_lyrics: Option<String>,
    plain_lyrics: Option<String>,
}

impl Lyrics {
    /// Build an editable draft from these lyrics. Existing timestamps are
    /// preserved when re-syncing a file that is already synchronized.
    pub fn to_sync_lines(&self) -> Vec<SyncLine> {
        self.lines
            .iter()
            .map(|line| SyncLine {
                text: line.text.clone(),
                time: if self.synced { Some(line.time) } else { None },
            })
            .collect()
    }

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

    /// Time left until the next timed line starts. Used to schedule a repaint
    /// exactly when the highlighted lyric changes instead of polling.
    #[cfg(any(feature = "gui", test))]
    pub fn next_line_in(&self, elapsed: Duration) -> Option<Duration> {
        let next = self.lines.partition_point(|line| line.time <= elapsed);
        self.lines
            .get(next)
            .map(|line| line.time.saturating_sub(elapsed))
    }

    pub fn adjacent_line_time(&self, elapsed: Duration, forward: bool) -> Option<Duration> {
        if !self.synced {
            return None;
        }

        let current = self.current_line(elapsed)?;
        let adjacent = if forward {
            current
                .checked_add(1)
                .filter(|&index| index < self.lines.len())?
        } else {
            current.checked_sub(1)?
        };
        Some(self.lines[adjacent].time)
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

pub fn flagged_lyrics_cache_dir() -> PathBuf {
    let dir = cache_dir().join("flagged");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Marker file recording that the lyrics for a track were flagged as wrong.
pub fn flagged_lyrics_path(track_path: &std::path::Path) -> PathBuf {
    let filename = format!("{}.flag", sanitize_path(track_path));
    flagged_lyrics_cache_dir().join(filename)
}

/// True when the user flagged this track's lyrics as wrong. Flagged lyrics are
/// hidden and are not looked up again until the flag is cleared.
pub fn has_flagged(track_path: &std::path::Path) -> bool {
    flagged_lyrics_path(track_path).exists()
}

#[cfg(any(feature = "gui", test))]
pub fn flag_lyrics(track_path: &std::path::Path) -> Result<()> {
    let path = flagged_lyrics_path(track_path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, "flagged\n")?;
    Ok(())
}

pub fn unflag_lyrics(track_path: &std::path::Path) -> Result<()> {
    match std::fs::remove_file(flagged_lyrics_path(track_path)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

// ── Manual lyrics path — stored in cache, NOT next to audio file ──────────────

pub fn manual_lyrics_path(track_path: &std::path::Path) -> PathBuf {
    let filename = format!("{}.lrc", sanitize_path(track_path));
    manual_lyrics_cache_dir().join(filename)
}

fn sanitize_path(path: &std::path::Path) -> String {
    path.to_string_lossy()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

pub fn load_manual(track_path: &std::path::Path) -> Option<Lyrics> {
    let content = std::fs::read_to_string(manual_lyrics_path(track_path)).ok()?;
    parse_lrc(&content)
}

pub fn has_manual(track_path: &std::path::Path) -> bool {
    manual_lyrics_path(track_path).exists()
}

/// Save synchronized lyrics in the same LRC format used by manual lyrics.
///
/// Only lines that have a timestamp are written. Returns the number of lines
/// written; when no line has been stamped the existing file is left untouched.
pub fn save_synced(track: &Track, lines: &[SyncLine]) -> Result<usize> {
    let Some((output, count)) = render_synced(track, lines) else {
        return Ok(0);
    };

    let path = manual_lyrics_path(&track.path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, output)?;
    Ok(count)
}

fn render_synced(track: &Track, lines: &[SyncLine]) -> Option<(String, usize)> {
    let stamped: Vec<&SyncLine> = lines.iter().filter(|line| line.time.is_some()).collect();
    if stamped.is_empty() {
        return None;
    }

    let mut output = String::new();
    output.push_str(&format!(
        "[ti:{}]\n[ar:{}]\n[al:{}]\n[by:aghani]\n\n",
        header_value(&track.title),
        header_value(&track.artist),
        header_value(&track.album),
    ));

    for line in &stamped {
        if let Some(time) = line.time {
            output.push_str(&format!("[{}] {}\n", format_timestamp(time), line.text));
        }
    }

    Some((output, stamped.len()))
}

fn header_value(value: &str) -> String {
    value.replace(['\r', '\n'], " ")
}

/// Format a duration as an LRC timestamp (`mm:ss.cc`).
pub fn format_timestamp(time: Duration) -> String {
    let centis = (time.as_secs_f64() * 100.0).round().max(0.0) as u64;
    let minutes = centis / 6_000;
    let seconds = (centis / 100) % 60;
    let fraction = centis % 100;
    format!("{minutes:02}:{seconds:02}.{fraction:02}")
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
    // Wrong lyrics are hidden until the user clears the flag, even if a
    // cached or manual copy exists.
    if has_flagged(track_path) {
        return None;
    }
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
        // Ignore LRC metadata tags and comments used by the manual template.
        if line.starts_with('#') || is_metadata_tag(line) {
            continue;
        }

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

fn is_metadata_tag(line: &str) -> bool {
    let Some(inner) = line
        .strip_prefix('[')
        .and_then(|line| line.split_once(']').map(|(inner, _)| inner))
    else {
        return false;
    };
    let Some((key, _)) = inner.split_once(':') else {
        return false;
    };
    matches!(
        key.to_ascii_lowercase().as_str(),
        "ti" | "ar" | "al" | "by" | "offset" | "re" | "ve" | "length" | "au"
    )
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

#[cfg(test)]
mod tests {
    use super::{
        fetch_lyrics, flag_lyrics, format_timestamp, has_flagged, manual_lyrics_path, parse_lrc,
        render_synced, unflag_lyrics, LrcLine, Lyrics, SyncLine,
    };
    use crate::library::track::Track;
    use std::path::PathBuf;
    use std::time::Duration;

    #[test]
    fn adjacent_synced_lyric_time_steps_between_verses() {
        let lyrics = Lyrics {
            lines: vec![
                LrcLine {
                    time: Duration::from_secs(2),
                    text: "First".into(),
                },
                LrcLine {
                    time: Duration::from_secs(5),
                    text: "Second".into(),
                },
                LrcLine {
                    time: Duration::from_secs(8),
                    text: "Third".into(),
                },
            ],
            synced: true,
        };

        assert_eq!(
            lyrics.adjacent_line_time(Duration::from_secs(6), true),
            Some(Duration::from_secs(8))
        );
        assert_eq!(
            lyrics.adjacent_line_time(Duration::from_secs(6), false),
            Some(Duration::from_secs(2))
        );
        assert_eq!(lyrics.adjacent_line_time(Duration::ZERO, false), None);
        assert_eq!(
            lyrics.adjacent_line_time(Duration::from_secs(9), true),
            None
        );
    }

    #[test]
    fn next_synced_lyric_line_reports_time_until_boundary() {
        let lyrics = Lyrics {
            lines: vec![
                LrcLine {
                    time: Duration::from_secs(2),
                    text: "First".into(),
                },
                LrcLine {
                    time: Duration::from_secs(5),
                    text: "Second".into(),
                },
            ],
            synced: true,
        };

        assert_eq!(
            lyrics.next_line_in(Duration::from_secs(1)),
            Some(Duration::from_secs(1))
        );
        assert_eq!(
            lyrics.next_line_in(Duration::from_secs(3)),
            Some(Duration::from_secs(2))
        );
        // At or after the last timestamp there is no next line to wait for.
        assert_eq!(lyrics.next_line_in(Duration::from_secs(5)), None);
        assert_eq!(lyrics.next_line_in(Duration::from_secs(9)), None);
    }

    #[test]
    fn adjacent_lyric_time_is_disabled_for_plain_lyrics() {
        let lyrics = Lyrics {
            lines: vec![LrcLine {
                time: Duration::ZERO,
                text: "Plain".into(),
            }],
            synced: false,
        };

        assert_eq!(lyrics.adjacent_line_time(Duration::ZERO, true), None);
    }

    #[test]
    fn flagged_lyrics_are_hidden_until_unflagged() {
        let track_path =
            PathBuf::from(format!("/tmp/aghani-lyrics-flag-{}.mp3", std::process::id()));
        let _ = unflag_lyrics(&track_path);

        // Seed manual lyrics so fetching would otherwise succeed offline.
        let manual = manual_lyrics_path(&track_path);
        std::fs::create_dir_all(manual.parent().unwrap()).unwrap();
        std::fs::write(&manual, "[00:01.00] line\n").unwrap();

        assert!(fetch_lyrics("Title", "Artist", "Album", 1.0, &track_path).is_some());

        flag_lyrics(&track_path).unwrap();
        assert!(has_flagged(&track_path));
        assert!(fetch_lyrics("Title", "Artist", "Album", 1.0, &track_path).is_none());

        unflag_lyrics(&track_path).unwrap();
        assert!(!has_flagged(&track_path));
        assert!(fetch_lyrics("Title", "Artist", "Album", 1.0, &track_path).is_some());

        let _ = std::fs::remove_file(manual);
    }

    #[test]
    fn formats_lrc_timestamps() {
        assert_eq!(format_timestamp(Duration::ZERO), "00:00.00");
        assert_eq!(format_timestamp(Duration::from_millis(12_340)), "00:12.34");
        assert_eq!(format_timestamp(Duration::from_millis(61_234)), "01:01.23");
        // Rounding must roll over into the next minute.
        assert_eq!(format_timestamp(Duration::from_millis(59_999)), "01:00.00");
    }

    #[test]
    fn renders_synced_lyrics_in_existing_lrc_format() {
        let track = Track::new(PathBuf::from("/tmp/sync-test.mp3"));
        let lines = vec![
            SyncLine {
                text: "First line".to_string(),
                time: Some(Duration::from_millis(12_340)),
            },
            SyncLine {
                text: "Not stamped yet".to_string(),
                time: None,
            },
            SyncLine {
                text: "Last line".to_string(),
                time: Some(Duration::from_millis(61_234)),
            },
        ];

        let (output, count) = render_synced(&track, &lines).expect("some lines are stamped");
        assert_eq!(count, 2);
        assert!(output.contains("[00:12.34] First line"));
        assert!(output.contains("[01:01.23] Last line"));
        assert!(!output.contains("Not stamped yet"));

        let parsed = parse_lrc(&output).expect("valid LRC");
        assert!(parsed.synced);
        assert_eq!(parsed.lines.len(), 2);
        assert_eq!(parsed.lines[0].time, Duration::from_millis(12_340));
        assert_eq!(parsed.lines[1].text, "Last line");
    }
}
