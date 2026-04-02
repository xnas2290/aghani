use anyhow::Result;
use image::DynamicImage;
use std::path::Path; //, PathBuf};
use std::sync::mpsc::Receiver;
// use ratatui::text::Line;
use crate::audio::metadata::enrich_track;
use crate::audio::player::Player;
use crate::cover::extractor::{decode_cover, load_fallback};
use crate::library::PlaylistScope;
use crate::library::playlist::Playlist;
// use crate::cover::renderer::render_halfblock;
use crate::library::scanner::scan_directory;
use crate::library::track::Track;
use crate::library::{LibraryScope, LibraryTab};
use std::collections::HashMap;
// use std::path::PathBuf;
#[derive(Debug, Clone, PartialEq)]
pub enum PlayContext {
    AllTracks,
    Album,
    Playlist(usize), // playlist index
}
pub struct App {
    pub play_context: PlayContext,
    // pub playlists: Vec<Playlist>,
    pub playlist_scope: PlaylistScope,
    // pub playlist_selected: Option<usize>, // cursor in playlist list
    pub playlist_song_selected: Option<usize>, // cursor inside an open playlist
    // pub music_dir: PathBuf,
    pub playlists: Vec<Playlist>,
    // Save-to-playlist overlay
    pub save_mode: bool,
    pub save_candidates: Vec<usize>, // playlist indices that don't have the song yet
    pub save_selected: usize,
    pub artist_album_counts: HashMap<String, usize>,
    pub tracks: Vec<Track>,
    pub player: Player,
    pub current_index: Option<usize>,
    pub selected_index: Option<usize>,
    pub scoped_song_selected: Option<usize>,
    /// Pre-rendered half-block lines for the current cover
    pub cover_image: Option<ratatui_image::protocol::StatefulProtocol>,
    pub picker: ratatui_image::picker::Picker,
    metadata_rx: Receiver<(usize, Track, Option<Vec<u8>>)>,
    pub loading: bool,
    pub loaded_count: usize,
    fallback_cover: Option<DynamicImage>,
    //scroll
    pub scroll_offset: usize,
    pub active_tab: LibraryTab,
    pub albums: Vec<String>,  // unique album names
    pub artists: Vec<String>, // unique artist names
    // pub playlists: Vec<String>, // placeholder for now
    pub album_selected: Option<usize>,
    pub artist_scoped_album_selected: Option<usize>,
    pub artist_selected: Option<usize>,
    pub playlist_selected: Option<usize>,
    // pub scope: LibraryScope,
    pub album_scope: LibraryScope,  // Albums tab drill-down
    pub artist_scope: LibraryScope, // Artists tab drill-down
    // pub scope_selected: Option<usize>, // selection within a scope view
    pub all_track_indices: Vec<usize>,
    pub scoped_track_indices: Vec<usize>, // indices into self.tracks for current scope
    pub scoped_albums: Vec<String>,       // albums for current artist scope
}

impl App {
    pub fn new(
        music_dir: &Path,
        metadata_rx: Receiver<(usize, Track, Option<Vec<u8>>)>,
    ) -> Result<Self> {
        let player = Player::new()?;
        // Load playlists
        let favorites_path = music_dir.join("favorites.m3u");
        let mut playlists: Vec<Playlist> = walkdir::WalkDir::new(music_dir)
            .max_depth(2)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().map(|x| x == "m3u").unwrap_or(false))
            .filter_map(|e| Playlist::load(e.path()).ok())
            .collect();

        // Ensure Favorites is always first
        if !playlists
            .iter()
            .any(|p| p.name.to_lowercase() == "favorites")
        {
            let fav = Playlist::new("Favorites".to_string(), Some(favorites_path));
            let _ = fav.save(); // create empty file if not exists
            playlists.insert(0, fav);
        } else {
            // Move favorites to front
            if let Some(i) = playlists
                .iter()
                .position(|p| p.name.to_lowercase() == "favorites")
            {
                let fav = playlists.remove(i);
                playlists.insert(0, fav);
            }
        }
        // Scan and eagerly enrich metadata (titles, duration, etc.)
        let tracks = scan_directory(music_dir)?;

