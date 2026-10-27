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
use std::collections::HashSet;
use std::path::{Path, PathBuf};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SongSort {
    LastAdded,
    FirstAdded,
    Title,
    Artist,
    Album,
    Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlbumSort { Name, Artist, Tracks }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtistSort {
    Name,
    Albums,
    Songs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaylistSort { Name, Tracks }

impl SongSort {
    fn next(self) -> Self {
        match self {
            Self::LastAdded => Self::FirstAdded,
            Self::FirstAdded => Self::Title,
            Self::Title => Self::Artist,
            Self::Artist => Self::Album,
            Self::Album => Self::Duration,
            Self::Duration => Self::LastAdded,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::LastAdded => "Last added",
            Self::FirstAdded => "First added",
            Self::Title => "Title",
            Self::Artist => "Artist",
            Self::Album => "Album",
            Self::Duration => "Duration",
        }
    }
}
impl AlbumSort {
    fn next(self) -> Self { match self { Self::Name => Self::Artist, Self::Artist => Self::Tracks, Self::Tracks => Self::Name } }
    pub fn label(self) -> &'static str { match self { Self::Name => "Name", Self::Artist => "Artist", Self::Tracks => "Track count" } }
}
impl ArtistSort {
    fn next(self) -> Self {
        match self {
            Self::Name => Self::Albums,
            Self::Albums => Self::Songs,
            Self::Songs => Self::Name,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::Albums => "Album count",
            Self::Songs => "Song count",
        }
    }
}
impl PlaylistSort {
    fn next(self) -> Self { match self { Self::Name => Self::Tracks, Self::Tracks => Self::Name } }
    pub fn label(self) -> &'static str { match self { Self::Name => "Name", Self::Tracks => "Track count" } }
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

/// Lower-cased metadata kept in sync with `App::tracks` so typing in the
/// search overlay does not lower-case every track on every keystroke.
#[derive(Debug, Clone, Default)]
struct SearchTerms {
    title: String,
    artist: String,
    album: String,
}

impl SearchTerms {
    fn from_track(track: &Track) -> Self {
        Self {
            title: track.title.to_lowercase(),
            artist: track.artist.to_lowercase(),
            album: track.album.to_lowercase(),
        }
    }

    fn update(&mut self, track: &Track) {
        self.title.clear();
        self.title.push_str(&track.title.to_lowercase());
        self.artist.clear();
        self.artist.push_str(&track.artist.to_lowercase());
        self.album.clear();
        self.album.push_str(&track.album.to_lowercase());
    }
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
    pub history: Vec<PathBuf>,
    pub play_counts: HashMap<PathBuf, u64>,
    pub save_mode: bool,
    pub save_candidates: Vec<usize>,
    pub save_selected: usize,
    pub artist_album_counts: HashMap<String, usize>,
    album_track_counts: HashMap<String, usize>,
    album_artist_map: HashMap<String, String>,
    artist_song_counts: HashMap<String, usize>,
    album_cover_paths: HashMap<String, PathBuf>,
    pub tracks: Vec<Track>,
    track_indices: HashMap<PathBuf, usize>,
    search_terms: Vec<SearchTerms>,
    pub player: Player,
    pub current_index: Option<usize>,
    pub selected_index: Option<usize>,
    pub scoped_song_selected: Option<usize>,
    pub cover_image: Option<ratatui_image::protocol::StatefulProtocol>,
    cover_rx: std::sync::mpsc::Receiver<(usize, Option<DynamicImage>)>,
    cover_tx: std::sync::mpsc::Sender<(usize, Option<DynamicImage>)>,
    pub picker: ratatui_image::picker::Picker,
    metadata_rx: Receiver<(usize, Track)>,
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
    album_return_artist: Option<String>,
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
    mpris_last_track: Option<usize>,
    mpris_cover_url: Option<String>,
    mpris_last_update: Option<std::time::Instant>,
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
    /// Manual scroll offset for the lyrics panel. `None` follows playback.
    pub lyrics_scroll: Option<usize>,
    /// Line clicked to seek, whose viewport should stay put until playback
    /// moves on to the next line.
    pub lyrics_clicked_line: Option<usize>,
    lyrics_rx: Option<std::sync::mpsc::Receiver<Option<crate::lyrics::Lyrics>>>,
    pub lyrics_sync_mode: bool,
    pub lyrics_sync_lines: Vec<crate::lyrics::SyncLine>,
    pub lyrics_sync_index: Option<usize>,
    lyrics_sync_track: Option<usize>,
    /// Short-lived message shown at the bottom of the screen.
    pub notice: Option<(String, std::time::Instant)>,
    pub needs_full_redraw: bool,
    last_mouse_click: Option<(u16, u16, std::time::Instant)>,
    pub artwork: HashMap<String, crate::library::artwork::Thumbnail>,
    pub artwork_protocols: HashMap<String, ratatui_image::protocol::StatefulProtocol>,
    pub artwork_overrides: HashMap<String, PathBuf>,
    artwork_rx: std::sync::mpsc::Receiver<(String, Option<crate::library::artwork::Thumbnail>)>,
    artwork_tx: std::sync::mpsc::Sender<(String, Option<crate::library::artwork::Thumbnail>)>,
    artwork_pending: HashSet<String>,
    /// Keys whose artwork lookup returned nothing. Remembering them prevents
    /// spawning a worker thread on every frame for covers that do not exist.
    artwork_missing: HashSet<String>,
    pub artwork_path_mode: bool,
    pub artwork_path_input: String,
    pub metadata_edit_mode: bool,
    pub metadata_edit_index: Option<usize>,
    pub metadata_edit_field: usize,
    pub metadata_edit_values: [String; 4],
    pub metadata_edit_cursors: [usize; 4],
    pub metadata_edit_error: Option<String>,
    pub show_artwork: bool,
    pub grid_view: bool,
    pub pinned_playlists: Vec<String>,
    pub song_sort: SongSort,
    pub playlist_song_sort: SongSort,
    pub album_sort: AlbumSort,
    pub artist_sort: ArtistSort,
    pub playlist_sort: PlaylistSort,
}

impl App {
    pub fn new(
        music_dir: &Path,
        metadata_rx: Receiver<(usize, Track)>,
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
        playlists.retain(|playlist| {
            !matches!(
                playlist.name.to_lowercase().as_str(),
                "history" | "most played"
            )
        });
        playlists.push(Playlist::new("History".to_string(), None));
        playlists.push(Playlist::new("Most Played".to_string(), None));
        let tracks = scan_directory(music_dir)?;
        let selected_index = if tracks.is_empty() { None } else { Some(0) };
        let fallback_cover = load_fallback(Path::new("assets/default_cover.png")).ok();
        let picker = ratatui_image::picker::Picker::from_query_stdio()
            .unwrap_or_else(|_| ratatui_image::picker::Picker::halfblocks());
        let (artwork_tx, artwork_rx) = std::sync::mpsc::channel();

        let all_track_indices = (0..tracks.len()).collect();
        let scoped_track_indices = (0..tracks.len()).collect();
        let track_indices: HashMap<PathBuf, usize> = tracks
            .iter()
            .enumerate()
            .map(|(index, track)| (track.path.clone(), index))
            .collect();
        let search_terms: Vec<SearchTerms> = tracks.iter().map(SearchTerms::from_track).collect();
        let (cover_tx, cover_rx) = std::sync::mpsc::channel();

        // Apply startup volume
        let volume = config.startup.volume.unwrap_or(1.0).clamp(0.0, 2.0);
        player.set_volume(volume);
        let mut app = App {
            // active_tab,
            tracks,
            track_indices,
            search_terms,
            player,
            current_index: None,
            selected_index,
            cover_image: None,
            cover_rx,
            cover_tx,
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
            album_track_counts: HashMap::new(),
            album_artist_map: HashMap::new(),
            artist_song_counts: HashMap::new(),
            album_cover_paths: HashMap::new(),
            fallback_cover,
            active_tab: LibraryTab::Songs,
            album_selected: None,
            artist_selected: None,
            playlist_selected: None,
            album_scope: LibraryScope::All,
            album_return_artist: None,
            artist_scope: LibraryScope::All,
            scoped_song_selected: None,
            artist_scoped_album_selected: None,
            playlist_scope: PlaylistScope::All,
            playlist_song_selected: None,
            save_mode: false,
            save_candidates: vec![],
            save_selected: 0,
            playlists,
            history: Vec::new(),
            play_counts: HashMap::new(),
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
            mpris_last_track: None,
            mpris_cover_url: None,
            mpris_last_update: None,
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
            lyrics_scroll: None,
            lyrics_clicked_line: None,
            lyrics_rx: None,
            lyrics_sync_mode: false,
            lyrics_sync_lines: Vec::new(),
            lyrics_sync_index: None,
            lyrics_sync_track: None,
            notice: None,
            needs_full_redraw: false,
            last_mouse_click: None,
            artwork: HashMap::new(),
            artwork_protocols: HashMap::new(),
            artwork_overrides: HashMap::new(),
            artwork_rx,
            artwork_tx,
            artwork_pending: HashSet::new(),
            artwork_missing: HashSet::new(),
            artwork_path_mode: false,
            artwork_path_input: String::new(),
            metadata_edit_mode: false,
            metadata_edit_index: None,
            metadata_edit_field: 0,
            metadata_edit_values: std::array::from_fn(|_| String::new()),
            metadata_edit_cursors: [0; 4],
            metadata_edit_error: None,
            show_artwork: false,
            grid_view: false,
            pinned_playlists: Vec::new(),
            song_sort: SongSort::LastAdded,
            playlist_song_sort: SongSort::LastAdded,
            album_sort: AlbumSort::Name,
            artist_sort: ArtistSort::Name,
            playlist_sort: PlaylistSort::Name,
            // show_lyrics: config.layout.show_lyrics,
        };
        app.sort_all_tracks();
        app.sort_scoped_tracks();
        app.sort_albums();
        app.sort_artists();
        app.reorder_playlists();
        Ok(app)
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
            _none => return Ok(()),
        };

        for cmd in commands {
            match cmd {
                MprisCommand::Play => {
                    if self.player.is_paused() {
                        self.player.resume();
                    }
                }
                MprisCommand::Pause => {
                    if !self.player.is_paused() {
                        self.player.pause();
                    }
                }
                MprisCommand::PlayPause => self.toggle_play()?,
                MprisCommand::Next => self.next_track()?,
                MprisCommand::Prev => self.prev_track()?,
                MprisCommand::Stop => self.player.stop(),

                // Relative seek — offset in microseconds (can be negative)
                MprisCommand::Seek(offset_us) => {
                    let current = self.player.elapsed();
                    let offset = std::time::Duration::from_micros(offset_us.unsigned_abs());
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

    pub fn update_mpris_state(&mut self) {
        use crate::mpris::MprisUpdate;

        const UPDATE_INTERVAL: std::time::Duration = std::time::Duration::from_millis(250);

        if self.mpris_update_tx.is_none() {
            return;
        }

        let now = std::time::Instant::now();
        let track_changed = self.mpris_last_track != self.current_index;
        if !track_changed {
            if let Some(last) = self.mpris_last_update {
                if now.duration_since(last) < UPDATE_INTERVAL {
                    return;
                }
            }
        }

        // Extracting the cover reads tags from disk, so only do it when the
        // playing track actually changes.
        if track_changed {
            self.mpris_cover_url = self.current_index.and_then(|idx| {
                let track: &Track = &self.tracks[idx];
                extract_cover_to_file(&track.path)
            });
            self.mpris_last_track = self.current_index;
        }

        let upd = if let Some(track) = self.current_track() {
            MprisUpdate {
                title: track.title.clone(),
                artist: track.artist.clone(),
                album: track.album.clone(),
                duration_us: track.duration.as_micros() as i64,
                position_us: self.player.elapsed().as_micros() as i64,
                playing: self.player.is_paused() == false,
                volume: self.player.volume as f64,
                shuffle: self.shuffle,
                cover_url: self.mpris_cover_url.clone(),
            }
        } else {
            MprisUpdate::default()
        };

        if let Some(tx) = &self.mpris_update_tx {
            let _ = tx.try_send(upd);
        }
        self.mpris_last_update = Some(now);
    }

    pub fn set_volume(&mut self, vol: f32) {
        self.player.set_volume(vol);
    }

    pub fn toggle_repeat(&mut self) {
        self.repeat = self.repeat.next();
    }

    pub fn restore_session_from_state(&mut self, state: &crate::config::State) -> Result<()> {
        self.history = state.history.clone();
        self.play_counts = state.play_counts.clone();
        self.artwork_overrides = state.artwork_overrides.clone();
        self.pinned_playlists = state.pinned_playlists.clone();
        for (key, path) in &self.artwork_overrides {
            if let Some(thumb) = crate::library::artwork::from_path(key, path) {
                self.artwork_protocols
                    .insert(key.clone(), self.picker.new_resize_protocol(thumb.image()));
                self.artwork.insert(key.clone(), thumb);
            }
        }
        self.refresh_generated_playlists();
        self.reorder_playlists();
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
            if let Some(&idx) = self.track_indices.get(last_path) {
                let position = state.last_position.unwrap_or(0.0);

                // Sync UI state to the restored track
                self.current_index = Some(idx);
                self.selected_index = self.all_track_indices.iter().position(|&i| i == idx);
                self.songs_offset = scroll_offset(self.selected_index, 0, self.list_height);

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
        let mut indices: Vec<usize> = pl
            .track_paths
            .iter()
            .filter_map(|path| self.track_indices.get(path).copied())
            .collect();
        let playlist = &self.playlists[playlist_idx];
        if !matches!(
            playlist.name.to_lowercase().as_str(),
            "history" | "most played"
        ) {
            sort_track_indices(&mut indices, &self.tracks, self.playlist_song_sort);
        }
        indices
    }

    pub fn enter_playlist(&mut self, idx: usize) {
        self.playlist_scope = PlaylistScope::Open(idx);
        let resolved = self.playlist_track_indices(idx);
        self.playlist_song_selected = if resolved.is_empty() { None } else { Some(0) };
        self.playlist_songs_offset = scroll_offset(
            self.playlist_song_selected,
            0,
            self.list_height,
        );
    }

    // ── Playlist management ───────────────────────────────────────────────────────

    pub fn start_new_playlist(&mut self) {
        if self.active_tab == LibraryTab::Playlists {
            self.new_playlist_mode = true;
            self.new_playlist_name.clear();
        }
    }

    pub fn start_artwork_path(&mut self) {
        let can_set = match self.active_tab {
            LibraryTab::Albums => matches!(&self.album_scope, LibraryScope::All),
            LibraryTab::Artists => matches!(&self.artist_scope, LibraryScope::All),
            LibraryTab::Playlists => matches!(&self.playlist_scope, PlaylistScope::All),
            LibraryTab::Songs => false,
        };
        if can_set {
            self.artwork_path_mode = true;
            self.artwork_path_input.clear();
        }
    }

    pub fn toggle_artwork(&mut self) {
        self.show_artwork = match self.show_artwork {
            true => false,
            false => true,
        };
        self.albums_offset = 0;
        self.artists_offset = 0;
        self.artist_albums_offset = 0;
        self.playlist_list_offset = 0;
    }

    pub fn toggle_grid_view(&mut self) {
        self.grid_view = !self.grid_view;
        self.albums_offset = 0;
        self.artists_offset = 0;
        self.artist_albums_offset = 0;
        self.playlist_list_offset = 0;
    }

    pub fn cycle_sort(&mut self) {
        match self.active_tab {
            LibraryTab::Songs => {
                self.song_sort = self.song_sort.next();
                self.sort_all_tracks();
            }
            LibraryTab::Albums => match self.album_scope {
                LibraryScope::All => {
                    self.album_sort = self.album_sort.next();
                    self.sort_albums();
                }
                LibraryScope::Album(_) => {
                    self.song_sort = self.song_sort.next();
                    self.sort_scoped_tracks();
                }
                _ => {}
            },
            LibraryTab::Artists => match self.artist_scope {
                LibraryScope::All => {
                    self.artist_sort = self.artist_sort.next();
                    self.sort_artists();
                }
                LibraryScope::Artist(_) => {
                    self.album_sort = self.album_sort.next();
                    self.sort_artist_albums();
                }
                _ => {}
            },
            LibraryTab::Playlists => match self.playlist_scope {
                PlaylistScope::All => {
                    self.playlist_sort = self.playlist_sort.next();
                    self.reorder_playlists();
                }
                PlaylistScope::Open(_) => {
                    let playlist_index = match self.playlist_scope {
                        PlaylistScope::Open(index) => index,
                        PlaylistScope::All => return,
                    };
                    if self.playlists.get(playlist_index).is_some_and(|playlist| {
                        matches!(
                            playlist.name.to_lowercase().as_str(),
                            "history" | "most played"
                        )
                    }) {
                        return;
                    }
                    let selected_track = self
                        .playlist_song_selected
                        .and_then(|i| self.playlist_track_indices(playlist_index).get(i).copied());
                    self.playlist_song_sort = self.playlist_song_sort.next();
                    let sorted = self.playlist_track_indices(playlist_index);
                    self.playlist_song_selected =
                        selected_track.and_then(|track| sorted.iter().position(|&i| i == track));
                    self.playlist_songs_offset = scroll_offset(
                        self.playlist_song_selected,
                        0,
                        self.list_height,
                    );
                }
            },
        }
    }

    pub fn sort_label(&self) -> &'static str {
        match self.active_tab {
            LibraryTab::Songs => self.song_sort.label(),
            LibraryTab::Albums => match self.album_scope {
                LibraryScope::Album(_) => self.song_sort.label(),
                _ => self.album_sort.label(),
            },
            LibraryTab::Artists => match self.artist_scope {
                LibraryScope::Artist(_) => self.album_sort.label(),
                _ => self.artist_sort.label(),
            },
            LibraryTab::Playlists => match self.playlist_scope {
                PlaylistScope::Open(index) => self
                    .playlists
                    .get(index)
                    .map(|playlist| match playlist.name.to_lowercase().as_str() {
                        "history" => "History order",
                        "most played" => "Play count",
                        _ => self.playlist_song_sort.label(),
                    })
                    .unwrap_or(self.playlist_song_sort.label()),
                _ => self.playlist_sort.label(),
            },
        }
    }

    fn sort_all_tracks(&mut self) {
        let selected = self.selected_index.and_then(|i| self.all_track_indices.get(i).copied());
        sort_track_indices(&mut self.all_track_indices, &self.tracks, self.song_sort);
        self.selected_index = selected.and_then(|id| self.all_track_indices.iter().position(|&i| i == id));
        self.songs_offset = scroll_offset(self.selected_index, 0, self.list_height);
    }

    fn sort_scoped_tracks(&mut self) {
        let selected = self.scoped_song_selected.and_then(|i| self.scoped_track_indices.get(i).copied());
        sort_track_indices(&mut self.scoped_track_indices, &self.tracks, self.song_sort);
        self.scoped_song_selected = selected.and_then(|id| self.scoped_track_indices.iter().position(|&i| i == id));
        self.scoped_songs_offset =
            scroll_offset(self.scoped_song_selected, 0, self.scoped_list_height);
    }

    fn sort_albums(&mut self) {
        let selected = self.album_selected.and_then(|i| self.albums.get(i).cloned());
        let selected_scoped = self
            .artist_scoped_album_selected
            .and_then(|i| self.scoped_albums.get(i).cloned());
        self.sort_album_names_in_place();
        self.album_selected = selected.and_then(|name| self.albums.iter().position(|item| item == &name));
        self.artist_scoped_album_selected = selected_scoped
            .and_then(|name| self.scoped_albums.iter().position(|item| item == &name));
        self.albums_offset = scroll_offset(self.album_selected, 0, self.list_height);
        self.artist_albums_offset =
            scroll_offset(self.artist_scoped_album_selected, 0, self.scoped_list_height);
    }

    fn sort_album_names_in_place(&mut self) {
        let mut albums = std::mem::take(&mut self.albums);
        let mut scoped_albums = std::mem::take(&mut self.scoped_albums);
        self.sort_album_names(&mut albums);
        self.sort_album_names(&mut scoped_albums);
        self.albums = albums;
        self.scoped_albums = scoped_albums;
    }

    fn sort_album_names(&self, albums: &mut [String]) {
        match self.album_sort {
            AlbumSort::Name => albums.sort_by_cached_key(|album| album.to_lowercase()),
            AlbumSort::Artist => {
                albums.sort_by_cached_key(|album| (self.album_artist(album), album.clone()))
            }
            AlbumSort::Tracks => albums.sort_by_cached_key(|album| {
                (std::cmp::Reverse(self.album_track_count(album)), album.clone())
            }),
        }
    }

    fn sort_artist_albums(&mut self) {
        let selected = self.artist_scoped_album_selected.and_then(|i| self.scoped_albums.get(i).cloned());
        let mut scoped_albums = std::mem::take(&mut self.scoped_albums);
        self.sort_album_names(&mut scoped_albums);
        self.scoped_albums = scoped_albums;
        self.artist_scoped_album_selected = selected.and_then(|name| self.scoped_albums.iter().position(|item| item == &name));
        self.artist_albums_offset =
            scroll_offset(self.artist_scoped_album_selected, 0, self.scoped_list_height);
    }

    fn sort_artists(&mut self) {
        let selected = self.artist_selected.and_then(|i| self.artists.get(i).cloned());
        let mut artists = std::mem::take(&mut self.artists);
        match self.artist_sort {
            ArtistSort::Name => artists.sort_by_cached_key(|artist| artist.to_lowercase()),
            ArtistSort::Albums => artists.sort_by_cached_key(|artist| {
                (
                    std::cmp::Reverse(self.artist_album_count(artist)),
                    artist.clone(),
                )
            }),
            ArtistSort::Songs => artists.sort_by_cached_key(|artist| {
                (
                    std::cmp::Reverse(self.artist_song_count(artist)),
                    artist.clone(),
                )
            }),
        }
        self.artists = artists;
        self.artist_selected = selected.and_then(|name| self.artists.iter().position(|item| item == &name));
        self.artists_offset = scroll_offset(self.artist_selected, 0, self.list_height);
    }

    fn album_artist(&self, album: &str) -> String {
        self.album_artist_map
            .get(album)
            .cloned()
            .unwrap_or_default()
    }

    fn artist_album_count(&self, artist: &str) -> usize {
        self.artist_album_counts.get(artist).copied().unwrap_or(0)
    }

    fn artist_song_count(&self, artist: &str) -> usize {
        self.artist_song_counts.get(artist).copied().unwrap_or(0)
    }

    pub fn toggle_pin_selected_playlist(&mut self) {
        if self.active_tab != LibraryTab::Playlists
            || !matches!(&self.playlist_scope, PlaylistScope::All)
        {
            return;
        }
        let Some(selected) = self.playlist_selected else {
            return;
        };
        let Some(name) = self.playlists.get(selected).map(|p| p.name.clone()) else {
            return;
        };
        if matches!(
            name.to_lowercase().as_str(),
            "favorites" | "most played" | "history"
        ) {
            return;
        }
        if let Some(pos) = self.pinned_playlists.iter().position(|p| p == &name) {
            self.pinned_playlists.remove(pos);
        } else {
            self.pinned_playlists.push(name.clone());
        }
        self.reorder_playlists();
        self.playlist_selected = self.playlists.iter().position(|p| p.name == name);
    }

    fn reorder_playlists(&mut self) {
        let selected_name = self
            .playlist_selected
            .and_then(|i| self.playlists.get(i))
            .map(|playlist| playlist.name.clone());
        let playing_playlist_name = match self.play_context {
            PlayContext::Playlist(i) => self.playlists.get(i).map(|playlist| playlist.name.clone()),
            _ => None,
        };
        let open_playlist_name = match &self.playlist_scope {
            PlaylistScope::Open(i) => self.playlists.get(*i).map(|playlist| playlist.name.clone()),
            PlaylistScope::All => None,
        };
        let mut remaining = std::mem::take(&mut self.playlists);
        let mut ordered = Vec::with_capacity(remaining.len());
        for fixed in ["Favorites", "Most Played", "History"] {
            if let Some(pos) = remaining
                .iter()
                .position(|p| p.name.eq_ignore_ascii_case(fixed))
            {
                ordered.push(remaining.remove(pos));
            }
        }
        let (mut pinned, mut unpinned): (Vec<_>, Vec<_>) = remaining
            .into_iter()
            .partition(|playlist| self.pinned_playlists.contains(&playlist.name));
        let sort_playlists = |playlists: &mut Vec<Playlist>| {
            playlists.sort_by(|a, b| match self.playlist_sort {
                PlaylistSort::Name => a
                    .name
                    .to_lowercase()
                    .cmp(&b.name.to_lowercase())
                    .then_with(|| a.name.cmp(&b.name)),
                PlaylistSort::Tracks => b
                    .track_paths
                    .len()
                    .cmp(&a.track_paths.len())
                    .then_with(|| {
                        a.name
                            .to_lowercase()
                            .cmp(&b.name.to_lowercase())
                            .then_with(|| a.name.cmp(&b.name))
                    }),
            });
        };
        sort_playlists(&mut ordered);
        sort_playlists(&mut pinned);
        sort_playlists(&mut unpinned);
        ordered.append(&mut pinned);
        ordered.append(&mut unpinned);
        self.playlists = ordered;
        self.playlist_selected = selected_name
            .and_then(|name| self.playlists.iter().position(|playlist| playlist.name == name));
        if let Some(name) = playing_playlist_name {
            if let Some(index) = self
                .playlists
                .iter()
                .position(|playlist| playlist.name == name)
            {
                self.play_context = PlayContext::Playlist(index);
            }
        }
        if let Some(name) = open_playlist_name {
            if let Some(index) = self
                .playlists
                .iter()
                .position(|playlist| playlist.name == name)
            {
                self.playlist_scope = PlaylistScope::Open(index);
            }
        }
        self.playlist_list_offset = scroll_offset(self.playlist_selected, 0, self.list_height);
    }

    pub fn cancel_artwork_path(&mut self) {
        self.artwork_path_mode = false;
        self.artwork_path_input.clear();
    }

    fn selected_artwork_key(&self) -> Option<String> {
        match self.active_tab {
            LibraryTab::Albums => self
                .album_selected
                .and_then(|i| self.albums.get(i))
                .map(|v| format!("album:{}", v)),
            LibraryTab::Artists => self
                .artist_selected
                .and_then(|i| self.artists.get(i))
                .map(|v| format!("artist:{}", v)),
            LibraryTab::Playlists => self
                .playlist_selected
                .and_then(|i| self.playlists.get(i))
                .map(|v| format!("playlist:{}", v.name)),
            _ => None,
        }
    }

    pub fn confirm_artwork_path(&mut self) {
        let key = self.selected_artwork_key();
        let path = PathBuf::from(self.artwork_path_input.trim());
        if let Some(key) = key {
            if path.is_file() {
                if let Some(thumb) = crate::library::artwork::from_path(&key, &path) {
                    self.artwork_protocols
                        .insert(key.clone(), self.picker.new_resize_protocol(thumb.image()));
                    self.artwork.insert(key.clone(), thumb);
                    self.artwork_overrides.insert(key, path);
                }
                self.cancel_artwork_path();
            }
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
        self.reorder_playlists();
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
                    if matches!(
                        self.playlists[pi].name.to_lowercase().as_str(),
                        "favorites" | "history" | "most played"
                    ) {
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
                    let raw_index = self
                        .playlist_track_indices(pi)
                        .get(si)
                        .and_then(|&idx| self.tracks.get(idx))
                        .and_then(|track| {
                            self.playlists[pi]
                                .track_paths
                                .iter()
                                .position(|path| path == &track.path)
                        })
                        .unwrap_or(si);
                    self.confirm_dialog = Some(ConfirmDialog {
                        message: format!("Remove \"{}\" from playlist?", track_name),
                        action: ConfirmAction::DeleteSongFromPlaylist(pi, raw_index),
                    });
                }
            }
        }
    }

    pub fn confirm_dialog_confirm(&mut self) -> Result<()> {
        let action = match self.confirm_dialog.take() {
            Some(d) => d.action,
            _none => return Ok(()),
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
                let selected = self.playlist_song_selected;
                self.playlists[pi].remove_track(si);
                self.playlists[pi].save()?;
                let len = self.playlist_track_indices(pi).len();
                self.playlist_song_selected = if len == 0 {
                    None
                } else {
                    Some(selected.unwrap_or(0).min(len - 1))
                };
                self.reorder_playlists();
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

    /// Toggle the views used when clicking the album art:
    /// view 2 with lyrics <-> view 1 without lyrics.
    pub fn toggle_cover_lyrics_view(&mut self) {
        if self.layout_mode == "clean_lyrics" {
            self.layout_mode = "default".to_string();
        } else {
            self.layout_mode = "clean_lyrics".to_string();
        }
    }

    // ── Metadata editing ─────────────────────────────────────────────────────────

    /// The song row currently highlighted in a song list, if any.
    pub fn selected_track_index(&self) -> Option<usize> {
        match self.active_tab {
            LibraryTab::Songs => self
                .selected_index
                .and_then(|index| self.all_track_indices.get(index).copied()),
            LibraryTab::Albums => match &self.album_scope {
                LibraryScope::Album(_) => self
                    .scoped_song_selected
                    .and_then(|index| self.scoped_track_indices.get(index).copied()),
                _ => None,
            },
            LibraryTab::Artists => None,
            LibraryTab::Playlists => match &self.playlist_scope {
                PlaylistScope::Open(playlist_index) => {
                    self.playlist_song_selected.and_then(|index| {
                        self.playlist_track_indices(*playlist_index)
                            .get(index)
                            .copied()
                    })
                }
                PlaylistScope::All => None,
            },
        }
    }

    /// The highlighted song, falling back to the currently playing one.
    pub fn target_track_index(&self) -> Option<usize> {
        self.selected_track_index().or(self.current_index)
    }

    pub fn start_metadata_edit(&mut self) {
        let index = match self.target_track_index() {
            Some(index) => index,
            None => return,
        };
        let track = &self.tracks[index];

        self.metadata_edit_values = [
            track.title.clone(),
            track.artist.clone(),
            track.album.clone(),
            String::new(),
        ];
        self.metadata_edit_cursors = [
            self.metadata_edit_values[0].chars().count(),
            self.metadata_edit_values[1].chars().count(),
            self.metadata_edit_values[2].chars().count(),
            0,
        ];
        self.metadata_edit_index = Some(index);
        self.metadata_edit_field = 0;
        self.metadata_edit_error = None;
        self.metadata_edit_mode = true;
    }

    pub fn cancel_metadata_edit(&mut self) {
        self.metadata_edit_mode = false;
        self.metadata_edit_index = None;
        self.metadata_edit_error = None;
    }

    pub fn metadata_edit_next_field(&mut self) {
        self.metadata_edit_field = (self.metadata_edit_field + 1) % self.metadata_edit_values.len();
    }

    pub fn metadata_edit_prev_field(&mut self) {
        let count = self.metadata_edit_values.len();
        self.metadata_edit_field = (self.metadata_edit_field + count - 1) % count;
    }

    pub fn metadata_edit_insert_char(&mut self, c: char) {
        let field = self.metadata_edit_field;
        let cursor = self.metadata_edit_cursors[field];
        let value = &mut self.metadata_edit_values[field];
        let byte_index = char_index_to_byte(value, cursor);
        value.insert(byte_index, c);
        self.metadata_edit_cursors[field] = cursor + 1;
    }

    pub fn metadata_edit_backspace(&mut self) {
        let field = self.metadata_edit_field;
        let cursor = self.metadata_edit_cursors[field];
        if cursor == 0 {
            return;
        }
        let value = &mut self.metadata_edit_values[field];
        let start = char_index_to_byte(value, cursor - 1);
        let end = char_index_to_byte(value, cursor);
        value.replace_range(start..end, "");
        self.metadata_edit_cursors[field] = cursor - 1;
    }

    pub fn metadata_edit_delete(&mut self) {
        let field = self.metadata_edit_field;
        let cursor = self.metadata_edit_cursors[field];
        let value = &mut self.metadata_edit_values[field];
        let length = value.chars().count();
        if cursor >= length {
            return;
        }
        let start = char_index_to_byte(value, cursor);
        let end = char_index_to_byte(value, cursor + 1);
        value.replace_range(start..end, "");
    }

    pub fn metadata_edit_cursor_left(&mut self) {
        let field = self.metadata_edit_field;
        self.metadata_edit_cursors[field] = self.metadata_edit_cursors[field].saturating_sub(1);
    }

    pub fn metadata_edit_cursor_right(&mut self) {
        let field = self.metadata_edit_field;
        let length = self.metadata_edit_values[field].chars().count();
        if self.metadata_edit_cursors[field] < length {
            self.metadata_edit_cursors[field] += 1;
        }
    }

    pub fn metadata_edit_cursor_home(&mut self) {
        self.metadata_edit_cursors[self.metadata_edit_field] = 0;
    }

    pub fn metadata_edit_cursor_end(&mut self) {
        let field = self.metadata_edit_field;
        self.metadata_edit_cursors[field] = self.metadata_edit_values[field].chars().count();
    }

    pub fn save_metadata_edit(&mut self) -> Result<()> {
        let index = match self.metadata_edit_index {
            Some(index) => index,
            None => {
                self.metadata_edit_mode = false;
                return Ok(());
            }
        };

        let track_path = self.tracks[index].path.clone();
        let fallback_title = track_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let title = trimmed_or(&self.metadata_edit_values[0], &fallback_title);
        let artist = trimmed_or(&self.metadata_edit_values[1], "Unknown Artist");
        let album = trimmed_or(&self.metadata_edit_values[2], "Unknown Album");
        let artwork_url = self.metadata_edit_values[3].trim().to_string();

        // Fetch and validate the artwork before touching the audio file.
        let downloaded_artwork = if artwork_url.is_empty() {
            None
        } else {
            match crate::audio::metadata::download_artwork_to_temp(&artwork_url) {
                Ok(artwork) => Some(artwork),
                Err(error) => {
                    self.metadata_edit_error = Some(format!("Could not fetch artwork: {error}"));
                    return Ok(());
                }
            }
        };
        let artwork_path = downloaded_artwork
            .as_ref()
            .map(|artwork| artwork.path());

        match crate::audio::metadata::write_track_metadata(
            &track_path,
            &title,
            &artist,
            &album,
            artwork_path,
        ) {
            Ok(()) => {
                let old_artist = self.tracks[index].artist.clone();
                let old_album = self.tracks[index].album.clone();

                self.tracks[index].title = title;
                self.tracks[index].artist = artist.clone();
                self.tracks[index].album = album.clone();
                self.search_terms[index].update(&self.tracks[index]);

                if let Some(artwork_path) = artwork_path {
                    self.tracks[index].has_cover = true;
                    self.refresh_album_artwork(&album, artwork_path);
                    if self.current_index == Some(index) {
                        let _ = self.refresh_cover(index);
                    }
                }
                // The image is embedded now; remove the temporary download.
                drop(downloaded_artwork);

                if let LibraryScope::Album(name) = &mut self.album_scope {
                    if name == &old_album {
                        *name = album;
                    }
                }
                if let LibraryScope::Artist(name) = &mut self.artist_scope {
                    if name == &old_artist {
                        *name = artist;
                    }
                }

                self.cancel_metadata_edit();
                self.rebuild_albums_and_artists();
                self.sort_all_tracks();
                self.sort_scoped_tracks();
                self.sort_albums();
                self.sort_artists();
                self.rebuild_playlist_artwork();
                self.update_mpris_state();
                self.needs_full_redraw = true;
                Ok(())
            }
            Err(error) => {
                self.metadata_edit_error = Some(format!("Could not save tags: {error}"));
                Ok(())
            }
        }
    }

    /// Rewrite the cached artwork for an album after its embedded cover changes.
    fn refresh_album_artwork(&mut self, album: &str, artwork_path: &std::path::Path) {
        let key = format!("album:{album}");
        if self.artwork_overrides.contains_key(&key) {
            return;
        }
        let Ok(bytes) = std::fs::read(artwork_path) else {
            return;
        };
        if let Some(thumb) = crate::library::artwork::from_bytes(&key, &bytes) {
            self.artwork_protocols.insert(
                key.clone(),
                self.picker.new_resize_protocol(thumb.image()),
            );
            self.artwork.insert(key, thumb);
        }
    }

    pub fn open_selected_in_file_manager(&self) -> Result<()> {
        let index = match self.selected_track_index() {
            Some(index) => index,
            None => return Ok(()),
        };
        crate::file_manager::reveal(&self.tracks[index].path)?;
        Ok(())
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
            _none => return,
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
        if self.save_mode == false {
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
            _none => return Ok(()),
        };

        if let Some(&pi) = self.save_candidates.get(self.save_selected) {
            self.playlists[pi].add_track(track_path);
            self.playlists[pi].save()?;
        }
        self.save_mode = false;
        self.reorder_playlists();
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
            _none => return Ok(()),
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

        // Metadata rows are cheap once loaded from the cache, so consume a
        // larger batch and avoid a long "loading" tail on big libraries.
        const MAX_METADATA_PER_TICK: usize = 512;

        while count < MAX_METADATA_PER_TICK {
            match self.metadata_rx.try_recv() {
                Ok((i, track)) => {
                    self.search_terms[i].update(&track);
                    self.tracks[i] = track;
                    self.loaded_count += 1;
                    changed = true;
                    count += 1;
                }
                Err(_) => break,
            }
        }

        if self.loaded_count >= self.tracks.len() && self.loading {
            self.loading = false;
        }

        // Rebuild incrementally while metadata streams in, but not on every
        // small batch: about every 250 tracks, and once more at the end.
        let crossed_checkpoint = self.loaded_count % 250 < count;
        if changed && (!self.loading || crossed_checkpoint) {
            self.rebuild_albums_and_artists();
            self.sort_all_tracks();
            self.sort_scoped_tracks();
            self.sort_albums();
            self.sort_artists();
            self.rebuild_playlist_artwork();
        }
    }

    fn rebuild_playlist_artwork(&mut self) {
        let entries: Vec<(String, String)> = self
            .playlists
            .iter()
            .filter_map(|playlist| {
                let first = playlist.track_paths.first()?;
                let index = *self.track_indices.get(first)?;
                let track = self.tracks.get(index)?;
                Some((
                    format!("playlist:{}", playlist.name),
                    format!("album:{}", track.album),
                ))
            })
            .collect();
        for (playlist_key, album_key) in entries {
            if let Some(thumb) = self.artwork.get(&album_key).cloned() {
                if !self.artwork_overrides.contains_key(&playlist_key) {
                    self.artwork_protocols.insert(
                        playlist_key.clone(),
                        self.picker.new_resize_protocol(thumb.image()),
                    );
                    self.artwork.insert(playlist_key.clone(), thumb);
                }
                self.artwork_pending.remove(&playlist_key);
            }
        }
    }

    pub fn drain_artwork(&mut self) {
        let mut received_artwork = false;
        while let Ok((key, thumb)) = self.artwork_rx.try_recv() {
            if !self.artwork_overrides.contains_key(&key) {
                match thumb {
                    Some(thumb) => {
                        self.artwork_protocols.insert(
                            key.clone(),
                            self.picker.new_resize_protocol(thumb.image()),
                        );
                        self.artwork.insert(key.clone(), thumb);
                        self.artwork_missing.remove(&key);
                        if key.starts_with("album:") {
                            received_artwork = true;
                        }
                    }
                    None => {
                        self.artwork_missing.insert(key.clone());
                    }
                }
            }
            self.artwork_pending.remove(&key);
        }
        if received_artwork {
            self.rebuild_playlist_artwork();
        }
    }

    pub fn artwork_pending_count(&self) -> usize {
        self.artwork_pending.len()
    }

    fn request_artist_artwork(&mut self, artist: &str) {
        let key = format!("artist:{}", artist);
        if self.artwork.contains_key(&key) || self.artwork_pending.contains(&key) {
            return;
        }
        self.artwork_pending.insert(key.clone());
        let tx = self.artwork_tx.clone();
        let artist = artist.to_string();
        std::thread::spawn(move || {
            if let Some(thumb) = crate::library::artwork::load(&key) {
                let _ = tx.send((key, Some(thumb)));
                return;
            }
            let encoded: String = artist
                .bytes()
                .flat_map(|b| {
                    if b.is_ascii_alphanumeric() || b == b' ' {
                        vec![b as char]
                    } else {
                        format!("%{:02X}", b).chars().collect()
                    }
                })
                .collect();
            let result = ureq::get(&format!(
                "https://api.deezer.com/search/artist?q={}",
                encoded
            ))
            .timeout(std::time::Duration::from_secs(4))
            .call()
            .ok()
            .and_then(|r| r.into_json::<serde_json::Value>().ok())
            .and_then(|json| json["data"].as_array()?.first().cloned())
            .and_then(|artist| artist["picture_medium"].as_str().map(str::to_owned))
            .and_then(|url| {
                ureq::get(&url)
                    .timeout(std::time::Duration::from_secs(4))
                    .call()
                    .ok()
            })
            .and_then(|r| {
                let mut reader = r.into_reader();
                let mut bytes = Vec::new();
                std::io::Read::read_to_end(&mut reader, &mut bytes)
                    .ok()
                    .map(|_| bytes)
            });
            if let Some(bytes) = result {
                let thumb =
                    crate::library::artwork::from_bytes(&format!("artist:{}", artist), &bytes);
                let _ = tx.send((format!("artist:{}", artist), thumb));
            } else {
                let _ = tx.send((format!("artist:{}", artist), None));
            }
        });
    }

    /// Load artwork from the persistent cache, or extract it from the first
    /// track that has an embedded cover. Only visible rows call this, keeping
    /// startup and idle CPU low.
    fn request_album_artwork(&mut self, album: &str) {
        let key = format!("album:{album}");
        if self.artwork.contains_key(&key) || !self.artwork_pending.insert(key.clone()) {
            return;
        }
        if key == "album:" {
            self.artwork_pending.remove(&key);
            self.artwork_missing.insert(key);
            return;
        }

        let source = self.album_cover_paths.get(album).cloned();
        let tx = self.artwork_tx.clone();
        let album = album.to_string();
        std::thread::spawn(move || {
            let key = format!("album:{album}");
            let thumb = crate::library::artwork::load(&key).or_else(|| {
                let path = source?;
                let mut track = Track::new(path);
                let bytes = enrich_track(&mut track).ok().flatten()?;
                crate::library::artwork::from_bytes(&key, &bytes)
            });
            let _ = tx.send((key, thumb));
        });
    }

    fn request_playlist_artwork(&mut self, name: &str) {
        let key = format!("playlist:{name}");
        if self.artwork.contains_key(&key) || !self.artwork_pending.insert(key.clone()) {
            return;
        }

        let Some(playlist) = self.playlists.iter().find(|playlist| playlist.name == name) else {
            self.artwork_pending.remove(&key);
            return;
        };
        let mut source = None;
        for path in &playlist.track_paths {
            if let Some(&index) = self.track_indices.get(path) {
                if self.tracks[index].has_cover {
                    source = Some((self.tracks[index].path.clone(), self.tracks[index].album.clone()));
                    break;
                }
                if source.is_none() {
                    source = Some((self.tracks[index].path.clone(), self.tracks[index].album.clone()));
                }
            }
        }
        let Some((path, album)) = source else {
            self.artwork_pending.remove(&key);
            return;
        };

        let tx = self.artwork_tx.clone();
        std::thread::spawn(move || {
            let album_key = format!("album:{album}");
            let thumb = crate::library::artwork::load(&album_key).or_else(|| {
                let mut track = Track::new(path);
                let bytes = enrich_track(&mut track).ok().flatten()?;
                crate::library::artwork::from_bytes(&album_key, &bytes)
            });
            let _ = tx.send((key, thumb));
        });
    }

    /// Ensure artwork for a visible list key is requested at most once.
    pub fn ensure_artwork_requested(&mut self, key: &str) {
        if self.artwork.contains_key(key)
            || self.artwork_pending.contains(key)
            || self.artwork_missing.contains(key)
        {
            return;
        }
        if let Some(album) = key.strip_prefix("album:") {
            self.request_album_artwork(album);
        } else if let Some(artist) = key.strip_prefix("artist:") {
            self.request_artist_artwork(artist);
        } else if let Some(name) = key.strip_prefix("playlist:") {
            self.request_playlist_artwork(name);
        }
    }

    fn rebuild_albums_and_artists(&mut self) {
        let selected_album = self
            .album_selected
            .and_then(|i| self.albums.get(i))
            .cloned();
        let selected_artist = self
            .artist_selected
            .and_then(|i| self.artists.get(i))
            .cloned();
        let selected_artist_album = self
            .artist_scoped_album_selected
            .and_then(|i| self.scoped_albums.get(i))
            .cloned();
        let mut seen_albums = std::collections::HashSet::new();
        let mut seen_artists = std::collections::HashSet::new();
        let mut albums = Vec::new();
        let mut artists = Vec::new();
        let mut album_track_counts: HashMap<String, usize> = HashMap::new();
        let mut album_artist_map: HashMap<String, String> = HashMap::new();
        let mut artist_song_counts: HashMap<String, usize> = HashMap::new();
        let mut artist_albums: HashMap<String, std::collections::HashSet<String>> = HashMap::new();
        let mut album_cover_paths: HashMap<String, PathBuf> = HashMap::new();

        for t in &self.tracks {
            if seen_albums.insert(t.album.clone()) {
                albums.push(t.album.clone());
            }
            if seen_artists.insert(t.artist.clone()) {
                artists.push(t.artist.clone());
            }
            *album_track_counts.entry(t.album.clone()).or_insert(0) += 1;
            *artist_song_counts.entry(t.artist.clone()).or_insert(0) += 1;
            album_artist_map
                .entry(t.album.clone())
                .or_insert_with(|| t.artist.to_lowercase());
            artist_albums
                .entry(t.artist.clone())
                .or_default()
                .insert(t.album.clone());
            if t.has_cover {
                album_cover_paths
                    .entry(t.album.clone())
                    .or_insert_with(|| t.path.clone());
            }
        }

        albums.sort_unstable();
        artists.sort_unstable();

        self.albums = albums;
        self.artists = artists;
        self.album_track_counts = album_track_counts;
        self.album_artist_map = album_artist_map;
        self.artist_song_counts = artist_song_counts;
        self.album_cover_paths = album_cover_paths;
        self.artist_album_counts = artist_albums
            .into_iter()
            .map(|(artist, albums)| (artist, albums.len()))
            .collect();

        self.rebuild_album_cache();
        self.rebuild_artist_cache();
        self.album_selected = selected_album
            .and_then(|name| self.albums.iter().position(|item| item == &name));
        self.artist_selected = selected_artist
            .and_then(|name| self.artists.iter().position(|item| item == &name));
        self.artist_scoped_album_selected = selected_artist_album
            .and_then(|name| self.scoped_albums.iter().position(|item| item == &name));
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
        self.record_play(&path);
        let gain = self.tracks[index].replaygain;
        self.player.play_with_gain(&path, gain)?;
        self.refresh_cover(index)?;
        self.update_mpris_state();
        self.fetch_lyrics(index);
        Ok(())
    }

    fn record_play(&mut self, path: &Path) {
        self.history.retain(|item| item != path);
        self.history.insert(0, path.to_path_buf());
        self.history.truncate(100);
        *self.play_counts.entry(path.to_path_buf()).or_insert(0) += 1;
        self.refresh_generated_playlists();
    }

    fn refresh_generated_playlists(&mut self) {
        let history = self.history.clone();
        let mut most_played: Vec<PathBuf> = self.play_counts.keys().cloned().collect();
        most_played.sort_by(|a, b| {
            self.play_counts
                .get(b)
                .cmp(&self.play_counts.get(a))
                .then_with(|| a.cmp(b))
        });
        most_played.truncate(100);

        for playlist in &mut self.playlists {
            match playlist.name.as_str() {
                "History" => playlist.track_paths = history.clone(),
                "Most Played" => playlist.track_paths = most_played.clone(),
                _ => {}
            }
        }
        self.reorder_playlists();
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
        self.lyrics_scroll = None;
        self.lyrics_clicked_line = None;

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
                            self.album_return_artist = None;
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
                            self.album_return_artist = match &self.artist_scope {
                                LibraryScope::Artist(artist) => Some(artist.clone()),
                                _ => None,
                            };
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

    pub fn activate_selected(&mut self) -> Result<()> {
        let selected_track = match self.active_tab {
            LibraryTab::Songs => self
                .selected_index
                .and_then(|index| self.all_track_indices.get(index).copied()),
            LibraryTab::Albums => match &self.album_scope {
                LibraryScope::Album(_) => self
                    .scoped_song_selected
                    .and_then(|index| self.scoped_track_indices.get(index).copied()),
                _ => None,
            },
            LibraryTab::Playlists => match &self.playlist_scope {
                PlaylistScope::Open(playlist_index) => {
                    self.playlist_song_selected.and_then(|index| {
                        self.playlist_track_indices(*playlist_index)
                            .get(index)
                            .copied()
                    })
                }
                PlaylistScope::All => None,
            },
            LibraryTab::Artists => None,
        };

        if selected_track.is_some() && selected_track == self.current_index {
            self.player.toggle_pause();
            Ok(())
        } else {
            self.play_selected()
        }
    }

    pub fn seek_to_lyric(&mut self, time: std::time::Duration) -> Result<()> {
        let path = match self.current_track() {
            Some(track) => track.path.clone(),
            None => return Ok(()),
        };
        self.player.seek(&path, time)?;
        self.player.finish_seek()
    }

    pub fn move_selected_song(&mut self, to_bottom: bool) -> Result<()> {
        let (playlist_index, selected_index) = match &self.playlist_scope {
            PlaylistScope::Open(index) => (*index, self.playlist_song_selected),
            PlaylistScope::All => return Ok(()),
        };
        let selected_index = match selected_index {
            Some(index) => index,
            None => return Ok(()),
        };
        let playlist_name = self.playlists[playlist_index].name.to_lowercase();
        if matches!(
            playlist_name.as_str(),
            "favorites" | "history" | "most played"
        ) {
            return Ok(());
        }

        let track_indices = self.playlist_track_indices(playlist_index);
        let real_index = match track_indices.get(selected_index) {
            Some(index) => *index,
            None => return Ok(()),
        };
        let path = self.tracks[real_index].path.clone();
        let raw_index = match self.playlists[playlist_index]
            .track_paths
            .iter()
            .position(|item| item == &path)
        {
            Some(index) => index,
            None => return Ok(()),
        };
        let path = self.playlists[playlist_index].track_paths.remove(raw_index);
        let target = if to_bottom {
            self.playlists[playlist_index].track_paths.len()
        } else {
            0
        };
        self.playlists[playlist_index]
            .track_paths
            .insert(target, path);
        self.playlists[playlist_index].save()?;
        let sorted_indices = self.playlist_track_indices(playlist_index);
        self.playlist_song_selected = sorted_indices.iter().position(|&index| index == real_index);
        self.playlist_songs_offset = scroll_offset(
            self.playlist_song_selected,
            self.playlist_songs_offset,
            self.scoped_list_height,
        );
        Ok(())
    }

    pub fn toggle_play(&mut self) -> Result<()> {
        if self.current_index.is_none() {
            self.play_selected()?;
        } else {
            self.player.toggle_pause();
        }
        Ok(())
    }

    fn is_selection_in_sync(&self) -> bool {
        // If nothing is playing, always allow
        let current = match self.current_index {
            Some(c) => c,
            _none => return true,
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
            _none => return Ok(()),
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
            _none => return Ok(()),
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
        self.finish_seek_if_idle()?;
        self.drain_cover();
        self.drain_metadata();
        self.drain_artwork();
        self.drain_ipc()?;
        self.drain_mpris()?;
        self.update_mpris_state();
        // Check if lyrics arrived
        if let Some(ref rx) = self.lyrics_rx {
            if let Ok(result) = rx.try_recv() {
                self.lyrics_loading = false;
                self.lyrics_error = result.is_none();
                self.lyrics = result;
                self.lyrics_rx = None;
            }
        }
        // After clicking a lyric, keep the viewport still while that line is
        // current. Resume auto-following once playback moves to another line.
        if let Some(clicked_line) = self.lyrics_clicked_line {
            let current = self.lyrics.as_ref().and_then(|lyrics| {
                if lyrics.synced {
                    lyrics.current_line(self.player.elapsed())
                } else {
                    None
                }
            });
            if current != Some(clicked_line) {
                self.lyrics_clicked_line = None;
                self.lyrics_scroll = None;
            }
        }
        if self
            .notice
            .as_ref()
            .is_some_and(|(_, shown_at)| shown_at.elapsed() >= std::time::Duration::from_secs(4))
        {
            self.notice = None;
        }
        // A track change or the end of the song should not lose the
        // timestamps recorded so far.
        if self.lyrics_sync_mode
            && (self.player.is_finished() || self.current_index != self.lyrics_sync_track)
        {
            self.finish_lyrics_sync(false)?;
        }
        if self.current_index.is_some() && self.player.is_finished() {
            match self.repeat {
                RepeatMode::One => {
                    // Replay same track
                    if let Some(idx) = self.current_index {
                        let path = self.tracks[idx].path.clone();
                        self.record_play(&path);
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
                    _none => 0,
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
                        _none => 0,
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
                    _none => 0,
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
                        _none => 0,
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

    /// Start loading cover art on a worker thread so playback and the first
    /// frame are never blocked by decoding a large embedded image.
    fn refresh_cover(&mut self, index: usize) -> Result<()> {
        self.cover_image = None;
        crate::ui::theme::clear_dynamic_colors();

        let path = match self.tracks.get(index) {
            Some(track) => track.path.clone(),
            None => return Ok(()),
        };
        let tx = self.cover_tx.clone();
        std::thread::spawn(move || {
            let mut track = Track::new(path);
            let bytes = enrich_track(&mut track).ok().flatten();
            let image = bytes.and_then(|bytes| decode_cover(&bytes).ok()).map(|image| {
                // Terminal covers never need the full camera-resolution image.
                const MAX_COVER_DIMENSION: u32 = 1024;
                if image.width() > MAX_COVER_DIMENSION || image.height() > MAX_COVER_DIMENSION {
                    image.resize(
                        MAX_COVER_DIMENSION,
                        MAX_COVER_DIMENSION,
                        image::imageops::FilterType::Triangle,
                    )
                } else {
                    image
                }
            });
            let _ = tx.send((index, image));
        });
        Ok(())
    }

    fn drain_cover(&mut self) {
        while let Ok((index, image)) = self.cover_rx.try_recv() {
            if self.current_index != Some(index) {
                continue;
            }

            let image = image.or_else(|| self.fallback_cover.clone());
            if let Some(ref image) = image {
                if self.dynamic_theme {
                    let colors = crate::ui::dynamic_theme::extract_colors(
                        image,
                        &self.dynamic_theme_style,
                    );
                    crate::ui::theme::set_dynamic_colors(colors);
                }
            } else {
                crate::ui::theme::clear_dynamic_colors();
            }
            self.cover_image = image.map(|image| self.picker.new_resize_protocol(image));
        }
    }

    pub fn tab_next(&mut self) {
        let tabs = LibraryTab::all();
        let next = (self.active_tab.index() + 1) % tabs.len();
        self.active_tab = tabs.get(next).unwrap().clone();
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
        self.active_tab = tabs.get(prev).unwrap().clone();
        self.ensure_selection();
    }

    pub fn select_tab(&mut self, tab: LibraryTab) {
        self.active_tab = tab;
        self.ensure_selection();
    }

    pub fn is_double_mouse_click(&mut self, x: u16, y: u16) -> bool {
        let now = std::time::Instant::now();
        let is_double = self
            .last_mouse_click
            .map(|(last_x, last_y, last_time)| {
                last_x == x && last_y == y && last_time.elapsed().as_millis() <= 400
            })
            .unwrap_or(false);
        self.last_mouse_click = Some((x, y, now));
        is_double
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
        self.album_track_counts.get(album).copied().unwrap_or(0)
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
                let return_artist = self.album_return_artist.take();
                let album_name = match &self.album_scope {
                    LibraryScope::Album(album) => Some(album.clone()),
                    _ => None,
                };
                self.album_scope = LibraryScope::All;
                self.rebuild_album_cache();
                self.scoped_songs_offset = 0;
                if let Some(artist) = return_artist {
                    self.active_tab = LibraryTab::Artists;
                    self.artist_scope = LibraryScope::Artist(artist);
                    self.rebuild_artist_cache();
                    self.artist_scoped_album_selected = album_name
                        .and_then(|album| self.scoped_albums.iter().position(|item| item == &album));
                    self.artist_albums_offset = 0;
                }
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
            _none => return Ok(()),
        };
        if let Some(fav) = self
            .playlists
            .iter_mut()
            .find(|playlist| playlist.name.eq_ignore_ascii_case("favorites"))
        {
            if let Some(index) = fav.track_paths.iter().position(|path| path == &track_path) {
                fav.remove_track(index);
            } else {
                fav.add_track(track_path);
            }
            fav.save()?;
        }
        self.reorder_playlists();
        Ok(())
    }

    pub fn is_favorite(&self, path: &Path) -> bool {
        self.playlists.iter().any(|playlist| {
            playlist.name.eq_ignore_ascii_case("favorites")
                && playlist
                    .track_paths
                    .iter()
                    .any(|track_path| track_path == path)
        })
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

    pub fn update_seek(&mut self, forward: bool) {
        if let Some(track) = self.current_track() {
            self.player.update_seek(forward, Some(track.duration));
        }
    }

    pub fn finish_seek(&mut self) -> Result<()> {
        self.player.finish_seek()
    }

    pub fn finish_seek_if_idle(&mut self) -> Result<()> {
        self.player
            .finish_seek_if_idle(std::time::Duration::from_millis(180))
    }

    pub fn toggle_shuffle(&mut self) {
        self.shuffle = if self.shuffle { false } else { true };
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

        for (i, terms) in self.search_terms.iter().enumerate() {
            if terms.title.contains(&q) || terms.artist.contains(&q) || terms.album.contains(&q) {
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
        let mut listed: std::collections::HashSet<usize> = self
            .search_results
            .iter()
            .filter_map(|result| match result {
                SearchResult::Track(index) => Some(*index),
                _ => None,
            })
            .collect();
        for (pi, playlist) in self.playlists.iter().enumerate() {
            for path in &playlist.track_paths {
                let Some(&ti) = self.track_indices.get(path) else {
                    continue;
                };
                let terms = &self.search_terms[ti];
                if (terms.title.contains(&q) || terms.artist.contains(&q))
                    && listed.insert(ti)
                {
                    self.search_results.push(SearchResult::PlaylistTrack {
                        playlist_idx: pi,
                        track_idx: ti,
                    });
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
            _none => 0,
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
            _none => 0,
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
            _none => return Ok(()),
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
                self.selected_index = self.all_track_indices.iter().position(|&i| i == ti);
                self.songs_offset =
                    scroll_offset(self.selected_index, self.songs_offset, self.list_height);
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
            _none => {}
        }
        Ok(())
    }

    pub fn go_to_playing(&mut self) {
        let ci = match self.current_index {
            Some(i) => i,
            _none => return,
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
                self.selected_index = self.all_track_indices.iter().position(|&i| i == ci);
                self.songs_offset =
                    get_new_offset(
                        self.songs_offset,
                        self.selected_index.unwrap_or(0),
                        self.list_height,
                        self.all_track_indices.len(),
                    );
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
            _none => return Ok(()),
        };

        let lrc_path = crate::lyrics::manual_lyrics_path(&track.path);

        // If no manual file exists yet, create a template
        if !lrc_path.exists() {
            let template = build_lrc_template(track);
            std::fs::write(&lrc_path, template)?;
        }

        // Suspend TUI. Mouse capture must be released too, otherwise mouse
        // movement is sent to the editor as escape sequences.
        crossterm::terminal::disable_raw_mode()?;
        crossterm::execute!(
            std::io::stdout(),
            crossterm::event::DisableMouseCapture,
            crossterm::terminal::LeaveAlternateScreen
        )?;

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
        let editor_result = std::process::Command::new(&editor)
            .arg(&lrc_path)
            .status();

        // Restore the TUI even if the editor could not be started.
        crossterm::terminal::enable_raw_mode()?;
        crossterm::execute!(
            std::io::stdout(),
            crossterm::terminal::EnterAlternateScreen,
            crossterm::event::EnableMouseCapture,
            crossterm::terminal::Clear(crossterm::terminal::ClearType::All)
        )?;
        self.needs_full_redraw = true;
        // Reload lyrics from the edited file
        if let Some(idx) = self.current_index {
            self.fetch_lyrics(idx);
        }
        if let Err(error) = editor_result {
            self.show_notice(format!("Could not open editor: {error}"));
        }

        Ok(())
    }

    /// Show a short-lived message at the bottom of the screen.
    pub fn show_notice(&mut self, message: impl Into<String>) {
        self.notice = Some((message.into(), std::time::Instant::now()));
    }

    /// Start synchronizing the current song's lyrics. Plain lyrics start with
    /// no timestamps; already-synchronized lyrics keep their existing times so
    /// an interrupted re-sync does not discard them.
    pub fn start_lyrics_sync(&mut self) {
        if self.lyrics_sync_mode {
            return;
        }
        let index = match self.current_index {
            Some(index) => index,
            None => {
                self.show_notice("No song is playing");
                return;
            }
        };
        if self.lyrics_loading {
            self.show_notice("Lyrics are still loading");
            return;
        }
        let lyrics = match self.lyrics.as_ref() {
            Some(lyrics) if !lyrics.lines.is_empty() => lyrics,
            _ => {
                self.show_notice("No lyrics available for this song");
                return;
            }
        };

        self.lyrics_sync_lines = lyrics.to_sync_lines();
        self.lyrics_sync_index = Some(0);
        self.lyrics_sync_track = Some(index);
        self.lyrics_sync_mode = true;
        self.lyrics_scroll = None;
        self.notice = None;
    }

    /// Assign the current playback position to the selected line and move to
    /// the next line. Finishing the last line saves automatically.
    pub fn stamp_lyrics_sync_line(&mut self) -> Result<()> {
        if !self.lyrics_sync_mode {
            return Ok(());
        }
        let index = self.lyrics_sync_index.unwrap_or(0);
        if index >= self.lyrics_sync_lines.len() {
            return self.finish_lyrics_sync(false);
        }

        let timestamp = self.player.elapsed();
        self.lyrics_sync_lines[index].time = Some(timestamp);

        // If these lyrics already had timestamps, keep the file monotonic:
        // later lines whose old timestamp is no longer after this one are
        // cleared and can be stamped again.
        for line in self.lyrics_sync_lines.iter_mut().skip(index + 1) {
            if line.time.is_some_and(|time| time <= timestamp) {
                line.time = None;
            }
        }

        if index + 1 < self.lyrics_sync_lines.len() {
            self.lyrics_sync_index = Some(index + 1);
        } else {
            self.finish_lyrics_sync(true)?;
        }
        Ok(())
    }

    /// Exit sync mode and write every timestamp recorded so far to the manual
    /// lyrics file. If nothing was stamped, the existing lyrics are untouched.
    pub fn finish_lyrics_sync(&mut self, completed: bool) -> Result<()> {
        if !self.lyrics_sync_mode {
            return Ok(());
        }

        let track_index = self.lyrics_sync_track.or(self.current_index);
        let total = self.lyrics_sync_lines.len();
        let mut saved = 0usize;

        if let Some(index) = track_index {
            if let Some(track) = self.tracks.get(index) {
                match crate::lyrics::save_synced(track, &self.lyrics_sync_lines) {
                    Ok(count) => saved = count,
                    Err(error) => {
                        // Keep the draft alive so the timestamps are not lost.
                        self.show_notice(format!("Could not save lyrics: {error}"));
                        return Ok(());
                    }
                }
            }
        }

        self.lyrics_sync_mode = false;
        self.lyrics_sync_index = None;
        self.lyrics_sync_track = None;
        self.lyrics_sync_lines.clear();

        if saved == 0 {
            self.show_notice("Sync cancelled; no timestamps recorded");
            return Ok(());
        }

        if completed && saved == total {
            self.show_notice(format!("Synchronized lyrics saved ({saved} lines)"));
        } else {
            self.show_notice(format!("Saved {saved} timestamps; sync exited"));
        }

        if track_index == self.current_index {
            if let Some(index) = track_index {
                self.fetch_lyrics(index);
            }
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
    let pic = tag
        .pictures()
        .iter()
        .find(|pic| pic.pic_type() == lofty::picture::PictureType::CoverFront)
        .or_else(|| tag.pictures().first())?;

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
        _none => return current_offset,
    };
    if sel < current_offset {
        sel
    } else if sel >= current_offset + visible_height {
        sel + 1 - visible_height
    } else {
        current_offset
    }
}

fn sort_track_indices(indices: &mut [usize], tracks: &[Track], mode: SongSort) {
    match mode {
        SongSort::LastAdded => indices.sort_by(|&a, &b| b.cmp(&a)),
        SongSort::FirstAdded => indices.sort_by(|&a, &b| a.cmp(&b)),
        SongSort::Duration => indices.sort_by(|&a, &b| {
            let left = &tracks[a];
            let right = &tracks[b];
            right
                .duration
                .cmp(&left.duration)
                .then_with(|| left.title.cmp(&right.title))
        }),
        SongSort::Title | SongSort::Artist | SongSort::Album => {
            // Lower-case keys are computed once per track instead of on every
            // comparison, which is a big win for libraries with many tracks.
            let mut keyed: Vec<(String, &str, usize)> = indices
                .iter()
                .map(|&index| {
                    let track = &tracks[index];
                    let key = match mode {
                        SongSort::Title => track.title.to_lowercase(),
                        SongSort::Artist => track.artist.to_lowercase(),
                        SongSort::Album => track.album.to_lowercase(),
                        _ => unreachable!(),
                    };
                    let tie = if mode == SongSort::Title {
                        ""
                    } else {
                        track.title.as_str()
                    };
                    (key, tie, index)
                })
                .collect();
            keyed.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(b.1)));
            for (slot, (_, _, index)) in indices.iter_mut().zip(keyed) {
                *slot = index;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{sort_track_indices, SongSort};
    use crate::library::track::Track;
    use std::path::PathBuf;

    #[test]
    fn last_added_sort_reverses_library_scan_order() {
        let tracks = (0..3)
            .map(|index| Track::new(PathBuf::from(format!("track-{index}.mp3"))))
            .collect::<Vec<_>>();
        let mut indices = vec![0, 1, 2];

        sort_track_indices(&mut indices, &tracks, SongSort::LastAdded);

        assert_eq!(indices, vec![2, 1, 0]);
    }

    #[test]
    fn first_added_sort_uses_library_scan_order() {
        let tracks = (0..3)
            .map(|index| Track::new(PathBuf::from(format!("track-{index}.mp3"))))
            .collect::<Vec<_>>();
        let mut indices = vec![2, 1, 0];

        sort_track_indices(&mut indices, &tracks, SongSort::FirstAdded);

        assert_eq!(indices, vec![0, 1, 2]);
    }

    #[test]
    fn title_sort_ignores_case() {
        let mut tracks = (0..3)
            .map(|index| Track::new(PathBuf::from(format!("track-{index}.mp3"))))
            .collect::<Vec<_>>();
        tracks[0].title = "beta".into();
        tracks[1].title = "Alpha".into();
        tracks[2].title = "gamma".into();
        let mut indices = vec![0, 1, 2];

        sort_track_indices(&mut indices, &tracks, SongSort::Title);

        assert_eq!(indices, vec![1, 0, 2]);
    }
}

fn char_index_to_byte(value: &str, char_index: usize) -> usize {
    value
        .char_indices()
        .nth(char_index)
        .map(|(index, _)| index)
        .unwrap_or(value.len())
}

fn trimmed_or(value: &str, fallback: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        fallback.to_string()
    } else {
        value.to_string()
    }
}

pub fn center_offset(selected: Option<usize>, visible_height: usize) -> usize {
    let sel = match selected {
        Some(s) => s,
        _none => return 0,
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
        _none => 0,
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
        _none => 0,
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
