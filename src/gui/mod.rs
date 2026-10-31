#[allow(dead_code)]
use crate::app::{App, PlayContext, RepeatMode, SearchResult, SongSort};
use crate::cli::{self, CliAction};
use crate::config::{Config, State};
use crate::library::{LibraryScope, LibraryTab, PlaylistScope};
use anyhow::{Context, Result};
use eframe::egui;
use image::DynamicImage;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime};

const GRID_CARD_MIN_WIDTH: f32 = 190.0;
const GRID_CARD_FOOTER: f32 = 10.0;
const GRID_SPACING: f32 = 14.0;
const WAVEFORM_BAR_COUNT: usize = 240;
/// Below this width the song table drops the Artist and Album columns so the
/// title and duration keep readable space.
const SONG_METADATA_MIN_WIDTH: f32 = 500.0;
/// Minimum remaining height needed to show the Up next section in the Now
/// playing panel. Below this the section is hidden to avoid squeezing playback.
const UP_NEXT_MIN_HEIGHT: f32 = 124.0;
/// Releasing the Now playing panel below this width collapses it, mirroring
/// the snap-to-collapse behaviour of VS Code's side bars.
const NOW_PLAYING_COLLAPSE_THRESHOLD: f32 = 280.0;

const LYRICS_COLLAPSE_THRESHOLD: f32 = 340.0;
const LYRICS_TAKE_OVER_THRESHOLD: f32 = 0.45;
const LYRICS_PANEL_FILL_ALPHA: u8 = 120;
/// Allow only a short travel past a snap threshold before the divider stops.
const PANEL_SNAP_OVERSHOOT: f32 = 12.0;
/// Wash drawn over a panel while releasing the divider would collapse it.
const PANEL_COLLAPSE_DIM_ALPHA: u8 = 110;

#[derive(Clone, Copy)]
struct Palette {
    background: egui::Color32,
    panel: egui::Color32,
    raised: egui::Color32,
    text: egui::Color32,
    muted: egui::Color32,
    border: egui::Color32,
}

impl Palette {
    fn for_theme(dark: bool, theme: &str) -> Self {
        // Keep surfaces neutral so album-art accents feel intentional rather than muddy.
        if theme == "oled" {
            Self {
                background: egui::Color32::BLACK,
                panel: egui::Color32::from_rgb(10, 12, 16),
                raised: egui::Color32::from_rgb(22, 25, 32),
                text: egui::Color32::from_rgb(245, 247, 251),
                muted: egui::Color32::from_rgb(155, 164, 180),
                border: egui::Color32::from_rgb(39, 44, 55),
            }
        } else if dark {
            Self {
                background: egui::Color32::from_rgb(13, 15, 21),
                panel: egui::Color32::from_rgb(20, 23, 31),
                raised: egui::Color32::from_rgb(30, 35, 46),
                text: egui::Color32::from_rgb(239, 242, 248),
                muted: egui::Color32::from_rgb(157, 166, 183),
                border: egui::Color32::from_rgb(43, 49, 63),
            }
        } else {
            Self {
                background: egui::Color32::from_rgb(245, 247, 250),
                panel: egui::Color32::from_rgb(255, 255, 255),
                raised: egui::Color32::from_rgb(235, 239, 245),
                text: egui::Color32::from_rgb(27, 32, 43),
                muted: egui::Color32::from_rgb(101, 110, 127),
                border: egui::Color32::from_rgb(220, 224, 233),
            }
        }
    }
}

fn layout_visibility(mode: &str) -> (bool, bool, bool) {
    let base = mode.strip_suffix("_lyrics").unwrap_or(mode);
    match base {
        "clean" => (true, false, true),
        "compact" => (false, true, true),
        "minimal" => (false, true, false),
        _ => (true, true, true),
    }
}

fn playing_row_scroll_offset(
    row: usize,
    row_height: f32,
    row_spacing: f32,
    row_count: usize,
    viewport_height: f32,
) -> f32 {
    let row_step = row_height + row_spacing;
    let content_height = (row_count as f32 * row_step - row_spacing).max(0.0);
    let max_offset = (content_height - viewport_height).max(0.0);
    let centered_offset = row as f32 * row_step - (viewport_height - row_height) * 0.5;
    centered_offset.clamp(0.0, max_offset)
}

/// How long the wheel's momentum takes to bleed off. Larger feels floatier.
const MOMENTUM_GLIDE_SECONDS: f32 = 0.30;
/// Fraction of a wheel notch that is replayed as extra gliding distance.
const MOMENTUM_GAIN: f32 = 1.9;
/// How long a trackpad flick keeps coasting. Larger glides further.
const TOUCHPAD_FRICTION_SECONDS: f32 = 0.45;
/// Cap on flick speed in points per second.
const TOUCHPAD_MAX_VELOCITY: f32 = 7000.0;
/// Below this speed the coast ends.
const TOUCHPAD_STOP_VELOCITY: f32 = 25.0;
/// No scroll events for this long means the fingers have lifted.
const TOUCHPAD_COAST_START: Duration = Duration::from_millis(40);
/// A pause longer than this before lift-off cancels the flick.
const TOUCHPAD_COAST_WINDOW: Duration = Duration::from_millis(250);
/// Momentum beyond this many points is dropped so a flick cannot fly away.
const MOMENTUM_MAX: f32 = 1600.0;
/// Below this many points the glide is over.
const MOMENTUM_STOP: f32 = 0.35;

/// Distance to add this frame so the remaining momentum decays exponentially.
fn momentum_step(remaining: f32, dt: f32) -> f32 {
    if dt <= 0.0 || remaining == 0.0 {
        return 0.0;
    }
    let glide = 1.0 - (-dt / MOMENTUM_GLIDE_SECONDS).exp();
    remaining * glide
}

/// Tracks the snap-to-collapse gesture shared by the resizable side panels.
///
/// While the divider is dragged into its collapse zone the panel is *armed*.
/// The layout changes only on release; moving back out disarms it again.
#[derive(Debug, Default)]
struct PanelCollapse {
    armed: bool,
}

impl PanelCollapse {
    /// Feed one frame of resize state into the gesture for collapsing when width < threshold.
    fn update_collapse_under(
        &mut self,
        dragging: bool,
        drag_stopped: bool,
        width: f32,
        threshold: f32,
    ) -> bool {
        if dragging {
            self.armed = width < threshold;
            false
        } else {
            self.armed = false;
            drag_stopped && width < threshold
        }
    }

    /// Feed one frame of resize state into the gesture for collapsing when width > threshold.
    fn update_collapse_over(
        &mut self,
        dragging: bool,
        drag_stopped: bool,
        width: f32,
        threshold: f32,
    ) -> bool {
        if dragging {
            self.armed = width > threshold;
            false
        } else {
            self.armed = false;
            drag_stopped && width > threshold
        }
    }

    fn is_armed(&self) -> bool {
        self.armed
    }

    fn cancel(&mut self) {
        self.armed = false;
    }
}

/// Width of a side panel when its divider is dragged to `pointer_x`.
fn panel_width_at_pointer(edge_x: f32, pointer_x: f32) -> f32 {
    (pointer_x - edge_x).abs()
}

/// Current drag width, preferring the live pointer position so a release in
/// the same frame as the final movement is measured correctly.
fn live_panel_width(resize: Option<&egui::Response>, edge_x: f32, fallback: f32) -> f32 {
    resize
        .filter(|response| response.dragged() || response.drag_stopped())
        .and_then(|response| response.interact_pointer_pos())
        .map_or(fallback, |pointer| panel_width_at_pointer(edge_x, pointer.x))
}

fn responsive_grid_layout(available_width: f32) -> (usize, f32) {
    let columns = ((available_width + GRID_SPACING) / (GRID_CARD_MIN_WIDTH + GRID_SPACING))
        .floor()
        .max(1.0) as usize;
    let card_width = ((available_width - GRID_SPACING * columns.saturating_sub(1) as f32)
        / columns as f32)
        .max(1.0);
    (columns, card_width)
}

fn visible_album_entries(
    albums: &[String],
    hide_single_track: bool,
    mut track_count: impl FnMut(&str) -> usize,
) -> Vec<(usize, String)> {
    albums
        .iter()
        .enumerate()
        .filter(|(_, album)| !hide_single_track || track_count(album) > 1)
        .map(|(index, album)| (index, album.clone()))
        .collect()
}

/// Fingerprint of every directory in the music folder. Adding or removing a
/// file updates its parent directory's mtime, which tells the GUI when a
/// library rescan is needed.
fn directory_stamp(root: &Path) -> Vec<(PathBuf, Option<SystemTime>)> {
    let mut stamp: Vec<(PathBuf, Option<SystemTime>)> = walkdir::WalkDir::new(root)
        .follow_links(true)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_dir())
        .map(|entry| {
            (
                entry.path().to_path_buf(),
                entry
                    .metadata()
                    .ok()
                    .and_then(|metadata| metadata.modified().ok()),
            )
        })
        .collect();
    stamp.sort_by(|a, b| a.0.cmp(&b.0));
    stamp
}

fn centered_image_uv(image_size: egui::Vec2, target_size: egui::Vec2) -> egui::Rect {
    let image_aspect = image_size.x / image_size.y.max(1.0);
    let target_aspect = target_size.x / target_size.y.max(1.0);
    if image_aspect > target_aspect {
        let visible_width = target_aspect / image_aspect;
        egui::Rect::from_min_max(
            egui::pos2((1.0 - visible_width) * 0.5, 0.0),
            egui::pos2((1.0 + visible_width) * 0.5, 1.0),
        )
    } else {
        let visible_height = image_aspect / target_aspect;
        egui::Rect::from_min_max(
            egui::pos2(0.0, (1.0 - visible_height) * 0.5),
            egui::pos2(1.0, (1.0 + visible_height) * 0.5),
        )
    }
}

fn with_alpha(color: egui::Color32, alpha: u8) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha)
}
/// A lighter version of the accent, readable on top of accent-tinted backgrounds.
fn bright_accent(accent: egui::Color32) -> egui::Color32 {
    accent.lerp_to_gamma(egui::Color32::WHITE, 0.45)
}

/// Returns a fuzzy subsequence score. Higher scores are better; None means
/// at least one query character could not be matched in order.
fn fuzzy_match_score(candidate: &str, query: &str) -> Option<i64> {
    let candidate = candidate.to_lowercase();
    let query = query.to_lowercase();
    let query = query.trim();
    if query.is_empty() {
        return Some(0);
    }
    if let Some(position) = candidate.find(query) {
        return Some(10_000 - position as i64 * 3 - (candidate.len() - query.len()) as i64);
    }

    let mut cursor = 0usize;
    let mut previous = None;
    let mut score = 0i64;
    for needle in query.chars() {
        let relative = candidate[cursor..].find(needle)?;
        let position = cursor + relative;
        score += 100 - relative as i64 * 5;
        if previous == Some(position.saturating_sub(1)) {
            score += 18;
        }
        if position == 0
            || candidate[..position]
                .chars()
                .last()
                .is_some_and(|ch| ch.is_whitespace() || "-_.(".contains(ch))
        {
            score += 12;
        }
        previous = Some(position);
        cursor = position + needle.len_utf8();
    }
    Some(score - candidate.len() as i64 / 8)
}

/// A compact egui layout job that highlights the characters matched by the
/// same subsequence rule used to rank search results.
fn fuzzy_highlight_job(
    text: &str,
    query: &str,
    color: egui::Color32,
    accent: egui::Color32,
    size: f32,
    strong: bool,
) -> egui::text::LayoutJob {
    let query = query.to_lowercase();
    let mut matched = HashSet::new();
    let lower = text.to_lowercase();
    let mut cursor = 0usize;
    for needle in query.chars().filter(|ch| !ch.is_whitespace()) {
        if let Some(relative) = lower.get(cursor..).and_then(|tail| tail.find(needle)) {
            let position = cursor + relative;
            matched.insert(position);
            cursor = position + needle.len_utf8();
        }
    }

    let mut job = egui::text::LayoutJob::default();
    for (position, ch) in text.char_indices() {
        let highlighted = matched.contains(&position);
        let mut format = egui::TextFormat {
            font_id: egui::FontId::proportional(size),
            color: if highlighted { accent } else { color },
            background: if highlighted {
                accent.gamma_multiply(0.20)
            } else {
                egui::Color32::TRANSPARENT
            },
            ..Default::default()
        };
        if strong || highlighted {
            format.italics = false;
            format.font_id = egui::FontId::proportional(size);
        }
        job.append(&ch.to_string(), 0.0, format);
    }
    job
}

fn waveform_peaks(samples: &[f32], bar_count: usize) -> Vec<f32> {
    if samples.is_empty() || bar_count == 0 {
        return Vec::new();
    }

    let mut peaks = (0..bar_count)
        .map(|bar| {
            let start = bar * samples.len() / bar_count;
            let end = ((bar + 1) * samples.len() / bar_count).max(start + 1);
            let samples = samples[start..end.min(samples.len())]
                .iter()
                .filter(|sample| sample.is_finite())
                .map(|sample| sample.abs().clamp(0.0, 1.0))
                .collect::<Vec<_>>();
            if samples.is_empty() {
                return 0.0;
            }

            let peak = samples.iter().copied().fold(0.0_f32, f32::max);
            let rms = (samples.iter().map(|sample| sample * sample).sum::<f32>()
                / samples.len() as f32)
                .sqrt();
            rms * 0.72 + peak * 0.28
        })
        .collect::<Vec<_>>();

    let mut sorted = peaks.clone();
    sorted.sort_by(f32::total_cmp);
    let reference = sorted[(sorted.len() - 1) * 9 / 10].max(f32::EPSILON);
    peaks.iter_mut().for_each(|peak| {
        *peak = (*peak / reference).clamp(0.0, 1.0).powf(0.78);
    });
    peaks
}

#[derive(Clone, Copy)]
enum Icon {
    Settings,
    Sun,
    Moon,
    Songs,
    Albums,
    Artists,
    Playlists,
    Play,
    Pause,
    Previous,
    Next,
    Shuffle,
    Repeat,
    Volume,
    VolumeMuted,
    Queue,
    Lyrics,
    Close,
    Up,
    Down,
    Artwork,
    Add,
    Heart,
}

#[derive(Clone)]
enum EntityLinkTarget {
    Album {
        album: String,
        artist: Option<String>,
    },
    Artist(String),
}

struct EntityItemResponse {
    item: egui::Response,
    activated: Option<EntityLinkTarget>,
}

struct SongRowResponse {
    row: egui::Response,
    activated: Option<EntityLinkTarget>,
    queue: bool,
}

fn icon_button(ui: &mut egui::Ui, icon: Icon, tooltip: &str) -> egui::Response {
    let response = ui
        .add_sized(
            [34.0, 34.0],
            egui::Button::new("").corner_radius(egui::CornerRadius::same(17)),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(tooltip);
    let rect = response.rect.shrink(8.0);
    let color = if response.hovered() {
        ui.visuals().widgets.hovered.fg_stroke.color
    } else {
        ui.visuals().text_color()
    };
    paint_icon(ui.painter(), icon, rect, color);
    response
}
fn icon_button_enabled(
    ui: &mut egui::Ui,
    icon: Icon,
    tooltip: &str,
    enabled: bool,
) -> egui::Response {
    ui.add_enabled_ui(enabled, |ui| icon_button(ui, icon, tooltip))
        .inner
}
fn small_remove_button(ui: &mut egui::Ui, tooltip: &str) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());
    let response = response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(tooltip);
    let red = egui::Color32::from_rgb(237, 105, 105);
    let color = if response.hovered() {
        red
    } else {
        ui.visuals().weak_text_color()
    };
    if response.hovered() {
        ui.painter()
            .circle_filled(rect.center(), 10.0, with_alpha(red, 36));
    }
    paint_icon(ui.painter(), Icon::Close, rect.shrink(5.0), color);
    response
}
fn icon_toggle_button(
    ui: &mut egui::Ui,
    icon: Icon,
    tooltip: &str,
    active: bool,
) -> egui::Response {
    let response = ui
        .add_sized(
            [34.0, 34.0],
            egui::Button::new("")
                .selected(active)
                .corner_radius(egui::CornerRadius::same(17)),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(tooltip);
    let rect = response.rect.shrink(8.0);
    let color = if active || response.hovered() {
        ui.visuals().widgets.hovered.fg_stroke.color
    } else {
        ui.visuals().text_color()
    };
    paint_icon(ui.painter(), icon, rect, color);
    response
}

fn navigation_tab(
    ui: &mut egui::Ui,
    icon: Icon,
    label: &str,
    active: bool,
    compact: bool,
) -> egui::Response {
    let size = if compact {
        egui::vec2(40.0, 34.0)
    } else {
        egui::vec2(92.0, 36.0)
    };
    let response = ui
        .add_sized(
            size,
            egui::Button::new("")
                .selected(active)
                .corner_radius(egui::CornerRadius::same(12)),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(label);
    let rect = response.rect;
    let color = if active {
        ui.visuals().selection.stroke.color
    } else if response.hovered() {
        ui.visuals().widgets.hovered.fg_stroke.color
    } else {
        ui.visuals().text_color()
    };
    let icon_rect = if compact {
        egui::Rect::from_center_size(rect.center(), egui::vec2(16.0, 16.0))
    } else {
        egui::Rect::from_min_size(
            rect.left_top() + egui::vec2(12.0, (rect.height() - 14.0) * 0.5),
            egui::vec2(14.0, 14.0),
        )
    };
    paint_icon(ui.painter(), icon, icon_rect, color);
    if !compact {
        ui.painter().text(
            rect.left_center() + egui::vec2(34.0, 0.0),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(14.0),
            color,
        );
    }
    response
}

fn paint_favorite_badge(
    painter: &egui::Painter,
    image_rect: egui::Rect,
    background: egui::Color32,
    size: f32,
) {
    let center = egui::pos2(
        image_rect.right() - size * 0.5 - 6.0,
        image_rect.top() + size * 0.5 + 6.0,
    );
    let circle = egui::Rect::from_center_size(center, egui::vec2(size, size));
    painter.circle_filled(circle.center(), size * 0.5, with_alpha(background, 200));
    paint_filled_heart(
        painter,
        circle.shrink(size * 0.22),
        egui::Color32::from_rgb(221, 100, 120),
    );
}

/// Icons are Phosphor Icons (MIT) rendered from the `egui-phosphor` font.
fn paint_icon(painter: &egui::Painter, icon: Icon, rect: egui::Rect, color: egui::Color32) {
    use egui_phosphor::regular as p;
    let glyph = match icon {
        Icon::Settings => p::GEAR_SIX,
        Icon::Sun => p::SUN,
        Icon::Moon => p::MOON,
        Icon::Songs => p::MUSIC_NOTES,
        Icon::Albums => p::VINYL_RECORD,
        Icon::Artists => p::MICROPHONE_STAGE,
        Icon::Playlists => p::PLAYLIST,
        Icon::Queue => p::QUEUE,
        Icon::Play => p::PLAY,
        Icon::Pause => p::PAUSE,
        Icon::Previous => p::SKIP_BACK,
        Icon::Next => p::SKIP_FORWARD,
        Icon::Shuffle => p::SHUFFLE,
        Icon::Repeat => p::REPEAT,
        Icon::Volume => p::SPEAKER_HIGH,
        Icon::VolumeMuted => p::SPEAKER_SLASH,
        Icon::Lyrics => p::TEXT_ALIGN_LEFT,
        Icon::Close => p::X,
        Icon::Up => p::CARET_UP,
        Icon::Down => p::CARET_DOWN,
        Icon::Artwork => p::IMAGE,
        Icon::Add => p::PLUS,
        Icon::Heart => p::HEART,
    };
    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        glyph,
        egui::FontId::proportional(rect.width().max(rect.height()) + 4.0),
        color,
    );
}

pub fn run() -> Result<()> {
    let dir_override = match cli::parse_args() {
        CliAction::Run(dir) => dir,
        CliAction::Help => {
            cli::print_help();
            return Ok(());
        }
        CliAction::ShowCaches => {
            cli::show_caches();
            return Ok(());
        }
        CliAction::ClearAllCaches => {
            cli::clear_caches(crate::cli::ClearTarget::All);
            return Ok(());
        }
        CliAction::ClearManualLyrics => {
            cli::clear_caches(crate::cli::ClearTarget::ManualLyrics);
            return Ok(());
        }
        CliAction::ClearAutoLyrics => {
            cli::clear_caches(crate::cli::ClearTarget::AutoLyrics);
            return Ok(());
        }
        CliAction::ClearMusicDataCache => {
            cli::clear_caches(crate::cli::ClearTarget::MusicData);
            return Ok(());
        }
    };

    super::check_dependencies()?;
    let lock_path = "/tmp/aghani.lock";
    super::ensure_single_instance(lock_path)?;
    let result = run_window(dir_override);
    let _ = std::fs::remove_file(lock_path);
    result
}

fn run_window(dir_override: Option<PathBuf>) -> Result<()> {
    let config = Config::load()?;
    let music_dir = dir_override.unwrap_or_else(|| config.music_dir.clone());
    if !music_dir.is_dir() {
        anyhow::bail!("Music directory does not exist: {}", music_dir.display());
    }

    let mut app = App::new(&music_dir, &config)?;
    app.ipc_rx = Some(start_ipc());
    let (mpris_rx, mpris_update_tx) = crate::mpris::start_mpris();
    app.mpris_rx = Some(mpris_rx);
    app.mpris_update_tx = Some(mpris_update_tx);
    app.drain_metadata();
    app.restore_session_from_state(&State::load())?;

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([900.0, 620.0])
            .with_decorations(false),
        vsync: false,
        ..Default::default()
    };
    eframe::run_native(
        "Aghani",
        native_options,
        Box::new(move |cc| Ok(Box::new(Gui::new(cc, app, config)?))),
    )
    .map_err(|error| anyhow::anyhow!("Could not start the Aghani GUI: {error:?}"))
}

fn start_ipc() -> std::sync::mpsc::Receiver<crate::ipc::IpcCommand> {
    let (tx, rx) = std::sync::mpsc::channel();
    crate::ipc::server::start_ipc(tx);
    rx
}

/// Preprocessed lyric lines keyed by the app's lyrics revision.
///
/// Arabic reshaping and BiDi reordering are expensive, and the lyrics panel is
/// laid out on every repaint. Cache the display strings so only a lyrics
/// change invalidates them.
struct LyricsDisplayCache {
    revision: u64,
    lines: Vec<String>,
}

struct Gui {
    app: App,
    config: Config,
    settings_open: bool,
    lyrics_panel_open: bool,
    lyrics_auto_follow: bool,
    /// Remembered (saved) lyrics panel width; never the maximized width.
    lyrics_panel_width: f32,
    /// The panel's actual width this session, including while maximized.
    lyrics_live_width: f32,
    lyrics_panel_generation: u64,
    lyrics_display_cache: Option<LyricsDisplayCache>,
    lyrics_collapse: PanelCollapse,
    lyrics_maximize_collapse: PanelCollapse,
    now_playing_collapse: PanelCollapse,
    now_playing_panel_generation: u64,
    lyrics_editor_open: bool,
    queue_open: bool,
    muted_volume: Option<f32>,
    lyrics_font_size: f32,
    dark_mode: bool,
    lyrics_draft: String,
    search_input: String,
    playlist_name: String,
    music_dir_input: String,
    /// Cache sizes shown in Settings; measured when Settings opens.
    cache_info: Option<cli::CacheInfo>,
    artwork_path: String,
    textures: HashMap<String, egui::TextureHandle>,
    waveforms: HashMap<PathBuf, Vec<f32>>,
    waveform_pending: HashSet<PathBuf>,
    waveform_tx: mpsc::Sender<(PathBuf, Result<Vec<f32>, String>)>,
    waveform_rx: mpsc::Receiver<(PathBuf, Result<Vec<f32>, String>)>,
    dynamic_accent_cache: Option<(String, egui::Color32)>,
    accent: egui::Color32,
    focus_search: bool,
    pending_scroll_to_playing: bool,
    pending_scroll_to_selection: bool,
    search_rect: Option<egui::Rect>,
    search_window_rect: Option<egui::Rect>,
    applied_theme: Option<bool>,
    applied_theme_name: Option<String>,
    search_debounce_until: Option<Instant>,
    library_check_at: Option<Instant>,
    library_dir_stamp: Option<Vec<(PathBuf, Option<SystemTime>)>>,
    /// Scroll distance still owed to the viewport, used for momentum scrolling.
    momentum_scroll: f32,
    touchpad_velocity: f32,
    touchpad_last_event: Option<Instant>,
    touchpad_coasting: bool,
    /// True while lyrics are covering the library because the window is narrow.
    lyrics_overlay: bool,
    /// Largest cover side the Now playing panel can use at the current window height.
    cover_max_side: f32,
}

