use crate::audio::metadata::enrich_track;
use crate::audio::player::Player;

// use crate::audio::player::PlayerCommand;
use crate::cover::extractor::{decode_cover, load_fallback};
use crate::library::playlist::Playlist;
use crate::library::scanner::scan_directory;
use crate::library::track::Track;
use crate::library::PlaylistScope;
use crate::library::{LibraryScope, LibraryTab};
use anyhow::Result;
use image::DynamicImage;
use std::collections::HashMap;
use std::path::Path;
// use std::sync::Arc;
use std::sync::mpsc::Receiver;
#[derive(Debug, Clone, PartialEq)]
pub enum RepeatMode {
    Off,
    All,
    One,
}

impl RepeatMode {
    pub fn next(&self) -> Self {
        match self {
            RepeatMode::Off => RepeatMode::All,
            RepeatMode::All => RepeatMode::One,
            RepeatMode::One => RepeatMode::Off,
        }
    }

    // pub fn icon(&self) -> &'static str {
    //     match self {
    //         RepeatMode::Off => "  ",
    //         RepeatMode::All => "🔁",
    //         RepeatMode::One => "🔂",
    //     }
    // }
}
#[derive(Debug, Clone, PartialEq)]
pub enum PlayContext {
    AllTracks,
    Album,
    Playlist(usize),
}

#[derive(Debug, Clone)]
pub enum SearchResult {
    Track(usize),
    Album(String),
    Artist(String),
    PlaylistTrack {
        playlist_idx: usize,
        track_idx: usize,
    },
}
#[derive(Debug, Clone, PartialEq)]
pub enum ConfirmAction {
    DeletePlaylist(usize),                // playlist index
    DeleteSongFromPlaylist(usize, usize), // playlist idx, song idx
}

pub struct ConfirmDialog {
    pub message: String,
    pub action: ConfirmAction,
}
pub struct App {
    pub play_context: PlayContext,
    pub playlist_scope: PlaylistScope,
    pub playlist_song_selected: Option<usize>,
    pub playlists: Vec<Playlist>,
    pub save_mode: bool,
    pub save_candidates: Vec<usize>,
    pub save_selected: usize,
    pub artist_album_counts: HashMap<String, usize>,
    pub tracks: Vec<Track>,
    pub player: Player,
    pub current_index: Option<usize>,
    pub selected_index: Option<usize>,
    pub scoped_song_selected: Option<usize>,
    pub cover_image: Option<ratatui_image::protocol::StatefulProtocol>,
    pub picker: ratatui_image::picker::Picker,
    metadata_rx: Receiver<(usize, Track, Option<Vec<u8>>)>,
    pub loading: bool,
    pub loaded_count: usize,
    fallback_cover: Option<DynamicImage>,
    pub active_tab: LibraryTab,
    pub albums: Vec<String>,
    pub artists: Vec<String>,
    pub album_selected: Option<usize>,
    pub artist_scoped_album_selected: Option<usize>,
    pub artist_selected: Option<usize>,
    pub playlist_selected: Option<usize>,
    pub album_scope: LibraryScope,
    pub artist_scope: LibraryScope,
    pub all_track_indices: Vec<usize>,
    pub scoped_track_indices: Vec<usize>,
    pub scoped_albums: Vec<String>,
    pub shuffle: bool,
    pub shuffle_order: Vec<usize>,
    pub search_mode: bool,
    pub search_query: String,
    pub search_results: Vec<SearchResult>,
    pub search_selected: Option<usize>,
    pub save_offset: usize,
    // ── Scroll offsets (one per list) ────────────────────────────────────────
    pub songs_offset: usize,
    pub albums_offset: usize,
    pub artists_offset: usize,
    pub scoped_songs_offset: usize,
    pub artist_albums_offset: usize,
    pub playlist_list_offset: usize,
    pub playlist_songs_offset: usize,
    pub search_offset: usize,
    pub list_height: usize, // updated each frame from layout
    pub scoped_list_height: usize,
    pub search_list_height: usize,
    pub keys: crate::config::KeysConfig,
    pub ipc_rx: Option<std::sync::mpsc::Receiver<crate::ipc::IpcCommand>>,
    pub repeat: RepeatMode,
    pub mpris_rx: Option<std::sync::mpsc::Receiver<crate::mpris::MprisCommand>>,
    pub mpris_update_tx: Option<std::sync::mpsc::SyncSender<crate::mpris::MprisUpdate>>,
    pub dynamic_theme: bool,
    pub dynamic_theme_style: String,
    pub layout_mode: String,
    pub show_cover: bool,
    pub show_player: bool,
    // pub show_lyrics: bool,
    pub cover_width: u16,
    pub cover_height: u16,
    pub player_height: u16,
    pub confirm_dialog: Option<ConfirmDialog>,
    // New playlist creation state
    pub new_playlist_mode: bool,
    pub new_playlist_name: String,
    pub music_dir: std::path::PathBuf,
    pub lyrics: Option<crate::lyrics::Lyrics>,
    pub lyrics_loading: bool,
    pub lyrics_error: bool,
    lyrics_rx: Option<std::sync::mpsc::Receiver<Option<crate::lyrics::Lyrics>>>,
    pub needs_full_redraw: bool,
}

impl App {
    pub fn new(
        music_dir: &Path,
        metadata_rx: Receiver<(usize, Track, Option<Vec<u8>>)>,
        config: &crate::config::Config,
        // cmd_rx: Receiver<PlayerCommand>,
    ) -> Result<Self> {
        // let mut player = Player::new()?;

        // let favorites_path = music_dir.join("favorites.m3u");
        // let mut playlists: Vec<Playlist> = walkdir::WalkDir::new(music_dir)
        //     .max_depth(2)
        //     .into_iter()
        //     .filter_map(|e| e.ok())
        //     .filter(|e| e.path().extension().map(|x| x == "m3u").unwrap_or(false))
        //     .filter_map(|e| Playlist::load(e.path()).ok())
        //     .collect();

        // if !playlists
        //     .iter()
        //     .any(|p| p.name.to_lowercase() == "favorites")
        // {
        //     let fav = Playlist::new("Favorites".to_string(), Some(favorites_path));
        //     let _ = fav.save();
        //     playlists.insert(0, fav);
        // } else {
        //     if let Some(i) = playlists
        //         .iter()
        //         .position(|p| p.name.to_lowercase() == "favorites")
        //     {
        //         let fav = playlists.remove(i);
        //         playlists.insert(0, fav);
        //     }
        // }
        let mut player = Player::new()?;

        // Define the authoritative path for the single favorites playlist
        let favorites_path = music_dir.join("favorites.m3u");

        let mut playlists: Vec<Playlist> = walkdir::WalkDir::new(&music_dir)
            .max_depth(2)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                let path = e.path();
                let is_m3u = path.extension().map(|x| x == "m3u").unwrap_or(false);

                // Ignore any file named "favorites.m3u" regardless of where it is in the walk
                // This prevents duplicates from being picked up in the main scan
                let is_favorites = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_lowercase() == "favorites.m3u")
                    .unwrap_or(false);

