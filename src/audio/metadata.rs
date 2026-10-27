// use std::path::Path;
// use std::time::Duration;

use crate::library::track::Track;
use anyhow::Result;
use lofty::config::WriteOptions;
use lofty::file::TaggedFileExt;
use lofty::picture::{MimeType, Picture, PictureType};
use lofty::prelude::ItemKey;
use lofty::prelude::*;
use lofty::read_from_path;
use lofty::tag::{Tag, TagItem};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MAX_ARTWORK_BYTES: u64 = 20 * 1024 * 1024;

pub fn enrich_track(track: &mut Track) -> Result<Option<Vec<u8>>> {
    enrich_track_impl(track, true)
}

/// Read tags and audio properties without copying the embedded picture into
/// memory. Used while scanning a library where most covers are never needed.
pub fn enrich_track_metadata(track: &mut Track) -> Result<()> {
    enrich_track_impl(track, false).map(|_| ())
}

fn enrich_track_impl(track: &mut Track, include_cover: bool) -> Result<Option<Vec<u8>>> {
    let tagged_file = read_from_path(&track.path)?;
    let props = tagged_file.properties();

    track.duration = props.duration();
    track.bitrate = props.audio_bitrate();
    track.sample_rate = props.sample_rate();
    track.channels = props.channels();

    let tag = tagged_file
        .primary_tag()
        .or_else(|| tagged_file.first_tag());

    let cover_data = if let Some(tag) = tag {
        track.title = tag.title().map(|s| s.to_string()).unwrap_or_else(|| {
            track
                .path
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        });
        track.artist = tag
            .artist()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "Unknown Artist".to_string());
        track.album = tag
            .album()
            .map(|s| s.to_string())
            .unwrap_or_else(|| "Unknown Album".to_string());

        // Read ReplayGain — try track gain first, fall back to album gain
        track.replaygain = tag
            .get_string(ItemKey::ReplayGainTrackGain)
            .or_else(|| tag.get_string(ItemKey::ReplayGainAlbumGain))
            .and_then(|s| parse_replaygain(s));
        let pictures = tag.pictures();
        track.has_cover = !pictures.is_empty();
        if include_cover {
            pictures
                .iter()
                .find(|pic| pic.pic_type() == PictureType::CoverFront)
                .or_else(|| pictures.first())
                .map(|pic| pic.data().to_vec())
        } else {
            None
        }
    } else {
        track.title = track
            .path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        None
    };

    if include_cover {
        track.has_cover = cover_data.is_some();
    }
    Ok(cover_data)
}