impl Gui {
    fn new(cc: &eframe::CreationContext<'_>, mut app: App, mut config: Config) -> Result<Self> {
        app.show_artwork = true;
        app.grid_view = true;
        install_fallback_fonts(&cc.egui_ctx);
        if config.gui.theme == "classic_dark" && !config.gui.dark_mode {
            config.gui.theme = "light".into();
        }
        let dark_mode = config.gui.theme != "light";
        let lyrics_font_size = config.gui.lyrics_font_size.clamp(16.0, 32.0);
        let lyrics_panel_width = config.gui.lyrics_panel_width;
        let muted_volume = config.gui.muted_volume;
        let lyrics_panel_open = app.layout_mode.ends_with("_lyrics");
        let (waveform_tx, waveform_rx) = mpsc::channel();
        let music_dir_input = app.music_dir.display().to_string();

        Ok(Self {
            app,
            config,
            settings_open: false,
            lyrics_panel_open,
            lyrics_auto_follow: true,
            lyrics_panel_width,
            lyrics_live_width: lyrics_panel_width,
            lyrics_panel_generation: 0,
            lyrics_display_cache: None,
            lyrics_collapse: PanelCollapse::default(),
            lyrics_maximize_collapse: PanelCollapse::default(),
            now_playing_collapse: PanelCollapse::default(),
            now_playing_panel_generation: 0,
            lyrics_editor_open: false,
            queue_open: false,
            muted_volume,
            lyrics_font_size,
            dark_mode,
            lyrics_draft: String::new(),
            search_input: String::new(),
            playlist_name: String::new(),
            music_dir_input,
            cache_info: None,
            artwork_path: String::new(),
            textures: HashMap::new(),
            waveforms: HashMap::new(),
            waveform_pending: HashSet::new(),
            waveform_tx,
            waveform_rx,
            dynamic_accent_cache: None,
            accent: egui::Color32::from_rgb(113, 156, 255),
            focus_search: false,
            pending_scroll_to_playing: false,
            pending_scroll_to_selection: false,
            search_rect: None,
            search_window_rect: None,
            applied_theme: None,
            applied_theme_name: None,
            search_debounce_until: None,
            library_check_at: None,
            library_dir_stamp: None,
            momentum_scroll: 0.0,
            touchpad_velocity: 0.0,
            touchpad_last_event: None,
            touchpad_coasting: false,
            lyrics_overlay: false,
            cover_max_side: 440.0,
        })
    }

    /// Close lyrics when they are covering the library, so navigation is visible.
    fn close_overlay_lyrics(&mut self) {
        if self.lyrics_overlay {
            self.set_lyrics_panel_open(false);
            self.lyrics_overlay = false;
        }
    }

    fn apply_momentum(&mut self, ctx: &egui::Context) {
        let (dt, wheel, discrete) = ctx.input(|input| {
            let dt = input.stable_dt.clamp(0.0, 0.1);
            let discrete = input.events.iter().any(|event| {
                matches!(
                    event,
                    egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Line | egui::MouseWheelUnit::Page,
                        delta,
                        ..
                    } if delta.y != 0.0
                )
            });
            (dt, input.raw_scroll_delta.y, discrete)
        });

        if wheel != 0.0 {
            if discrete {
                self.touchpad_velocity = 0.0;
                self.touchpad_coasting = false;
                self.momentum_scroll =
                    (self.momentum_scroll + wheel * MOMENTUM_GAIN).clamp(-MOMENTUM_MAX, MOMENTUM_MAX);
            } else {
                let instant = wheel / dt.max(1.0 / 240.0);
                self.touchpad_velocity = (self.touchpad_velocity * 0.5 + instant * 0.5)
                    .clamp(-TOUCHPAD_MAX_VELOCITY, TOUCHPAD_MAX_VELOCITY);
                self.touchpad_last_event = Some(Instant::now());
                self.touchpad_coasting = false;
                self.momentum_scroll = 0.0;
            }
        } else if self.touchpad_velocity != 0.0 {
            let gap = self
                .touchpad_last_event
                .map_or(Duration::MAX, |last| last.elapsed());
            if !self.touchpad_coasting {
                if gap < TOUCHPAD_COAST_START {
                    ctx.request_repaint();
                } else if gap <= TOUCHPAD_COAST_WINDOW {
                    self.touchpad_coasting = true;
                } else {
                    self.touchpad_velocity = 0.0;
                }
            }
            if self.touchpad_coasting {
                let step = self.touchpad_velocity * dt;
                self.touchpad_velocity *= (-dt / TOUCHPAD_FRICTION_SECONDS).exp();
                if self.touchpad_velocity.abs() < TOUCHPAD_STOP_VELOCITY {
                    self.touchpad_velocity = 0.0;
                    self.touchpad_coasting = false;
                } else {
                    ctx.input_mut(|input| input.smooth_scroll_delta.y += step);
                    ctx.request_repaint();
                }
            }
        }

