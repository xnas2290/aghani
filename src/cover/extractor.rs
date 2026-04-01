use std::path::Path;

use anyhow::Result;
use image::DynamicImage;

/// Decode raw cover bytes (from ID3/Vorbis tags) into a DynamicImage.
pub fn decode_cover(data: &[u8]) -> Result<DynamicImage> {
    let img = image::load_from_memory(data)?;
    Ok(img)
}

/// Load a fallback cover from disk (e.g. assets/default_cover.png).
pub fn load_fallback(path: &Path) -> Result<DynamicImage> {
    let img = image::open(path)?;
    Ok(img)
}