        let mut albums: Vec<String> = tracks
            .iter()
            .map(|t| t.album.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        albums.sort();

        let mut artists: Vec<String> = tracks
            .iter()
            .map(|t| t.artist.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        artists.sort();
        let selected_index = if tracks.is_empty() { None } else { Some(0) };

        // Try to load fallback cover
        let fallback_cover = load_fallback(Path::new("assets/default_cover.png")).ok();
        let picker = ratatui_image::picker::Picker::from_query_stdio()
            .unwrap_or_else(|_| ratatui_image::picker::Picker::halfblocks());
        let album_selected = if albums.is_empty() { None } else { Some(0) };
        let artist_selected = if artists.is_empty() { None } else { Some(0) };
        let scoped_track_indices = (0..tracks.len()).collect();
        let all_track_indices = (0..tracks.len()).collect();

        Ok(App {
            tracks,
            player,
            current_index: None,
            selected_index,
            cover_image: None,
            picker,
            metadata_rx,
            loading: true,
            loaded_count: 0,
            albums: vec![],
            artists: vec![],
            scoped_albums: vec![],
            all_track_indices,
            scoped_track_indices,
            artist_album_counts: HashMap::new(),
            fallback_cover,
            scroll_offset: 0,
            active_tab: LibraryTab::Songs,
            album_selected,
            artist_selected,
            playlist_selected: None,
            // scope: LibraryScope::All,
            album_scope: LibraryScope::All,
            artist_scope: LibraryScope::All,
            scoped_song_selected: None,
            artist_scoped_album_selected: None,
            // playlists,
            playlist_scope: PlaylistScope::All,
            // playlist_selected: Some(0),
            playlist_song_selected: None,
            // music_dir: music_dir.to_path_buf(),
            save_mode: false,
            save_candidates: vec![],
            save_selected: 0,
            playlists,
            play_context: PlayContext::AllTracks,
        })
    }
    /// Resolve playlist track paths to indices in self.tracks
    pub fn playlist_track_indices(&self, playlist_idx: usize) -> Vec<usize> {
        let pl = &self.playlists[playlist_idx];
        pl.track_paths
            .iter()
            .filter_map(|path| {
                let result = self.tracks.iter().position(|t| &t.path == path);
                result
            })
            .collect()
    }

    pub fn enter_playlist(&mut self, idx: usize) {
        self.playlist_scope = PlaylistScope::Open(idx);
        self.playlist_song_selected = if self.playlists[idx].track_paths.is_empty() {
            None
        } else {
            Some(0)
        };
    }

    pub fn delete_from_playlist(&mut self) -> Result<()> {
        if let PlaylistScope::Open(pi) = self.playlist_scope {
            if let Some(si) = self.playlist_song_selected {
                self.playlists[pi].remove_track(si);
                self.playlists[pi].save()?;
                // Adjust cursor
                let len = self.playlists[pi].track_paths.len();
                self.playlist_song_selected = if len == 0 {
                    None
                } else {
                    Some(si.min(len - 1))
                };
            }
        }
        Ok(())
    }

    /// Open save-to-playlist overlay for the currently selected/playing song
    pub fn open_save_mode(&mut self) {
        // Which track are we saving?
        let track_path = match self.active_tab {
            LibraryTab::Songs => self
                .selected_index
                .and_then(|i| self.all_track_indices.get(i))
                .map(|&i| self.tracks[i].path.clone()),
            _ => self.current_track().map(|t| t.path.clone()),
        };
        let track_path = match track_path {
            Some(p) => p,
            None => return,
        };

        // Playlists that DON'T already contain this track
        self.save_candidates = self
            .playlists
            .iter()
            .enumerate()
            .filter(|(_, pl)| !pl.track_paths.contains(&track_path))
            .map(|(i, _)| i)
            .collect();

        if self.save_candidates.is_empty() {
            return; // song is already in all playlists, nothing to show
        }
        self.save_mode = true;
        self.save_selected = 0;
    }

    pub fn confirm_save(&mut self) -> Result<()> {
        if !self.save_mode {
            return Ok(());
        }

        let track_path = match self.active_tab {
            LibraryTab::Songs => self
                .selected_index
                .and_then(|i| self.all_track_indices.get(i))
                .map(|&i| self.tracks[i].path.clone()),
            _ => self.current_track().map(|t| t.path.clone()),
        };
        let track_path = match track_path {
            Some(p) => p,
            None => return Ok(()),
        };

        if let Some(&pi) = self.save_candidates.get(self.save_selected) {
            self.playlists[pi].add_track(track_path);
            self.playlists[pi].save()?;
        }
        self.save_mode = false;
        Ok(())
    }

    pub fn cancel_save(&mut self) {
        self.save_mode = false;
    }
    pub fn drain_metadata(&mut self) {
        let mut changed = false;

        for _ in 0..5 {
            // ← 5 instead of 50
            match self.metadata_rx.try_recv() {
                Ok((i, track, _cover)) => {
                    self.tracks[i].title = track.title;
                    self.tracks[i].artist = track.artist;
                    self.tracks[i].album = track.album;
                    self.tracks[i].duration = track.duration;
                    self.loaded_count += 1;
                    changed = true;
                }
                Err(_) => break,
            }
        }

        if self.loaded_count >= self.tracks.len() {
            self.loading = false;
        }

        // Only rebuild albums/artists every 50 tracks, not every tick
        if changed && self.loaded_count % 50 == 0 {
            self.rebuild_albums_and_artists();
        } else if !self.loading && changed {
            self.rebuild_albums_and_artists(); // final rebuild when done
        }
    }
    fn rebuild_albums_and_artists(&mut self) {
        let mut albums: Vec<String> = self
            .tracks
            .iter()
            .map(|t| t.album.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        albums.sort();

        let mut artists: Vec<String> = self
            .tracks
            .iter()
            .map(|t| t.artist.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        artists.sort();

        self.artist_album_counts = artists
            .iter()
            .map(|artist| {
                let count = albums
                    .iter()
                    .filter(|album| {
                        self.tracks
                            .iter()
                            .any(|t| &t.artist == artist && &t.album == *album)
                    })
                    .count();
                (artist.clone(), count)
            })
            .collect();

        self.albums = albums;
        self.artists = artists;

        // Rebuild scope cache in case albums/artists changed
        self.rebuild_album_cache();
        self.rebuild_artist_cache();
    }

    fn rebuild_album_cache(&mut self) {
        match &self.album_scope {
            LibraryScope::All => {
                self.scoped_track_indices = (0..self.tracks.len()).collect();
            }
            LibraryScope::Album(album) => {
                let album = album.clone();
                self.scoped_track_indices = self
                    .tracks
                    .iter()
                    .enumerate()
                    .filter(|(_, t)| t.album == album)
                    .map(|(i, _)| i)
                    .collect();
            }
            _ => {}
        }
    }

    fn rebuild_artist_cache(&mut self) {
        match &self.artist_scope {
            LibraryScope::All => {
                self.scoped_albums = self.albums.clone();
            }
            LibraryScope::Artist(artist) => {
                let artist = artist.clone();
                self.scoped_albums = self
                    .albums
                    .iter()
                    .filter(|album| {
                        self.tracks
                            .iter()
                            .any(|t| &t.artist == &artist && &t.album == *album)
                    })
                    .cloned()
                    .collect();
            }
            _ => {}
        }
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
        let path = self.tracks[index].path.clone();
        self.player.play(&path)?;
        self.refresh_cover(index)?;
        Ok(())
    }
    pub fn play_selected(&mut self) -> Result<()> {
        match self.active_tab {
            LibraryTab::Songs => {
                if let Some(pos) = self.selected_index {
                    if let Some(&real_idx) = self.all_track_indices.get(pos) {
                        self.play_context = PlayContext::AllTracks;
                        self.play_index(real_idx)?;
                    }
                }
            }
            LibraryTab::Albums => match &self.album_scope {
                LibraryScope::All => {
                    if let Some(ai) = self.album_selected {
                        if let Some(album) = self.albums.get(ai).cloned() {
                            self.enter_album(album);
                        }
                    }
                }
                LibraryScope::Album(_) => {
                    if let Some(pos) = self.scoped_song_selected {
                        if let Some(&real_idx) = self.scoped_track_indices.get(pos) {
                            self.play_context = PlayContext::Album;
                            self.play_index(real_idx)?;
                        }
                    }
                }
                _ => {}
            },

            LibraryTab::Artists => match &self.artist_scope {
                LibraryScope::All => {
                    if let Some(ai) = self.artist_selected {
                        if let Some(artist) = self.artists.get(ai).cloned() {
                            self.enter_artist(artist);
                        }
                    }
                }
                LibraryScope::Artist(_) => {
                    if let Some(ai) = self.artist_scoped_album_selected {
                        if let Some(album) = self.scoped_albums.get(ai).cloned() {
                            self.active_tab = LibraryTab::Albums;
                            self.enter_album(album);
                        }
                    }
                }
                _ => {}
            },
            LibraryTab::Playlists => match &self.playlist_scope {
                PlaylistScope::All => {
                    if let Some(pi) = self.playlist_selected {
                        self.enter_playlist(pi);
                    }
                }
                PlaylistScope::Open(pi) => {
                    let pi = *pi;
                    let indices = self.playlist_track_indices(pi);
                    if let Some(si) = self.playlist_song_selected {
                        if let Some(&real_idx) = indices.get(si) {
                            self.play_context = PlayContext::Playlist(pi);
                            self.play_index(real_idx)?;
                        }
                    }
                }
            },
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
        let indices = self.current_play_indices();
        if indices.is_empty() {
            return Ok(());
        }
        let current_pos = self
            .current_index
            .and_then(|ci| indices.iter().position(|&i| i == ci))
            .unwrap_or(0);
        let next_pos = (current_pos + 1) % indices.len();
        self.play_index(indices[next_pos])
    }

    pub fn prev_track(&mut self) -> Result<()> {
        let indices = self.current_play_indices();
        if indices.is_empty() {
            return Ok(());
        }
        let current_pos = self
            .current_index
            .and_then(|ci| indices.iter().position(|&i| i == ci))
            .unwrap_or(0);
        let prev_pos = if current_pos == 0 {
            indices.len() - 1
        } else {
            current_pos - 1
        };
        self.play_index(indices[prev_pos])
    }

    /// Returns the index list for whatever context is currently playing
    fn current_play_indices(&self) -> Vec<usize> {
        match &self.play_context {
            PlayContext::Playlist(pi) => {
                let resolved = self.playlist_track_indices(*pi);
                if !resolved.is_empty() {
                    return resolved;
                }
                self.all_track_indices.clone()
            }
            PlayContext::Album => {
                if !self.scoped_track_indices.is_empty() {
                    return self.scoped_track_indices.clone();
                }
                self.all_track_indices.clone()
            }
            PlayContext::AllTracks => self.all_track_indices.clone(),
        }
    }

    /// Called every tick — auto-advance when a track finishes
    pub fn tick(&mut self) -> Result<()> {
        self.drain_metadata();

        if self.current_index.is_some() && self.player.is_finished() {
            self.next_track()?;
        }
        Ok(())
    }

    // ── Library navigation ───────────────────────────────────────────────────

    pub fn select_next(&mut self) {
        match self.active_tab {
            LibraryTab::Songs => {
                let indices = &self.all_track_indices; // always the full list
                let len = indices.len();
                if len == 0 {
                    return;
                }
                self.selected_index = Some(match self.selected_index {
                    Some(i) => (i + 1) % len,
                    None => 0,
                });
            }

            LibraryTab::Albums => match &self.album_scope {
                LibraryScope::All => cycle_next(&mut self.album_selected, self.albums.len()),
                LibraryScope::Album(_) => {
                    let len = self.scoped_track_indices.len();
                    if len == 0 {
                        return;
                    }
                    self.scoped_song_selected = Some(match self.scoped_song_selected {
                        Some(i) => (i + 1) % len,
                        None => 0,
                    });
                }
                _ => {}
            },

            LibraryTab::Artists => match &self.artist_scope {
                LibraryScope::All => cycle_next(&mut self.artist_selected, self.artists.len()),
                LibraryScope::Artist(_) => cycle_next(
                    &mut self.artist_scoped_album_selected,
                    self.scoped_albums.len(),
                ),
                _ => {}
            },
            LibraryTab::Playlists => match &self.playlist_scope {
                PlaylistScope::All => cycle_next(&mut self.playlist_selected, self.playlists.len()),
                PlaylistScope::Open(pi) => {
                    let len = self.playlists[*pi].track_paths.len();
                    cycle_next(&mut self.playlist_song_selected, len);
                }
            },
        }
    }

    pub fn select_prev(&mut self) {
        match self.active_tab {
            LibraryTab::Songs => {
                let len = self.all_track_indices.len();
                if len == 0 {
                    return;
                }
                self.selected_index = Some(match self.selected_index {
                    Some(i) => {
                        if i == 0 {
                            len - 1
                        } else {
                            i - 1
                        }
                    }
                    None => 0,
                });
            }
            LibraryTab::Albums => match &self.album_scope {
                LibraryScope::All => cycle_prev(&mut self.album_selected, self.albums.len()),
                LibraryScope::Album(_) => {
                    let len = self.scoped_track_indices.len();
                    if len == 0 {
                        return;
                    }
                    self.scoped_song_selected = Some(match self.scoped_song_selected {
                        Some(i) => {
                            if i == 0 {
                                len - 1
                            } else {
                                i - 1
                            }
                        }
                        None => 0,
                    });
                }
                _ => {}
            },

            LibraryTab::Artists => match &self.artist_scope {
                LibraryScope::All => cycle_prev(&mut self.artist_selected, self.artists.len()),
                LibraryScope::Artist(_) => cycle_prev(
                    &mut self.artist_scoped_album_selected,
                    self.scoped_albums.len(),
                ),
                _ => {}
            },
            LibraryTab::Playlists => match &self.playlist_scope {
                PlaylistScope::All => cycle_prev(&mut self.playlist_selected, self.playlists.len()),
                PlaylistScope::Open(pi) => {
                    let len = self.playlists[*pi].track_paths.len();
                    cycle_prev(&mut self.playlist_song_selected, len);
                }
            },
        }
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
        let cover_data = enrich_track(track)?;

        let img: Option<DynamicImage> = cover_data
            .as_deref()
            .and_then(|bytes| decode_cover(bytes).ok())
            .or_else(|| self.fallback_cover.clone());

        self.cover_image = img.map(|i| self.picker.new_resize_protocol(i));
        Ok(())
    }

    pub fn tab_next(&mut self) {
        let tabs = LibraryTab::all();
        let next = (self.active_tab.index() + 1) % tabs.len();
        self.active_tab = tabs[next].clone();
    }

    pub fn tab_prev(&mut self) {
        let tabs = LibraryTab::all();
        let current = self.active_tab.index();
        let prev = if current == 0 {
            tabs.len() - 1
        } else {
            current - 1
        };
        self.active_tab = tabs[prev].clone();
    }

    /// Song count for a given album
    pub fn album_track_count(&self, album: &str) -> usize {
        self.tracks.iter().filter(|t| t.album == album).count()
    }

    /// Enter album scope (from Albums tab)
    pub fn enter_album(&mut self, album: String) {
        self.album_scope = LibraryScope::Album(album);
        self.rebuild_album_cache();
        self.scoped_song_selected = if self.scoped_track_indices.is_empty() {
            None
        } else {
            Some(0)
        };
    }

    pub fn enter_artist(&mut self, artist: String) {
        self.artist_scope = LibraryScope::Artist(artist);
        self.rebuild_artist_cache();
        self.artist_scoped_album_selected = if self.scoped_albums.is_empty() {
            None
        } else {
            Some(0)
        };
    }

    pub fn exit_scope(&mut self) {
        match self.active_tab {
            LibraryTab::Albums => {
                self.album_scope = LibraryScope::All;
                self.rebuild_album_cache();
            }
            LibraryTab::Artists => {
                self.artist_scope = LibraryScope::All;
                self.rebuild_artist_cache();
            }
            LibraryTab::Playlists => {
                self.playlist_scope = PlaylistScope::All;
            }
            _ => {}
        }
    }

    /// Status line: "12 / 42 songs"
    pub fn scope_status(&self) -> String {
        let total = self.scoped_track_indices.len();
        let current = self
            .current_index
            .and_then(|ci| self.scoped_track_indices.iter().position(|&i| i == ci))
            .map(|p| p + 1)
            .unwrap_or(0);

        if current > 0 {
            format!("{} / {} songs", current, total)
        } else {
            format!("{} songs", total)
        }
    }
    pub fn add_to_favorites(&mut self) -> Result<()> {
        let track_path = match self.active_tab {
            LibraryTab::Songs => self
                .selected_index
                .and_then(|i| self.all_track_indices.get(i))
                .map(|&i| self.tracks[i].path.clone()),
            _ => self.current_track().map(|t| t.path.clone()),
        };
        let track_path = match track_path {
            Some(p) => p,
            None => return Ok(()),
        };

        // Favorites is always index 0
        if let Some(fav) = self.playlists.first_mut() {
            if fav.add_track(track_path) {
                fav.save()?;
            }
            // if add_track returns false it's already in favorites — silently ignore
        }
        Ok(())
    }
}

// Helper fns (outside impl block):
fn cycle_next(idx: &mut Option<usize>, len: usize) {
    if len == 0 {
        return;
    }
    *idx = Some(match *idx {
        Some(i) => (i + 1) % len,
        None => 0,
    });
}

fn cycle_prev(idx: &mut Option<usize>, len: usize) {
    if len == 0 {
        return;
    }
    *idx = Some(match *idx {
        Some(i) => {
            if i == 0 {
                len - 1
            } else {
                i - 1
            }
        }
        None => 0,
    });
}