        if self.momentum_scroll.abs() <= MOMENTUM_STOP {
            self.momentum_scroll = 0.0;
            return;
        }
        let delta = momentum_step(self.momentum_scroll, dt);
        if delta == 0.0 {
            return;
        }
        self.momentum_scroll -= delta;
        ctx.input_mut(|input| input.smooth_scroll_delta.y += delta);
        ctx.request_repaint();
    }

    fn update_search_query(&mut self, force: bool) {
        let due = self
            .search_debounce_until
            .is_some_and(|deadline| Instant::now() >= deadline);
        if force || due {
            self.search_debounce_until = None;
            self.app.set_search_query(self.search_input.clone());
        }
    }

    fn poll_library_changes(&mut self) {
        if self.app.loading {
            return;
        }
        let due = self
            .library_check_at
            .map_or(true, |last| last.elapsed() >= Duration::from_secs(2));
        if !due {
            return;
        }
        self.library_check_at = Some(Instant::now());

        let stamp = directory_stamp(&self.app.music_dir);
        let changed_stamp = self
            .library_dir_stamp
            .as_ref()
            .map_or(true, |previous| previous != &stamp);
        if !changed_stamp {
            return;
        }

        match self.app.refresh_library() {
            Ok(changed) => {
                self.library_dir_stamp = Some(stamp);
                if changed {
                    self.app.show_notice("Library updated");
                }
            }
            Err(error) => {
                self.app
                    .show_notice(format!("Could not refresh library: {error:#}"));
            }
        }
    }

    fn player_fill(&self) -> egui::Color32 {
        let palette = self.palette();
        if self.is_ambient_theme() {
            egui::Color32::from_rgba_unmultiplied(0, 0, 0, 185)
        } else if self.dark_mode {
            palette.panel.lerp_to_gamma(egui::Color32::BLACK, 0.45)
        } else {
            palette.panel.lerp_to_gamma(egui::Color32::BLACK, 0.07)
        }
    }

    fn playlist_cover_key(&self, key: &str) -> Option<String> {
        let name = key.strip_prefix("playlist:")?;
        let lower = name.to_lowercase();
        let first_path = |playlist_name: &str| {
            self.app
                .playlists
                .iter()
                .find(|playlist| playlist.name.eq_ignore_ascii_case(playlist_name))
                .and_then(|playlist| playlist.track_paths.first().cloned())
        };
        let path = match lower.as_str() {
            "history" => self
                .app
                .current_track()
                .map(|track| track.path.clone())
                .or_else(|| first_path(name)),
            "favorites" | "favourites" => first_path(name),
            _ => return None,
        }?;
        let track = self.app.tracks.iter().find(|track| track.path == path)?;
        Some(format!("album:{}", track.album))
    }

    fn favorite_to_top(&mut self, action: impl FnOnce(&mut App) -> Result<()>) {
        let is_favorites = |name: &str| {
            name.eq_ignore_ascii_case("favorites") || name.eq_ignore_ascii_case("favourites")
        };
        let before = self
            .app
            .playlists
            .iter()
            .find(|playlist| is_favorites(&playlist.name))
            .map(|playlist| playlist.track_paths.clone())
            .unwrap_or_default();
        self.report(action);
        if let Some(playlist) = self
            .app
            .playlists
            .iter_mut()
            .find(|playlist| is_favorites(&playlist.name))
        {
            let added: Vec<_> = playlist
                .track_paths
                .iter()
                .filter(|path| !before.contains(path))
                .cloned()
                .collect();
            if !added.is_empty() {
                playlist.track_paths.retain(|path| !added.contains(path));
                for (position, path) in added.into_iter().enumerate() {
                    playlist.track_paths.insert(position, path);
                }
            }
        }
    }

    fn report(&mut self, action: impl FnOnce(&mut App) -> Result<()>) {
        if let Err(error) = action(&mut self.app) {
            self.app.show_notice(format!("{error:#}"));
        }
    }

    fn lyrics_takes_over(&self, ctx: &egui::Context) -> bool {
        if !self.lyrics_panel_open || self.settings_open {
            return false;
        }

        let screen = ctx.screen_rect();
        let screen_width = screen.width();
        let panel_id = egui::Id::new(format!(
            "lyrics_panel_{}",
            self.lyrics_panel_generation
        ));
        let resize = ctx.read_response(panel_id.with("__resize"));
        let width = live_panel_width(
            resize.as_ref(),
            screen.right(),
            self.lyrics_live_width,
        );
        let dragging = resize.as_ref().is_some_and(|response| response.dragged());

        if width >= screen_width * LYRICS_TAKE_OVER_THRESHOLD && !dragging {
            return true;
        }

        let min_width = if self.shows_now_playing() { 1100.0 } else { 800.0 };
        screen_width < min_width
    }

    fn selection_state(&self) -> [Option<usize>; 3] {
        [
            self.app.selected_index,
            self.app.scoped_song_selected,
            self.app.playlist_song_selected,
        ]
    }

    fn skip_track(&mut self, forward: bool) {
        let before = self.selection_state();
        let result = if forward {
            self.app.next_track()
        } else {
            self.app.prev_track()
        };
        if let Err(error) = result {
            self.app.show_notice(format!("{error:#}"));
        }
        if self.selection_state() != before {
            self.pending_scroll_to_playing = self.app.shuffle;
        }
    }

    fn notify_sort(&mut self) {
        if let Some((mode, descending)) = self.table_sort() {
            self.app.show_notice(format!(
                "Sorted by {} {}",
                mode.label(),
                if descending { "↓" } else { "↑" }
            ));
            return;
        }

        let message = match self.app.active_tab {
            LibraryTab::Albums => match self.app.album_scope {
                LibraryScope::All => format!("Albums sorted by {}", self.app.album_sort.label()),
                _ => return,
            },
            LibraryTab::Artists => match self.app.artist_scope {
                LibraryScope::All => format!("Artists sorted by {}", self.app.artist_sort.label()),
                LibraryScope::Artist(_) => {
                    format!("Albums sorted by {}", self.app.album_sort.label())
                }
                _ => return,
            },
            LibraryTab::Playlists => match self.app.playlist_scope {
                PlaylistScope::All => {
                    format!("Playlists sorted by {}", self.app.playlist_sort.label())
                }
                PlaylistScope::Open(_) => return,
            },
            LibraryTab::Songs => return,
        };
        self.app.show_notice(message);
    }

    fn change_sort(&mut self, sort: SongSort) {
        self.app.set_song_sort(sort);
        self.notify_sort();
    }

    fn apply_style(&mut self, ctx: &egui::Context) {
        let accent = self.current_accent();
        if accent != self.accent
            || self.applied_theme != Some(self.dark_mode)
            || self.applied_theme_name.as_deref() != Some(self.config.gui.theme.as_str())
        {
            self.accent = accent;
            self.applied_theme = Some(self.dark_mode);
            self.applied_theme_name = Some(self.config.gui.theme.clone());
            let palette = self.palette();
            let mut style = (*ctx.style()).clone();
            style.visuals = if self.dark_mode {
                egui::Visuals::dark()
            } else {
                egui::Visuals::light()
            };
            let ambient = self.config.gui.theme == "artwork_ambient";
            let background = if ambient {
                palette.background.blend(accent.gamma_multiply(0.18))
            } else {
                palette.background
            };
            let panel = if ambient {
                palette.panel.blend(accent.gamma_multiply(0.22))
            } else {
                palette.panel
            };
            style.visuals.panel_fill = self.surface_fill(background, 202);
            style.visuals.window_fill = self.surface_fill(panel, 208);
            style.visuals.extreme_bg_color = if ambient {
                with_alpha(background.lerp_to_gamma(egui::Color32::BLACK, 0.3), 200)
            } else {
                self.surface_fill(background, 216)
            };
            style.visuals.faint_bg_color = self.surface_fill(panel, 208);
            style.visuals.override_text_color = Some(palette.text);
            style.visuals.selection.bg_fill = accent.gamma_multiply(0.12);
            style.visuals.selection.stroke = egui::Stroke::new(1.0, accent);
            style.visuals.widgets.noninteractive.bg_fill = palette.panel;
            style.visuals.widgets.noninteractive.fg_stroke.color = palette.text;
            style.visuals.widgets.inactive.bg_fill = palette.raised;
            style.visuals.widgets.inactive.fg_stroke.color = palette.text;
            style.visuals.widgets.inactive.bg_stroke.color = palette.border;
            style.visuals.widgets.hovered.bg_fill = accent.gamma_multiply(0.10);
            style.visuals.widgets.hovered.fg_stroke.color = palette.text;
            style.visuals.widgets.hovered.bg_stroke.color = accent.gamma_multiply(0.30);
            style.visuals.widgets.active.bg_fill = accent.gamma_multiply(0.28);
            style.visuals.widgets.active.fg_stroke.color = palette.text;
            style.visuals.widgets.open.bg_fill = accent.gamma_multiply(0.18);
            style.visuals.widgets.open.fg_stroke.color = palette.text;
            style.visuals.window_stroke.color = palette.border;
            style.visuals.widgets.noninteractive.bg_stroke.color = palette.border;

            let corner = egui::CornerRadius::same(10);
            for state in [
                &mut style.visuals.widgets.noninteractive,
                &mut style.visuals.widgets.inactive,
                &mut style.visuals.widgets.hovered,
                &mut style.visuals.widgets.active,
                &mut style.visuals.widgets.open,
            ] {
                state.corner_radius = corner;
            }
            style.visuals.widgets.inactive.weak_bg_fill = palette.raised;
            style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, palette.border);
            style.visuals.widgets.hovered.weak_bg_fill = accent.gamma_multiply(0.16);
            style.visuals.widgets.hovered.bg_stroke =
                egui::Stroke::new(1.0, accent.gamma_multiply(0.58));
            style.visuals.widgets.active.weak_bg_fill = accent.gamma_multiply(0.28);
            style.visuals.widgets.active.bg_stroke =
                egui::Stroke::new(1.0, accent.gamma_multiply(0.72));
            style.visuals.widgets.open.weak_bg_fill = accent.gamma_multiply(0.18);
            style.visuals.window_corner_radius = egui::CornerRadius::same(18);
            style.visuals.menu_corner_radius = egui::CornerRadius::same(12);
            style.visuals.slider_trailing_fill = true;
            style.spacing.item_spacing = egui::vec2(10.0, 8.0);
            style.spacing.button_padding = egui::vec2(10.0, 7.0);
            ctx.set_style(style);
        }
    }

    fn current_accent(&mut self) -> egui::Color32 {
        if self.app.dynamic_theme || self.config.gui.theme == "artwork_ambient" {
            if let Some(track) = self.app.current_track().cloned() {
                let key = format!("{}:{}", track.album, self.app.dynamic_theme_style);
                if let Some((cached_key, color)) = &self.dynamic_accent_cache {
                    if cached_key == &key {
                        return *color;
                    }
                }

                if let Some(art) = self.app.artwork.get(&format!("album:{}", track.album)) {
                    let color = dynamic_accent(&art.image(), &self.app.dynamic_theme_style);
                    self.dynamic_accent_cache = Some((key, color));
                    return color;
                }
            }
        }
        match self.config.colors.accent.to_lowercase().as_str() {
            "cyan" => egui::Color32::from_rgb(65, 201, 220),
            "green" => egui::Color32::from_rgb(93, 204, 139),
            "yellow" => egui::Color32::from_rgb(235, 190, 86),
            "red" => egui::Color32::from_rgb(237, 105, 105),
            "blue" => egui::Color32::from_rgb(113, 156, 255),
            "magenta" => egui::Color32::from_rgb(212, 117, 230),
            _ => egui::Color32::from_rgb(113, 156, 255),
        }
    }

    fn palette(&self) -> Palette {
        Palette::for_theme(self.dark_mode, &self.config.gui.theme)
    }

    fn is_ambient_theme(&self) -> bool {
        self.config.gui.theme == "artwork_ambient"
    }

    fn surface_fill(&self, color: egui::Color32, opacity: u8) -> egui::Color32 {
        if self.is_ambient_theme() {
            with_alpha(color, opacity.saturating_sub(80))
        } else {
            color
        }
    }

    fn paint_panel_collapse_hint(
        &self,
        ctx: &egui::Context,
        rect: egui::Rect,
        layer: &'static str,
    ) {
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Foreground,
            egui::Id::new(layer),
        ));
        painter.rect_filled(
            rect,
            0.0,
            egui::Color32::from_black_alpha(PANEL_COLLAPSE_DIM_ALPHA),
        );
    }

    fn popup_frame(&self, ctx: &egui::Context) -> egui::Frame {
        let palette = self.palette();
        let fill = if self.is_ambient_theme() {
            let tinted = palette.panel.blend(self.accent.gamma_multiply(0.22));
            with_alpha(tinted.lerp_to_gamma(egui::Color32::BLACK, 0.3), 225)
        } else {
            self.surface_fill(palette.panel, 232)
        };
        egui::Frame::window(&ctx.style())
            .fill(fill)
            .stroke(egui::Stroke::new(1.0, palette.border))
            .inner_margin(14.0)
    }

    fn open_entity_page(&mut self, target: EntityLinkTarget) {
        self.close_overlay_lyrics();
        self.app.exit_search();
        self.search_input.clear();
        self.search_debounce_until = None;
        self.settings_open = false;
        match target {
            EntityLinkTarget::Album { album, artist } => {
                if let Some(artist) = artist {
                    self.app.enter_album_from_artist(artist, album);
                } else {
                    self.app.select_tab(LibraryTab::Albums);
                    self.app.enter_album(album);
                }
            }
            EntityLinkTarget::Artist(artist) => {
                self.app.select_tab(LibraryTab::Artists);
                self.app.enter_artist(artist);
            }
        }
    }

    fn set_theme(&mut self, theme: &str) {
        self.config.gui.theme = theme.to_owned();
        self.dark_mode = theme != "light";
        self.config.gui.dark_mode = self.dark_mode;
        self.dynamic_accent_cache = None;
    }

    fn paint_ambient_background(&mut self, ctx: &egui::Context) {
        if !self.is_ambient_theme() {
            return;
        }
        let rect = ctx.screen_rect();
        let painter = ctx.layer_painter(egui::LayerId::background());
        let accent = self.accent;

        painter.rect_filled(
            rect,
            0.0,
            self.palette().background.lerp_to_gamma(accent, 0.45),
        );

        if let Some(track) = self.app.current_track().cloned() {
            let artwork_key = format!("album:{}", track.album);
            self.app.ensure_artwork_requested(&artwork_key);
            let texture_key = format!("{artwork_key}:ambient_blur");
            let cache_key = self.texture_cache_key(&texture_key);
            let texture = self.textures.get(&cache_key).cloned().or_else(|| {
                let image = self
                    .app
                    .artwork
                    .get(&artwork_key)
                    .map(crate::library::artwork::Thumbnail::image)?;
                let small = image.resize_exact(64, 64, image::imageops::FilterType::Triangle);
                Some(self.texture_for(ctx, &texture_key, small.blur(6.0)))
            });

            if let Some(texture) = texture {
                painter.image(
                    texture.id(),
                    rect,
                    centered_image_uv(texture.size_vec2(), rect.size()),
                    egui::Color32::WHITE,
                );
            }
        }

        painter.rect_filled(
            rect,
            0.0,
            egui::Color32::from_rgba_unmultiplied(0, 0, 0, 150),
        );

        painter.rect_filled(rect, 0.0, with_alpha(accent, 28));
    }

    fn save_settings(&mut self) -> Result<()> {
        self.config.keys = self.app.keys.clone();
        self.config.colors.dynamic_theme = self.app.dynamic_theme;
        self.config.colors.dynamic_theme_style = self.app.dynamic_theme_style.clone();
        self.config.layout.mode = self.app.layout_mode.clone();
        self.config.layout.show_cover = self.app.show_cover;
        self.config.layout.show_player = self.app.show_player;
        self.config.layout.cover_width = self.app.cover_width;
        self.config.layout.cover_height = self.app.cover_height;
        self.config.layout.player_height = self.app.player_height;
        self.config.startup.volume = Some(self.app.player.volume);
        self.config.gui.dark_mode = self.dark_mode;
        self.config.gui.lyrics_font_size = self.lyrics_font_size;
        self.config.gui.lyrics_panel_width = self.lyrics_panel_width;
        self.config.gui.muted_volume = self.muted_volume;
        self.config.save()
    }

    fn layout_base(&self) -> &str {
        self.app
            .layout_mode
            .strip_suffix("_lyrics")
            .unwrap_or(&self.app.layout_mode)
    }

    fn shows_now_playing(&self) -> bool {
        layout_visibility(&self.app.layout_mode).0
    }

    fn shows_library(&self) -> bool {
        layout_visibility(&self.app.layout_mode).1
    }

    fn shows_playback_bar(&self) -> bool {
        layout_visibility(&self.app.layout_mode).2
    }

    fn set_lyrics_panel_open(&mut self, open: bool) {
        if open && !self.lyrics_panel_open {
            self.lyrics_panel_generation = self.lyrics_panel_generation.wrapping_add(1);
            // Reopening starts from the remembered size, not a maximized one.
            self.lyrics_live_width = self.lyrics_panel_width;
        }
        self.lyrics_panel_open = open;
        let base = self.layout_base().to_owned();
        self.app.layout_mode = if open { format!("{base}_lyrics") } else { base };
    }

    fn go_to_playing(&mut self) {
        if self.app.current_index.is_none() {
            return;
        }
        self.app.go_to_playing();
        self.pending_scroll_to_playing = true;
        self.momentum_scroll = 0.0;
    }

    fn toggle_now_playing(&mut self) {
        let base = self.layout_base();
        let next = match base {
            "default" => "compact",
            "compact" => "default",
            "clean" => "minimal",
            "minimal" => "clean",
            _ => "compact",
        };
        let lyrics_suffix = self.lyrics_panel_open;
        self.app.layout_mode = if lyrics_suffix {
            format!("{next}_lyrics")
        } else {
            next.to_owned()
        };
    }

    fn playback_controls(&mut self, ui: &mut egui::Ui) {
        ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
            ui.horizontal(|ui| {
                ui.add_space((ui.available_width() - 230.0).max(0.0) * 0.5);
                if icon_toggle_button(ui, Icon::Shuffle, "Shuffle", self.app.shuffle).clicked() {
                    self.app.toggle_shuffle();
                }
                let can_prev = self.app.can_prev();
                if icon_button_enabled(
                    ui,
                    Icon::Previous,
                    if can_prev {
                        "Previous track"
                    } else {
                        "Start of album/playlist"
                    },
                    can_prev,
                )
                .clicked()
                {
                    self.skip_track(false);
                }
                let playing = !self.app.player.is_paused();
                if icon_toggle_button(
                    ui,
                    if playing { Icon::Pause } else { Icon::Play },
                    if playing { "Pause" } else { "Play" },
                    true,
                )
                .clicked()
                {
                    self.report(|app| app.toggle_play());
                }
                let can_next = self.app.can_next();
                if icon_button_enabled(
                    ui,
                    Icon::Next,
                    if can_next {
                        "Next track"
                    } else {
                        "End of album/playlist"
                    },
                    can_next,
                )
                .clicked()
                {
                    self.skip_track(true);
                }
                let repeat_label = match self.app.repeat {
                    RepeatMode::Off => "Off",
                    RepeatMode::All => "All",
                    RepeatMode::One => "One",
                };
                if icon_toggle_button(
                    ui,
                    Icon::Repeat,
                    &format!("Repeat: {repeat_label}"),
                    !matches!(self.app.repeat, RepeatMode::Off),
                )
                .clicked()
                {
                    self.app.toggle_repeat();
                }
            });
        });
    }

    fn playback_options(&mut self, ui: &mut egui::Ui) {
        if icon_toggle_button(
            ui,
            Icon::Lyrics,
            "Toggle lyrics panel",
            self.lyrics_panel_open,
        )
        .clicked()
        {
            self.set_lyrics_panel_open(!self.lyrics_panel_open);
        }
        let muted = self.app.player.volume <= f32::EPSILON;
        if icon_button(
            ui,
            if muted { Icon::VolumeMuted } else { Icon::Volume },
            if muted { "Unmute" } else { "Mute" },
        )
        .clicked()
        {
            if muted {
                self.app.set_volume(self.muted_volume.take().unwrap_or(1.0));
                self.config.gui.muted_volume = None;
            } else {
                self.muted_volume = Some(self.app.player.volume.clamp(0.0, 1.0));
                self.config.gui.muted_volume = self.muted_volume;
                self.app.set_volume(0.0);
            }
        }
        let mut volume = self.app.player.volume.clamp(0.0, 1.0);
        let slider_width = (ui.available_width() - 56.0).clamp(60.0, 180.0);
        let (rect, response) = ui.allocate_exact_size(
            egui::vec2(slider_width, 16.0),
            egui::Sense::click_and_drag(),
        );
        let response = response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text("Volume");

        let knob_radius = if response.hovered() || response.dragged() { 7.0 } else { 6.0 };
        let track = egui::Rect::from_min_max(
            egui::pos2(rect.left() + 7.0, rect.center().y - 2.0),
            egui::pos2(rect.right() - 7.0, rect.center().y + 2.0),
        );

        let mut changed = false;
        if response.dragged() || response.clicked() {
            if let Some(position) = response.interact_pointer_pos() {
                volume = ((position.x - track.left()) / track.width()).clamp(0.0, 1.0);
                changed = true;
            }
        }
        if response.hovered() {
            let scroll = ui.input(|input| input.raw_scroll_delta.y);
            if scroll != 0.0 {
                volume = (volume + scroll / 1000.0).clamp(0.0, 1.0);
                self.momentum_scroll = 0.0;
                changed = true;
            }
        }

        let painter = ui.painter();
        painter.rect_filled(track, 2.0, with_alpha(ui.visuals().text_color(), 60));
        let filled_x = track.left() + track.width() * volume;
        painter.rect_filled(
            egui::Rect::from_min_max(track.min, egui::pos2(filled_x, track.bottom())),
            2.0,
            self.accent,
        );
        painter.circle_filled(
            egui::pos2(filled_x, track.center().y),
            knob_radius,
            bright_accent(self.accent),
        );

        if changed {
            self.app.set_volume(volume);
            if volume > 0.0 {
                self.muted_volume = None;
                self.config.gui.muted_volume = None;
            }
        }
    }

    fn request_waveform(&mut self, path: PathBuf) -> Result<()> {
        if self.waveforms.contains_key(&path) || !self.waveform_pending.insert(path.clone()) {
            return Ok(());
        }

        let sender = self.waveform_tx.clone();
        std::thread::Builder::new()
            .name("aghani-waveform".into())
            .spawn(move || {
                let result = (|| {
                    let output = std::process::Command::new("ffmpeg")
                        .args(["-nostdin", "-v", "error", "-i"])
                        .arg(&path)
                        .args([
                            "-vn",
                            "-ac",
                            "1",
                            "-ar",
                            "100",
                            "-acodec",
                            "pcm_f32le",
                            "-f",
                            "f32le",
                            "pipe:1",
                        ])
                        .output()
                        .map_err(|error| format!("Could not run ffmpeg: {error}"))?;

                    if !output.status.success() {
                        let error = String::from_utf8_lossy(&output.stderr);
                        return Err(if error.trim().is_empty() {
                            format!("ffmpeg exited with {}", output.status)
                        } else {
                            error.trim().to_owned()
                        });
                    }

                    let samples = output
                        .stdout
                        .chunks_exact(4)
                        .map(|bytes| f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
                        .collect::<Vec<_>>();
                    if samples.is_empty() {
                        return Err("ffmpeg returned no audio samples".into());
                    }
                    Ok(waveform_peaks(&samples, WAVEFORM_BAR_COUNT))
                })();
                let _ = sender.send((path, result));
            })
            .context("Could not start waveform extraction thread")?;
        Ok(())
    }

    fn drain_waveform_results(&mut self) {
        while let Ok((path, result)) = self.waveform_rx.try_recv() {
            self.waveform_pending.remove(&path);
            match result {
                Ok(peaks) => {
                    self.waveforms.insert(path, peaks);
                }
                Err(error) => {
                    self.app
                        .show_notice(format!("Could not load waveform: {error}"));
                }
            }
        }
    }

    fn playback_progress(&mut self, ui: &mut egui::Ui, width: f32, height: f32) {
        let elapsed = self.app.player.elapsed().as_secs_f64();
        let current_path = self.app.current_track().map(|track| track.path.clone());
        if self.config.gui.show_waveform {
            if let Some(path) = current_path.as_ref() {
                if let Err(error) = self.request_waveform(path.clone()) {
                    self.waveform_pending.remove(path);
                    self.app.show_notice(format!("{error:#}"));
                }
            }
        }
        let duration = self
            .app
            .current_track()
            .map(|track| track.duration.as_secs_f64())
            .unwrap_or_default();
        let progress = if duration > 0.0 {
            (elapsed / duration).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::click_and_drag());
        let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
        let painter = ui.painter().with_clip_rect(rect);
        let text_color = ui.visuals().text_color();
        let muted_color = ui.visuals().weak_text_color();
        let label_width = 42.0;
        painter.text(
            egui::pos2(rect.left(), rect.center().y),
            egui::Align2::LEFT_CENTER,
            format_time(elapsed),
            egui::FontId::proportional(12.0),
            muted_color,
        );
        painter.text(
            egui::pos2(rect.right(), rect.center().y),
            egui::Align2::RIGHT_CENTER,
            format_time(duration),
            egui::FontId::proportional(12.0),
            muted_color,
        );
        let waveform_rect = egui::Rect::from_min_max(
            egui::pos2(rect.left() + label_width, rect.top() + 4.0),
            egui::pos2(rect.right() - label_width, rect.bottom() - 4.0),
        );
        let pointer_ratio = response
            .interact_pointer_pos()
            .filter(|position| waveform_rect.contains(*position))
            .map(|position| {
                ((position.x - waveform_rect.left()) / waveform_rect.width()).clamp(0.0, 1.0)
            });
        let display_progress = if response.is_pointer_button_down_on() {
            pointer_ratio.unwrap_or(progress)
        } else {
            progress
        };
        if response.clicked() || response.drag_stopped() {
            if let Some(position) = response
                .interact_pointer_pos()
                .filter(|position| waveform_rect.contains(*position))
            {
                let ratio =
                    ((position.x - waveform_rect.left()) / waveform_rect.width()).clamp(0.0, 1.0);
                let target = Duration::from_secs_f64(duration * f64::from(ratio));
                self.report(|app| app.seek_to_lyric(target));
            }
        }

        if !self.config.gui.show_waveform {
            let rail = egui::Rect::from_center_size(
                waveform_rect.center(),
                egui::vec2(waveform_rect.width(), 4.0),
            );
            painter.rect_filled(rail, 2.0, with_alpha(text_color, 52));
            let played = egui::Rect::from_min_max(
                rail.min,
                egui::pos2(rail.left() + rail.width() * display_progress, rail.bottom()),
            );
            painter.rect_filled(played, 2.0, self.accent);
            if response.hovered() {
                response.on_hover_text(format!(
                    "Seek to {}",
                    format_time(duration * f64::from(display_progress))
                ));
            }
            return;
        }

        const MIN_BAR_WIDTH: f32 = 1.25;
        let gap = 2.5;
        let bar_count = WAVEFORM_BAR_COUNT
            .min(((waveform_rect.width() + gap) / (MIN_BAR_WIDTH + gap)).floor() as usize)
            .max(1);
        let bar_width = ((waveform_rect.width() - gap * (bar_count - 1) as f32) / bar_count as f32)
            .max(MIN_BAR_WIDTH);
        let center_y = waveform_rect.center().y;
        let peaks = current_path
            .as_ref()
            .and_then(|path| self.waveforms.get(path));
        for index in 0..bar_count {
            let amplitude = peaks
                .map(|values| {
                    let start = index * values.len() / bar_count;
                    let end = ((index + 1) * values.len() / bar_count).max(start + 1);
                    values[start..end.min(values.len())]
                        .iter()
                        .copied()
                        .fold(0.0_f32, f32::max)
                })
                .unwrap_or(0.0);
            let bar_height = (waveform_rect.height() * (0.05 + amplitude * 0.95))
                .clamp(2.5, waveform_rect.height());
            let left = waveform_rect.left() + index as f32 * (bar_width + gap);
            let bar_rect = egui::Rect::from_center_size(
                egui::pos2(left + bar_width * 0.5, center_y),
                egui::vec2(bar_width, bar_height),
            );
            let color = if (index + 1) as f32 / bar_count as f32 <= display_progress {
                self.accent
            } else {
                with_alpha(text_color, 72)
            };
            painter.rect_filled(bar_rect, bar_width * 0.5, color);
        }
        if response.hovered() {
            response.on_hover_text(format!(
                "Seek to {}",
                format_time(duration * f64::from(display_progress))
            ));
        }
    }

    fn header(&mut self, ctx: &egui::Context) {
        let palette = self.palette();
        egui::TopBottomPanel::top("top_bar")
            .frame(
                egui::Frame::default()
                    .fill(self.surface_fill(palette.panel, 226))
                    .stroke(egui::Stroke::new(1.0, palette.border))
                    .inner_margin(egui::Margin::symmetric(18, 9)),
            )
            .show(ctx, |ui| {
                let drag = ui.interact(
                    ui.max_rect().expand2(egui::vec2(18.0, 9.0)),
                    ui.id().with("window_drag"),
                    egui::Sense::click_and_drag(),
                );
                if drag.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                if drag.double_clicked() {
                    let maximized = ctx.input(|input| input.viewport().maximized.unwrap_or(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
                }
                ui.horizontal(|ui| {
                    let available = ui.available_width();
                    let compact = available < 760.0;
                    let minimal = available < 620.0;
                    if !minimal {
                        ui.label(egui::RichText::new("aghani").size(21.0).strong());
                        ui.add_space(if compact { 8.0 } else { 10.0 });
                    }
                    for (tab, icon) in LibraryTab::all().iter().zip([
                        Icon::Songs,
                        Icon::Albums,
                        Icon::Artists,
                        Icon::Playlists,
                    ]) {
                        if navigation_tab(
                            ui,
                            icon,
                            tab.label(),
                            self.app.active_tab == *tab,
                            compact,
                        )
                        .clicked()
                        {
                            self.close_overlay_lyrics();
                            self.app.select_tab(tab.clone());
                            self.search_input.clear();
                            self.search_debounce_until = None;
                            self.app.exit_search();
                        }
                    }
                    ui.add_space(if compact { 8.0 } else { 12.0 });

                    let right_width = 34.0 * 2.0 + ui.spacing().item_spacing.x * 2.0;
                    let free_width = (ui.available_width() - right_width - 12.0).max(0.0);
                    let search_width = free_width.min(420.0);
                    if search_width >= 130.0 {
                        if free_width > search_width + 16.0 {
                            ui.add_space((free_width - search_width) * 0.5);
                        }
                        let search_rect = egui::Rect::from_min_size(
                            ui.cursor().min,
                            egui::vec2(search_width, 34.0),
                        );
                        let search = ui
                            .allocate_new_ui(
                                egui::UiBuilder::new()
                                    .max_rect(search_rect)
                                    .layout(egui::Layout::left_to_right(egui::Align::Center)),
                                |ui| {
                                    let radius = egui::CornerRadius::same(17);
                                    let widgets = &mut ui.style_mut().visuals.widgets;
                                    for state in [
                                        &mut widgets.noninteractive,
                                        &mut widgets.inactive,
                                        &mut widgets.hovered,
                                        &mut widgets.active,
                                        &mut widgets.open,
                                    ] {
                                        state.corner_radius = radius;
                                    }
                                    ui.add_sized(
                                        [search_rect.width(), 34.0],
                                        egui::TextEdit::singleline(&mut self.search_input)
                                            .hint_text("Search your library")
                                            .margin(egui::vec2(12.0, 7.0))
                                            .vertical_align(egui::Align::Center),
                                    )
                                },
                            )
                            .inner;
                        self.search_rect = Some(search.rect);
                        if self.focus_search {
                            search.request_focus();
                            self.focus_search = false;
                        }
                        if search.changed() {
                            if !self.app.search_mode {
                                self.app.enter_search();
                            }
                            self.search_debounce_until =
                                Some(Instant::now() + Duration::from_millis(120));
                        }
                        if self.app.search_mode
                            && self.search_input.is_empty()
                            && !search.has_focus()
                        {
                            self.search_debounce_until = None;
                            self.app.exit_search();
                        }
                    } else {
                        self.search_rect = None;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icon_toggle_button(ui, Icon::Settings, "Settings", self.settings_open)
                            .clicked()
                        {
                            if !self.settings_open {
                                self.close_overlay_lyrics();
                            }
                            self.settings_open = !self.settings_open;
                        }
                        let theme = self.config.gui.theme.clone();
                        if icon_button(
                            ui,
                            theme_icon(&theme),
                            &format!("Theme: {} · click to cycle", theme_label(&theme)),
                        )
                        .clicked()
                        {
                            let next = next_theme(&theme);
                            self.set_theme(next);
                        }
                    });
                });
            });
    }

    fn breadcrumb_ui(&mut self, ui: &mut egui::Ui, palette: Palette) {
        let active_tab = self.app.active_tab.clone();
        let album_scope = self.app.album_scope.clone();
        let artist_scope = self.app.artist_scope.clone();
        let playlist_scope = self.app.playlist_scope.clone();
        let album_artist = self.app.album_return_artist().map(str::to_owned);

        match (active_tab, album_scope, artist_scope, playlist_scope) {
            (LibraryTab::Albums, crate::library::LibraryScope::Album(album), _, _) => {
                if let Some(artist) = album_artist {
                    if breadcrumb_link(ui, "Artists", self.accent) {
                        self.app.exit_scope();
                        self.app.exit_scope();
                    }
                    breadcrumb_separator(ui, palette);
                    if breadcrumb_link(ui, &artist, self.accent) {
                        self.app.exit_scope();
                    }
                    breadcrumb_separator(ui, palette);
                    if breadcrumb_link(ui, &album, self.accent) {
                        self.app.enter_album_from_artist(artist, album);
                    }
                } else {
                    if breadcrumb_link(ui, "Albums", self.accent) {
                        self.app.exit_scope();
                    }
                    breadcrumb_separator(ui, palette);
                    if breadcrumb_link(ui, &album, self.accent) {
                        self.app.enter_album(album);
                    }
                }
            }
            (LibraryTab::Artists, _, crate::library::LibraryScope::Artist(artist), _) => {
                if breadcrumb_link(ui, "Artists", self.accent) {
                    self.app.exit_scope();
                }
                breadcrumb_separator(ui, palette);
                if breadcrumb_link(ui, &artist, self.accent) {
                    self.app.enter_artist(artist);
                }
            }
            (LibraryTab::Playlists, _, _, crate::library::PlaylistScope::Open(index)) => {
                let playlist = self
                    .app
                    .playlists
                    .get(index)
                    .map(|playlist| playlist.name.clone());
                if breadcrumb_link(ui, "Playlists", self.accent) {
                    self.app.exit_scope();
                }
                if let Some(playlist) = playlist {
                    breadcrumb_separator(ui, palette);
                    if breadcrumb_link(ui, &playlist, self.accent) {
                        self.app.enter_playlist(index);
                    }
                }
            }
            _ => {}
        }
    }

    fn library_panel(&mut self, ctx: &egui::Context) -> egui::Rect {
        let palette = self.palette();
        let response = egui::CentralPanel::default()
            .frame(
                egui::Frame::default()
                    .fill(self.surface_fill(palette.background, 214))
                    .inner_margin(egui::Margin::symmetric(22, 18)),
            )
            .show(ctx, |ui| {
                if self.is_scoped_view() {
                    ui.horizontal(|ui| self.breadcrumb_ui(ui, palette));
                    ui.add_space(6.0);
                }
                match &self.app.active_tab {
                    LibraryTab::Songs => self.song_list(ui),
                    LibraryTab::Albums => self.album_list(ui),
                    LibraryTab::Artists => self.artist_list(ui),
                    LibraryTab::Playlists => self.playlist_list(ui),
                }
            });
        response.response.rect
    }

    fn table_sort(&self) -> Option<(SongSort, bool)> {
        match self.app.active_tab {
            LibraryTab::Songs => Some((self.app.song_sort, self.app.song_sort_descending)),
            LibraryTab::Albums => matches!(
                self.app.album_scope,
                crate::library::LibraryScope::Album(_)
            )
            .then_some((self.app.song_sort, self.app.song_sort_descending)),
            LibraryTab::Playlists => {
                let crate::library::PlaylistScope::Open(index) = self.app.playlist_scope else {
                    return None;
                };
                let fixed = self.app.playlists.get(index).is_some_and(|playlist| {
                    matches!(
                        playlist.name.to_lowercase().as_str(),
                        "history" | "most played"
                    )
                });
                (!fixed).then_some((
                    self.app.playlist_song_sort,
                    self.app.playlist_song_sort_descending,
                ))
            }
            LibraryTab::Artists => None,
        }
    }

    fn is_scoped_view(&self) -> bool {
        match self.app.active_tab {
            LibraryTab::Songs => false,
            LibraryTab::Albums => matches!(
                self.app.album_scope,
                crate::library::LibraryScope::Album(_)
            ),
            LibraryTab::Artists => matches!(
                self.app.artist_scope,
                crate::library::LibraryScope::Artist(_)
            ),
            LibraryTab::Playlists => matches!(
                self.app.playlist_scope,
                crate::library::PlaylistScope::Open(_)
            ),
        }
    }

    fn song_list(&mut self, ui: &mut egui::Ui) {
        if self.app.tracks.is_empty() {
            let palette = self.palette();
            ui.centered_and_justified(|ui| {
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new(if self.app.loading {
                            "Scanning your music folder…"
                        } else {
                            "No supported audio files found"
                        })
                        .size(20.0)
                        .strong(),
                    );
                    ui.label(
                        display_rich(self.app.music_dir.display().to_string())
                            .small()
                            .color(palette.muted),
                    );
                });
            });
            return;
        }
        let show_metadata = ui.available_width() >= SONG_METADATA_MIN_WIDTH;
        if let Some(sort) = self.track_table_header(ui, show_metadata) {
            self.change_sort(sort);
        }
        let indices = self.app.all_track_indices.clone();
        let palette = self.palette();
        let target_row = self
            .pending_scroll_to_playing
            .then_some(self.app.selected_index)
            .flatten();
        let jump_to_selection = std::mem::take(&mut self.pending_scroll_to_selection);
        let mut scroll = egui::ScrollArea::vertical().id_salt("songs");
        if self.pending_scroll_to_playing {
            if let Some(row) = target_row {
                scroll = scroll.vertical_scroll_offset(playing_row_scroll_offset(
                    row,
                    58.0,
                    ui.spacing().item_spacing.y,
                    indices.len(),
                    ui.available_height(),
                ));
            }
        } else if jump_to_selection {
            if let Some(row) = self.app.selected_index {
                scroll = scroll.vertical_scroll_offset(playing_row_scroll_offset(
                    row,
                    58.0,
                    ui.spacing().item_spacing.y,
                    indices.len(),
                    ui.available_height(),
                ));
            }
        }
        scroll.show_rows(ui, 58.0, indices.len(), |ui, visible_rows| {
            for position in visible_rows {
                let Some(&index) = indices.get(position) else {
                    continue;
                };
                let Some(track) = self.app.tracks.get(index).cloned() else {
                    continue;
                };
                let current = self.app.current_index == Some(index);
                let selected = self.app.selected_index == Some(position);
                let show_artwork = self.app.show_artwork;
                let artwork = show_artwork
                    .then(|| self.track_artwork(ui, &track))
                    .flatten();
                let response = song_row(
                    ui,
                    &track,
                    selected,
                    current,
                    self.app.is_favorite(&track.path),
                    show_artwork,
                    show_metadata,
                    artwork.as_ref(),
                    palette,
                    self.accent,
                    None,
                );
                if response.queue {
                    self.app.selected_index = Some(position);
                    self.app.queue_selected_track();
                } else if let Some(target) = response.activated {
                    self.open_entity_page(target);
                } else {
                    if response.row.clicked() {
                        self.app.selected_index = Some(position);
                    }
                    if response.row.double_clicked() {
                        self.app.selected_index = Some(position);
                        self.report(|app| app.activate_selected());
                    }
                }
                if current {
                    if !self.app.shuffle {
                        let viewport = ui.clip_rect();
                        let row_rect = response.row.rect;
                        if row_rect.top() < viewport.top() + 6.0 {
                            ui.scroll_to_rect_animation(
                                row_rect,
                                Some(egui::Align::Min),
                                egui::style::ScrollAnimation::none(),
                            );
                        } else if row_rect.bottom() > viewport.bottom() - 58.0 {
                            ui.scroll_to_rect_animation(
                                row_rect,
                                Some(egui::Align::Max),
                                egui::style::ScrollAnimation::none(),
                            );
                        }
                    }
                    ui.ctx().request_repaint_after(Duration::from_secs(1));
                }
            }
        });
        self.pending_scroll_to_playing = false;
    }

    fn entity_card(
        &mut self,
        ui: &mut egui::Ui,
        key: &str,
        title: &str,
        subtitle: &str,
        selected: bool,
        card_width: f32,
        _title_target: Option<EntityLinkTarget>,
        _subtitle_target: Option<EntityLinkTarget>,
    ) -> EntityItemResponse {
        let title_target: Option<EntityLinkTarget> = None;
        let subtitle_target: Option<EntityLinkTarget> = None;
        let palette = self.palette();
        let art_key = self
            .playlist_cover_key(key)
            .unwrap_or_else(|| key.to_string());
        let texture = self
            .app
            .artwork
            .get(&art_key)
            .map(crate::library::artwork::Thumbnail::image)
            .map(|image| self.texture_for(ui.ctx(), &art_key, image));
        let response = self.artwork_card(
            ui,
            key,
            title,
            subtitle,
            texture.as_ref(),
            selected,
            card_width,
            card_width + GRID_CARD_FOOTER,
            palette,
            title_target,
            subtitle_target,
        );
        if self.app.show_artwork && response.item.rect.intersects(ui.clip_rect()) {
            self.app.ensure_artwork_requested(&art_key);
        }
        response
    }

    fn artwork_card(
        &mut self,
        ui: &mut egui::Ui,
        key: &str,
        title: &str,
        subtitle: &str,
        texture: Option<&egui::TextureHandle>,
        selected: bool,
        card_width: f32,
        card_height: f32,
        palette: Palette,
        title_target: Option<EntityLinkTarget>,
        subtitle_target: Option<EntityLinkTarget>,
    ) -> EntityItemResponse {
        let card = ui.allocate_space(egui::vec2(card_width, card_height)).1;
        let response = ui
            .interact(
                card,
                ui.make_persistent_id(("artwork_card", key)),
                egui::Sense::click(),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        let hovered = response.hovered();
        let motion = if ui.style().animation_time > 0.0 {
            ui.ctx()
                .animate_bool_responsive(response.id.with("hover"), hovered)
        } else {
            f32::from(hovered)
        };
        let circular = key.starts_with("artist:");
        let radius = 11.0;
        let painter = ui.painter().with_clip_rect(card);
        if circular {
            painter.rect_filled(card, radius, with_alpha(palette.background, 170));
            painter.rect_stroke(
                card,
                radius,
                egui::Stroke::new(1.0, egui::Color32::from_white_alpha(16)),
                egui::StrokeKind::Inside,
            );
        } else {
            painter.rect_filled(card, radius, palette.panel);
        }
        let image_rect = if circular {
            let diameter = (card_width - 56.0).max(60.0);
            egui::Rect::from_center_size(
                egui::pos2(card.center().x, card.top() + 8.0 + diameter * 0.5),
                egui::vec2(diameter, diameter),
            )
        } else {
            egui::Rect::from_min_size(
                card.min,
                egui::vec2(card_width, (card_height - 10.0).min(card_width)),
            )
        };
        let image_radius = if circular { image_rect.width() * 0.5 } else { radius };

        if let Some(texture) = texture {
            let image = egui::Image::new(texture)
                .fit_to_exact_size(image_rect.size())
                .uv(centered_image_uv(texture.size_vec2(), image_rect.size()))
                .corner_radius(image_radius);
            let mut image_ui = ui.new_child(egui::UiBuilder::new().max_rect(card));
            image_ui.set_clip_rect(card.intersect(ui.clip_rect()));
            image.paint_at(&image_ui, image_rect);
        } else {
            painter.rect_filled(image_rect, image_radius, palette.raised);
            paint_icon(
                &painter,
                if circular { Icon::Artists } else { Icon::Albums },
                egui::Rect::from_center_size(image_rect.center(), egui::vec2(38.0, 38.0)),
                palette.muted,
            );
        }

        painter.rect_filled(
            image_rect,
            image_radius,
            egui::Color32::from_black_alpha((motion * 46.0) as u8),
        );
        if !circular {
            let text_backplate = egui::Rect::from_min_max(
                egui::pos2(image_rect.left(), image_rect.bottom() - 78.0),
                image_rect.right_bottom(),
            );
            painter.rect_filled(
                text_backplate,
                egui::CornerRadius { nw: 0, ne: 0, sw: radius as u8, se: radius as u8 },
                egui::Color32::from_black_alpha((150.0 + motion * 30.0) as u8),
            );
        }
        if selected {
            painter.rect_stroke(
                image_rect,
                image_radius,
                egui::Stroke::new(1.5, self.accent.gamma_multiply(0.6)),
                egui::StrokeKind::Inside,
            );
        }

        let (info_rect, info_layout, title_color, subtitle_color, clip) = if circular {
            (
                egui::Rect::from_min_max(
                    egui::pos2(card.left() + 8.0, image_rect.bottom() + 8.0),
                    egui::pos2(card.right() - 8.0, card.bottom() - 4.0),
                ),
                egui::Layout::top_down(egui::Align::Center),
                palette.text,
                palette.muted,
                card,
            )
        } else {
            (
                egui::Rect::from_min_max(
                    egui::pos2(image_rect.left() + 13.0, image_rect.bottom() - 67.0 - motion * 6.0),
                    egui::pos2(image_rect.right() - 13.0, image_rect.bottom() - 10.0),
                ),
                egui::Layout::top_down(egui::Align::Min),
                egui::Color32::WHITE,
                egui::Color32::from_white_alpha(220),
                image_rect.shrink(8.0),
            )
        };
        let mut info_ui = ui.new_child(
            egui::UiBuilder::new().max_rect(info_rect).layout(info_layout),
        );
        info_ui.set_clip_rect(clip.intersect(ui.clip_rect()));
        let title_clicked = if let Some(target) = title_target {
            metadata_link(
                &mut info_ui,
                title,
                egui::Color32::WHITE,
                self.accent,
                16.0,
                true,
            )
            .clicked()
            .then_some(target)
        } else {
            info_ui.add(
                egui::Label::new(
                    display_rich(title)
                        .size(16.0)
                        .strong()
                        .color(egui::Color32::WHITE),
                )
                .truncate(),
            );
            None
        };
        let subtitle_clicked = if let Some(target) = subtitle_target {
            metadata_link(
                &mut info_ui,
                subtitle,
                title_color,
                self.accent,
                12.0,
                false,
            )
            .clicked()
            .then_some(target)
        } else {
            info_ui.add(
                egui::Label::new(
                    display_rich(subtitle)
                        .size(12.0)
                        .color(subtitle_color),
                )
                .truncate(),
            );
            None
        };

        EntityItemResponse {
            item: response.on_hover_text(format!("{title}\n{subtitle}")),
            activated: title_clicked.or(subtitle_clicked),
        }
    }

    fn album_list(&mut self, ui: &mut egui::Ui) {
        let jump_to_selection = std::mem::take(&mut self.pending_scroll_to_selection);
        if let crate::library::LibraryScope::Album(_) = &self.app.album_scope {
            let show_metadata = ui.available_width() >= SONG_METADATA_MIN_WIDTH;
            if let Some(sort) = self.track_table_header(ui, show_metadata) {
                self.change_sort(sort);
            }
            let indices = self.app.scoped_track_indices.clone();
            let palette = self.palette();
            let target_row = self
                .pending_scroll_to_playing
                .then_some(self.app.scoped_song_selected)
                .flatten();
            let mut scroll = egui::ScrollArea::vertical().id_salt("album_tracks");
            if self.pending_scroll_to_playing {
                if let Some(row) = target_row {
                    scroll = scroll.vertical_scroll_offset(playing_row_scroll_offset(
                        row,
                        58.0,
                        ui.spacing().item_spacing.y,
                        indices.len(),
                        ui.available_height(),
                    ));
                }
            } else if jump_to_selection {
                if let Some(row) = self.app.scoped_song_selected {
                    scroll = scroll.vertical_scroll_offset(playing_row_scroll_offset(
                        row,
                        58.0,
                        ui.spacing().item_spacing.y,
                        indices.len(),
                        ui.available_height(),
                    ));
                }
            }
            scroll.show_rows(ui, 58.0, indices.len(), |ui, visible_rows| {
                for pos in visible_rows {
                    let Some(&index) = indices.get(pos) else {
                        continue;
                    };
                    let Some(track) = self.app.tracks.get(index).cloned() else {
                        continue;
                    };
                    let current = self.app.current_index == Some(index);
                    let selected = self.app.scoped_song_selected == Some(pos);
                    let show_artwork = self.app.show_artwork;
                    let artwork = show_artwork
                        .then(|| self.track_artwork(ui, &track))
                        .flatten();
                    let response = song_row(
                        ui,
                        &track,
                        selected,
                        current,
                        self.app.is_favorite(&track.path),
                        show_artwork,
                        show_metadata,
                        artwork.as_ref(),
                        palette,
                        self.accent,
                        None,
                    );
                    if current && !self.app.shuffle {
                        let viewport = ui.clip_rect();
                        let row_rect = response.row.rect;
                        if row_rect.top() < viewport.top() + 6.0 {
                            ui.scroll_to_rect_animation(
                                row_rect,
                                Some(egui::Align::Min),
                                egui::style::ScrollAnimation::none(),
                            );
                        } else if row_rect.bottom() > viewport.bottom() - 58.0 {
                            ui.scroll_to_rect_animation(
                                row_rect,
                                Some(egui::Align::Max),
                                egui::style::ScrollAnimation::none(),
                            );
                        }
                    }
                    if response.queue {
                        self.app.scoped_song_selected = Some(pos);
                        self.app.queue_selected_track();
                    } else if let Some(target) = response.activated {
                        self.open_entity_page(target);
                    } else {
                        if response.row.clicked() {
                            self.app.scoped_song_selected = Some(pos);
                        }
                        if response.row.double_clicked() {
                            self.app.scoped_song_selected = Some(pos);
                            self.report(|app| app.activate_selected());
                        }
                    }
                }
            });
            if target_row.is_some() {
                self.pending_scroll_to_playing = false;
            }
            return;
        }
        let albums = visible_album_entries(
            &self.app.albums,
            self.config.gui.hide_single_track_albums,
            |album| self.app.album_track_count(album),
        );
        if self.app.grid_view && self.app.show_artwork {
            let (columns, card_width) = responsive_grid_layout(
                ui.available_width()
                    - ui.spacing().scroll.bar_width
                    - ui.spacing().scroll.bar_inner_margin,
            );
            let row_count = albums.len().div_ceil(columns);
            let selected_position = self.app.album_selected.and_then(|selected| {
                albums
                    .iter()
                    .position(|(index, _)| *index == selected)
            });
            let mut scroll = egui::ScrollArea::vertical().id_salt("albums_grid");
            if jump_to_selection {
                if let Some(position) = selected_position {
                    scroll = scroll.vertical_scroll_offset(playing_row_scroll_offset(
                        position / columns,
                        card_width + GRID_CARD_FOOTER,
                        ui.spacing().item_spacing.y,
                        row_count,
                        ui.available_height(),
                    ));
                }
            }
            scroll.show_rows(
                ui,
                card_width + GRID_CARD_FOOTER,
                row_count,
                |ui, visible_rows| {
                    for row_index in visible_rows {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = GRID_SPACING;
                            let start = row_index * columns;
                            let end = (start + columns).min(albums.len());
                            for i in start..end {
                                let (album_index, album) = &albums[i];
                                let artist = self
                                    .app
                                    .tracks
                                    .iter()
                                    .find(|track| &track.album == album)
                                    .map(|track| track.artist.clone())
                                    .unwrap_or_else(|| "Unknown Artist".to_string());
                                let subtitle = format!(
                                    "{artist} · {} tracks",
                                    self.app.album_track_count(album)
                                );
                                let key = format!("album:{album}");
                                let response = self.entity_card(
                                    ui,
                                    &key,
                                    album,
                                    &subtitle,
                                    self.app.album_selected == Some(*album_index),
                                    card_width,
                                    Some(EntityLinkTarget::Album {
                                        album: album.clone(),
                                        artist: Some(artist.clone()),
                                    }),
                                    Some(EntityLinkTarget::Artist(artist.clone())),
                                );
                                if let Some(target) = response.activated {
                                    self.open_entity_page(target);
                                } else {
                                    if response.item.clicked() {
                                        self.app.album_selected = Some(*album_index);
                                    }
                                    if response.item.double_clicked() {
                                        self.app.album_selected = Some(*album_index);
                                        self.app.enter_album(album.clone());
                                    }
                                }
                            }
                        });
                    }
                },
            );
        } else {
            let selected_position = self.app.album_selected.and_then(|selected| {
                albums
                    .iter()
                    .position(|(index, _)| *index == selected)
            });
            let mut scroll = egui::ScrollArea::vertical().id_salt("albums");
            if jump_to_selection {
                if let Some(position) = selected_position {
                    scroll = scroll.vertical_scroll_offset(playing_row_scroll_offset(
                        position,
                        60.0,
                        ui.spacing().item_spacing.y,
                        albums.len(),
                        ui.available_height(),
                    ));
                }
            }
            scroll.show_rows(
                ui,
                60.0,
                albums.len(),
                |ui, visible_rows| {
                    for i in visible_rows {
                        let (album_index, album) = &albums[i];
                        let selected = self.app.album_selected == Some(*album_index);
                        let artist = self
                            .app
                            .tracks
                            .iter()
                            .find(|track| &track.album == album)
                            .map(|track| track.artist.clone())
                            .unwrap_or_else(|| "Unknown Artist".to_string());
                        let count = self.app.album_track_count(album);
                        let row = self.entity_row(
                            ui,
                            &format!("album:{album}"),
                            album,
                            &format!("{artist} · {count} tracks"),
                            selected,
                            Some(EntityLinkTarget::Album {
                                album: album.clone(),
                                artist: Some(artist.clone()),
                            }),
                            Some(EntityLinkTarget::Artist(artist)),
                        );
                        if let Some(target) = row.activated {
                            self.open_entity_page(target);
                        } else {
                            if row.item.clicked() {
                                self.app.album_selected = Some(*album_index);
                            }
                            if row.item.double_clicked() {
                                self.app.album_selected = Some(*album_index);
                                self.app.enter_album(album.clone());
                            }
                        }
                    }
                },
            );
        }
    }

    fn artist_list(&mut self, ui: &mut egui::Ui) {
        let jump_to_selection = std::mem::take(&mut self.pending_scroll_to_selection);
        if let crate::library::LibraryScope::Artist(artist_name) = &self.app.artist_scope {
            let artist_name = artist_name.clone();
            let albums = visible_album_entries(
                &self.app.scoped_albums,
                false,
                |album| self.app.album_track_count(album),
            );
            if self.app.grid_view && self.app.show_artwork {
                let (columns, card_width) = responsive_grid_layout(
                    ui.available_width()
                        - ui.spacing().scroll.bar_width
                        - ui.spacing().scroll.bar_inner_margin,
                );
                let row_count = albums.len().div_ceil(columns);
                let selected_position = self.app.artist_scoped_album_selected.and_then(
                    |selected| {
                        albums
                            .iter()
                            .position(|(index, _)| *index == selected)
                    },
                );
                let mut scroll =
                    egui::ScrollArea::vertical().id_salt("artist_albums_grid");
                if jump_to_selection {
                    if let Some(position) = selected_position {
                        scroll = scroll.vertical_scroll_offset(playing_row_scroll_offset(
                            position / columns,
                            card_width + GRID_CARD_FOOTER,
                            ui.spacing().item_spacing.y,
                            row_count,
                            ui.available_height(),
                        ));
                    }
                }
                scroll.show_rows(
                    ui,
                    card_width + GRID_CARD_FOOTER,
                    row_count,
                    |ui, visible_rows| {
                        for row_index in visible_rows {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = GRID_SPACING;
                                let start = row_index * columns;
                                let end = (start + columns).min(albums.len());
                                for i in start..end {
                                    let (album_index, album) = &albums[i];
                                    let response = self.entity_card(
                                        ui,
                                        &format!("album:{album}"),
                                        album,
                                        &format!(
                                            "{} tracks",
                                            self.app.album_track_count(album)
                                        ),
                                        self.app.artist_scoped_album_selected
                                            == Some(*album_index),
                                        card_width,
                                        Some(EntityLinkTarget::Album {
                                            album: album.clone(),
                                            artist: Some(artist_name.clone()),
                                        }),
                                        None,
                                    );
                                    if let Some(target) = response.activated {
                                        self.open_entity_page(target);
                                    } else {
                                        if response.item.clicked() {
                                            self.app.artist_scoped_album_selected =
                                                Some(*album_index);
                                        }
                                        if response.item.double_clicked() {
                                            self.app.artist_scoped_album_selected =
                                                Some(*album_index);
                                            self.report(|app| app.play_selected());
                                        }
                                    }
                                }
                            });
                        }
                    },
                );
            } else {
                let selected_position = self.app.artist_scoped_album_selected.and_then(
                    |selected| {
                        albums
                            .iter()
                            .position(|(index, _)| *index == selected)
                    },
                );
                let mut scroll = egui::ScrollArea::vertical().id_salt("artist_albums");
                if jump_to_selection {
                    if let Some(position) = selected_position {
                        scroll = scroll.vertical_scroll_offset(playing_row_scroll_offset(
                            position,
                            60.0,
                            ui.spacing().item_spacing.y,
                            albums.len(),
                            ui.available_height(),
                        ));
                    }
                }
                scroll.show_rows(ui, 60.0, albums.len(), |ui, visible_rows| {
                    for i in visible_rows {
                        let (album_index, album) = &albums[i];
                        let count = self.app.album_track_count(album);
                        let row = self.entity_row(
                            ui,
                            &format!("album:{album}"),
                            album,
                            &format!("{count} tracks"),
                            self.app.artist_scoped_album_selected == Some(*album_index),
                            Some(EntityLinkTarget::Album {
                                album: album.clone(),
                                artist: Some(artist_name.clone()),
                            }),
                            None,
                        );
                        if let Some(target) = row.activated {
                            self.open_entity_page(target);
                        } else {
                            if row.item.clicked() {
                                self.app.artist_scoped_album_selected = Some(*album_index);
                            }
                            if row.item.double_clicked() {
                                self.app.artist_scoped_album_selected = Some(*album_index);
                                self.report(|app| app.play_selected());
                            }
                        }
                    }
                });
            }
            return;
        }
        let artists = self.app.artists.clone();
        if self.app.grid_view && self.app.show_artwork {
            let (columns, card_width) = responsive_grid_layout(
                ui.available_width()
                    - ui.spacing().scroll.bar_width
                    - ui.spacing().scroll.bar_inner_margin,
            );
            let row_count = artists.len().div_ceil(columns);
            let mut scroll = egui::ScrollArea::vertical().id_salt("artists_grid");
            if jump_to_selection {
                if let Some(position) = self.app.artist_selected {
                    scroll = scroll.vertical_scroll_offset(playing_row_scroll_offset(
                        position / columns,
                        card_width + GRID_CARD_FOOTER,
                        ui.spacing().item_spacing.y,
                        row_count,
                        ui.available_height(),
                    ));
                }
            }
            scroll.show_rows(
                ui,
                card_width + GRID_CARD_FOOTER,
                row_count,
                |ui, visible_rows| {
                    for row_index in visible_rows {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = GRID_SPACING;
                            let start = row_index * columns;
                            let end = (start + columns).min(artists.len());
                            for i in start..end {
                                let artist = &artists[i];
                                let album_count = self
                                    .app
                                    .artist_album_counts
                                    .get(artist)
                                    .copied()
                                    .unwrap_or(0);
                                let song_count = self
                                    .app
                                    .tracks
                                    .iter()
                                    .filter(|track| &track.artist == artist)
                                    .count();
                                let response = self.entity_card(
                                    ui,
                                    &format!("artist:{artist}"),
                                    artist,
                                    &format!("{album_count} albums · {song_count} songs"),
                                    self.app.artist_selected == Some(i),
                                    card_width,
                                    Some(EntityLinkTarget::Artist(artist.clone())),
                                    None,
                                );
                                if let Some(target) = response.activated {
                                    self.open_entity_page(target);
                                } else {
                                    if response.item.clicked() {
                                        self.app.artist_selected = Some(i);
                                    }
                                    if response.item.double_clicked() {
                                        self.app.artist_selected = Some(i);
                                        self.app.enter_artist(artist.clone());
                                    }
                                }
                            }
                        });
                    }
                },
            );
        } else {
            let mut scroll = egui::ScrollArea::vertical().id_salt("artists");
            if jump_to_selection {
                if let Some(position) = self.app.artist_selected {
                    scroll = scroll.vertical_scroll_offset(playing_row_scroll_offset(
                        position,
                        60.0,
                        ui.spacing().item_spacing.y,
                        artists.len(),
                        ui.available_height(),
                    ));
                }
            }
            scroll.show_rows(
                ui,
                60.0,
                artists.len(),
                |ui, visible_rows| {
                    for i in visible_rows {
                        let artist = &artists[i];
                        let album_count = self
                            .app
                            .artist_album_counts
                            .get(artist)
                            .copied()
                            .unwrap_or(0);
                        let song_count = self
                            .app
                            .tracks
                            .iter()
                            .filter(|track| &track.artist == artist)
                            .count();
                        let row = self.entity_row(
                            ui,
                            &format!("artist:{artist}"),
                            artist,
                            &format!("{album_count} albums · {song_count} songs"),
                            self.app.artist_selected == Some(i),
                            Some(EntityLinkTarget::Artist(artist.clone())),
                            None,
                        );
                        if let Some(target) = row.activated {
                            self.open_entity_page(target);
                        } else {
                            if row.item.clicked() {
                                self.app.artist_selected = Some(i);
                            }
                            if row.item.double_clicked() {
                                self.app.artist_selected = Some(i);
                                self.app.enter_artist(artist.clone());
                            }
                        }
                    }
                },
            );
        }
    }

    fn playlist_list(&mut self, ui: &mut egui::Ui) {
        let jump_to_selection = std::mem::take(&mut self.pending_scroll_to_selection);
        if let crate::library::PlaylistScope::Open(pi) = &self.app.playlist_scope {
            let pi = *pi;
            ui.horizontal(|ui| {
                if icon_button(ui, Icon::Up, "Move playlist track up").clicked() {
                    self.report(|app| app.move_selected_song(false));
                }
                if icon_button(ui, Icon::Down, "Move playlist track down").clicked() {
                    self.report(|app| app.move_selected_song(true));
                }
            });
            ui.separator();
            let indices = self.app.playlist_track_indices(pi);
            let is_most_played = self
                .app
                .playlists
                .get(pi)
                .is_some_and(|playlist| playlist.name.eq_ignore_ascii_case("most played"));
            let show_metadata = ui.available_width() >= SONG_METADATA_MIN_WIDTH;
            if let Some(sort) = self.track_table_header(ui, show_metadata) {
                self.change_sort(sort);
            }
            let palette = self.palette();
            let target_row = self
                .pending_scroll_to_playing
                .then_some(self.app.playlist_song_selected)
                .flatten();
            let mut scroll = egui::ScrollArea::vertical().id_salt("playlist_tracks");
            if self.pending_scroll_to_playing {
                if let Some(row) = target_row {
                    scroll = scroll.vertical_scroll_offset(playing_row_scroll_offset(
                        row,
                        58.0,
                        ui.spacing().item_spacing.y,
                        indices.len(),
                        ui.available_height(),
                    ));
                }
            } else if jump_to_selection {
                if let Some(row) = self.app.playlist_song_selected {
                    scroll = scroll.vertical_scroll_offset(playing_row_scroll_offset(
                        row,
                        58.0,
                        ui.spacing().item_spacing.y,
                        indices.len(),
                        ui.available_height(),
                    ));
                }
            }
            scroll.show_rows(ui, 58.0, indices.len(), |ui, visible_rows| {
                for position in visible_rows {
                    let Some(&index) = indices.get(position) else {
                        continue;
                    };
                    let Some(track) = self.app.tracks.get(index).cloned() else {
                        continue;
                    };
                    let show_artwork = self.app.show_artwork;
                    let artwork = show_artwork
                        .then(|| self.track_artwork(ui, &track))
                        .flatten();
                    let play_count = if is_most_played {
                        Some(
                            self.app
                                .play_counts
                                .get(&track.path)
                                .map(|count| count.to_string())
                                .unwrap_or_else(|| "0".to_string()),
                        )
                    } else {
                        None
                    };
                    let response = song_row(
                        ui,
                        &track,
                        self.app.playlist_song_selected == Some(position),
                        self.app.current_index == Some(index),
                        self.app.is_favorite(&track.path),
                        show_artwork,
                        show_metadata,
                        artwork.as_ref(),
                        palette,
                        self.accent,
                        play_count,
                    );
                    if response.queue {
                        self.app.playlist_song_selected = Some(position);
                        self.app.queue_selected_track();
                    } else if let Some(target) = response.activated {
                        self.open_entity_page(target);
                    } else {
                        if response.row.clicked() {
                            self.app.playlist_song_selected = Some(position);
                        }

                        if response.row.double_clicked() {
                            self.app.playlist_song_selected = Some(position);
                            self.report(|app| app.activate_selected());
                        }
                    }
                }
            });
            if target_row.is_some() {
                self.pending_scroll_to_playing = false;
            }
            return;
        }
        ui.horizontal(|ui| {
            if ui
                .button("Pin / unpin")
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                self.app.toggle_pin_selected_playlist();
            }
            if ui
                .button("New playlist")
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                self.playlist_name.clear();
                self.app.start_new_playlist();
            }
            if ui
                .button("Delete playlist")
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                self.app.request_delete_playlist();
            }
        });
        ui.separator();
        let playlists = self.app.playlists.clone();
        if self.app.grid_view && self.app.show_artwork {
            let (columns, card_width) = responsive_grid_layout(
                ui.available_width()
                    - ui.spacing().scroll.bar_width
                    - ui.spacing().scroll.bar_inner_margin,
            );
            let row_count = playlists.len().div_ceil(columns);
            let mut scroll = egui::ScrollArea::vertical().id_salt("playlists_grid");
            if jump_to_selection {
                if let Some(position) = self.app.playlist_selected {
                    scroll = scroll.vertical_scroll_offset(playing_row_scroll_offset(
                        position / columns,
                        card_width + GRID_CARD_FOOTER,
                        ui.spacing().item_spacing.y,
                        row_count,
                        ui.available_height(),
                    ));
                }
            }
            scroll.show_rows(
                ui,
                card_width + GRID_CARD_FOOTER,
                row_count,
                |ui, visible_rows| {
                    for row_index in visible_rows {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = GRID_SPACING;
                            let start = row_index * columns;
                            let end = (start + columns).min(playlists.len());
                            for i in start..end {
                                let playlist = &playlists[i];
                                let key = format!("playlist:{}", playlist.name);
                                let title = if matches!(
                                    playlist.name.as_str(),
                                    "History" | "Most Played"
                                ) {
                                    format!("Smart · {}", playlist.name)
                                } else if self.app.pinned_playlists.contains(&playlist.name) {
                                    format!("Pinned · {}", playlist.name)
                                } else {
                                    playlist.name.clone()
                                };
                                let response = self.entity_card(
                                    ui,
                                    &key,
                                    &title,
                                    &format!("{} tracks", playlist.track_paths.len()),
                                    self.app.playlist_selected == Some(i),
                                    card_width,
                                    None,
                                    None,
                                );
                                if response.item.clicked() {
                                    self.app.playlist_selected = Some(i);
                                }
                                if response.item.double_clicked() {
                                    self.app.playlist_selected = Some(i);
                                    self.app.enter_playlist(i);
                                }
                            }
                        });
                    }
                },
            );
        } else {
            let mut scroll = egui::ScrollArea::vertical().id_salt("playlists");
            if jump_to_selection {
                if let Some(position) = self.app.playlist_selected {
                    scroll = scroll.vertical_scroll_offset(playing_row_scroll_offset(
                        position,
                        60.0,
                        ui.spacing().item_spacing.y,
                        playlists.len(),
                        ui.available_height(),
                    ));
                }
            }
            scroll.show_rows(
                ui,
                60.0,
                playlists.len(),
                |ui, visible_rows| {
                    for i in visible_rows {
                        let playlist = &playlists[i];
                        let subtitle = format!(
                            "{}{} tracks",
                            if matches!(playlist.name.as_str(), "History" | "Most Played") {
                                "Smart · "
                            } else if self.app.pinned_playlists.contains(&playlist.name) {
                                "Pinned · "
                            } else {
                                ""
                            },
                            playlist.track_paths.len()
                        );
                        let row = self.entity_row(
                            ui,
                            &format!("playlist:{}", playlist.name),
                            &playlist.name,
                            &subtitle,
                            self.app.playlist_selected == Some(i),
                            None,
                            None,
                        );
                        if row.item.clicked() {
                            self.app.playlist_selected = Some(i);
                        }
                        if row.item.double_clicked() {
                            self.app.playlist_selected = Some(i);
                            self.app.enter_playlist(i);
                        }
                    }
                },
            );
        }
    }

    fn track_table_header(&mut self, ui: &mut egui::Ui, show_metadata: bool) -> Option<SongSort> {
        let active = self.table_sort();
        let headers: &[(&str, SongSort)] = if show_metadata {
            &[
                ("TITLE", SongSort::Title),
                ("ARTIST", SongSort::Artist),
                ("ALBUM", SongSort::Album),
                ("TIME", SongSort::Duration),
            ]
        } else {
            &[
                ("TITLE", SongSort::Title),
                ("TIME", SongSort::Duration),
            ]
        };
        self.table_header(ui, headers, active)
    }

    fn table_header(
        &self,
        ui: &mut egui::Ui,
        headers: &[(&str, SongSort)],
        active: Option<(SongSort, bool)>,
    ) -> Option<SongSort> {
        let palette = self.palette();
        let mut clicked = None;
        egui::Frame::default()
            .fill(self.surface_fill(palette.panel, 226))
            .corner_radius(egui::CornerRadius::same(8))
            .inner_margin(egui::Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.columns(headers.len(), |columns| {
                    for (column, (text, sort)) in columns.iter_mut().zip(headers) {
                        let is_active = active.map(|(mode, _)| mode) == Some(*sort);
                        let descending = active.is_some_and(|(mode, desc)| mode == *sort && desc);
                        let text = if is_active {
                            format!("{text} {}", if descending { "↓" } else { "↑" })
                        } else {
                            text.to_string()
                        };
                        let mut label = egui::RichText::new(text).small().strong();
                        label = if is_active {
                            label.color(self.accent).underline()
                        } else {
                            label.color(palette.muted)
                        };
                        if active.is_none() {
                            column.label(label);
                            continue;
                        }
                        let response = column
                            .add(egui::Label::new(label).sense(egui::Sense::click()))
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .on_hover_text(if is_active {
                                format!("Sorted by {} — click to reverse", (*sort).label())
                            } else {
                                format!("Sort by {}", (*sort).label())
                            });
                        if response.clicked() {
                            clicked = Some(*sort);
                        }
                    }
                });
            });
        clicked
    }

    fn search_window(&mut self, ctx: &egui::Context) {
        if !self.app.search_mode {
            self.search_window_rect = None;
            return;
        }
        let palette = self.palette();
        let screen = ctx.screen_rect();
        let width = (screen.width() - 36.0).clamp(340.0, 620.0);
        let height = (screen.height() - 120.0).clamp(220.0, 520.0);
        let query = self.search_input.trim().to_owned();
        let mut results = Vec::new();
        if !query.is_empty() {
            let mut ranked: Vec<(u8, i64, SearchResult)> = Vec::new();

            for album in &self.app.albums {
                if let Some(score) = fuzzy_match_score(album, &query) {
                    ranked.push((0, score, SearchResult::Album(album.clone())));
                }
            }
            for artist in &self.app.artists {
                if let Some(score) = fuzzy_match_score(artist, &query) {
                    ranked.push((1, score, SearchResult::Artist(artist.clone())));
                }
            }
            for (track_index, track) in self.app.tracks.iter().enumerate() {
                let searchable = format!("{} {} {}", track.title, track.artist, track.album);
                if let Some(score) = fuzzy_match_score(&searchable, &query) {
                    ranked.push((2, score, SearchResult::Track(track_index)));
                }
            }
            for (playlist_idx, playlist) in self.app.playlists.iter().enumerate() {
                for path in &playlist.track_paths {
                    if let Some(track_idx) = self.app.tracks.iter().position(|track| &track.path == path) {
                        let track = &self.app.tracks[track_idx];
                        let searchable = format!(
                            "{} {} {} {}",
                            playlist.name, track.title, track.artist, track.album
                        );
                        if let Some(score) = fuzzy_match_score(&searchable, &query) {
                            ranked.push((
                                3,
                                score,
                                SearchResult::PlaylistTrack { playlist_idx, track_idx },
                            ));
                        }
                    }
                }
            }
            ranked.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.cmp(&a.1)));
            results = ranked.into_iter().map(|(_, _, result)| result).collect();
        }
        // Keep keyboard confirmation and the visible rows on the same ranking.
        self.app.search_results = results.clone();
        let frame = self.popup_frame(ctx);
        let (closed, rect) = popup_window(
            ctx,
            "library_search_window",
            "Search results",
            &format!("{} matches", results.len()),
            width,
            height,
            frame,
            palette,
            |ui| {
                if self.search_input.is_empty() {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            egui::RichText::new("Type to search your library").color(palette.muted),
                        );
                    });
                } else if self.search_debounce_until.is_some() {
                    ui.centered_and_justified(|ui| {
                        ui.label(egui::RichText::new("Searching…").color(palette.muted));
                    });
                } else if results.is_empty() {
                    ui.centered_and_justified(|ui| {
                        let text = if self.app.loading {
                            "Scanning your music library…".to_string()
                        } else {
                            format!("No matches for “{}”", self.search_input)
                        };
                        ui.label(display_rich(text).color(palette.muted));
                    });
                } else {
                    egui::ScrollArea::vertical()
                        .id_salt("floating_search_results")
                        .auto_shrink([false, false])
                        .show_rows(ui, 58.0, results.len(), |ui, visible_rows| {
                            for index in visible_rows {
                                let Some(item) = results.get(index).cloned() else {
                                    continue;
                                };
                                let (
                                    title,
                                    detail,
                                    artwork_key,
                                    track_index,
                                    title_target,
                                    detail_targets,
                                ) = match item {
                                    SearchResult::Track(track_index) => self
                                        .app
                                        .tracks
                                        .get(track_index)
                                        .map(|track| {
                                            (
                                                track.title.clone(),
                                                format!("{} · {}", track.artist, track.album),
                                                format!("album:{}", track.album),
                                                Some(track_index),
                                                None,
                                                vec![
                                                    (
                                                        track.artist.clone(),
                                                        EntityLinkTarget::Artist(
                                                            track.artist.clone(),
                                                        ),
                                                    ),
                                                    (
                                                        track.album.clone(),
                                                        EntityLinkTarget::Album {
                                                            album: track.album.clone(),
                                                            artist: Some(track.artist.clone()),
                                                        },
                                                    ),
                                                ],
                                            )
                                        })
                                        .unwrap_or_default(),
                                    SearchResult::Album(album) => (
                                        album.clone(),
                                        "Album".to_string(),
                                        format!("album:{album}"),
                                        None,
                                        Some(EntityLinkTarget::Album {
                                            album,
                                            artist: None,
                                        }),
                                        Vec::new(),
                                    ),
                                    SearchResult::Artist(artist) => (
                                        artist.clone(),
                                        "Artist".to_string(),
                                        format!("artist:{artist}"),
                                        None,
                                        Some(EntityLinkTarget::Artist(artist)),
                                        Vec::new(),
                                    ),
                                    SearchResult::PlaylistTrack {
                                        playlist_idx,
                                        track_idx,
                                    } => self
                                        .app
                                        .tracks
                                        .get(track_idx)
                                        .map(|track| {
                                            (
                                                track.title.clone(),
                                                format!(
                                                    "{} · {}",
                                                    track.artist,
                                                    self.app
                                                        .playlists
                                                        .get(playlist_idx)
                                                        .map(|playlist| playlist.name.as_str())
                                                        .unwrap_or("Playlist")
                                                ),
                                                format!("album:{}", track.album),
                                                Some(track_idx),
                                                None,
                                                vec![(
                                                    track.artist.clone(),
                                                    EntityLinkTarget::Artist(track.artist.clone()),
                                                )],
                                            )
                                        })
                                        .unwrap_or_default(),
                                };
                                self.app.ensure_artwork_requested(&artwork_key);
                                let thumbnail = self
                                    .app
                                    .artwork
                                    .get(&artwork_key)
                                    .map(crate::library::artwork::Thumbnail::image);
                                let texture = thumbnail
                                    .map(|image| self.texture_for(ui.ctx(), &artwork_key, image));
                                let selected = self.app.search_selected == Some(index);
                                let (row_rect, row) = ui.allocate_exact_size(
                                    egui::vec2(ui.available_width(), 58.0),
                                    egui::Sense::click(),
                                );
                                let row = row.on_hover_cursor(egui::CursorIcon::PointingHand);
                                let mut row_ui = ui.new_child(
                                    egui::UiBuilder::new()
                                        .max_rect(row_rect.shrink2(egui::vec2(8.0, 5.0)))
                                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                                );
                                row_ui.set_clip_rect(row_rect.intersect(ui.clip_rect()));
                                if let Some(texture) = texture {
                                    row_ui.add(
                                        egui::Image::new((texture.id(), egui::vec2(40.0, 40.0)))
                                            .maintain_aspect_ratio(true),
                                    );
                                } else {
                                    let (rect, _) = row_ui.allocate_exact_size(
                                        egui::vec2(40.0, 40.0),
                                        egui::Sense::hover(),
                                    );
                                    row_ui.painter().rect_filled(rect, 5.0, palette.raised);
                                    paint_icon(
                                        row_ui.painter(),
                                        if track_index.is_some() {
                                            Icon::Songs
                                        } else {
                                            Icon::Albums
                                        },
                                        rect.shrink(12.0),
                                        palette.muted,
                                    );
                                }
                                let mut text_ui = row_ui.new_child(
                                    egui::UiBuilder::new()
                                        .max_rect(row_ui.available_rect_before_wrap())
                                        .layout(egui::Layout::top_down(egui::Align::Min)),
                                );
                                let title_response = text_ui
                                    .add(
                                        egui::Label::new(fuzzy_highlight_job(
                                            &title,
                                            &query,
                                            palette.text,
                                            self.accent,
                                            14.0,
                                            true,
                                        ))
                                        .truncate()
                                        .sense(egui::Sense::click()),
                                    )
                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                    .on_hover_text(&title);
                                let title_clicked = if title_response.clicked() {
                                    title_target.clone()
                                } else {
                                    None
                                };
                                let detail_clicked = if detail_targets.is_empty() {
                                    text_ui
                                        .add(
                                            egui::Label::new(fuzzy_highlight_job(
                                                &detail,
                                                &query,
                                                palette.muted,
                                                self.accent,
                                                12.0,
                                                false,
                                            ))
                                            .truncate(),
                                        )
                                        .on_hover_text(&detail);
                                    None
                                } else {
                                    text_ui
                                        .horizontal(|ui| {
                                            detail_targets.iter().enumerate().find_map(
                                                |(detail_index, (label, target))| {
                                                    if detail_index > 0 {
                                                        ui.label("·");
                                                    }
                                                    ui.add(
                                                        egui::Button::new(fuzzy_highlight_job(
                                                            label,
                                                            &query,
                                                            palette.text,
                                                            self.accent,
                                                            12.0,
                                                            false,
                                                        ))
                                                        .frame(false),
                                                    )
                                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                                    .on_hover_text(label)
                                                    .clicked()
                                                    .then(|| target.clone())
                                                },
                                            )
                                        })
                                        .inner
                                };
                                if selected {
                                    ui.painter().rect_stroke(
                                        row_rect,
                                        5.0,
                                        egui::Stroke::new(1.0, self.accent),
                                        egui::StrokeKind::Inside,
                                    );
                                }
                                if let Some(target) = title_clicked.or(detail_clicked) {
                                    self.open_entity_page(target);
                                } else if row.clicked() {
                                    self.app.search_selected = Some(index);
                                    self.report(|app| app.search_confirm());
                                    if !self.app.search_mode {
                                        self.search_input.clear();
                                        self.search_debounce_until = None;
                                    }
                                }
                            }
                        });
                }
            },
        );
        if closed {
            self.search_input.clear();
            self.search_debounce_until = None;
            self.app.exit_search();
        }
        self.search_window_rect = rect;
        let outside_click = ctx.input(|input| {
            input
                .pointer
                .any_click()
                .then(|| input.pointer.interact_pos())
                .flatten()
        });
        if let Some(position) = outside_click {
            let inside_search = self.search_rect.is_some_and(|rect| rect.contains(position));
            let inside_results = self
                .search_window_rect
                .is_some_and(|rect| rect.contains(position));
            if !inside_search && !inside_results {
                self.search_input.clear();
                self.search_debounce_until = None;
                self.app.exit_search();
            }
        }
    }

    fn now_playing_panel(&mut self, ctx: &egui::Context) {
        if !self.shows_now_playing() {
            self.now_playing_collapse.cancel();
            return;
        }
        let palette = self.palette();
        let panel_id = egui::Id::new(format!(
            "now_playing_panel_{}",
            self.now_playing_panel_generation
        ));
        let response = egui::SidePanel::left(panel_id)
            .resizable(true)
            .default_width(330.0)
            .width_range(
                (NOW_PLAYING_COLLAPSE_THRESHOLD - PANEL_SNAP_OVERSHOOT).max(180.0)
                    ..=(self.cover_max_side + 52.0).clamp(
                        (NOW_PLAYING_COLLAPSE_THRESHOLD - PANEL_SNAP_OVERSHOOT).max(180.0),
                        470.0,
                    ),
            )
            .frame(
                egui::Frame::default()
                    .fill(self.surface_fill(palette.panel, 220))
                    .stroke(egui::Stroke::new(1.0, palette.border))
                    .inner_margin(egui::Margin { left: 16, right: 16, top: 16, bottom: 0 })
            )
            .show(ctx, |ui| {
                let track = self.app.current_track().cloned();
                if let Some(track) = track {
                    let key = format!("album:{}", track.album);
                    self.app.ensure_artwork_requested(&key);
                    ui.vertical_centered(|ui| {
                        let badge = self.lyrics_status_label(&track);
                        let favorite = self.app.is_favorite(&track.path);
                        self.artwork(ui, &key, badge, favorite);
                        ui.add_space(9.0);
                        ui.add(
                            egui::Label::new(
                                display_rich(&track.title)
                                    .size(24.0)
                                    .strong()
                                    .color(palette.text),
                            )
                            .truncate(),
                        )
                        .on_hover_text(&track.title);
                        if metadata_link(
                            ui,
                            &track.artist,
                            palette.text,
                            self.accent,
                            16.0,
                            false,
                        )
                        .clicked()
                        {
                            self.open_entity_page(EntityLinkTarget::Artist(
                                track.artist.clone(),
                            ));
                        }
                        if metadata_link(
                            ui,
                            &track.album,
                            palette.text,
                            self.accent,
                            16.0,
                            false,
                        )
                        .clicked()
                        {
                            self.open_entity_page(EntityLinkTarget::Album {
                                album: track.album.clone(),
                                artist: Some(track.artist.clone()),
                            });
                        }
                        ui.label(
                            egui::RichText::new(format!(
                                "{} · {}",
                                track.bitrate_str(),
                                track.sample_rate_str()
                            ))
                            .small()
                            .color(palette.muted),
                        );
                    });
                } else {
                    ui.allocate_ui_with_layout(
                        egui::vec2(ui.available_width(), 170.0),
                        egui::Layout::centered_and_justified(egui::Direction::TopDown),
                        |ui| {
                            ui.label(
                                egui::RichText::new("No track selected")
                                    .color(palette.muted),
                            );
                        },
                    );
                }
                self.selected_actions(ui);
                if ui.available_height() >= UP_NEXT_MIN_HEIGHT {
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new("Up next")
                            .size(18.0)
                            .strong()
                            .color(palette.text),
                    );
                    ui.add_space(4.0);
                    self.queue_panel(ui);
                }
            });
        let panel_rect = response.response.rect;
        let resize = ctx.read_response(panel_id.with("__resize"));
        let width = live_panel_width(resize.as_ref(), panel_rect.left(), panel_rect.width());
        let collapse = self.now_playing_collapse.update_collapse_under(
            resize.as_ref().is_some_and(|response| response.dragged()),
            resize.as_ref().is_some_and(|response| response.drag_stopped()),
            width,
            NOW_PLAYING_COLLAPSE_THRESHOLD,
        );
        if self.now_playing_collapse.is_armed() {
            self.paint_panel_collapse_hint(ctx, panel_rect, "now_playing_collapse_dim");
        }
        if collapse {
            self.now_playing_panel_generation =
                self.now_playing_panel_generation.wrapping_add(1);
            self.toggle_now_playing();
        }
    }

    fn lyrics_side_panel(&mut self, ctx: &egui::Context) {
        if !self.lyrics_panel_open {
            self.lyrics_collapse.cancel();
            self.lyrics_maximize_collapse.cancel();
            return;
        }
        let palette = self.palette();
        let frame = egui::Frame::default()
            .fill(self.surface_fill(palette.panel, LYRICS_PANEL_FILL_ALPHA))
            .stroke(egui::Stroke::new(1.0, palette.border))
            .inner_margin(18.0);
        if (self.shows_library() || self.settings_open) && !self.lyrics_takes_over(ctx) {
            let panel_id = egui::Id::new(format!(
                "lyrics_panel_{}",
                self.lyrics_panel_generation
            ));
            let screen_width = ctx.screen_rect().width();
            let max_threshold = screen_width * LYRICS_TAKE_OVER_THRESHOLD;
            let max_width = (max_threshold + PANEL_SNAP_OVERSHOOT).min(screen_width);
            let min_width = LYRICS_COLLAPSE_THRESHOLD.min(max_width);
            let default_width = self.lyrics_panel_width.clamp(min_width, max_width);
            let response = egui::SidePanel::right(panel_id)
                .resizable(true)
                .default_width(default_width)
                .width_range(min_width..=max_width)
                .frame(frame)
                .show(ctx, |ui| self.lyrics_panel_content(ui));
            let panel_rect = response.response.rect;
            self.lyrics_live_width = panel_rect.width();
            // A panel dragged into the maximize zone is about to take over the
            // window, so that width must not become the remembered size.
            if panel_rect.width() < max_threshold {
                self.lyrics_panel_width = panel_rect.width();
            }
            let resize = ctx.read_response(panel_id.with("__resize"));
            let width = live_panel_width(resize.as_ref(), panel_rect.right(), panel_rect.width());
            
            let minimize_collapse = self.lyrics_collapse.update_collapse_under(
                resize.as_ref().is_some_and(|response| response.dragged()),
                resize.as_ref().is_some_and(|response| response.drag_stopped()),
                width,
                LYRICS_COLLAPSE_THRESHOLD,
            );
            
            let _ = self.lyrics_maximize_collapse.update_collapse_over(
                resize.as_ref().is_some_and(|response| response.dragged()),
                resize.as_ref().is_some_and(|response| response.drag_stopped()),
                width,
                max_threshold,
            );

            if self.lyrics_collapse.is_armed() {
                self.paint_panel_collapse_hint(ctx, panel_rect, "lyrics_collapse_dim");
            }
            if minimize_collapse {
                self.set_lyrics_panel_open(false);
            }
        } else {
            self.lyrics_collapse.cancel();
            self.lyrics_maximize_collapse.cancel();
            egui::CentralPanel::default()
                .frame(frame)
                .show(ctx, |ui| self.lyrics_panel_content(ui));
        }
    }

    fn lyrics_panel_content(&mut self, ui: &mut egui::Ui) {
        if ui.rect_contains_pointer(ui.max_rect())
            && ui.input(|input| input.raw_scroll_delta.y != 0.0)
        {
            self.lyrics_auto_follow = false;
        }
        ui.horizontal(|ui| {
            ui.heading("Lyrics");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icon_toggle_button(
                    ui,
                    Icon::Down,
                    "Follow the current lyric line",
                    self.lyrics_auto_follow,
                )
                .clicked()
                {
                    self.lyrics_auto_follow = !self.lyrics_auto_follow;
                }
                if ui
                    .button(if self.app.lyrics_sync_mode {
                        "Finish sync"
                    } else {
                        "Sync"
                    })
                    .clicked()
                {
                    if self.app.lyrics_sync_mode {
                        self.report(|app| app.finish_lyrics_sync(false));
                    } else {
                        self.lyrics_auto_follow = true;
                        self.app.start_lyrics_sync();
                    }
                }
                if ui
                    .button("Edit")
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    self.open_lyrics_editor();
                }
                if !self.app.lyrics_sync_mode && self.app.current_track().is_some() {
                    let (label, hint) = if self.app.lyrics_flagged {
                        ("Unflag", "Show these lyrics again")
                    } else {
                        ("Flag", "Flag these lyrics as wrong and hide them")
                    };
                    if ui
                        .button(label)
                        .on_hover_text(hint)
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        if self.app.lyrics_flagged {
                            self.report(|app| app.unflag_current_lyrics());
                        } else {
                            self.report(|app| app.flag_current_lyrics());
                        }
                    }
                }
            });
        });
        ui.separator();
        self.lyrics_panel(ui);
    }

    fn lyrics_status_label(
        &self,
        track: &crate::library::track::Track,
    ) -> Option<&'static str> {
        if self.app.lyrics_loading {
            return None;
        }
        if self.app.lyrics_flagged {
            return Some("lyrics hidden");
        }
        let local = crate::lyrics::has_manual(&track.path);
        match self.app.lyrics.as_ref() {
            Some(lyrics) if lyrics.synced => Some(if local {
                "local sync lyrics"
            } else {
                "auto sync lyrics"
            }),
            Some(_) => Some(if local { "local lyrics" } else { "auto lyrics" }),
            None if local => Some("local lyrics"),
            None => Some("no lyrics"),
        }
    }

    fn artwork(&mut self, ui: &mut egui::Ui, key: &str, badge: Option<&str>, favorite: bool) {
        let height_limit = (ui.available_height() * 0.42).max(100.0);
        self.cover_max_side = height_limit;
        // Shrink the cover slightly while paused so play/pause has a visible
        // physical cue. The panel width is unaffected because it is based on
        // the unshrunk cover size.
        let paused = self.app.player.is_paused();
        let motion = if ui.style().animation_time > 0.0 {
            ui.ctx().animate_bool_with_time(
                ui.make_persistent_id("now_playing_cover_pause"),
                paused,
                0.22,
            )
        } else {
            f32::from(paused)
        };
        let full_side = ui.available_width().min(height_limit);
        let side = full_side * (1.0 - 0.06 * motion);
        let palette = self.palette();
        let radius = 10.0;
        // Reserve the full cover footprint so shrinking the artwork while
        // paused does not shift the title, artist, or controls around it.
        let (outer_rect, _) =
            ui.allocate_exact_size(egui::vec2(full_side, full_side), egui::Sense::hover());
        let image_rect = egui::Rect::from_center_size(outer_rect.center(), egui::vec2(side, side));
        if let Some(thumb) = self.app.artwork.get(key) {
            let texture = self.texture_for(ui.ctx(), &format!("{key}:art"), thumb.image());
            egui::Image::new((texture.id(), egui::vec2(side, side)))
                .maintain_aspect_ratio(true)
                .corner_radius(radius)
                .paint_at(ui, image_rect);
        } else {
            ui.painter().rect_filled(image_rect, radius, palette.raised);
            paint_icon(
                ui.painter(),
                Icon::Songs,
                image_rect.shrink(side * 0.4),
                palette.muted,
            );
        }
        ui.painter().rect_stroke(
            image_rect,
            radius,
            egui::Stroke::new(1.0, self.accent),
            egui::StrokeKind::Inside,
        );
        if let Some(label) = badge {
            paint_lyrics_badge(ui.painter(), image_rect, label, palette, self.accent);
        }
        if favorite {
            paint_favorite_badge(ui.painter(), image_rect, palette.background, 24.0);
        }
    }

    fn track_artwork(
        &mut self,
        ui: &mut egui::Ui,
        track: &crate::library::track::Track,
    ) -> Option<egui::TextureHandle> {
        let key = format!("album:{}", track.album);
        self.app.ensure_artwork_requested(&key);
        let image = self
            .app
            .artwork
            .get(&key)
            .map(crate::library::artwork::Thumbnail::image);
        image.map(|image| self.texture_for(ui.ctx(), &key, image))
    }

    fn entity_row(
        &mut self,
        ui: &mut egui::Ui,
        key: &str,
        title: &str,
        subtitle: &str,
        selected: bool,
        _title_target: Option<EntityLinkTarget>,
        _subtitle_target: Option<EntityLinkTarget>,
    ) -> EntityItemResponse {
        let title_target: Option<EntityLinkTarget> = None;
        let subtitle_target: Option<EntityLinkTarget> = None;
        let palette = self.palette();
        let render_artwork =
            self.app.show_artwork && ui.available_rect_before_wrap().intersects(ui.clip_rect());
        let art_key = self
            .playlist_cover_key(key)
            .unwrap_or_else(|| key.to_string());
        let texture = if render_artwork {
            self.app
                .artwork
                .get(&art_key)
                .map(crate::library::artwork::Thumbnail::image)
                .map(|image| self.texture_for(ui.ctx(), &art_key, image))
        } else {
            None
        };
        let fill = if selected {
            with_alpha(palette.raised, 170)
        } else {
            egui::Color32::TRANSPARENT
        };
        let (rect, item) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), 60.0), egui::Sense::click());
        let item = item.on_hover_cursor(egui::CursorIcon::PointingHand);
        ui.painter().rect_filled(rect, 0.0, fill);
        let mut row_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect.shrink2(egui::vec2(10.0, 5.0)))
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        row_ui.set_clip_rect(rect.intersect(ui.clip_rect()));
        if self.app.show_artwork {
            if let Some(texture) = texture {
                row_ui.add(
                    egui::Image::new((texture.id(), egui::vec2(38.0, 38.0)))
                        .maintain_aspect_ratio(true),
                );
            } else {
                let (image_rect, _) =
                    row_ui.allocate_exact_size(egui::vec2(38.0, 38.0), egui::Sense::hover());
                row_ui.painter().rect_filled(image_rect, 5.0, palette.panel);
                paint_icon(
                    row_ui.painter(),
                    Icon::Albums,
                    image_rect.shrink(12.0),
                    palette.muted,
                );
            }
        }
        let mut text_ui = row_ui.new_child(
            egui::UiBuilder::new()
                .max_rect(row_ui.available_rect_before_wrap())
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        let title_clicked = if let Some(target) = title_target {
            metadata_link(&mut text_ui, title, palette.text, self.accent, 16.0, true)
                .clicked()
                .then_some(target)
        } else {
            text_ui
                .add(egui::Label::new(display_rich(title).size(16.0).strong()).truncate())
                .on_hover_text(title);
            None
        };
        let subtitle_color = if matches!(&subtitle_target, Some(EntityLinkTarget::Artist(_))) {
            palette.text
        } else {
            palette.muted
        };
        let subtitle_clicked = if let Some(target) = subtitle_target {
            metadata_link(
                &mut text_ui,
                subtitle,
                subtitle_color,
                self.accent,
                13.0,
                false,
            )
            .clicked()
            .then_some(target)
        } else {
            text_ui
                .add(
                    egui::Label::new(display_rich(subtitle).small().color(subtitle_color))
                        .truncate(),
                )
                .on_hover_text(subtitle);
            None
        };
        if self.app.show_artwork && item.rect.intersects(ui.clip_rect()) {
            self.app.ensure_artwork_requested(&art_key);
        }
        EntityItemResponse {
            item,
            activated: title_clicked.or(subtitle_clicked),
        }
    }

    fn texture_cache_key(&self, key: &str) -> String {
        let base = key
            .strip_suffix(":ambient_blur")
            .or_else(|| key.strip_suffix(":art"))
            .unwrap_or(key);
        let revision = self.app.artwork_revisions.get(base).copied().unwrap_or(0);
        format!("{key}@{revision}")
    }

    fn texture_for(
        &mut self,
        ctx: &egui::Context,
        key: &str,
        image: DynamicImage,
    ) -> egui::TextureHandle {
        let cache_key = self.texture_cache_key(key);
        if let Some(texture) = self.textures.get(&cache_key) {
            return texture.clone();
        }
        let rgba = image.to_rgba8();
        let color = egui::ColorImage::from_rgba_unmultiplied(
            [rgba.width() as usize, rgba.height() as usize],
            rgba.as_raw(),
        );
        let texture = ctx.load_texture(&cache_key, color, egui::TextureOptions::LINEAR);
        if self.textures.len() >= 128 {
            if let Some(oldest_key) = self.textures.keys().next().cloned() {
                self.textures.remove(&oldest_key);
            }
        }
        self.textures.insert(cache_key, texture.clone());
        texture
    }

    fn queue_panel(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        ui.set_width(ui.available_width());

        let list_height = ui.available_height().max(0.0);
        let max_rows = ((list_height / 36.0).ceil() as usize + 1).clamp(4, 40);

        if !self.app.queue.is_empty() {
            let queued = self.app.queue.clone();
            egui::ScrollArea::vertical()
                .max_height(list_height)
                .auto_shrink([false, false])
                .id_salt("queued_tracks")
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    for (position, index) in queued.iter().copied().enumerate().take(max_rows) {
                        let Some(track) = self.app.tracks.get(index).cloned() else {
                            continue;
                        };
                        ui.horizontal(|ui| {
                            ui.set_width(ui.available_width());
                            if self.queue_track_cover(ui, &track, "Play this queued song") {
                                self.report(|app| app.play_queued_track(position));
                            }

                            let text_width = (ui.available_width()
                                - 20.0
                                - ui.spacing().item_spacing.x)
                                .max(0.0);
                            ui.allocate_ui_with_layout(
                                egui::vec2(text_width, 34.0),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    ui.set_width(text_width);
                                    ui.add(
                                        egui::Label::new(display_rich(&track.title).strong())
                                            .truncate(),
                                    )
                                    .on_hover_text(&track.title);
                                    if metadata_link(
                                        ui,
                                        &track.artist,
                                        palette.muted,
                                        self.accent,
                                        12.0,
                                        false,
                                    )
                                    .clicked()
                                    {
                                        self.open_entity_page(EntityLinkTarget::Artist(
                                            track.artist.clone(),
                                        ));
                                    }
                                },
                            );
                            if small_remove_button(ui, "Remove from queue").clicked() {
                                self.app.remove_queued_track(position);
                            }
                        });
                    }
                });
            return;
        }

        let queue = self.queue_indices();
        egui::ScrollArea::vertical()
            .max_height(list_height)
            .auto_shrink([false, false])
            .id_salt("up_next")
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                if queue.is_empty() {
                    ui.label(
                        egui::RichText::new("Play a song to see what’s next")
                            .color(palette.muted),
                    );
                }
                for index in queue.into_iter().take(max_rows) {
                    if let Some(track) = self.app.tracks.get(index).cloned() {
                        ui.horizontal(|ui| {
                            ui.set_width(ui.available_width());
                            if self.queue_track_cover(ui, &track, "Play next") {
                                self.report(|app| app.play_index(index));
                            }
                            ui.vertical(|ui| {
                                ui.add(
                                    egui::Label::new(display_rich(&track.title).strong())
                                        .truncate(),
                                )
                                .on_hover_text(&track.title);
                                if metadata_link(
                                    ui,
                                    &track.artist,
                                    palette.text,
                                    self.accent,
                                    12.0,
                                    false,
                                )
                                .clicked()
                                {
                                    self.open_entity_page(EntityLinkTarget::Artist(
                                        track.artist.clone(),
                                    ));
                                }
                            });
                        });
                    }
                }
            });
    }

    fn queue_track_cover(
        &mut self,
        ui: &mut egui::Ui,
        track: &crate::library::track::Track,
        tooltip: &str,
    ) -> bool {
        let palette = self.palette();
        let key = format!("album:{}", track.album);
        self.app.ensure_artwork_requested(&key);
        let cache_key = self.texture_cache_key(&key);
        let texture = if let Some(texture) = self.textures.get(&cache_key) {
            Some(texture.clone())
        } else {
            let image = self
                .app
                .artwork
                .get(&key)
                .map(crate::library::artwork::Thumbnail::image);
            image.map(|image| self.texture_for(ui.ctx(), &key, image))
        };
        let (rect, response) =
            ui.allocate_exact_size(egui::vec2(34.0, 34.0), egui::Sense::click());
        let response = response
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(tooltip);
        if let Some(texture) = texture {
            egui::Image::new((texture.id(), rect.size()))
                .uv(centered_image_uv(texture.size_vec2(), rect.size()))
                .corner_radius(4.0)
                .paint_at(ui, rect);
        } else {
            ui.painter().rect_filled(rect, 4.0, palette.raised);
            paint_icon(
                ui.painter(),
                Icon::Songs,
                rect.shrink(10.0),
                palette.muted,
            );
        }
        response.clicked()
    }

    fn queue_indices(&self) -> Vec<usize> {
        let mut indices = match &self.app.play_context {
            PlayContext::AllTracks => self.app.all_track_indices.clone(),
            PlayContext::Album => self.app.scoped_track_indices.clone(),
            PlayContext::Playlist(index) => self.app.playlist_track_indices(*index),
        };
        if self.app.shuffle && !self.app.shuffle_order.is_empty() {
            indices = self.app.shuffle_order.clone();
        }
        let Some(current) = self.app.current_index else {
            return Vec::new();
        };
        let Some(position) = indices.iter().position(|&index| index == current) else {
            return Vec::new();
        };
        let mut upcoming = indices
            .iter()
            .skip(position + 1)
            .copied()
            .collect::<Vec<_>>();
        match &self.app.repeat {
            RepeatMode::One => vec![current],
            RepeatMode::All => {
                upcoming.extend(indices.iter().take(position).copied());
                upcoming
            }
            RepeatMode::Off => upcoming,
        }
    }

    fn queue_window(&mut self, ctx: &egui::Context) {
        if !self.queue_open {
            return;
        }
        let mut open = self.queue_open;
        egui::Window::new("Playback queue")
            .open(&mut open)
            .resizable(true)
            .default_size([420.0, 480.0])
            .show(ctx, |ui| self.queue_panel(ui));
        self.queue_open = open;
    }

    fn lyrics_panel(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        if self.app.lyrics_sync_mode {
            self.lyrics_sync_panel(ui);
            return;
        }
        if self.app.lyrics_loading {
            ui.label(egui::RichText::new("Looking up lyrics…").color(palette.muted));
            return;
        }
        if self.app.lyrics_flagged {
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("Lyrics hidden because they were flagged as wrong")
                    .color(palette.muted),
            );
            ui.add_space(4.0);
            if ui.button("Show lyrics again").clicked() {
                self.report(|app| app.unflag_current_lyrics());
            }
            return;
        }
        let Some(lyrics) = self.app.lyrics.as_ref() else {
            if self.app.lyrics_error {
                ui.label(egui::RichText::new("Lyrics unavailable").color(palette.muted));
            } else {
                ui.label(
                    egui::RichText::new("Select a song to view its lyrics").color(palette.muted),
                );
            }
            return;
        };
        if lyrics.lines.is_empty() {
            ui.label(egui::RichText::new("No lyrics found for this track").color(palette.muted));
            return;
        }

        let revision = self.app.lyrics_revision;
        if self
            .lyrics_display_cache
            .as_ref()
            .is_none_or(|cache| cache.revision != revision)
        {
            self.lyrics_display_cache = Some(LyricsDisplayCache {
                revision,
                lines: lyrics
                    .lines
                    .iter()
                    .map(|line| display_text(&line.text))
                    .collect(),
            });
        }
        let display_lines = &self.lyrics_display_cache.as_ref().unwrap().lines;

        let synced = lyrics.synced;
        let current = synced
            .then(|| lyrics.current_line(self.app.player.elapsed()))
            .flatten();
        let auto_follow = self.lyrics_auto_follow;
        let font_size = self.lyrics_font_size;
        let accent = self.accent;
        let mut seek_to = None;

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .id_salt("lyrics_scroll")
            .show(ui, |ui| {
                for (index, line) in lyrics.lines.iter().enumerate() {
                    let selected = current == Some(index);
                    let text = egui::RichText::new(&display_lines[index])
                        .color(if selected {
                            accent
                        } else {
                            palette.muted.gamma_multiply(0.82)
                        })
                        .size(if selected {
                            font_size + 6.0
                        } else {
                            font_size
                        });
                    let text = if selected { text.strong() } else { text };
                    let label = egui::Label::new(text).wrap().halign(egui::Align::Center);
                    let label = if synced {
                        label.sense(egui::Sense::click())
                    } else {
                        label
                    };
                    let response = ui
                        .with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                            ui.add(label)
                        })
                        .inner
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text(&line.text);
                    if selected && auto_follow {
                        ui.scroll_to_rect_animation(
                            response.rect,
                            Some(egui::Align::Center),
                            egui::style::ScrollAnimation::none(),
                        );
                    }
                    if synced && response.clicked() {
                        seek_to = Some(line.time);
                    }
                    ui.add_space(5.0);
                }
            });

        if let Some(time) = seek_to {
            self.report(|app| app.seek_to_lyric(time));
        }
    }

    fn lyrics_sync_panel(&mut self, ui: &mut egui::Ui) {
        let palette = self.palette();
        let total = self.app.lyrics_sync_lines.len();
        let selected = self
            .app
            .lyrics_sync_index
            .unwrap_or(0)
            .min(total.saturating_sub(1));
        let elapsed = self.app.player.elapsed();

        egui::Frame::default()
            .fill(self.surface_fill(palette.raised, 210))
            .corner_radius(egui::CornerRadius::same(8))
            .inner_margin(egui::Margin::symmetric(10, 6))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} {}",
                            egui_phosphor::regular::TIMER,
                            crate::lyrics::format_timestamp(elapsed)
                        ))
                        .strong()
                        .size(18.0)
                        .color(self.accent),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            egui::RichText::new(format!(
                                "Line {}/{}",
                                selected + 1,
                                total.max(1)
                            ))
                            .small()
                            .color(palette.muted),
                        );
                    });
                });
            });
        ui.add_space(8.0);

        let sync_lines = self.app.lyrics_sync_lines.clone();
        let mut seek_to = None;
        let mut select_line = None;
        let scroll_height = (ui.available_height() - 48.0).max(40.0);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .max_height(scroll_height)
            .id_salt("lyrics_sync_scroll")
            .show(ui, |ui| {
                for (index, line) in sync_lines.iter().enumerate() {
                    let is_selected = index == selected;
                    let text = display_rich(&line.text)
                        .color(if is_selected {
                            self.accent
                        } else if line.time.is_some() {
                            palette.text
                        } else {
                            palette.muted.gamma_multiply(0.82)
                        })
                        .size(if is_selected {
                            self.lyrics_font_size + 6.0
                        } else {
                            self.lyrics_font_size
                        });
                    let text = if is_selected { text.strong() } else { text };
                    let label = egui::Label::new(text)
                        .wrap()
                        .halign(egui::Align::Center)
                        .sense(egui::Sense::click());
                    let response = ui
                        .with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                            ui.add(label)
                        })
                        .inner
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .on_hover_text(&line.text);
                    if is_selected && self.lyrics_auto_follow {
                        ui.scroll_to_rect_animation(
                            response.rect,
                            Some(egui::Align::Center),
                            egui::style::ScrollAnimation::none(),
                        );
                    }
                    if response.clicked() {
                        self.lyrics_auto_follow = true;
                        select_line = Some(index);
                        if let Some(time) = line.time {
                            seek_to = Some(time);
                        }
                    }
                    ui.add_space(5.0);
                }
            });

        if let Some(index) = select_line {
            self.app.lyrics_sync_index = Some(index);
        }
        if let Some(time) = seek_to {
            self.report(|app| app.seek_to_lyric(time));
        }

        ui.separator();
        ui.horizontal(|ui| {
            if ui
                .button("Stamp current time")
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                self.lyrics_auto_follow = true;
                self.report(|app| app.stamp_lyrics_sync_line());
            }
            if ui
                .button("Finish")
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                self.report(|app| app.finish_lyrics_sync(false));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new("Space play/pause · Esc finish")
                        .small()
                        .color(palette.muted),
                );
            });
        });
    }

    fn open_lyrics_editor(&mut self) {
        let Some(track) = self.app.current_track() else {
            self.app.show_notice("Select a song before editing lyrics");
            return;
        };
        let path = crate::lyrics::manual_lyrics_path(&track.path);
        self.lyrics_draft = match std::fs::read_to_string(&path) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => format!(
                "[ti:{}]\n[ar:{}]\n[al:{}]\n[by:aghani]\n\n",
                track.title, track.artist, track.album
            ),
            Err(error) => {
                self.app
                    .show_notice(format!("Could not read lyrics: {error}"));
                return;
            }
        };
        self.lyrics_editor_open = true;
    }

    fn lyrics_editor(&mut self, ctx: &egui::Context) {
        if !self.lyrics_editor_open {
            return;
        }
        let palette = self.palette();
        let screen = ctx.screen_rect();
        let width = (screen.width() - 36.0).clamp(340.0, 620.0);
        let height = (screen.height() - 120.0).clamp(320.0, 640.0);
        let subtitle = self
            .app
            .current_track()
            .map(|track| format!("{} · {}", track.title, track.artist))
            .unwrap_or_default();
        let frame = self.popup_frame(ctx);
        let (closed, _) = popup_window(
            ctx,
            "lyrics_editor_window",
            "Edit lyrics",
            &subtitle,
            width,
            height,
            frame,
            palette,
            |ui| {
                ui.label(
                    egui::RichText::new(
                        "Plain text or LRC timestamps are supported. These lyrics are saved locally for this song.",
                    )
                    .small()
                    .color(palette.muted),
                );
                ui.add_space(8.0);
                let editor_height = (ui.available_height() - 44.0).max(120.0);
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .max_height(editor_height)
                    .id_salt("lyrics_editor_scroll")
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut self.lyrics_draft)
                                .font(egui::TextStyle::Monospace)
                                .desired_width(f32::INFINITY)
                                .desired_rows(18),
                        );
                    });
                ui.add_space(6.0);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button("Save lyrics")
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        if let Some(track) = self.app.current_track().cloned() {
                            let path = crate::lyrics::manual_lyrics_path(&track.path);
                            let result = path
                                .parent()
                                .context("Lyrics cache has no parent directory")
                                .and_then(|parent| {
                                    std::fs::create_dir_all(parent)
                                        .context("Could not create lyrics cache")
                                })
                                .and_then(|()| {
                                    std::fs::write(&path, &self.lyrics_draft)
                                        .context("Could not save lyrics")
                                });
                            match result {
                                Ok(()) => {
                                    let _ = crate::lyrics::unflag_lyrics(&track.path);
                                    self.app.reload_lyrics();
                                    self.lyrics_editor_open = false;
                                    self.app.show_notice("Lyrics saved");
                                }
                                Err(error) => self.app.show_notice(format!("{error:#}")),
                            }
                        }
                    }
                    if ui
                        .button("Cancel")
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        self.lyrics_editor_open = false;
                    }
                });
            },
        );
        if closed {
            self.lyrics_editor_open = false;
        }
    }

    fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            // Measure again the next time Settings opens.
            self.cache_info = None;
            return;
        }
        let palette = self.palette();
        egui::CentralPanel::default()
            .frame(
                egui::Frame::default()
                    .fill(self.surface_fill(palette.background, 214))
                    .inner_margin(egui::Margin::symmetric(22, 18)),
            )
            .show(ctx, |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(14.0, 12.0);
                ui.spacing_mut().interact_size.y = 34.0;
                ui.horizontal(|ui| {
                    ui.heading("Settings");
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            if ui
                                .button("Done")
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .clicked()
                            {
                                self.settings_open = false;
                            }
                        },
                    );
                });
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.heading("Appearance");
                    egui::ComboBox::from_label("Theme")
                        .selected_text(theme_label(&self.config.gui.theme))
                        .show_ui(ui, |ui| {
                            for (theme, label) in THEMES {
                                if ui
                                    .selectable_label(self.config.gui.theme == theme, label)
                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                    .clicked()
                                {
                                    self.set_theme(theme);
                                }
                            }
                        });
                    ui.checkbox(
                        &mut self.app.dynamic_theme,
                        "Extract accent colors from album artwork",
                    )
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                    egui::ComboBox::from_label("Artwork palette")
                        .selected_text(&self.app.dynamic_theme_style)
                        .show_ui(ui, |ui| {
                            for style in ["vibrant", "muted", "dark", "light"] {
                                ui.selectable_value(
                                    &mut self.app.dynamic_theme_style,
                                    style.to_string(),
                                    style,
                                )
                                .on_hover_cursor(egui::CursorIcon::PointingHand);
                            }
                        });
                    egui::ComboBox::from_label("Static accent")
                        .selected_text(&self.config.colors.accent)
                        .show_ui(ui, |ui| {
                            for color in ["Blue", "Cyan", "Green", "Yellow", "Red", "Magenta"] {
                                ui.selectable_value(
                                    &mut self.config.colors.accent,
                                    color.to_string(),
                                    color,
                                )
                                .on_hover_cursor(egui::CursorIcon::PointingHand);
                            }
                        });
                    egui::ComboBox::from_label("Layout")
                        .selected_text(&self.app.layout_mode)
                        .show_ui(ui, |ui| {
                            for (idx, layout) in ["default", "clean", "compact", "minimal"].iter().enumerate() {
                                if ui
                                    .selectable_label(self.app.layout_mode.starts_with(layout), *layout)
                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                    .clicked()
                                {
                                    self.app.set_layout(idx);
                                }
                            }
                        });
                    ui.checkbox(&mut self.app.show_cover, "Show cover panel in TUI")
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    ui.checkbox(&mut self.app.show_player, "Show player panel in TUI")
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    ui.add(egui::Slider::new(&mut self.app.cover_width, 20..=80).text("TUI cover width"));
                    ui.add(egui::Slider::new(&mut self.app.cover_height, 12..=40).text("TUI cover height"));
                    ui.add(egui::Slider::new(&mut self.app.player_height, 8..=24).text("TUI player height"));
                    ui.separator();
                    ui.heading("Reading");
                    ui.add(
                        egui::Slider::new(&mut self.lyrics_font_size, 16.0..=32.0)
                            .text("Lyrics font size")
                            .suffix(" px"),
                    );
                    ui.checkbox(
                        &mut self.config.gui.show_waveform,
                        "Show waveform in playback bar",
                    )
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                    ui.separator();
                    ui.heading("Library");
                    ui.checkbox(
                        &mut self.config.gui.hide_single_track_albums,
                        "Hide albums with one song",
                    )
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                    ui.label(display_rich(format!(
                        "Current music folder: {}",
                        self.app.music_dir.display()
                    )));
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.music_dir_input)
                                .hint_text("/path/to/your/music")
                                .margin(egui::vec2(10.0, 8.0))
                                .desired_width((ui.available_width() - 90.0).max(120.0)),
                        );
                        if ui
                            .button("Apply")
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            self.apply_music_dir_input();
                        }
                    });
                    ui.label(
                        egui::RichText::new("The new folder is loaded the next time you start Aghani.")
                            .small()
                            .color(palette.muted),
                    );
                    ui.separator();
                    ui.heading("Cache");
                    if self.cache_info.is_none() {
                        self.cache_info = Some(cli::cache_info());
                    }
                    if let Some(info) = self.cache_info.clone() {
                        ui.label(display_rich(format!("Cache folder: {}", info.dir.display())));
                        ui.label(format!("Total size: {}", cli::format_size(info.total)));
                        ui.label(
                            egui::RichText::new(format!(
                                "Music data {} · Auto lyrics {} · Your lyrics {}",
                                cli::format_size(info.music_data),
                                cli::format_size(info.auto_lyrics),
                                cli::format_size(info.manual_lyrics),
                            ))
                            .small()
                            .color(palette.muted),
                        );
                    }
                    if ui
                        .button("Clear cache")
                        .on_hover_text(
                            "Removes the music data cache and auto-fetched lyrics. Lyrics you wrote or synced are kept.",
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        cli::clear_regenerable_caches();
                        self.cache_info = None;
                        self.app
                            .show_notice("Cache cleared. It is rebuilt as you use the app");
                    }
                    ui.separator();
                    ui.heading("Keyboard shortcuts");
                    ui.label(egui::RichText::new("Shortcuts are shared by both interfaces; in the GUI, uppercase L opens lyrics and the panel's Edit action opens the editor.").small().color(palette.muted));
                    ui.add_space(5.0);
                    egui::Grid::new("key_settings").num_columns(2).striped(true).show(ui, |ui| {
                        key_field(ui, "Play / pause", &mut self.app.keys.play_pause); key_field_end(ui);
                        key_field(ui, "Next track", &mut self.app.keys.next); key_field_end(ui);
                        key_field(ui, "Previous track", &mut self.app.keys.prev); key_field_end(ui);
                        key_field(ui, "Seek forward", &mut self.app.keys.seek_forward); key_field_end(ui);
                        key_field(ui, "Seek backward", &mut self.app.keys.seek_backward); key_field_end(ui);
                        key_field(ui, "Volume up", &mut self.app.keys.volume_up); key_field_end(ui);
                        key_field(ui, "Volume down", &mut self.app.keys.volume_down); key_field_end(ui);
                        key_field(ui, "Select next", &mut self.app.keys.select_down); key_field_end(ui);
                        key_field(ui, "Select previous", &mut self.app.keys.select_up); key_field_end(ui);
                        key_field(ui, "Activate selection", &mut self.app.keys.confirm); key_field_end(ui);
                        key_field(ui, "Next library tab", &mut self.app.keys.tab_next); key_field_end(ui);
                        key_field(ui, "Previous library tab", &mut self.app.keys.tab_prev); key_field_end(ui);
                        key_field(ui, "Exit scope", &mut self.app.keys.exit_scope); key_field_end(ui);
                        key_field(ui, "Search", &mut self.app.keys.search); key_field_end(ui);
                        key_field(ui, "Shuffle", &mut self.app.keys.shuffle); key_field_end(ui);
                        key_field(ui, "Repeat", &mut self.app.keys.toggle_repeat); key_field_end(ui);
                        key_field(ui, "Add to favorites", &mut self.app.keys.add_to_favorites); key_field_end(ui);
                        key_field(ui, "Save to playlist", &mut self.app.keys.save_to_playlist); key_field_end(ui);
                        key_field(ui, "Delete from playlist", &mut self.app.keys.delete_from_playlist); key_field_end(ui);
                        key_field(ui, "New playlist", &mut self.app.keys.new_playlist); key_field_end(ui);
                        key_field(ui, "Delete playlist", &mut self.app.keys.delete_playlist); key_field_end(ui);
                        key_field(ui, "Go to playing", &mut self.app.keys.go_to_playing); key_field_end(ui);
                        key_field(ui, "Go to top", &mut self.app.keys.go_to_top); key_field_end(ui);
                        key_field(ui, "Go to bottom", &mut self.app.keys.go_to_bottom); key_field_end(ui);
                        key_field(ui, "Layout 1", &mut self.app.keys.set_layout_1); key_field_end(ui);
                        key_field(ui, "Layout 2", &mut self.app.keys.set_layout_2); key_field_end(ui);
                        key_field(ui, "Layout 3", &mut self.app.keys.set_layout_3); key_field_end(ui);
                        key_field(ui, "Layout 4", &mut self.app.keys.set_layout_4); key_field_end(ui);
                        key_field(ui, "Show lyrics", &mut self.app.keys.show_lyrics); key_field_end(ui);
                        key_field(ui, "Edit lyrics (TUI)", &mut self.app.keys.edit_lyrics); key_field_end(ui);
                        key_field(ui, "Set artwork", &mut self.app.keys.set_artwork); key_field_end(ui);
                        key_field(ui, "Toggle artwork", &mut self.app.keys.toggle_artwork); key_field_end(ui);
                        key_field(ui, "Edit metadata", &mut self.app.keys.edit_metadata); key_field_end(ui);
                        key_field(ui, "Open in file manager", &mut self.app.keys.open_in_file_manager); key_field_end(ui);
                        key_field(ui, "Quit", &mut self.app.keys.quit); key_field_end(ui);
                    });
                    ui.add_space(12.0);
                    if ui
                        .button("Save settings")
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        match self.save_settings() {
                            Ok(()) => self.app.show_notice("Settings saved"),
                            Err(error) => self.app.show_notice(format!("Could not save settings: {error:#}")),
                        }
                    } 
                });
            });
    }

    /// Validate the typed music folder and store it in the configuration.
    fn apply_music_dir_input(&mut self) {
        let typed = self.music_dir_input.trim();
        let expanded = match typed.strip_prefix("~/") {
            Some(rest) => std::env::var("HOME")
                .map(|home| PathBuf::from(home).join(rest))
                .unwrap_or_else(|_| PathBuf::from(typed)),
            None => PathBuf::from(typed),
        };
        if typed.is_empty() || !expanded.is_dir() {
            self.app
                .show_notice("Enter the path of an existing folder");
            return;
        }
        self.config.music_dir = expanded.clone();
        self.music_dir_input = expanded.display().to_string();
        match self.save_settings() {
            Ok(()) => self
                .app
                .show_notice("Music folder saved. Restart Aghani to load it"),
            Err(error) => self
                .app
                .show_notice(format!("Could not save settings: {error:#}")),
        }
    }

    fn apply_artwork_input(&mut self) {
        let input = self.artwork_path.trim().to_string();
        if input.starts_with("http://") || input.starts_with("https://") {
            self.app.artwork_path_input = input;
            self.report(|app| app.confirm_artwork_path());
            return;
        }
        let path = PathBuf::from(&input);
        if path.is_file() && image::open(&path).is_ok() {
            self.app.artwork_path_input = input;
            self.report(|app| app.confirm_artwork_path());
        } else {
            self.app
                .show_notice("Enter an image URL or a readable image file");
        }
    }

    fn overlays(&mut self, ctx: &egui::Context) {
        if self.app.save_mode {
            let palette = self.palette();
            let candidates = self.app.save_candidates.len();
            let columns = 2usize;
            let rows = candidates.div_ceil(columns);
            let width = (ctx.screen_rect().width() - 36.0).clamp(340.0, 460.0);
            let height = (110.0 + rows as f32 * 36.0).clamp(200.0, 420.0);
            let frame = self.popup_frame(ctx);
            let (closed, _) = popup_window(
                ctx,
                "add_to_playlist_window",
                "Add to playlist",
                "Choose where to save the song",
                width,
                height,
                frame,
                palette,
                |ui| {
                    let list_height = (ui.available_height() - 40.0).max(60.0);
                    egui::ScrollArea::vertical()
                        .max_height(list_height)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            if self.app.save_candidates.is_empty() {
                                ui.label(
                                    egui::RichText::new(
                                        "No playlists yet. Create one from the Playlists tab.",
                                    )
                                    .color(palette.muted),
                                );
                            }
                            let spacing = ui.spacing().item_spacing.x;
                            let cell_width = ((ui.available_width()
                                - spacing * (columns - 1) as f32)
                                / columns as f32)
                                .max(80.0);
                            egui::Grid::new("save_playlist_grid")
                                .num_columns(columns)
                                .spacing([spacing, 6.0])
                                .show(ui, |ui| {
                                    for (position, playlist_index) in self
                                        .app
                                        .save_candidates
                                        .clone()
                                        .iter()
                                        .copied()
                                        .enumerate()
                                    {
                                        if let Some(playlist) =
                                            self.app.playlists.get(playlist_index)
                                        {
                                            let selected = self.app.save_selected == position;
                                            if ui
                                                .add_sized(
                                                    [cell_width, 34.0],
                                                    egui::Button::new(&playlist.name)
                                                        .selected(selected),
                                                )
                                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                                .clicked()
                                            {
                                                self.app.save_selected = position;
                                            }
                                        }
                                        if (position + 1) % columns == 0 {
                                            ui.end_row();
                                        }
                                    }
                                });
                        });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button("Add")
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            self.report(|app| app.confirm_save());
                        }
                        if ui
                            .button("Cancel")
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            self.app.cancel_save();
                        }
                    });
                },
            );
            if closed {
                self.app.cancel_save();
            }
        }
        if self.app.new_playlist_mode {
            let palette = self.palette();
            let width = (ctx.screen_rect().width() - 48.0).clamp(420.0, 640.0);
            let frame = self.popup_frame(ctx);
            let (closed, _) = popup_window(
                ctx,
                "create_playlist_window",
                "Create playlist",
                "Name your new playlist",
                width,
                250.0,
                frame,
                palette,
                |ui| {
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut self.playlist_name)
                            .font(egui::TextStyle::Heading)
                            .margin(egui::vec2(10.0, 10.0))
                            .hint_text("Playlist name")
                            .desired_width(f32::INFINITY),
                    );
                    if response.lost_focus()
                        && ui.input(|input| input.key_pressed(egui::Key::Enter))
                    {
                        self.app.new_playlist_name = self.playlist_name.clone();
                        self.report(|app| app.new_playlist_confirm());
                    }
                    let footer_space = (ui.available_height() - 34.0).max(10.0);
                    ui.add_space(footer_space);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button("Create")
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            self.app.new_playlist_name = self.playlist_name.clone();
                            self.report(|app| app.new_playlist_confirm());
                        }
                        if ui
                            .button("Cancel")
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            self.app.new_playlist_cancel();
                        }
                    });
                },
            );
            if closed {
                self.app.new_playlist_cancel();
            }
        }
        if let Some(dialog) = &self.app.confirm_dialog {
            let message = dialog.message.clone();
            let palette = self.palette();
            let width = (ctx.screen_rect().width() - 36.0).clamp(340.0, 440.0);
            let frame = self.popup_frame(ctx);
            let (closed, _) = popup_window(
                ctx,
                "confirm_dialog_window",
                "Confirm",
                "",
                width,
                190.0,
                frame,
                palette,
                |ui| {
                    ui.label(message);
                    let footer_space = (ui.available_height() - 34.0).max(10.0);
                    ui.add_space(footer_space);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button("Delete")
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            self.report(|app| app.confirm_dialog_confirm());
                        }
                        if ui
                            .button("Cancel")
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            self.app.confirm_dialog_cancel();
                        }
                    });
                },
            );
            if closed {
                self.app.confirm_dialog_cancel();
            }
        }
        if self.app.artwork_path_mode {
            let palette = self.palette();
            let width = (ctx.screen_rect().width() - 36.0).clamp(340.0, 520.0);
            let subtitle = self
                .app
                .artwork_target_label()
                .unwrap_or_else(|| "Paste an image URL".to_string());
            let frame = self.popup_frame(ctx);
            let (closed, _) = popup_window(
                ctx,
                "set_artwork_window",
                "Set artwork",
                &subtitle,
                width,
                220.0,
                frame,
                palette,
                |ui| {
                    ui.label("Image URL");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.artwork_path)
                            .font(egui::TextStyle::Body)
                            .margin(egui::vec2(10.0, 9.0))
                            .hint_text("https://example.com/cover.jpg")
                            .desired_width(f32::INFINITY),
                    );
                    ui.label(
                        egui::RichText::new("Local image files also work.")
                            .small()
                            .color(palette.muted),
                    );
                    let footer_space = (ui.available_height() - 34.0).max(10.0);
                    ui.add_space(footer_space);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button("Apply")
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            self.apply_artwork_input();
                        }
                        if ui
                            .button("Cancel")
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            self.app.cancel_artwork_path();
                        }
                    });
                },
            );
            if closed {
                self.app.cancel_artwork_path();
            }
        }
        if self.app.metadata_edit_mode {
            let palette = self.palette();
            let screen = ctx.screen_rect();
            let width = (screen.width() - 48.0).clamp(420.0, 760.0);
            let height = (screen.height() - 140.0).clamp(420.0, 620.0);
            let frame = self.popup_frame(ctx);
            let (closed, _) = popup_window(
                ctx,
                "metadata_editor_window",
                "Edit track metadata",
                "Changes are written to the file tags",
                width,
                height,
                frame,
                palette,
                |ui| {
                    egui::Grid::new("track_metadata_fields")
                        .num_columns(2)
                        .spacing([12.0, 8.0])
                        .show(ui, |ui| {
                            for (label, value) in ["Title", "Artist", "Album", "Artwork URL"]
                                .into_iter()
                                .zip(self.app.metadata_edit_values.iter_mut())
                            {
                                ui.label(label);
                                ui.add(
                                    egui::TextEdit::singleline(value)
                                        .font(egui::TextStyle::Body)
                                        .margin(egui::vec2(10.0, 9.0))
                                        .desired_width(f32::INFINITY),
                                );
                                ui.end_row();
                            }
                        });
                    if let Some(error) = &self.app.metadata_edit_error {
                        ui.colored_label(egui::Color32::LIGHT_RED, error);
                    }
                    let footer_space = (ui.available_height() - 34.0).max(10.0);
                    ui.add_space(footer_space);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .button("Save tags")
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            self.report(|app| app.save_metadata_edit());
                        }
                        if ui
                            .button("Cancel")
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            self.app.cancel_metadata_edit();
                        }
                    });
                },
            );
            if closed {
                self.app.cancel_metadata_edit();
            }
        }
    }

    fn selected_actions(&mut self, ui: &mut egui::Ui) {
        let buttons_width = 4.0 * 34.0 + 3.0 * ui.spacing().item_spacing.x;
        ui.horizontal(|ui| {
            ui.add_space(((ui.available_width() - buttons_width) * 0.5).max(0.0));
            if icon_button(ui, Icon::Play, "Restart the playing song").clicked() {
                self.report(|app| app.play_current());
            }
            if icon_button(ui, Icon::Heart, "Add the playing song to favorites").clicked() {
                self.favorite_to_top(|app| app.toggle_current_favorite());
            }
            if icon_button(ui, Icon::Add, "Add the playing song to a playlist").clicked() {
                self.app.open_save_mode_for_current();
            }
            if icon_button(ui, Icon::Settings, "Edit the playing track metadata").clicked() {
                self.app.start_metadata_edit_current();
            }
        });
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let focused_text = ctx.wants_keyboard_input();
        let pressed = |key: &str| key_pressed(ctx, key);
        if self.app.lyrics_sync_mode {
            if pressed("Escape") || pressed("Esc") {
                self.report(|app| app.finish_lyrics_sync(false));
            }
            if pressed("Down") {
                self.lyrics_auto_follow = true;
                self.report(|app| app.stamp_lyrics_sync_line());
            }
            if pressed("Space") {
                self.report(|app| app.toggle_play());
            }
            return;
        }
        if self.app.search_mode {
            if pressed("Escape") || pressed("Esc") {
                self.app.exit_search();
                self.search_input.clear();
                self.search_debounce_until = None;
            }
            if pressed("Enter") {
                self.update_search_query(true);
                self.report(|app| app.search_confirm());
                if !self.app.search_mode {
                    self.search_input.clear();
                    self.search_debounce_until = None;
                }
            }
            if pressed("Down") {
                self.update_search_query(true);
                self.app.search_select_next();
            }
            if pressed("Up") {
                self.update_search_query(true);
                self.app.search_select_prev();
            }
            return;
        }
        if self.app.save_mode {
            if pressed("Escape") || pressed("Esc") {
                self.app.cancel_save();
            }
            if pressed("Enter") {
                self.report(|app| app.confirm_save());
            }
            if pressed("Down") {
                self.app.save_selected = (self.app.save_selected + 1)
                    .min(self.app.save_candidates.len().saturating_sub(1));
            }
            if pressed("Up") {
                self.app.save_selected = self.app.save_selected.saturating_sub(1);
            }
            return;
        }
        if self.app.confirm_dialog.is_some() {
            if pressed("Escape") || pressed("Esc") {
                self.app.confirm_dialog_cancel();
            }
            if pressed("Enter") {
                self.report(|app| app.confirm_dialog_confirm());
            }
            return;
        }
        if self.app.new_playlist_mode {
            if pressed("Escape") || pressed("Esc") {
                self.app.new_playlist_cancel();
            } else if pressed("Enter") {
                self.app.new_playlist_name = self.playlist_name.clone();
                self.report(|app| app.new_playlist_confirm());
            }
            return;
        }
        if self.app.artwork_path_mode {
            if pressed("Escape") || pressed("Esc") {
                self.app.cancel_artwork_path();
            } else if pressed("Enter") {
                self.apply_artwork_input();
            }
            return;
        }
        if self.app.metadata_edit_mode {
            if pressed("Escape") || pressed("Esc") {
                self.app.cancel_metadata_edit();
            } else if pressed("Enter") {
                self.report(|app| app.save_metadata_edit());
            }
            return;
        }
        if self.lyrics_editor_open {
            if pressed("Escape") || pressed("Esc") {
                self.lyrics_editor_open = false;
            }
            return;
        }
        if focused_text {
            return;
        }
        if pressed("Shift+G") {
            self.app.toggle_grid_view();
            ctx.request_repaint();
        } else if pressed(&self.app.keys.go_to_top) || pressed("t") || pressed("Home") {
            self.app.go_to_top();
            self.pending_scroll_to_selection = true;
            self.momentum_scroll = 0.0;
            ctx.request_repaint();
        } else if pressed(&self.app.keys.go_to_bottom) || pressed("b") || pressed("End") {
            self.app.go_to_bottom();
            self.pending_scroll_to_selection = true;
            self.momentum_scroll = 0.0;
            ctx.request_repaint();
        } else if pressed(&self.app.keys.quit) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else if pressed(&self.app.keys.play_pause) {
            self.report(|app| app.toggle_play());
        } else if pressed(&self.app.keys.next) {
            self.skip_track(true);
        } else if pressed(&self.app.keys.prev) {
            self.skip_track(false);
        } else if pressed(&self.app.keys.volume_up) {
            self.app.volume_up();
        } else if pressed(&self.app.keys.volume_down) {
            self.app.volume_down();
        } else if pressed(&self.app.keys.select_down) {
            self.app.select_next();
        } else if pressed(&self.app.keys.select_up) {
            self.app.select_prev();
        } else if pressed(&self.app.keys.confirm) {
            self.close_overlay_lyrics();
            self.report(|app| app.activate_selected());
        } else if pressed(&self.app.keys.tab_next) {
            self.close_overlay_lyrics();
            self.app.tab_next();
        } else if pressed(&self.app.keys.tab_prev) {
            self.close_overlay_lyrics();
            self.app.tab_prev();
        } else if pressed(&self.app.keys.exit_scope) {
            self.close_overlay_lyrics();
            self.app.exit_scope();
        } else if pressed(&self.app.keys.add_to_favorites) {
            self.favorite_to_top(|app| app.add_to_favorites());
        } else if pressed(&self.app.keys.save_to_playlist) {
            self.app.open_save_mode();
        } else if pressed(&self.app.keys.delete_from_playlist) {
            self.app.request_delete_playlist();
        } else if pressed(&self.app.keys.shuffle) {
            self.app.toggle_shuffle();
        } else if pressed(&self.app.keys.search) {
            self.app.enter_search();
            self.search_input.clear();
            self.search_debounce_until = None;
            self.focus_search = true;
        } else if pressed(&self.app.keys.go_to_playing) {
            self.go_to_playing();
        } else if pressed(&self.app.keys.toggle_repeat) {
            self.app.toggle_repeat();
        } else if pressed(&self.app.keys.new_playlist) {
            self.app.start_new_playlist();
        } else if pressed(&self.app.keys.delete_playlist) {
            self.app.request_delete_playlist();
        } else if pressed(&self.app.keys.show_lyrics) {
            self.set_lyrics_panel_open(!self.lyrics_panel_open);
        } else if pressed("L") {
            self.set_lyrics_panel_open(true);
        } else if pressed(&self.app.keys.edit_lyrics) {
            self.open_lyrics_editor();
        } else if pressed(&self.app.keys.edit_metadata) {
            self.app.start_metadata_edit();
        } else if pressed(&self.app.keys.set_artwork) {
            self.app.start_artwork_path();
        } else if pressed(&self.app.keys.toggle_artwork) {
            self.app.toggle_artwork();
        } else if pressed("Shift+Home") {
            self.report(|app| app.move_selected_song(false));
        } else if pressed("Shift+End") {
            self.report(|app| app.move_selected_song(true));
        } else if pressed("Shift+S") {
            self.app.cycle_sort();
            self.notify_sort();
        } else if pressed("'") {
            self.lyrics_auto_follow = true;
            self.app.start_lyrics_sync();
        } else if pressed("p")
            && self.app.active_tab == LibraryTab::Playlists
            && matches!(&self.app.playlist_scope, crate::library::PlaylistScope::All)
        {
            self.app.toggle_pin_selected_playlist();
        } else if pressed(&self.app.keys.seek_forward) {
            self.report(|app| {
                app.seek_forward()?;
                app.finish_seek()
            });
        } else if pressed(&self.app.keys.seek_backward) {
            self.report(|app| {
                app.seek_backward()?;
                app.finish_seek()
            });
        } else if pressed(&self.app.keys.open_in_file_manager) {
            if let Err(error) = self.app.open_selected_in_file_manager() {
                self.app.show_notice(format!("{error:#}"));
            }
        } else if self.app.layout_mode.ends_with("_lyrics") && pressed("Down") {
            self.report(|app| app.step_synced_lyrics(true));
        } else if self.app.layout_mode.ends_with("_lyrics") && pressed("Up") {
            self.report(|app| app.step_synced_lyrics(false));
        } else if pressed(&self.app.keys.set_layout_1) {
            self.app.set_layout(0);
        } else if pressed(&self.app.keys.set_layout_3) {
            self.app.set_layout(2);
        } else if pressed(&self.app.keys.set_layout_4) {
            self.app.set_layout(3);
        }
    }
}