                is_m3u && !is_favorites
            })
            .filter_map(|e| Playlist::load(e.path()).ok())
            .collect();

        // Now handle the single, top-level favorites playlist exclusively
        if favorites_path.exists() {
            if let Ok(fav) = Playlist::load(&favorites_path) {
                playlists.insert(0, fav);
            }
        } else {
            // Create it if it doesn't exist
            let fav = Playlist::new("Favorites".to_string(), Some(favorites_path));
            let _ = fav.save();
            playlists.insert(0, fav);
        }
        let tracks = scan_directory(music_dir)?;
        let selected_index = if tracks.is_empty() { None } else { Some(0) };
        let fallback_cover = load_fallback(Path::new("assets/default_cover.png")).ok();
        let picker = ratatui_image::picker::Picker::from_query_stdio()
            .unwrap_or_else(|_| ratatui_image::picker::Picker::halfblocks());

        let all_track_indices = (0..tracks.len()).collect();
        let scoped_track_indices = (0..tracks.len()).collect();

        // Apply startup volume
        let volume = config.startup.volume.unwrap_or(1.0).clamp(0.0, 2.0);
        player.set_volume(volume);
        Ok(App {
            // active_tab,
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
            active_tab: LibraryTab::Songs,
            album_selected: None,
            artist_selected: None,
            playlist_selected: None,
            album_scope: LibraryScope::All,
            artist_scope: LibraryScope::All,
            scoped_song_selected: None,
            artist_scoped_album_selected: None,
            playlist_scope: PlaylistScope::All,
            playlist_song_selected: None,
            save_mode: false,
            save_candidates: vec![],
            save_selected: 0,
            playlists,
            play_context: PlayContext::AllTracks,
            shuffle: false,
            shuffle_order: vec![],
            search_mode: false,
            search_query: String::new(),
            search_results: vec![],
            search_selected: None,
            save_offset: 0,
            // scroll offsets
            songs_offset: 0,
            albums_offset: 0,
            artists_offset: 0,
            scoped_songs_offset: 0,
            artist_albums_offset: 0,
            playlist_list_offset: 0,
            playlist_songs_offset: 0,
            search_offset: 0,
            list_height: 20,
            scoped_list_height: 19,
            search_list_height: 10,
            keys: config.keys.clone(),
            ipc_rx: None,
            repeat: RepeatMode::Off,
            mpris_rx: None,
            mpris_update_tx: None,
            dynamic_theme: config.colors.dynamic_theme,
            dynamic_theme_style: config.colors.dynamic_theme_style.clone(),
            layout_mode: config.layout.mode.clone(),
            show_cover: config.layout.show_cover,
            show_player: config.layout.show_player,
            cover_width: config.layout.cover_width,
            cover_height: config.layout.cover_height,
            player_height: config.layout.player_height,
            confirm_dialog: None,
            new_playlist_mode: false,
            new_playlist_name: String::new(),
            music_dir: music_dir.to_path_buf(),
            lyrics: None,
            lyrics_loading: false,
            lyrics_error: false,
            lyrics_rx: None,
            needs_full_redraw: false,
            // show_lyrics: config.layout.show_lyrics,
        })
    }


   pub fn drain_mpris(&mut self) -> Result<()> {
    use crate::mpris::MprisCommand;

    let commands: Vec<MprisCommand> = match &self.mpris_rx {
        Some(rx) => {
            let mut cmds = Vec::new();
            while let Ok(cmd) = rx.try_recv() {
                cmds.push(cmd);
            }
            cmds
        }
        None => return Ok(()),
    };

    for cmd in commands {
        match cmd {
            MprisCommand::Play => {
                if self.player.is_paused() { self.player.resume(); }
            }
            MprisCommand::Pause => {
                if !self.player.is_paused() { self.player.pause(); }
            }
            MprisCommand::PlayPause => self.toggle_play()?,
            MprisCommand::Next      => self.next_track()?,
            MprisCommand::Prev      => self.prev_track()?,
            MprisCommand::Stop      => self.player.stop(),

            // Relative seek — offset in microseconds (can be negative)
            MprisCommand::Seek(offset_us) => {
                let current = self.player.elapsed();
                let offset  = std::time::Duration::from_micros(offset_us.unsigned_abs());
                let new_pos = if offset_us >= 0 {
                    current + offset
                } else {
                    current.saturating_sub(offset)
                };
                if let Some(track) = self.current_track() {
                    if new_pos <= track.duration {
                        let path = track.path.clone();
                        self.player.seek(&path, new_pos)?;
                    }
                }
            }

            // Absolute position — position in microseconds from start
            MprisCommand::SetPosition(pos_us) => {
                let new_pos = std::time::Duration::from_micros(pos_us as u64);
                if let Some(track) = self.current_track() {
                    if new_pos <= track.duration {
                        let path = track.path.clone();
                        self.player.seek(&path, new_pos)?;
                    }
                }
            }

            MprisCommand::SetVolume(v) => self.set_volume(v as f32),
        }
    }

    self.update_mpris_state();
    Ok(())
}

    fn update_mpris_state(&self) {
        use crate::mpris::MprisUpdate;

        let tx = match &self.mpris_update_tx {
            Some(t) => t,
            None => return,
        };

        let cover_url = self.current_index.and_then(|idx| {
            // Re-read cover from tags and save to temp file
            let track: &Track = &self.tracks[idx];
            extract_cover_to_file(&track.path)
        });

        let upd = if let Some(track) = self.current_track() {
            MprisUpdate {
                title: track.title.clone(),
                artist: track.artist.clone(),
                album: track.album.clone(),
                duration_us: track.duration.as_micros() as i64,
                position_us: self.player.elapsed().as_micros() as i64,
                playing: !self.player.is_paused(),
                volume: self.player.volume as f64,
                shuffle: self.shuffle,
                cover_url,
            }
        } else {
            MprisUpdate::default()
        };

        let _ = tx.try_send(upd);
    }

    pub fn set_volume(&mut self, vol: f32) {
        self.player.set_volume(vol);
    }

    pub fn toggle_repeat(&mut self) {
        self.repeat = self.repeat.next();
    }

    pub fn restore_session_from_state(&mut self, state: &crate::config::State) -> Result<()> {
        if let Some(vol) = state.volume {
            self.player.set_volume(vol);
        }
        // 1. Restore the active tab
        if let Some(tab) = &state.last_tab {
            self.active_tab = match tab.as_str() {
                "Albums" => LibraryTab::Albums,
                "Artists" => LibraryTab::Artists,
                "Playlists" => LibraryTab::Playlists,
                _ => LibraryTab::Songs,
            };
        }

        // 2. Restore last track, position, and UI offsets
        if let Some(last_path) = &state.last_track {
            // Find if the last track still exists in our scanned library
            if let Some(idx) = self.tracks.iter().position(|t| &t.path == last_path) {
                let position = state.last_position.unwrap_or(0.0);

                // Sync UI state to the restored track
                self.current_index = Some(idx);
                self.selected_index = Some(idx);

                // Note: Ensure scroll_offset and self.list_height are accessible
                self.songs_offset = scroll_offset(Some(idx), 0, self.list_height);

                // 3. Setup the player
                let path = self.tracks[idx].path.clone();
                self.player
                    .seek(&path, std::time::Duration::from_secs_f64(position))?;

                // We usually start paused to avoid surprising the user with noise
                self.player.pause();

                // Load the album art for the restored track
                self.refresh_cover(idx)?;
            }
        }

        Ok(())
    }

    pub fn playlist_track_indices(&self, playlist_idx: usize) -> Vec<usize> {
        let pl = &self.playlists[playlist_idx];
        pl.track_paths
            .iter()
            .filter_map(|path| self.tracks.iter().position(|t| &t.path == path))
            .collect()
    }

    pub fn enter_playlist(&mut self, idx: usize) {
        self.playlist_scope = PlaylistScope::Open(idx);
        let resolved = self.playlist_track_indices(idx);
        self.playlist_song_selected = if resolved.is_empty() { None } else { Some(0) };
        self.playlist_songs_offset = 0;
    }

    // ── Playlist management ───────────────────────────────────────────────────────

    pub fn start_new_playlist(&mut self) {
        if self.active_tab == LibraryTab::Playlists {
            self.new_playlist_mode = true;
            self.new_playlist_name.clear();
        }
    }

    pub fn new_playlist_type_char(&mut self, c: char) {
        self.new_playlist_name.push(c);
    }

    pub fn new_playlist_backspace(&mut self) {
        self.new_playlist_name.pop();
    }

    pub fn new_playlist_confirm(&mut self) -> Result<()> {
        let name = self.new_playlist_name.trim().to_string();
        if name.is_empty() {
            self.new_playlist_mode = false;
            return Ok(());
        }

        // Check for duplicate name
        if self
            .playlists
            .iter()
            .any(|p| p.name.to_lowercase() == name.to_lowercase())
        {
            self.new_playlist_mode = false;
            return Ok(());
        }

        let path = self.music_dir.join(format!("{}.m3u", name));
        let playlist = Playlist::new(name, Some(path));
        let _ = playlist.save();
        self.playlists.push(playlist);
        self.playlist_selected = Some(self.playlists.len() - 1);
        self.new_playlist_mode = false;
        self.new_playlist_name.clear();
        Ok(())
    }

    pub fn new_playlist_cancel(&mut self) {
        self.new_playlist_mode = false;
        self.new_playlist_name.clear();
    }

    pub fn request_delete_playlist(&mut self) {
        if self.active_tab != LibraryTab::Playlists {
            return;
        }
        match &self.playlist_scope {
            PlaylistScope::All => {
                if let Some(pi) = self.playlist_selected {
                    // Protect favorites
                    if self.playlists[pi].name.to_lowercase() == "favorites" {
                        return;
                    }
                    self.confirm_dialog = Some(ConfirmDialog {
                        message: format!(
                            "Delete playlist \"{}\"? This cannot be undone.",
                            self.playlists[pi].name
                        ),
                        action: ConfirmAction::DeletePlaylist(pi),
                    });
                }
            }
            PlaylistScope::Open(pi) => {
                if let Some(si) = self.playlist_song_selected {
                    let pi = *pi;
                    let track_name = self
                        .playlist_track_indices(pi)
                        .get(si)
                        .and_then(|&idx| self.tracks.get(idx))
                        .map(|t| t.title.clone())
                        .unwrap_or_else(|| "this song".to_string());
                    self.confirm_dialog = Some(ConfirmDialog {
                        message: format!("Remove \"{}\" from playlist?", track_name),
                        action: ConfirmAction::DeleteSongFromPlaylist(pi, si),
                    });
                }
            }
        }
    }

    pub fn confirm_dialog_confirm(&mut self) -> Result<()> {
        let action = match self.confirm_dialog.take() {
            Some(d) => d.action,
            None => return Ok(()),
        };

        match action {
            ConfirmAction::DeletePlaylist(pi) => {
                // Delete the m3u file
                if let Some(path) = &self.playlists[pi].path {
                    let _ = std::fs::remove_file(path);
                }
                self.playlists.remove(pi);
                // Fix cursor
                let len = self.playlists.len();
                self.playlist_selected = if len == 0 {
                    None
                } else {
                    Some(pi.min(len - 1))
                };
            }
            ConfirmAction::DeleteSongFromPlaylist(pi, si) => {
                self.playlists[pi].remove_track(si);
                self.playlists[pi].save()?;
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

    pub fn confirm_dialog_cancel(&mut self) {
        self.confirm_dialog = None;
    }

    // ── Layout cycling ────────────────────────────────────────────────────────────
    pub fn set_layout(&mut self, idx: usize) {
        let layouts = ["default", "clean", "compact", "minimal"];

        // 1. Check if we currently have lyrics enabled
        let lyrics_on = self.layout_mode.ends_with("_lyrics");

        if let Some(&new_base) = layouts.get(idx) {
            // 2. If lyrics were on, append the suffix to the new base layout
            if lyrics_on {
                self.layout_mode = format!("{}_lyrics", new_base);
            } else {
                self.layout_mode = new_base.to_string();
            }
        }
    }

    pub fn toggle_lyrics(&mut self) {
        if self.layout_mode.ends_with("_lyrics") {
            // Remove the suffix
            self.layout_mode = self.layout_mode.replace("_lyrics", "");
        } else {
            // Append the suffix
            self.layout_mode.push_str("_lyrics");
        }
    }

    pub fn open_save_mode(&mut self) {
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

        self.save_candidates = self
            .playlists
            .iter()
            .enumerate()
            .filter(|(_, pl)| !pl.track_paths.contains(&track_path))
            .map(|(i, _)| i)
            .collect();

        if self.save_candidates.is_empty() {
            return;
        }
        self.save_mode = true;
        self.save_selected = 0;
        self.save_offset = 0;
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

    pub fn drain_ipc(&mut self) -> anyhow::Result<()> {
        use crate::ipc::IpcCommand;

        // Collect all pending commands first — releases the borrow on ipc_rx
        let commands: Vec<IpcCommand> = match &self.ipc_rx {
            Some(rx) => {
                let mut cmds = Vec::new();
                loop {
                    match rx.try_recv() {
                        Ok(cmd) => cmds.push(cmd),
                        Err(_) => break,
                    }
                }
                cmds
            }
            None => return Ok(()),
        };

        // Now process — no borrow conflict
        for cmd in commands {
            match cmd {
                IpcCommand::Play => {
                    if self.player.is_paused() {
                        self.player.resume();
                    }
                }
                IpcCommand::Pause => {
                    if !self.player.is_paused() {
                        self.player.pause();
                    }
                }
                IpcCommand::TogglePlay => self.toggle_play()?,
                IpcCommand::Next => self.next_track()?,
                IpcCommand::Prev => self.prev_track()?,
                IpcCommand::VolumeUp => self.volume_up(),
                IpcCommand::VolumeDown => self.volume_down(),
                IpcCommand::Status => {
                    let info = self.current_track().map(|t| {
                        (
                            t.title.clone(),
                            t.artist.clone(),
                            t.album.clone(),
                            t.duration_str(),
                            t.bitrate,
                            t.sample_rate,
                            t.extension(),
                        )
                    });
                    if let Some((title, artist, album, duration, bitrate, sample_rate, ext)) = info
                    {
                        let elapsed = self.player.elapsed();
                        let elapsed_str = format!(
                            "{:02}:{:02}",
                            elapsed.as_secs() / 60,
                            elapsed.as_secs() % 60
                        );
                        let paused = self.player.is_paused();
                        let vol = (self.player.volume * 100.0) as u32;

                        let json = format!(
                            r#"{{
  "title": "{}",
  "artist": "{}",
  "album": "{}",
  "duration": "{}",
  "elapsed": "{}",
  "elapsed_secs": {},
  "status": "{}",
  "volume": {},
  "bitrate": {},
  "sample_rate": {},
  "format": "{}",
  "shuffle": {}
}}"#,
                            title.replace('"', "\\\""),
                            artist.replace('"', "\\\""),
                            album.replace('"', "\\\""),
                            duration,
                            elapsed_str,
                            elapsed.as_secs(),
                            if paused { "paused" } else { "playing" },
                            vol,
                            bitrate.map(|b| b.to_string()).unwrap_or("null".into()),
                            sample_rate.map(|s| s.to_string()).unwrap_or("null".into()),
                            ext,
                            self.shuffle,
                        );

                        let _ = std::fs::write("/tmp/aghani-status.json", &json);

                        // Also write a simple one-liner for i3blocks
                        let oneliner = format!(
                            "{} {} — {} [{}] {}",
                            if paused { "⏸" } else { "▶" },
                            title,
                            artist,
                            elapsed_str,
                            if self.shuffle { "🔀" } else { "" }
                        );
                        let _ = std::fs::write("/tmp/aghani-status", oneliner);
                    } else {
                        let _ =
                            std::fs::write("/tmp/aghani-status.json", r#"{"status":"stopped"}"#);
                        let _ = std::fs::write("/tmp/aghani-status", "⏹ Not playing");
                    }
                }
            }
        }
        Ok(())
    }

    pub fn drain_metadata(&mut self) {
        let mut changed = false;
        let mut count = 0;

        while count < 50 {
            match self.metadata_rx.try_recv() {
                Ok((i, track, _)) => {
                    self.tracks[i].title = track.title;
                    self.tracks[i].artist = track.artist;
                    self.tracks[i].album = track.album;
                    self.tracks[i].duration = track.duration;
                    self.tracks[i].has_cover = track.has_cover;
                    self.tracks[i].bitrate = track.bitrate;
                    self.tracks[i].sample_rate = track.sample_rate;
                    self.tracks[i].channels = track.channels;
                    self.loaded_count += 1;
                    changed = true;
                    count += 1;
                }
                Err(_) => break,
            }
        }

        if self.loaded_count >= self.tracks.len() {
            self.loading = false;
        }

        if changed && (!self.loading || self.loaded_count % 100 == 0) {
            self.rebuild_albums_and_artists();
        }
    }

    fn rebuild_albums_and_artists(&mut self) {
        let mut seen_albums = std::collections::HashSet::new();
        let mut seen_artists = std::collections::HashSet::new();
        let mut albums = Vec::new();
        let mut artists = Vec::new();

        for t in &self.tracks {
            if seen_albums.insert(t.album.clone()) {
                albums.push(t.album.clone());
            }
            if seen_artists.insert(t.artist.clone()) {
                artists.push(t.artist.clone());
            }
        }

        albums.sort_unstable();
        artists.sort_unstable();

        if albums == self.albums && artists == self.artists {
            return;
        }

        let mut artist_albums: HashMap<String, std::collections::HashSet<String>> = HashMap::new();
        for t in &self.tracks {
            artist_albums
                .entry(t.artist.clone())
                .or_default()
                .insert(t.album.clone());
        }
        self.artist_album_counts = artist_albums
            .into_iter()
            .map(|(a, albums)| (a, albums.len()))
            .collect();

        self.albums = albums;
        self.artists = artists;
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

    pub fn play_index(&mut self, index: usize) -> Result<()> {
        if index >= self.tracks.len() {
            return Ok(());
        }
        self.current_index = Some(index);
        let path = self.tracks[index].path.clone();
        let gain = self.tracks[index].replaygain;
        self.player.play_with_gain(&path, gain)?;
        self.refresh_cover(index)?;
        self.update_mpris_state();
        self.fetch_lyrics(index);
        Ok(())
    }

    fn fetch_lyrics(&mut self, index: usize) {
        let track = &self.tracks[index];
        let title = track.title.clone();
        let artist = track.artist.clone();
        let album = track.album.clone();
        let duration = track.duration.as_secs_f64();
        let path = track.path.clone(); // ← add this

        self.lyrics = None;
        self.lyrics_loading = true;
        self.lyrics_error = false;

        let (tx, rx) = std::sync::mpsc::channel();
        self.lyrics_rx = Some(rx);

        std::thread::spawn(move || {
            let result = crate::lyrics::fetch_lyrics(
                &title, &artist, &album, duration, &path, // ← pass path
            );
            let _ = tx.send(result);
        });
    }

    pub fn play_selected(&mut self) -> Result<()> {
        match self.active_tab {
            LibraryTab::Songs => {
                if let Some(pos) = self.selected_index {
                    if let Some(&real_idx) = self.all_track_indices.get(pos) {
                        self.play_context = PlayContext::AllTracks;
                        self.play_index(real_idx)?;
                        if self.shuffle {
                            self.rebuild_shuffle();
                        }
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
                            if self.shuffle {
                                self.rebuild_shuffle();
                            }
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
                            if self.shuffle {
                                self.rebuild_shuffle();
                            }
                        }
                    }
                }
            },
        }
        Ok(())
    }

    pub fn toggle_play(&mut self) -> Result<()> {
        if self.current_index.is_none() {
            let idx = self.selected_index.unwrap_or(0);
            self.play_index(idx)?;
        } else {
            self.player.toggle_pause();
        }
        Ok(())
    }

    fn is_selection_in_sync(&self) -> bool {
        // If nothing is playing, always allow
        let current = match self.current_index {
            Some(c) => c,
            None => return true,
        };

        // Get what's actually selected in current tab
        let selected_real: Option<usize> = match self.active_tab {
            LibraryTab::Songs => self
                .selected_index
                .and_then(|si| self.all_track_indices.get(si).copied()),

            LibraryTab::Albums => match &self.album_scope {
                LibraryScope::Album(_) => self
                    .scoped_song_selected
                    .and_then(|si| self.scoped_track_indices.get(si).copied()),
                _ => return true,
            },

            LibraryTab::Playlists => match &self.playlist_scope {
                PlaylistScope::Open(pi) => {
                    let indices = self.playlist_track_indices(*pi);
                    self.playlist_song_selected
                        .and_then(|si| indices.get(si).copied())
                }
                _ => return true,
            },

            LibraryTab::Artists => return true,
        };

        match selected_real {
            None => true,            // nothing selected → allow
            Some(s) => s == current, // selected matches playing → allow, else block
        }
    }

    pub fn next_track(&mut self) -> Result<()> {
        let indices = self.current_play_indices();
        if indices.is_empty() {
            return Ok(());
        }

        let current_pos = match self
            .current_index
            .and_then(|ci| indices.iter().position(|&i| i == ci))
        {
            Some(pos) => pos,
            None => return Ok(()),
        };

        let was_in_sync = self.is_selection_in_sync();
        let next_pos = (current_pos + 1) % indices.len();
        self.play_index(indices[next_pos])?;

        if was_in_sync {
            self.sync_cursor_to_track(indices[next_pos]);
        }
        Ok(())
    }

    pub fn prev_track(&mut self) -> Result<()> {
        let indices = self.current_play_indices();
        if indices.is_empty() {
            return Ok(());
        }

        let current_pos = match self
            .current_index
            .and_then(|ci| indices.iter().position(|&i| i == ci))
        {
            Some(pos) => pos,
            None => return Ok(()),
        };

        let was_in_sync = self.is_selection_in_sync();
        let prev_pos = if current_pos == 0 {
            indices.len() - 1
        } else {
            current_pos - 1
        };
        self.play_index(indices[prev_pos])?;

        if was_in_sync {
            self.sync_cursor_to_track(indices[prev_pos]);
        }
        Ok(())
    }

    fn sync_cursor_to_track(&mut self, real_idx: usize) {
        match self.active_tab {
            LibraryTab::Songs => {
                self.selected_index = self.all_track_indices.iter().position(|&i| i == real_idx);
                self.songs_offset = if self.shuffle {
                    center_offset(self.selected_index, self.list_height)
                } else {
                    scroll_offset(self.selected_index, self.songs_offset, self.list_height)
                };
            }
            LibraryTab::Albums => {
                if let LibraryScope::Album(_) = &self.album_scope {
                    self.scoped_song_selected = self
                        .scoped_track_indices
                        .iter()
                        .position(|&i| i == real_idx);
                    self.scoped_songs_offset = if self.shuffle {
                        center_offset(self.scoped_song_selected, self.scoped_list_height)
                    } else {
                        scroll_offset(
                            self.scoped_song_selected,
                            self.scoped_songs_offset,
                            self.scoped_list_height,
                        )
                    };
                }
            }
            LibraryTab::Playlists => {
                if let PlaylistScope::Open(pi) = &self.playlist_scope {
                    let indices = self.playlist_track_indices(*pi);
                    self.playlist_song_selected = indices.iter().position(|&i| i == real_idx);
                    self.playlist_songs_offset = if self.shuffle {
                        center_offset(self.playlist_song_selected, self.list_height)
                    } else {
                        scroll_offset(
                            self.playlist_song_selected,
                            self.playlist_songs_offset,
                            self.list_height,
                        )
                    };
                }
            }
            _ => {}
        }
    }

    pub fn tick(&mut self) -> Result<()> {
        self.drain_metadata();
        self.drain_ipc()?;
        self.drain_mpris()?; // ← add this
        self.update_mpris_state(); // keep position in sync
                                   // Check if lyrics arrived
        if let Some(ref rx) = self.lyrics_rx {
            if let Ok(result) = rx.try_recv() {
                self.lyrics_loading = false;
                self.lyrics_error = result.is_none();
                self.lyrics = result;
                self.lyrics_rx = None;
            }
        }
        if self.current_index.is_some() && self.player.is_finished() {
            match self.repeat {
                RepeatMode::One => {
                    // Replay same track
                    if let Some(idx) = self.current_index {
                        let path = self.tracks[idx].path.clone();
                        self.player.play(&path)?;
                    }
                }
                RepeatMode::Off => {
                    // Stop at end of list
                    let indices = self.current_play_indices();
                    if let Some(ci) = self.current_index {
                        let pos = indices.iter().position(|&i| i == ci).unwrap_or(0);
                        if pos + 1 < indices.len() {
                            self.next_track()?;
                        } else {
                            self.next_track()?;
                            self.player.pause();
                        }
                    }
                }
                RepeatMode::All => {
                    self.next_track()?; // already wraps around
                }
            }
        }
        Ok(())
    }

    pub fn select_next(&mut self) {
        match self.active_tab {
            LibraryTab::Songs => {
                let len = self.all_track_indices.len();
                if len == 0 {
                    return;
                }
                self.selected_index = Some(match self.selected_index {
                    Some(i) => (i + 1) % len,
                    None => 0,
                });
                self.songs_offset =
                    scroll_offset(self.selected_index, self.songs_offset, self.list_height);
            }
            LibraryTab::Albums => match &self.album_scope {
                LibraryScope::All => {
                    cycle_next(&mut self.album_selected, self.albums.len());
                    self.albums_offset =
                        scroll_offset(self.album_selected, self.albums_offset, self.list_height);
                }
                LibraryScope::Album(_) => {
                    let len = self.scoped_track_indices.len();
                    if len == 0 {
                        return;
                    }
                    self.scoped_song_selected = Some(match self.scoped_song_selected {
                        Some(i) => (i + 1) % len,
                        None => 0,
                    });
                    self.scoped_songs_offset = scroll_offset(
                        self.scoped_song_selected,
                        self.scoped_songs_offset,
                        self.scoped_list_height,
                    );
                }
                _ => {}
            },
            LibraryTab::Artists => match &self.artist_scope {
                LibraryScope::All => {
                    cycle_next(&mut self.artist_selected, self.artists.len());
                    self.artists_offset =
                        scroll_offset(self.artist_selected, self.artists_offset, self.list_height);
                }
                LibraryScope::Artist(_) => {
                    cycle_next(
                        &mut self.artist_scoped_album_selected,
                        self.scoped_albums.len(),
                    );
                    self.artist_albums_offset = scroll_offset(
                        self.artist_scoped_album_selected,
                        self.artist_albums_offset,
                        self.list_height,
                    );
                }
                _ => {}
            },
            LibraryTab::Playlists => match &self.playlist_scope {
                PlaylistScope::All => {
                    cycle_next(&mut self.playlist_selected, self.playlists.len());
                    self.playlist_list_offset = scroll_offset(
                        self.playlist_selected,
                        self.playlist_list_offset,
                        self.list_height,
                    );
                }
                PlaylistScope::Open(pi) => {
                    let len = self.playlists[*pi].track_paths.len();
                    cycle_next(&mut self.playlist_song_selected, len);
                    self.playlist_songs_offset = scroll_offset(
                        self.playlist_song_selected,
                        self.playlist_songs_offset,
                        self.scoped_list_height,
                    );
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
                self.songs_offset =
                    scroll_offset(self.selected_index, self.songs_offset, self.list_height);
            }
            LibraryTab::Albums => match &self.album_scope {
                LibraryScope::All => {
                    cycle_prev(&mut self.album_selected, self.albums.len());
                    self.albums_offset =
                        scroll_offset(self.album_selected, self.albums_offset, self.list_height);
                }
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
                    self.scoped_songs_offset = scroll_offset(
                        self.scoped_song_selected,
                        self.scoped_songs_offset,
                        self.scoped_list_height,
                    );
                }
                _ => {}
            },
            LibraryTab::Artists => match &self.artist_scope {
                LibraryScope::All => {
                    cycle_prev(&mut self.artist_selected, self.artists.len());
                    self.artists_offset =
                        scroll_offset(self.artist_selected, self.artists_offset, self.list_height);
                }
                LibraryScope::Artist(_) => {
                    cycle_prev(
                        &mut self.artist_scoped_album_selected,
                        self.scoped_albums.len(),
                    );
                    self.artist_albums_offset = scroll_offset(
                        self.artist_scoped_album_selected,
                        self.artist_albums_offset,
                        self.list_height,
                    );
                }
                _ => {}
            },
            LibraryTab::Playlists => match &self.playlist_scope {
                PlaylistScope::All => {
                    cycle_prev(&mut self.playlist_selected, self.playlists.len());
                    self.playlist_list_offset = scroll_offset(
                        self.playlist_selected,
                        self.playlist_list_offset,
                        self.list_height,
                    );
                }
                PlaylistScope::Open(pi) => {
                    let len = self.playlists[*pi].track_paths.len();
                    cycle_prev(&mut self.playlist_song_selected, len);
                    self.playlist_songs_offset = scroll_offset(
                        self.playlist_song_selected,
                        self.playlist_songs_offset,
                        self.scoped_list_height,
                    );
                }
            },
        }
    }

    pub fn volume_up(&mut self) {
        self.player.volume_up();
    }

    pub fn volume_down(&mut self) {
        self.player.volume_down();
    }

    fn refresh_cover(&mut self, index: usize) -> Result<()> {
        let track = &mut self.tracks[index];
        let cover_data = enrich_track(track)?;

        let img: Option<DynamicImage> = cover_data
            .as_deref()
            .and_then(|bytes| decode_cover(bytes).ok())
            .or_else(|| self.fallback_cover.clone());

        if let Some(ref i) = img {
            // Extract dynamic colors if enabled
            if self.dynamic_theme {
                let dc = crate::ui::dynamic_theme::extract_colors(i, &self.dynamic_theme_style);
                crate::ui::theme::set_dynamic_colors(dc);
            }
        } else {
            crate::ui::theme::clear_dynamic_colors();
        }

        self.cover_image = img.map(|i| self.picker.new_resize_protocol(i));
        Ok(())
    }

    pub fn tab_next(&mut self) {
        let tabs = LibraryTab::all();
        let next = (self.active_tab.index() + 1) % tabs.len();
        self.active_tab = tabs[next].clone();
        self.ensure_selection();
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
        self.ensure_selection();
    }

    fn ensure_selection(&mut self) {
        match self.active_tab {
            LibraryTab::Songs => {
                if self.selected_index.is_none() && !self.all_track_indices.is_empty() {
                    self.selected_index = Some(0);
                }
            }
            LibraryTab::Albums => {
                if self.album_selected.is_none() && !self.albums.is_empty() {
                    self.album_selected = Some(0);
                }
            }
            LibraryTab::Artists => {
                if self.artist_selected.is_none() && !self.artists.is_empty() {
                    self.artist_selected = Some(0);
                }
            }
            LibraryTab::Playlists => {
                if self.playlist_selected.is_none() && !self.playlists.is_empty() {
                    self.playlist_selected = Some(0);
                }
            }
        }
    }

    pub fn album_track_count(&self, album: &str) -> usize {
        self.tracks.iter().filter(|t| t.album == album).count()
    }

    pub fn enter_album(&mut self, album: String) {
        self.album_scope = LibraryScope::Album(album);
        self.rebuild_album_cache();
        self.scoped_song_selected = if self.scoped_track_indices.is_empty() {
            None
        } else {
            Some(0)
        };
        self.scoped_songs_offset = 0;
    }

    pub fn enter_artist(&mut self, artist: String) {
        self.artist_scope = LibraryScope::Artist(artist);
        self.rebuild_artist_cache();
        self.artist_scoped_album_selected = if self.scoped_albums.is_empty() {
            None
        } else {
            Some(0)
        };
        self.artist_albums_offset = 0;
    }

    pub fn exit_scope(&mut self) {
        match self.active_tab {
            LibraryTab::Albums => {
                self.album_scope = LibraryScope::All;
                self.rebuild_album_cache();
                self.scoped_songs_offset = 0;
            }
            LibraryTab::Artists => {
                self.artist_scope = LibraryScope::All;
                self.rebuild_artist_cache();
                self.artist_albums_offset = 0;
            }
            LibraryTab::Playlists => {
                self.playlist_scope = PlaylistScope::All;
                self.playlist_songs_offset = 0;
            }
            _ => {}
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
        if let Some(fav) = self.playlists.first_mut() {
            if fav.add_track(track_path) {
                fav.save()?;
            }
        }
        Ok(())
    }

    pub fn seek_forward(&mut self) -> Result<()> {
        let new_pos = self.player.elapsed() + std::time::Duration::from_secs(5);
        if let Some(track) = self.current_track() {
            if new_pos < track.duration {
                let path = track.path.clone();
                self.player.seek(&path, new_pos)?;
            }
        }
        Ok(())
    }

    pub fn seek_backward(&mut self) -> Result<()> {
        let elapsed = self.player.elapsed();
        let new_pos = elapsed.saturating_sub(std::time::Duration::from_secs(5));
        if let Some(track) = self.current_track() {
            let path = track.path.clone();
            self.player.seek(&path, new_pos)?;
        }
        Ok(())
    }

    pub fn toggle_shuffle(&mut self) {
        self.shuffle = !self.shuffle;
        if self.shuffle {
            self.rebuild_shuffle();
        } else {
            self.shuffle_order.clear();
        }
    }

    fn base_play_indices(&self) -> Vec<usize> {
        match &self.play_context {
            PlayContext::Playlist(pi) => {
                let r = self.playlist_track_indices(*pi);
                if !r.is_empty() {
                    r
                } else {
                    self.all_track_indices.clone()
                }
            }
            PlayContext::Album => {
                if !self.scoped_track_indices.is_empty() {
                    self.scoped_track_indices.clone()
                } else {
                    self.all_track_indices.clone()
                }
            }
            PlayContext::AllTracks => self.all_track_indices.clone(),
        }
    }

    fn current_play_indices(&self) -> Vec<usize> {
        if self.shuffle && !self.shuffle_order.is_empty() {
            return self.shuffle_order.clone();
        }
        self.base_play_indices()
    }

    fn rebuild_shuffle(&mut self) {
        let mut indices = self.base_play_indices();
        let n = indices.len();
        let mut rng_state: u64 = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos() as u64;
        rng_state ^= self.current_index.unwrap_or(0) as u64 * 0xdeadbeef;
        for i in (1..n).rev() {
            let j = rand_usize(&mut rng_state) % (i + 1);
            indices.swap(i, j);
        }
        if let Some(ci) = self.current_index {
            if let Some(pos) = indices.iter().position(|&i| i == ci) {
                indices.swap(0, pos);
            }
        }
        self.shuffle_order = indices;
    }

    pub fn enter_search(&mut self) {
        self.search_mode = true;
        self.search_query.clear();
        self.search_results.clear();
        self.search_selected = None;
        self.search_offset = 0;
    }

    pub fn exit_search(&mut self) {
        self.search_mode = false;
        self.search_query.clear();
        self.search_results.clear();
        self.search_selected = None;
        self.search_offset = 0;
    }

    pub fn search_type_char(&mut self, c: char) {
        self.search_query.push(c);
        self.run_search();
        self.search_offset = 0;
    }

    pub fn search_backspace(&mut self) {
        self.search_query.pop();
        self.run_search();
        self.search_offset = 0;
    }

    fn run_search(&mut self) {
        self.search_results.clear();
        self.search_selected = None;
        let q = self.search_query.to_lowercase();
        if q.is_empty() {
            return;
        }

        for (i, t) in self.tracks.iter().enumerate() {
            if t.title.to_lowercase().contains(&q)
                || t.artist.to_lowercase().contains(&q)
                || t.album.to_lowercase().contains(&q)
            {
                self.search_results.push(SearchResult::Track(i));
            }
        }
        for album in &self.albums {
            if album.to_lowercase().contains(&q) {
                self.search_results.push(SearchResult::Album(album.clone()));
            }
        }
        for artist in &self.artists {
            if artist.to_lowercase().contains(&q) {
                self.search_results
                    .push(SearchResult::Artist(artist.clone()));
            }
        }
        for (pi, playlist) in self.playlists.iter().enumerate() {
            for path in &playlist.track_paths {
                if let Some(ti) = self.tracks.iter().position(|t| &t.path == path) {
                    let t = &self.tracks[ti];
                    if t.title.to_lowercase().contains(&q) || t.artist.to_lowercase().contains(&q) {
                        if !self
                            .search_results
                            .iter()
                            .any(|r| matches!(r, SearchResult::Track(i) if *i == ti))
                        {
                            self.search_results.push(SearchResult::PlaylistTrack {
                                playlist_idx: pi,
                                track_idx: ti,
                            });
                        }
                    }
                }
            }
        }
        if !self.search_results.is_empty() {
            self.search_selected = Some(0);
        }
    }

    pub fn search_select_next(&mut self) {
        let len = self.search_results.len();
        if len == 0 {
            return;
        }
        self.search_selected = Some(match self.search_selected {
            Some(i) => (i + 1) % len,
            None => 0,
        });
        self.search_offset = scroll_offset(
            self.search_selected,
            self.search_offset,
            self.search_list_height,
        );
    }

    pub fn search_select_prev(&mut self) {
        let len = self.search_results.len();
        if len == 0 {
            return;
        }
        self.search_selected = Some(match self.search_selected {
            Some(i) => {
                if i == 0 {
                    len - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        });
        self.search_offset = scroll_offset(
            self.search_selected,
            self.search_offset,
            self.search_list_height,
        );
    }

    pub fn search_confirm(&mut self) -> Result<()> {
        let selected = match self.search_selected {
            Some(i) => i,
            None => return Ok(()),
        };
        match self.search_results.get(selected).cloned() {
            Some(SearchResult::Track(ti)) => {
                self.play_context = PlayContext::AllTracks;
                if self.shuffle {
                    self.rebuild_shuffle();
                }
                self.exit_search();
                // Switch to Songs tab and move cursor to the playing song
                self.active_tab = LibraryTab::Songs;
                self.selected_index = Some(ti);
                self.songs_offset = scroll_offset(Some(ti), self.songs_offset, self.list_height);
                self.play_index(ti)?;
            }
            Some(SearchResult::Album(album)) => {
                self.exit_search();
                self.active_tab = LibraryTab::Albums;
                self.enter_album(album);
            }
            Some(SearchResult::Artist(artist)) => {
                self.exit_search();
                self.active_tab = LibraryTab::Artists;
                self.enter_artist(artist);
            }
            Some(SearchResult::PlaylistTrack {
                playlist_idx,
                track_idx,
            }) => {
                self.play_context = PlayContext::Playlist(playlist_idx);
                if self.shuffle {
                    self.rebuild_shuffle();
                }
                self.exit_search();
                self.play_index(track_idx)?;
            }
            None => {}
        }
        Ok(())
    }

    pub fn go_to_playing(&mut self) {
        let ci = match self.current_index {
            Some(i) => i,
            None => return,
        };

        // Helper logic to calculate the new offset
        let get_new_offset =
            |current_offset: usize, target: usize, height: usize, total: usize| -> usize {
                // 1. If the item is already visible, don't move the scroll at all
                if target >= current_offset && target < current_offset + height {
                    return current_offset;
                }

                // 2. Calculate the ideal centered offset
                let ideal_offset = target.saturating_sub(height / 2);

                // 3. Clamp the offset so we don't show empty space at the bottom
                // The maximum the top index can be is (total_items - height)
                let max_offset = total.saturating_sub(height);

                ideal_offset.min(max_offset)
            };

        match &self.play_context.clone() {
            PlayContext::AllTracks => {
                self.active_tab = LibraryTab::Songs;
                self.selected_index = Some(ci);
                self.songs_offset =
                    get_new_offset(self.songs_offset, ci, self.list_height, self.tracks.len());
            }
            PlayContext::Album => {
                self.active_tab = LibraryTab::Albums;
                if let LibraryScope::Album(_) = &self.album_scope {
                    if let Some(pos) = self.scoped_track_indices.iter().position(|&i| i == ci) {
                        self.scoped_song_selected = Some(pos);
                        self.scoped_songs_offset = get_new_offset(
                            self.scoped_songs_offset,
                            pos,
                            self.list_height,
                            self.scoped_track_indices.len(),
                        );
                    }
                } else {
                    let album = self.tracks[ci].album.clone();
                    self.enter_album(album);
                    if let Some(pos) = self.scoped_track_indices.iter().position(|&i| i == ci) {
                        self.scoped_song_selected = Some(pos);
                        self.scoped_songs_offset = get_new_offset(
                            0, // Default to 0 since scope just opened
                            pos,
                            self.list_height,
                            self.scoped_track_indices.len(),
                        );
                    }
                }
            }
            PlayContext::Playlist(pi) => {
                let pi = *pi;
                self.active_tab = LibraryTab::Playlists;
                self.playlist_scope = PlaylistScope::Open(pi);
                self.playlist_selected = Some(pi);

                let indices = self.playlist_track_indices(pi);
                if let Some(pos) = indices.iter().position(|&i| i == ci) {
                    self.playlist_song_selected = Some(pos);
                    self.playlist_songs_offset = get_new_offset(
                        self.playlist_songs_offset,
                        pos,
                        self.list_height,
                        indices.len(),
                    );
                }
            }
        }
    }

    pub fn edit_lyrics(&mut self) -> Result<()> {
        let track = match self.current_track() {
            Some(t) => t,
            None => return Ok(()),
        };

        let lrc_path = crate::lyrics::manual_lyrics_path(&track.path);

        // If no manual file exists yet, create a template
        if !lrc_path.exists() {
            let template = build_lrc_template(track);
            std::fs::write(&lrc_path, template)?;
        }

        // Suspend TUI
        crossterm::terminal::disable_raw_mode()?;
        crossterm::execute!(std::io::stdout(), crossterm::terminal::LeaveAlternateScreen)?;

        // Open editor
        let editor = std::env::var("EDITOR")
            .or_else(|_| std::env::var("VISUAL"))
            .unwrap_or_else(|_| {
                // Try common editors in order
                for e in &["nvim", "vim", "vi", "nano"] {
                    if which(e) {
                        return e.to_string();
                    }
                }
                "nano".to_string()
            });
        std::process::Command::new(&editor)
            .arg(&lrc_path)
            .status()?;

        // Restore TUI
        crossterm::terminal::enable_raw_mode()?;
        crossterm::execute!(std::io::stdout(), crossterm::terminal::EnterAlternateScreen)?;
        self.needs_full_redraw = true;
        // Reload lyrics from the edited file
        if let Some(idx) = self.current_index {
            self.fetch_lyrics(idx);
        }

        Ok(())
    }
    pub fn go_to_top(&mut self) {
        match self.active_tab {
            LibraryTab::Songs => {
                self.selected_index = if self.all_track_indices.is_empty() {
                    None
                } else {
                    Some(0)
                };
                self.songs_offset = 0;
            }
            LibraryTab::Albums => match &self.album_scope {
                LibraryScope::All => {
                    self.album_selected = if self.albums.is_empty() {
                        None
                    } else {
                        Some(0)
                    };
                    self.albums_offset = 0;
                }
                LibraryScope::Album(_) => {
                    self.scoped_song_selected = if self.scoped_track_indices.is_empty() {
                        None
                    } else {
                        Some(0)
                    };
                    self.scoped_songs_offset = 0;
                }
                _ => {}
            },
            LibraryTab::Artists => match &self.artist_scope {
                LibraryScope::All => {
                    self.artist_selected = if self.artists.is_empty() {
                        None
                    } else {
                        Some(0)
                    };
                    self.artists_offset = 0;
                }
                LibraryScope::Artist(_) => {
                    self.artist_scoped_album_selected = if self.scoped_albums.is_empty() {
                        None
                    } else {
                        Some(0)
                    };
                    self.artist_albums_offset = 0;
                }
                _ => {}
            },
            LibraryTab::Playlists => match &self.playlist_scope {
                PlaylistScope::All => {
                    self.playlist_selected = if self.playlists.is_empty() {
                        None
                    } else {
                        Some(0)
                    };
                    self.playlist_list_offset = 0;
                }
                PlaylistScope::Open(_) => {
                    self.playlist_song_selected = Some(0);
                    self.playlist_songs_offset = 0;
                }
            },
        }
    }

    pub fn go_to_bottom(&mut self) {
        match self.active_tab {
            LibraryTab::Songs => {
                let len = self.all_track_indices.len();
                if len == 0 {
                    return;
                }
                self.selected_index = Some(len - 1);
                self.songs_offset = scroll_offset(self.selected_index, 0, self.list_height);
            }
            LibraryTab::Albums => match &self.album_scope {
                LibraryScope::All => {
                    let len = self.albums.len();
                    if len == 0 {
                        return;
                    }
                    self.album_selected = Some(len - 1);
                    self.albums_offset = scroll_offset(self.album_selected, 0, self.list_height);
                }
                LibraryScope::Album(_) => {
                    let len = self.scoped_track_indices.len();
                    if len == 0 {
                        return;
                    }
                    self.scoped_song_selected = Some(len - 1);
                    self.scoped_songs_offset =
                        scroll_offset(self.scoped_song_selected, 0, self.scoped_list_height);
                }
                _ => {}
            },
            LibraryTab::Artists => match &self.artist_scope {
                LibraryScope::All => {
                    let len = self.artists.len();
                    if len == 0 {
                        return;
                    }
                    self.artist_selected = Some(len - 1);
                    self.artists_offset = scroll_offset(self.artist_selected, 0, self.list_height);
                }
                LibraryScope::Artist(_) => {
                    let len = self.scoped_albums.len();
                    if len == 0 {
                        return;
                    }
                    self.artist_scoped_album_selected = Some(len - 1);
                    self.artist_albums_offset =
                        scroll_offset(self.artist_scoped_album_selected, 0, self.list_height);
                }
                _ => {}
            },
            LibraryTab::Playlists => match &self.playlist_scope {
                PlaylistScope::All => {
                    let len = self.playlists.len();
                    if len == 0 {
                        return;
                    }
                    self.playlist_selected = Some(len - 1);
                    self.playlist_list_offset =
                        scroll_offset(self.playlist_selected, 0, self.list_height);
                }
                PlaylistScope::Open(pi) => {
                    let len = self.playlists[*pi].track_paths.len();
                    if len == 0 {
                        return;
                    }
                    self.playlist_song_selected = Some(len - 1);
                    self.playlist_songs_offset =
                        scroll_offset(self.playlist_song_selected, 0, self.scoped_list_height);
                }
            },
        }
    }
}

fn extract_cover_to_file(track_path: &std::path::Path) -> Option<String> {
    use lofty::prelude::*;
    use lofty::read_from_path;

    let tagged = read_from_path(track_path).ok()?;
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag())?;
    let pic = tag.pictures().first()?;

    // Save to temp file
    let tmp_path = "/tmp/aghani-cover.jpg";
    std::fs::write(tmp_path, pic.data()).ok()?;

    // Return as file:// URL
    Some(format!("file://{}", tmp_path))
}

pub fn scroll_offset(
    selected: Option<usize>,
    current_offset: usize,
    visible_height: usize,
) -> usize {
    let sel = match selected {
        Some(s) => s,
        None => return current_offset,
    };
    if sel < current_offset {
        sel
    } else if sel >= current_offset + visible_height {
        sel + 1 - visible_height
    } else {
        current_offset
    }
}

pub fn center_offset(selected: Option<usize>, visible_height: usize) -> usize {
    let sel = match selected {
        Some(s) => s,
        None => return 0,
    };
    if sel < visible_height / 2 {
        0
    } else {
        sel - visible_height / 2
    }
}

fn rand_usize(state: &mut u64) -> usize {
    *state = state
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    (*state >> 33) as usize
}

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

fn build_lrc_template(track: &crate::library::track::Track) -> String {
    format!(
        "[ti:{}]\n[ar:{}]\n[al:{}]\n[by:aghani]\n\n\
         # Add lyrics below in LRC format:\n\
         # [mm:ss.xx] Lyric line\n\
         # Example:\n\
         # [00:12.00] First line of lyrics\n\
         # [00:17.20] Second line\n\
         #\n\
         # Or plain text (no timestamps):\n\
         # First line\n\
         # Second line\n",
        track.title, track.artist, track.album
    )
}

fn which(cmd: &str) -> bool {
    std::process::Command::new("which")
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
