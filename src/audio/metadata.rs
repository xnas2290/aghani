// use std::path::Path;
// use std::time::Duration;

use anyhow::Result;
use lofty::file::TaggedFileExt;
use lofty::prelude::*;
use lofty::read_from_path;

use crate::library::track::Track;

// pub struct Metadata {
//     pub title: String,
//     pub artist: String,
//     pub album: String,
//     pub duration: Duration,
//     pub cover_data: Option<Vec<u8>>,
// }

// pub fn read_metadata(path: &Path) -> Result<Metadata> {
//     let tagged_file = read_from_path(path)?;

//     let duration = tagged_file.properties().duration();

//     let tag = tagged_file
//         .primary_tag()
//         .or_else(|| tagged_file.first_tag());

//     let (title, artist, album, cover_data) = if let Some(tag) = tag {
//         let title = tag.title().map(|s| s.to_string()).unwrap_or_else(|| {
//             path.file_stem()
//                 .unwrap_or_default()
//                 .to_string_lossy()
//                 .to_string()
//         });
//         let artist = tag
//             .artist()
//             .map(|s| s.to_string())
//             .unwrap_or_else(|| "Unknown Artist".to_string());
//         let album = tag
//             .album()
//             .map(|s| s.to_string())
//             .unwrap_or_else(|| "Unknown Album".to_string());

//         let cover_data = tag.pictures().first().map(|pic| pic.data().to_vec());

//         (title, artist, album, cover_data)
//     } else {
//         let title = path
//             .file_stem()
//             .unwrap_or_default()
//             .to_string_lossy()
//             .to_string();
//         (
//             title,
//             "Unknown Artist".to_string(),
//             "Unknown Album".to_string(),
//             None,
//         )
//     };

//     Ok(Metadata {
//         title,
//         artist,
//         album,
//         duration,
//         cover_data,
//     })
// }

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