impl eframe::App for Gui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.lyrics_overlay = self.lyrics_takes_over(ctx);
        self.apply_momentum(ctx);
        self.drain_waveform_results();
        if let Err(error) = self.app.tick() {
            self.app
                .show_notice(format!("Application error: {error:#}"));
        }
        // The manual queue just ran out and playback returned to the list:
        // center the playing song, the same as pressing the go-to-playing key.
        if std::mem::take(&mut self.app.center_playing_request) {
            self.pending_scroll_to_playing = true;
            self.momentum_scroll = 0.0;
        }
        self.poll_library_changes();
        self.apply_style(ctx);
        self.paint_ambient_background(ctx);
        self.header(ctx);
        self.update_search_query(false);
        let palette = self.palette();

        if self.shows_playback_bar() {
            egui::TopBottomPanel::bottom("player")
                .frame(
                    egui::Frame::default()
                        .fill(self.player_fill())
                        .stroke(egui::Stroke::new(1.0, palette.border))
                        .inner_margin(egui::Margin::symmetric(18, 8)),
                )
                .show(ctx, |ui| {
                    let track = self.app.current_track().cloned();
                    ui.set_min_height(80.0);
                    let bounds = ui.max_rect();
                    let width = bounds.width();
                    let panel_height = bounds.height();
                    let top_height = (panel_height * 0.52).clamp(38.0, 44.0);
                    let progress_height = (panel_height * 0.30).clamp(22.0, 28.0);

                    let center_width = (width * 0.48).clamp(460.0, 680.0).min(width * 0.58);
                    let side_width = (width - center_width) * 0.5;
                    let left_rect =
                        egui::Rect::from_min_size(bounds.min, egui::vec2(side_width, panel_height));
                    let center_rect = egui::Rect::from_center_size(
                        bounds.center(),
                        egui::vec2(center_width, panel_height),
                    );
                    let right_rect = egui::Rect::from_min_size(
                        egui::pos2(bounds.right() - side_width, bounds.top()),
                        egui::vec2(side_width, panel_height),
                    );
                    let mut left_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .id_salt("player_left")
                            .max_rect(left_rect)
                            .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    );
                    if let Some(track) = &track {
                        let favorite = self.app.is_favorite(&track.path);
                        let texture = self.track_artwork(&mut left_ui, track);
                        let artwork_clicked = if let Some(texture) = texture {
                            let response = left_ui
                                .add(
                                    egui::Image::new((texture.id(), egui::vec2(52.0, 52.0)))
                                        .maintain_aspect_ratio(true)
                                        .sense(egui::Sense::click()),
                                )
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .on_hover_text("Show or hide Now playing");
                            if favorite {
                                paint_favorite_badge(
                                    left_ui.painter(),
                                    response.rect,
                                    palette.background,
                                    18.0,
                                );
                            }
                            response.clicked()
                        } else {
                            let (rect, response) = left_ui
                                .allocate_exact_size(egui::vec2(52.0, 52.0), egui::Sense::click());
                            left_ui.painter().rect_filled(rect, 5.0, palette.raised);
                            paint_icon(
                                left_ui.painter(),
                                Icon::Songs,
                                rect.shrink(14.0),
                                palette.muted,
                            );
                            if favorite {
                                paint_favorite_badge(
                                    left_ui.painter(),
                                    rect,
                                    palette.background,
                                    18.0,
                                );
                            }
                            response
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .on_hover_text("Show or hide Now playing")
                                .clicked()
                        };
                        if artwork_clicked {
                            self.toggle_now_playing();
                        }
                        let text_width = (left_rect.width()
                            - 52.0
                            - left_ui.spacing().item_spacing.x)
                            .max(0.0);
                        left_ui.allocate_ui_with_layout(
                            egui::vec2(text_width, 52.0),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                ui.spacing_mut().item_spacing.y = 1.0;
                                ui.add_space(8.0);
                                ui.add(
                                    egui::Label::new(display_rich(&track.title).strong()).truncate(),
                                )
                                .on_hover_text(&track.title);
                                if metadata_link(
                                    ui,
                                    &track.artist,
                                    palette.text,
                                    self.accent,
                                    12.0,
                                    false,
                                )
                                .clicked()
                                {
                                    self.open_entity_page(EntityLinkTarget::Artist(
                                        track.artist.clone(),
                                    ));
                                }
                            },
                        );
                    } else {
                        left_ui.label(egui::RichText::new("Choose a song").color(palette.muted));
                    }

                    let mut center_box = ui.new_child(
                        egui::UiBuilder::new()
                            .id_salt("player_center_box")
                            .max_rect(center_rect)
                            .layout(egui::Layout::top_down(egui::Align::Center)),
                    );
                    let controls_rect = egui::Rect::from_min_size(
                        egui::pos2(center_rect.left(), center_rect.top() + 2.0),
                        egui::vec2(center_rect.width(), top_height),
                    );
                    let mut center_ui = center_box.new_child(
                        egui::UiBuilder::new()
                            .id_salt("player_center_controls")
                            .max_rect(controls_rect)
                            .layout(egui::Layout::top_down(egui::Align::Center)),
                    );
                    self.playback_controls(&mut center_ui);

                    let mut right_ui = ui.new_child(
                        egui::UiBuilder::new()
                            .id_salt("player_right")
                            .max_rect(right_rect)
                            .layout(egui::Layout::right_to_left(egui::Align::Center)),
                    );
                    self.playback_options(&mut right_ui);

                    let progress_width = (center_rect.width() * 0.92)
                        .clamp(360.0, 620.0)
                        .min(center_rect.width());
                    let progress_rect = egui::Rect::from_min_size(
                        egui::pos2(
                            center_rect.center().x - progress_width * 0.5,
                            center_rect.top() + top_height + 4.0,
                        ),
                        egui::vec2(progress_width, progress_height),
                    );
                    let mut progress_ui = center_box.new_child(
                        egui::UiBuilder::new()
                            .id_salt("player_progress")
                            .max_rect(progress_rect)
                            .layout(egui::Layout::left_to_right(egui::Align::Center)),
                    );
                    self.playback_progress(&mut progress_ui, progress_width, progress_height);
                });
        }

        self.now_playing_panel(ctx);
        self.lyrics_side_panel(ctx);
        if self.settings_open {
            self.settings_window(ctx);
        } else if self.shows_library() && !self.lyrics_takes_over(ctx) {
            let library_rect = self.library_panel(ctx);
            if self.lyrics_maximize_collapse.is_armed() {
                self.paint_panel_collapse_hint(ctx, library_rect, "lyrics_middle_collapse_dim");
            }
        }
        self.queue_window(ctx);
        self.search_window(ctx);

        if let Some((message, _)) = &self.app.notice {
            egui::TopBottomPanel::bottom("notice")
                .frame(
                    egui::Frame::default()
                        .fill(self.accent.gamma_multiply(0.2))
                        .inner_margin(8.0),
                )
                .show(ctx, |ui| {
                    ui.label(message);
                });
        }
        self.lyrics_editor(ctx);
        self.overlays(ctx);
        self.handle_shortcuts(ctx);
        let mut repaint_interval = if self.app.lyrics_sync_mode {
            Duration::from_millis(100)
        } else if !self.app.player.is_paused()
            || self.app.loading
            || self.app.artwork_pending_count() > 0
            || self.app.lyrics_loading
        {
            Duration::from_millis(200)
        } else {
            Duration::from_millis(250)
        };
        if self.lyrics_panel_open && !self.app.player.is_paused() {
            if let Some(lyrics) = self.app.lyrics.as_ref().filter(|lyrics| lyrics.synced) {
                if let Some(until_next) = lyrics.next_line_in(self.app.player.elapsed()) {
                    // Wake up just after the highlighted line changes, but keep
                    // the progress bar updating at a low, steady cadence.
                    repaint_interval =
                        repaint_interval.min(until_next + Duration::from_millis(20));
                }
            }
        }
        ctx.request_repaint_after(repaint_interval);
        if let Some(deadline) = self.search_debounce_until {
            ctx.request_repaint_after(deadline.saturating_duration_since(Instant::now()));
        }
    }
}