/// Write title/artist/album tags back to the audio file, optionally replacing
/// the front cover with the image stored at `artwork`.
pub fn write_track_metadata(
    path: &Path,
    title: &str,
    artist: &str,
    album: &str,
    artwork: Option<&Path>,
) -> Result<()> {
    let mut tagged_file = read_from_path(path)?;
    if tagged_file.primary_tag().is_none() {
        let tag_type = tagged_file.primary_tag_type();
        tagged_file.insert_tag(Tag::new(tag_type));
    }
    let tag = tagged_file
        .primary_tag_mut()
        .expect("tag inserted just above");

    tag.set_title(title.to_string());
    tag.set_artist(artist.to_string());
    tag.set_album(album.to_string());
    if let Some(artwork) = artwork {
        apply_artwork(tag, artwork)?;
    }

    match tag.save_to_path(path, WriteOptions::default()) {
        Ok(()) => Ok(()),
        Err(error) if invalid_language(&error) => {
            // Some files contain malformed language frames that cannot be
            // written back. Drop format-specific frames and replace invalid
            // language codes, then retry once.
            let tag_type = tag.tag_type();
            tag.re_map(tag_type);
            sanitize_language_items(tag);
            tag.set_title(title.to_string());
            tag.set_artist(artist.to_string());
            tag.set_album(album.to_string());
            if let Some(artwork) = artwork {
                apply_artwork(tag, artwork)?;
            }
            tag.save_to_path(path, WriteOptions::default())?;
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

/// A downloaded image in the system temporary directory.
///
/// The file is removed automatically when this value is dropped.
pub struct TempArtwork {
    path: PathBuf,
}

impl TempArtwork {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempArtwork {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Download an image from a URL into the temp directory so it can be embedded
/// into a track's tags. The caller does not need to clean it up manually.
pub fn download_artwork_to_temp(url: &str) -> Result<TempArtwork> {
    let url = url.trim();
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        anyhow::bail!("artwork URL must start with http:// or https://");
    }

    let response = ureq::get(url)
        .timeout(Duration::from_secs(20))
        .call()
        .map_err(|error| anyhow::anyhow!("download failed: {error}"))?;

    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(MAX_ARTWORK_BYTES + 1)
        .read_to_end(&mut bytes)?;

    if bytes.is_empty() {
        anyhow::bail!("downloaded image is empty");
    }
    if bytes.len() as u64 > MAX_ARTWORK_BYTES {
        anyhow::bail!("image is larger than 20 MB");
    }

    // Reject anything that is not a decodable image before touching the track.
    image::load_from_memory(&bytes)
        .map_err(|error| anyhow::anyhow!("downloaded file is not a valid image: {error}"))?;

    let extension = image::guess_format(&bytes)
        .ok()
        .and_then(|format| format.extensions_str().first().copied())
        .unwrap_or("img");
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let path = std::env::temp_dir().join(format!(
        "aghani-artwork-{}-{stamp}.{extension}",
        std::process::id()
    ));
    std::fs::write(&path, &bytes)?;

    Ok(TempArtwork { path })
}

fn apply_artwork(tag: &mut Tag, artwork_path: &Path) -> Result<()> {
    let bytes = std::fs::read(artwork_path)?;
    let mime = artwork_mime(&bytes)?;
    let picture = Picture::unchecked(bytes)
        .pic_type(PictureType::CoverFront)
        .mime_type(mime)
        .build();

    tag.remove_picture_type(PictureType::CoverFront);
    tag.push_picture(picture);
    Ok(())
}

fn artwork_mime(bytes: &[u8]) -> Result<MimeType> {
    let format = image::guess_format(bytes)
        .map_err(|error| anyhow::anyhow!("unsupported image format: {error}"))?;

    Ok(match format {
        image::ImageFormat::Png => MimeType::Png,
        image::ImageFormat::Jpeg => MimeType::Jpeg,
        image::ImageFormat::Gif => MimeType::Gif,
        image::ImageFormat::Bmp => MimeType::Bmp,
        image::ImageFormat::Tiff => MimeType::Tiff,
        other => MimeType::Unknown(format!(
            "image/{}",
            other.extensions_str().first().copied().unwrap_or("octet-stream")
        )),
    })
}

fn invalid_language(error: &lofty::error::LoftyError) -> bool {
    if let lofty::error::ErrorKind::Id3v2(error) = error.kind() {
        return matches!(
            error.kind(),
            lofty::error::Id3v2ErrorKind::InvalidLanguage(_)
        );
    }
    false
}

/// Replace language codes that are not three ASCII letters (which ID3v2
/// refuses to write) with the "unknown language" code.
fn sanitize_language_items(tag: &mut Tag) {
    let items: Vec<TagItem> = tag
        .items()
        .cloned()
        .map(|mut item| {
            if !item.lang().iter().all(u8::is_ascii_alphabetic) {
                item.set_lang(*b"XXX");
            }
            item
        })
        .collect();

    tag.retain(|_| false);
    for item in items {
        tag.push_unchecked(item);
    }
}

fn parse_replaygain(s: &str) -> Option<f32> {
    // Format is typically "-6.54 dB" or "-6.54"
    s.split_whitespace()
        .next()
        .and_then(|v| v.parse::<f32>().ok())
}
