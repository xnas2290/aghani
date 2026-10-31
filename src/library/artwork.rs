//! Small, persistent artwork thumbnails used by the library lists.

use anyhow::Result;
use image::{DynamicImage, ImageFormat};
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
};

/// Library lists render artwork in small terminal rectangles. Keeping a
/// display-sized copy is enough for every image protocol and avoids holding a
/// full-resolution album cover in memory for every row.
const DISPLAY_MAX: u32 = 256;

#[derive(Debug, Clone)]
pub struct Thumbnail {
    image: DynamicImage,
}

impl Thumbnail {
    pub fn from_image(image: DynamicImage) -> Self {
        let image = if image.width() > DISPLAY_MAX || image.height() > DISPLAY_MAX {
            image.resize(
                DISPLAY_MAX,
                DISPLAY_MAX,
                image::imageops::FilterType::Triangle,
            )
        } else {
            image
        };
        Self { image }
    }

    /// Return the decoded image for ratatui-image rendering.
    pub fn image(&self) -> DynamicImage {
        self.image.clone()
    }
}

/// A real raster placeholder used when an artist or album has no artwork.
/// It is intentionally large enough for image protocols to scale cleanly.
#[cfg(feature = "tui")]
pub fn placeholder() -> Thumbnail {
    let image = image::RgbImage::from_fn(256, 256, |x, y| {
        let dx = x as i32 - 128;
        let dy = y as i32 - 128;
        let distance = ((dx * dx + dy * dy) as f32).sqrt();
        let glow = (1.0 - distance / 180.0).max(0.0);
        let stripe = ((x + y) / 24) % 2 == 0;
        let base = if stripe { 22 } else { 28 };
        let note = (92..124).contains(&x) && (70..190).contains(&y)
            || (116..158).contains(&x) && (60..84).contains(&y)
            || (80..125).contains(&x) && (178..216).contains(&y)
            || (122..166).contains(&x) && (142..180).contains(&y);
        if note {
            image::Rgb([235, 174, 82])
        } else {
            image::Rgb([
                (base as f32 + glow * 18.0) as u8,
                (base as f32 + glow * 12.0) as u8,
                (base as f32 + glow * 30.0) as u8,
            ])
        }
    });
    Thumbnail::from_image(DynamicImage::ImageRgb8(image))
}

pub fn cache_dir() -> PathBuf {
    let dir = dirs_next::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("aghani")
        .join("artwork");
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn file_for(key: &str) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    format!("v3:{key}").hash(&mut hasher);
    cache_dir().join(format!("{:016x}.png", hasher.finish()))
}

fn source_file_for(key: &str) -> PathBuf {
    let mut path = file_for(key);
    path.set_extension("source.png");
    path
}

pub fn from_bytes(key: &str, bytes: &[u8]) -> Option<Thumbnail> {
    let image = image::load_from_memory(bytes).ok()?;
    let thumb = Thumbnail::from_image(image);
    let _ = save_thumbnail(&thumb, &file_for(key));
    // Older builds stored full-resolution copies. They are no longer needed
    // once a display-sized cache entry exists.
    let _ = std::fs::remove_file(source_file_for(key));
    Some(thumb)
}

pub fn from_path(key: &str, path: &Path) -> Option<Thumbnail> {
    let image = image::open(path).ok()?;
    let thumb = Thumbnail::from_image(image);
    let _ = save_thumbnail(&thumb, &file_for(key));
    let _ = std::fs::remove_file(source_file_for(key));
    Some(thumb)
}

/// Path of the cached thumbnail for a key. URL-based artwork is stored in this
/// file so the override survives restarts without re-downloading the image.
pub fn thumbnail_path(key: &str) -> PathBuf {
    file_for(key)
}

pub fn load(key: &str) -> Option<Thumbnail> {
    let display_path = file_for(key);
    if let Ok(image) = image::open(&display_path) {
        // Upgrade a legacy 32x32 entry from its full-resolution source once.
        let legacy_small = image.width() < 128 && image.height() < 128;
        let source_path = source_file_for(key);
        if !legacy_small || !source_path.is_file() {
            return Some(Thumbnail::from_image(image));
        }
    }

    let source_path = source_file_for(key);
    let image = image::open(&source_path).ok()?;
    let thumb = Thumbnail::from_image(image);
    let _ = save_thumbnail(&thumb, &display_path);
    let _ = std::fs::remove_file(&source_path);
    Some(thumb)
}

fn save_thumbnail(thumb: &Thumbnail, path: &Path) -> Result<()> {
    thumb.image.save_with_format(path, ImageFormat::Png)?;
    Ok(())
}