impl Drop for Gui {
    fn drop(&mut self) {
        if let Err(error) = self.save_settings() {
            eprintln!("Could not save Aghani settings: {error:#}");
        }
        let mut state = State::load();
        state.last_track = self.app.current_track().map(|track| track.path.clone());
        state.last_position = state
            .last_track
            .as_ref()
            .map(|_| self.app.player.elapsed().as_secs_f64());
        state.last_tab = Some(format!("{:?}", self.app.active_tab));
        state.volume = Some(self.app.player.volume);
        state.history = self.app.history.clone();
        state.pending_history = self.app.pending_history.clone();
        state.play_counts = self.app.play_counts.clone();
        state.artwork_overrides = self.app.artwork_overrides.clone();
        state.pinned_playlists = self.app.pinned_playlists.clone();
        if let Err(error) = state.save() {
            eprintln!("Could not save Aghani session: {error:#}");
        }
        self.app.player.stop();
        let _ = std::fs::write("/tmp/aghani-status", "");
        let _ = std::fs::write("/tmp/aghani-status.json", r#"{"status":"stopped"}"#);
        let _ = std::fs::remove_file(crate::ipc::server::SOCKET_PATH);
    }
}

fn song_row(
    ui: &mut egui::Ui,
    track: &crate::library::track::Track,
    selected: bool,
    current: bool,
    favorite: bool,
    show_artwork: bool,
    show_metadata: bool,
    artwork: Option<&egui::TextureHandle>,
    palette: Palette,
    accent: egui::Color32,
    play_count: Option<String>,
) -> SongRowResponse {
    let (rect, row) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 58.0), egui::Sense::click());
    let row = row.on_hover_cursor(egui::CursorIcon::PointingHand);
    let hovered = ui.rect_contains_pointer(rect);

    let light_accent = bright_accent(accent);
    let (fill, stroke_color) = if current && selected {
        (
            with_alpha(accent, if hovered { 40 } else { 34 }),
            Some(with_alpha(accent, if hovered { 110 } else { 95 })),
        )
    } else if current {
        (
            with_alpha(accent, if hovered { 34 } else { 28 }),
            Some(with_alpha(accent, if hovered { 95 } else { 80 })),
        )
    } else if selected {
        (
            with_alpha(light_accent, if hovered { 24 } else { 16 }),
            Some(with_alpha(light_accent, if hovered { 75 } else { 55 })),
        )
    } else if hovered {
        (with_alpha(palette.text, 14), None)
    } else {
        (egui::Color32::TRANSPARENT, None)
    };
    let radius = 6.0;
    ui.painter().rect_filled(rect, radius, fill);
    ui.painter().rect_stroke(
        rect,
        radius,
        egui::Stroke::new(0.5, palette.border),
        egui::StrokeKind::Inside,
    );
    if let Some(stroke) = stroke_color {
        ui.painter().rect_stroke(
            rect,
            radius,
            egui::Stroke::new(1.0, stroke),
            egui::StrokeKind::Inside,
        );
    }

    let queue_rect = egui::Rect::from_center_size(
        egui::pos2(rect.right() - 30.0, rect.center().y),
        egui::vec2(30.0, 30.0),
    );
    let queue_response = ui
        .interact(queue_rect, row.id.with("queue"), egui::Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text("Add to Up next");
    if queue_response.hovered() {
        ui.painter()
            .circle_filled(queue_rect.center(), 15.0, with_alpha(palette.text, 24));
    }
    paint_icon(
        ui.painter(),
        Icon::Queue,
        queue_rect.shrink(7.0),
        if queue_response.hovered() || hovered {
            palette.text
        } else {
            with_alpha(palette.muted, 140)
        },
    );

    let mut content = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(egui::Rect::from_min_max(
                rect.min + egui::vec2(12.0, 8.0),
                egui::pos2(rect.right() - 52.0, rect.bottom() - 8.0),
            ))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    content.set_clip_rect(rect.intersect(ui.clip_rect()));
    content.style_mut().interaction.selectable_labels = false;

    let column_count = if show_metadata { 4 } else { 2 };
    let _ = content.columns(column_count, |columns| {
        columns[0].horizontal_centered(|ui| {
            if let Some(count) = &play_count {
                    ui.label(
                        egui::RichText::new(format!("{count}×"))
                            .size(17.0)
                            .strong()
                            .color(bright_accent(accent)),
                    );
                    ui.add_space(10.0);
                }
            if show_artwork {
                let artwork_rect = if let Some(texture) = artwork {
                    ui.add(
                        egui::Image::new((texture.id(), egui::vec2(38.0, 38.0)))
                            .maintain_aspect_ratio(true),
                    )
                    .rect
                } else {
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(38.0, 38.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 5.0, palette.panel);
                    paint_icon(ui.painter(), Icon::Songs, rect.shrink(12.0), palette.muted);
                    rect
                };
                if favorite {
                    paint_filled_heart(
                        ui.painter(),
                        egui::Rect::from_center_size(
                            artwork_rect.right_top() + egui::vec2(-5.0, 6.0),
                            egui::vec2(12.0, 12.0),
                        ),
                        egui::Color32::from_rgb(221, 100, 120),
                    );
                }
            }
            ui.add(
                egui::Label::new(display_rich(&track.title).size(16.0).strong().color(
                    if current {
                        bright_accent(accent)
                    } else {
                        palette.text
                    },
                ))
                .truncate(),
            );
        });
        if show_metadata {
            columns[1].horizontal_centered(|ui| {
 
                ui.add(
                    egui::Label::new(
                        display_rich(&track.artist).size(14.0).color(palette.text),
                    )
                    .truncate(),
                )
                .on_hover_text(&track.artist);
            });
            columns[2].horizontal_centered(|ui| {
                ui.add(
                    egui::Label::new(display_rich(&track.album).size(14.0).color(palette.text))
                        .truncate(),
                )
                .on_hover_text(&track.album);
            });
            columns[3].horizontal_centered(|ui| {

                ui.label(egui::RichText::new(track.duration_str()).color(palette.muted));
            });
        } else {
            columns[1].horizontal_centered(|ui| {
                if let Some(count) = &play_count {
                    ui.label(
                        egui::RichText::new(format!("{count}×"))
                            .size(17.0)
                            .strong()
                            .color(bright_accent(accent)),
                    );
                    ui.add_space(10.0);
                }
                ui.label(egui::RichText::new(track.duration_str()).color(palette.muted));
            });
        }
    });

    SongRowResponse {
        row,
        activated: None,
        queue: queue_response.clicked(),
    }
}

