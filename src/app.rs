use std::path::Path; //, PathBuf};

use anyhow::Result;
use image::DynamicImage;
use ratatui::text::Line;

use crate::audio::metadata::enrich_track;
use crate::audio::player::Player;
use crate::cover::extractor::{decode_cover, load_fallback};
use crate::cover::renderer::render_halfblock;
use crate::library::scanner::scan_directory;
use crate::library::track::Track;

pub struct App {
    pub tracks: Vec<Track>,
    pub player: Player,
    pub current_index: Option<usize>,
    pub selected_index: Option<usize>,
    /// Pre-rendered half-block lines for the current cover
    pub cover_lines: Option<Vec<Line<'static>>>,
    pub cover_area_w: u32,
    pub cover_area_h: u32,
    fallback_cover: Option<DynamicImage>,
    //scroll
    pub scroll_offset: usize,
}

impl App {
    pub fn new(music_dir: &Path) -> Result<Self> {
        let player = Player::new()?;

        // Scan and eagerly enrich metadata (titles, duration, etc.)
        let mut tracks = scan_directory(music_dir)?;
        for track in &mut tracks {
            if let Err(e) = enrich_track(track) {
                eprintln!("Warning: could not read tags for {:?}: {}", track.path, e);
            }
        }

        let selected_index = if tracks.is_empty() { None } else { Some(0) };

        // Try to load fallback cover
        let fallback_cover = load_fallback(Path::new("assets/default_cover.png")).ok();

        Ok(App {
            tracks,
            player,
            current_index: None,
            selected_index,
            cover_lines: None,
            cover_area_w: 38, // default; updated on first draw
            cover_area_h: 20,
            fallback_cover,
            scroll_offset: 0,
        })
    }

    pub fn current_track(&self) -> Option<&Track> {
        self.current_index.and_then(|i| self.tracks.get(i))
    }

    // ── Playback controls ────────────────────────────────────────────────────

    pub fn play_index(&mut self, index: usize) -> Result<()> {
        if index >= self.tracks.len() {
            return Ok(());
        }
        self.current_index = Some(index);
        self.selected_index = Some(index);

        let path = self.tracks[index].path.clone();
        self.player.play(&path)?;
        self.refresh_cover(index)?;
        Ok(())
    }

    pub fn play_selected(&mut self) -> Result<()> {
        if let Some(idx) = self.selected_index {
            self.play_index(idx)?;
        }
        Ok(())
    }

    pub fn toggle_play(&mut self) -> Result<()> {
        if self.current_index.is_none() {
            // Nothing loaded yet — play first / selected track
            let idx = self.selected_index.unwrap_or(0);
            self.play_index(idx)?;
        } else {
            self.player.toggle_pause();
        }
        Ok(())
    }

    pub fn next_track(&mut self) -> Result<()> {
        if self.tracks.is_empty() {
            return Ok(());
        }
        let next = match self.current_index {
            Some(i) => (i + 1) % self.tracks.len(),
            None => 0,
        };
        self.play_index(next)
    }

    pub fn prev_track(&mut self) -> Result<()> {
        if self.tracks.is_empty() {
            return Ok(());
        }
        let prev = match self.current_index {
            Some(i) => {
                if i == 0 {
                    self.tracks.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.play_index(prev)
    }

    /// Called every tick — auto-advance when a track finishes
    pub fn tick(&mut self) -> Result<()> {
        if self.current_index.is_some() && self.player.is_finished() {
            self.next_track()?;
        }
        Ok(())
    }

    // ── Library navigation ───────────────────────────────────────────────────

    pub fn select_next(&mut self) {
        if self.tracks.is_empty() {
            return;
        }
        self.selected_index = Some(match self.selected_index {
            Some(i) => (i + 1) % self.tracks.len(),
            None => 0,
        });
    }

    pub fn select_prev(&mut self) {
        if self.tracks.is_empty() {
            return;
        }
        self.selected_index = Some(match self.selected_index {
            Some(i) => {
                if i == 0 {
                    self.tracks.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        });
    }

    // ── Volume ───────────────────────────────────────────────────────────────

    pub fn volume_up(&mut self) {
        self.player.volume_up();
    }

    pub fn volume_down(&mut self) {
        self.player.volume_down();
    }

    // ── Cover art ────────────────────────────────────────────────────────────

    fn refresh_cover(&mut self, index: usize) -> Result<()> {
        let track = &mut self.tracks[index];

        // Try to re-read cover data from tags
        let cover_data = enrich_track(track)?;

        let img: Option<DynamicImage> = cover_data
            .as_deref()
            .and_then(|bytes| decode_cover(bytes).ok())
            .or_else(|| self.fallback_cover.clone());

        self.cover_lines = img.map(|i| render_halfblock(&i, self.cover_area_w, self.cover_area_h));

        Ok(())
    }

    /// Call when the terminal is resized / cover area dimensions change.
    pub fn update_cover_size(&mut self, w: u32, h: u32) {
        if w != self.cover_area_w || h != self.cover_area_h {
            self.cover_area_w = w;
            self.cover_area_h = h;
            if let Some(idx) = self.current_index {
                let _ = self.refresh_cover(idx);
            }
        }
    }
}
