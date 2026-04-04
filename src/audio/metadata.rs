// use std::path::Path;
// use std::time::Duration;

use crate::library::track::Track;
use anyhow::Result;
use lofty::file::TaggedFileExt;
use lofty::prelude::ItemKey;
use lofty::prelude::*;
use lofty::read_from_path;

pub fn enrich_track(track: &mut Track) -> Result<Option<Vec<u8>>> {
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
        tag.pictures().first().map(|pic| pic.data().to_vec())
    } else {
        track.title = track
            .path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        None
    };

    track.has_cover = cover_data.is_some();
    Ok(cover_data)
}

fn parse_replaygain(s: &str) -> Option<f32> {
    // Format is typically "-6.54 dB" or "-6.54"
    s.split_whitespace()
        .next()
        .and_then(|v| v.parse::<f32>().ok())
}