fn paint_lyrics_badge(
    painter: &egui::Painter,
    image_rect: egui::Rect,
    label: &str,
    palette: Palette,
    accent: egui::Color32,
) {
    let galley = painter.layout_no_wrap(
        label.to_owned(),
        egui::FontId::proportional(11.0),
        palette.text,
    );
    let padding = egui::vec2(7.0, 3.0);
    let size = galley.size() + padding * 2.0;
    let rect = egui::Rect::from_min_size(
        egui::pos2(
            image_rect.right() - size.x - 8.0,
            image_rect.bottom() - size.y - 8.0,
        ),
        size,
    );
    painter.rect_filled(rect, 6.0, with_alpha(palette.background, 215));
    painter.rect_stroke(
        rect,
        6.0,
        egui::Stroke::new(0.5, with_alpha(accent, 120)),
        egui::StrokeKind::Inside,
    );
    painter.galley(rect.min + padding, galley, palette.text);
}

fn paint_filled_heart(painter: &egui::Painter, rect: egui::Rect, color: egui::Color32) {
    let c = rect.center();
    let s = rect.width().min(rect.height());
    let lobe = s * 0.26;
    painter.circle_filled(egui::pos2(c.x - s * 0.235, c.y - s * 0.14), lobe, color);
    painter.circle_filled(egui::pos2(c.x + s * 0.235, c.y - s * 0.14), lobe, color);
    painter.add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(c.x - s * 0.48, c.y - s * 0.06),
            egui::pos2(c.x + s * 0.48, c.y - s * 0.06),
            egui::pos2(c.x, c.y + s * 0.46),
        ],
        color,
        egui::Stroke::NONE,
    ));
}

fn popup_window(
    ctx: &egui::Context,
    id: &str,
    title: &str,
    subtitle: &str,
    width: f32,
    height: f32,
    frame: egui::Frame,
    palette: Palette,
    body: impl FnOnce(&mut egui::Ui),
) -> (bool, Option<egui::Rect>) {
    let mut close = false;
    let response = egui::Window::new(title)
        .id(egui::Id::new(id))
        .title_bar(false)
        .collapsible(false)
        .resizable(true)
        .default_size([width, height])
        .min_size([width.min(340.0), height.min(180.0)])
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 70.0))
        .order(egui::Order::Foreground)
        .frame(frame)
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading(title);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if icon_button(ui, Icon::Close, "Close").clicked() {
                        close = true;
                    }
                });
            });
            if !subtitle.is_empty() {
                ui.add(
                    egui::Label::new(egui::RichText::new(subtitle).small().color(palette.muted))
                        .truncate(),
                )
                .on_hover_text(subtitle);
            }
            ui.separator();
            body(ui);
        });
    (close, response.map(|inner| inner.response.rect))
}

fn breadcrumb_link(ui: &mut egui::Ui, label: &str, accent: egui::Color32) -> bool {
    ui.scope(|ui| {
        ui.style_mut().visuals.widgets.hovered.fg_stroke.color = accent;
        ui.add(egui::Button::new(display_rich(label).size(24.0).strong()).frame(false))
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
    })
    .inner
}

fn metadata_link(
    ui: &mut egui::Ui,
    label: &str,
    color: egui::Color32,
    accent: egui::Color32,
    size: f32,
    strong: bool,
) -> egui::Response {
    ui.scope(|ui| {
        ui.spacing_mut().button_padding = egui::Vec2::ZERO;
        let visuals = &mut ui.style_mut().visuals.widgets;
        visuals.inactive.fg_stroke.color = color;
        visuals.hovered.fg_stroke.color = accent;
        visuals.active.fg_stroke.color = accent;
        let mut text = display_rich(label).size(size);
        if strong {
            text = text.strong();
        }
        ui.add(egui::Button::new(text).frame(false).truncate())
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .on_hover_text(label)
    })
    .inner
}

fn breadcrumb_separator(ui: &mut egui::Ui, palette: Palette) {
    ui.label(egui::RichText::new("/").size(24.0).color(palette.muted));
}

fn key_pressed(ctx: &egui::Context, key: &str) -> bool {
    ctx.input(|input| {
        let normalized = key.to_ascii_lowercase();
        if normalized == "shift+s" {
            return input.modifiers.shift && input.key_pressed(egui::Key::S);
        }
        if normalized == "shift+g" {
            return input.modifiers.shift && input.key_pressed(egui::Key::G);
        }
        if let Some(shifted_key) = key.strip_prefix("Shift+") {
            let pressed = match shifted_key.to_ascii_lowercase().as_str() {
                "home" => input.key_pressed(egui::Key::Home),
                "end" => input.key_pressed(egui::Key::End),
                _ => false,
            };
            return input.modifiers.shift && pressed;
        }
        let named = match normalized.as_str() {
            "space" => Some(egui::Key::Space),
            "enter" => Some(egui::Key::Enter),
            "esc" | "escape" => Some(egui::Key::Escape),
            "tab" => Some(egui::Key::Tab),
            "up" => Some(egui::Key::ArrowUp),
            "down" => Some(egui::Key::ArrowDown),
            "left" => Some(egui::Key::ArrowLeft),
            "right" => Some(egui::Key::ArrowRight),
            "home" => Some(egui::Key::Home),
            "end" => Some(egui::Key::End),
            "backspace" => Some(egui::Key::Backspace),
            _ => None,
        };
        if let Some(named) = named {
            return input.key_pressed(named);
        }
        if normalized == "backtab" {
            return input.key_pressed(egui::Key::Tab) && input.modifiers.shift;
        }
        input
            .events
            .iter()
            .any(|event| matches!(event, egui::Event::Text(text) if text == key))
            || input.events.iter().any(|event| {
                matches!(event, egui::Event::Key { key: pressed, pressed: true, modifiers, .. } if {
                    let letters_only = key.chars().all(char::is_alphabetic);
                    let case_matches = !letters_only
                        || (key.chars().any(char::is_uppercase) == modifiers.shift);
                    format!("{pressed:?}").eq_ignore_ascii_case(key) && case_matches
                })
            })
    })
}

fn display_text(text: &str) -> String {
    let mut visual_text = String::with_capacity(text.len());
    for (index, line) in text.split('\n').enumerate() {
        if index > 0 {
            visual_text.push('\n');
        }
        let contains_arabic = line.chars().any(|character| {
            matches!(
                character as u32,
                0x0600..=0x06ff | 0x0750..=0x077f | 0x08a0..=0x08ff | 0xfb50..=0xfdff | 0xfe70..=0xfeff
            )
        });
        if !contains_arabic {
            visual_text.push_str(line);
            continue;
        }
        let shaped = arabic_reshaper::arabic_reshape(line);
        let bidi = unicode_bidi::BidiInfo::new(&shaped, None);
        if let Some(paragraph) = bidi.paragraphs.first() {
            visual_text.push_str(&bidi.reorder_line(paragraph, paragraph.range.clone()));
        } else {
            visual_text.push_str(&shaped);
        }
    }
    visual_text
}

fn display_rich(text: impl AsRef<str>) -> egui::RichText {
    egui::RichText::new(display_text(text.as_ref()))
}

const THEMES: [(&str, &str); 4] = [
    ("classic_dark", "Classic Dark"),
    ("light", "Light"),
    ("artwork_ambient", "Artwork Ambient"),
    ("oled", "OLED / Deep Black"),
];

fn theme_label(theme: &str) -> &'static str {
    THEMES
        .iter()
        .find(|(id, _)| *id == theme)
        .map(|(_, label)| *label)
        .unwrap_or("Classic Dark")
}

fn next_theme(theme: &str) -> &'static str {
    let index = THEMES
        .iter()
        .position(|(id, _)| *id == theme)
        .unwrap_or(0);
    THEMES[(index + 1) % THEMES.len()].0
}

fn theme_icon(theme: &str) -> Icon {
    match theme {
        "light" => Icon::Sun,
        "artwork_ambient" => Icon::Artwork,
        _ => Icon::Moon,
    }
}

fn dynamic_accent(image: &DynamicImage, style: &str) -> egui::Color32 {
    match crate::dynamic_theme::extract_colors(image, style).primary {
        ratatui::style::Color::Rgb(r, g, b) => egui::Color32::from_rgb(r, g, b),
        ratatui::style::Color::Cyan => egui::Color32::from_rgb(65, 201, 220),
        ratatui::style::Color::Magenta => egui::Color32::from_rgb(212, 117, 230),
        ratatui::style::Color::Blue => egui::Color32::from_rgb(113, 156, 255),
        _ => egui::Color32::from_rgb(113, 156, 255),
    }
}

fn format_time(seconds: f64) -> String {
    let seconds = seconds.max(0.0) as u64;
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

fn install_fallback_fonts(ctx: &egui::Context) {
    let mut definitions = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut definitions, egui_phosphor::Variant::Regular);
    let paths = [
        "/usr/share/fonts/noto/NotoSansArabic-Regular.ttf",
        "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
        "/usr/share/fonts/truetype/noto/NotoSansArabic-Regular.ttf",
        "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        "C:/Windows/Fonts/arial.ttf",
        "C:/Windows/Fonts/meiryo.ttc",
    ];
    let mut added = 0usize;
    for path in paths {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let name = format!("aghani-fallback-{added}");
        definitions.font_data.insert(
            name.clone(),
            std::sync::Arc::new(egui::FontData::from_owned(bytes)),
        );
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            definitions
                .families
                .entry(family)
                .or_default()
                .push(name.clone());
        }
        added += 1;
    }
    let _ = added;
    ctx.set_fonts(definitions);
}

fn key_field(ui: &mut egui::Ui, label: &str, key: &mut String) {
    ui.label(label);
    ui.add_sized(
        [190.0, 36.0],
        egui::TextEdit::singleline(key)
            .font(egui::TextStyle::Body)
            .margin(egui::vec2(10.0, 8.0)),
    );
}

fn key_field_end(ui: &mut egui::Ui) {
    ui.end_row();
}

#[cfg(test)]
mod tests {
    use super::{
        centered_image_uv, display_text, install_fallback_fonts, layout_visibility, momentum_step,
        next_theme, playing_row_scroll_offset, responsive_grid_layout, visible_album_entries,
        waveform_peaks, with_alpha, Palette, GRID_CARD_MIN_WIDTH, GRID_SPACING, THEMES,
    };
    use eframe::egui;

    #[test]
    fn theme_button_cycles_through_every_theme() {
        let mut theme = THEMES[0].0;
        let mut seen = vec![theme];
        for _ in 1..THEMES.len() {
            theme = next_theme(theme);
            seen.push(theme);
        }
        assert_eq!(seen.len(), THEMES.len());
        assert_eq!(next_theme(theme), THEMES[0].0);
        assert_eq!(
            seen,
            THEMES.iter().map(|(theme, _)| *theme).collect::<Vec<_>>()
        );
    }

    #[test]
    fn momentum_glide_decays_without_overshooting() {
        let mut remaining = 400.0_f32;
        let mut total = 0.0_f32;
        let dt = 1.0 / 60.0;

        for _ in 0..240 {
            let step = momentum_step(remaining, dt);
            assert!(step >= 0.0);
            assert!(step <= remaining);
            remaining -= step;
            total += step;
        }

        assert!(remaining < 1.0);
        assert!((total - 400.0).abs() < 1.0);
    }

    #[test]
    fn momentum_step_is_inert_without_time_or_impulse() {
        assert_eq!(momentum_step(250.0, 0.0), 0.0);
        assert_eq!(momentum_step(0.0, 1.0 / 60.0), 0.0);
    }

    #[test]
    fn injected_momentum_delta_scrolls_the_hovered_area_across_frames() {
        let context = egui::Context::default();
        let mut offsets = Vec::new();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0));

        for frame in 0..6 {
            let mut raw = egui::RawInput {
                screen_rect: Some(screen),
                predicted_dt: 1.0 / 60.0,
                ..Default::default()
            };
            if frame == 0 {
                raw.events
                    .push(egui::Event::PointerMoved(egui::pos2(200.0, 150.0)));
            }

            let mut offset = 0.0;
            let _ = context.run(raw, |context| {
                if frame > 0 {
                    context.input_mut(|input| input.smooth_scroll_delta.y -= 30.0);
                }
                egui::CentralPanel::default().show(context, |ui| {
                    let output = egui::ScrollArea::vertical()
                        .id_salt("momentum_probe")
                        .show(ui, |ui| {
                            ui.set_min_width(380.0);
                            for _ in 0..200 {
                                ui.label("row");
                            }
                        });
                    offset = output.state.offset.y;
                });
            });
            offsets.push(offset);
        }

        assert!(offsets[1] > 0.0);
        assert!(offsets[2] > offsets[1]);
        assert!(offsets[5] > offsets[4]);
    }

    #[test]
    fn arabic_ui_text_is_reshaped_and_mixed_text_preserves_latin() {
        let text = display_text("سلام - Track 42");

        assert_ne!(text, "سلام - Track 42");
        assert!(text.contains("Track"));
        assert!(text.contains("42"));
    }

    #[test]
    fn japanese_ui_text_is_preserved() {
        assert_eq!(display_text("日本語の曲とLatin"), "日本語の曲とLatin");
    }

    #[test]
    fn ambient_surface_fill_applies_translucency() {
        let fill = with_alpha(egui::Color32::from_rgb(18, 24, 33), 214);

        assert_eq!(fill.a(), 214);
    }

    #[test]
    fn waveform_peaks_reduce_audio_samples_into_bars() {
        let peaks = waveform_peaks(&[0.1, -0.8, 0.2, f32::NAN, 0.4, -0.6], 3);

        assert_eq!(peaks.len(), 3);
        assert!(peaks[0] > peaks[1]);
        assert!(peaks[2] > peaks[1]);
        assert!(peaks.iter().all(|peak| (0.0..=1.0).contains(peak)));
    }

    #[test]
    fn waveform_amplitude_distribution_keeps_tiny_and_tall_bars() {
        let samples = [
            [0.01; 10], [0.06; 10], [0.2; 10], [0.8; 10], [0.03; 10], [0.4; 10], [1.0; 10],
            [0.1; 10],
        ]
        .concat();
        let peaks = waveform_peaks(&samples, 8);

        assert!(peaks.iter().copied().fold(1.0_f32, f32::min) < 0.15);
        assert!(peaks.iter().copied().fold(0.0_f32, f32::max) > 0.99);
        assert!(peaks.windows(2).any(|pair| (pair[0] - pair[1]).abs() > 0.3));
    }

    #[test]
    fn numbered_layouts_keep_lyrics_visibility_independent() {
        assert_eq!(layout_visibility("default"), (true, true, true));
        assert_eq!(layout_visibility("clean"), (true, false, true));
        assert_eq!(layout_visibility("compact"), (false, true, true));
        assert_eq!(layout_visibility("minimal"), (false, true, false));
        assert_eq!(layout_visibility("compact_lyrics"), (false, true, true));
    }

    #[test]
    fn playing_row_scroll_clamps_correctly_at_list_edges() {
        let row_height = 58.0;
        let row_spacing = 7.0;
        let viewport_height = 300.0;
        let row_count = 20;
        let row_step = row_height + row_spacing;
        let max_offset = row_count as f32 * row_step - row_spacing - viewport_height;

        assert_eq!(
            playing_row_scroll_offset(0, row_height, row_spacing, row_count, viewport_height),
            0.0
        );
        assert_eq!(
            playing_row_scroll_offset(
                row_count - 1,
                row_height,
                row_spacing,
                row_count,
                viewport_height
            ),
            max_offset
        );
        assert_eq!(
            playing_row_scroll_offset(10, row_height, row_spacing, row_count, viewport_height),
            10.0 * row_step - (viewport_height - row_height) * 0.5
        );
    }

    #[test]
    fn playing_row_scroll_stays_at_top_when_list_fits_viewport() {
        assert_eq!(playing_row_scroll_offset(2, 58.0, 7.0, 4, 300.0), 0.0);
    }

    #[test]
    fn artwork_grid_columns_adapt_to_available_width() {
        let (columns, card_width) = responsive_grid_layout(600.0);

        assert_eq!(columns, 3);
        assert!(card_width >= GRID_CARD_MIN_WIDTH);
        assert_eq!(
            columns as f32 * card_width + (columns - 1) as f32 * GRID_SPACING,
            600.0
        );

        let (narrow_columns, narrow_card_width) = responsive_grid_layout(160.0);
        assert_eq!(narrow_columns, 1);
        assert_eq!(narrow_card_width, 160.0);
    }

    #[test]
    fn artwork_grid_crops_images_to_fill_without_distortion() {
        let uv = centered_image_uv(egui::vec2(2.0, 1.0), egui::vec2(1.0, 1.0));

        assert_eq!(uv.min, egui::pos2(0.25, 0.0));
        assert_eq!(uv.max, egui::pos2(0.75, 1.0));
    }

    #[test]
    fn single_track_album_filter_preserves_original_album_indices() {
        let albums = vec!["Single".to_string(), "Multi".to_string()];
        let entries =
            visible_album_entries(&albums, true, |album| if album == "Single" { 1 } else { 3 });

        assert_eq!(entries, vec![(1, "Multi".to_string())]);
        assert_eq!(
            visible_album_entries(&albums, false, |_| 1),
            vec![(0, "Single".to_string()), (1, "Multi".to_string())]
        );
    }

    #[test]
    fn oled_palette_is_distinct_from_classic_dark() {
        let dark = Palette::for_theme(true, "classic_dark");
        let oled = Palette::for_theme(true, "oled");

        assert_ne!(dark.background, oled.background);
        assert_eq!(oled.background, egui::Color32::BLACK);
    }

    #[test]
    fn fallback_fonts_can_layout_mixed_rtl_and_cjk_text() {
        let context = egui::Context::default();
        install_fallback_fonts(&context);
        let text = display_text("سلام - 日本語 Track 42");

        let _ = context.run(egui::RawInput::default(), |context| {
            context.fonts(|fonts| {
                let galley = fonts.layout_no_wrap(
                    text.clone(),
                    egui::FontId::proportional(17.0),
                    egui::Color32::WHITE,
                );
                assert!(!galley.rows.is_empty());
            })
        });
    }
}