use mpris_server::{Metadata, PlaybackStatus, Player, Time};
use std::sync::mpsc;

#[derive(Debug)]
pub enum MprisCommand {
    Play,
    Pause,
    PlayPause,
    Next,
    Prev,
    Stop,
    Seek(i64),
    SetPosition(i64), // absolute position in microseconds  ← add this
    SetVolume(f64),
}

#[derive(Debug, Clone)]
pub struct MprisUpdate {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_us: i64,
    pub position_us: i64,
    pub playing: bool,
    pub volume: f64,
    pub shuffle: bool,
    pub cover_url: Option<String>,
}

impl Default for MprisUpdate {
    fn default() -> Self {
        MprisUpdate {
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            cover_url: None,
            duration_us: 0,
            position_us: 0,
            playing: false,
            volume: 1.0,
            shuffle: false,
        }
    }
}

pub fn start_mpris() -> (mpsc::Receiver<MprisCommand>, mpsc::SyncSender<MprisUpdate>) {
    let (cmd_tx, cmd_rx) = mpsc::sync_channel::<MprisCommand>(32);
    let (update_tx, update_rx) = mpsc::sync_channel::<MprisUpdate>(32);

    std::thread::spawn(move || {
        let ex = smol::LocalExecutor::new();
        smol::block_on(ex.run(async {
            let player = Player::builder("aghani")
                .identity("aghani") // ← add this
                .desktop_entry("aghani") // ← add this
                .can_play(true)
                .can_pause(true)
                .can_go_next(true)
                .can_go_previous(true)
                .can_seek(true)
                .can_control(true)
                .build()
                .await
                .expect("Failed to build MPRIS player");

            let tx = cmd_tx.clone();
            player.connect_play_pause(move |_| {
                let _ = tx.try_send(MprisCommand::PlayPause);
            });
            let tx = cmd_tx.clone();
            player.connect_play(move |_| {
                let _ = tx.try_send(MprisCommand::Play);
            });
            let tx = cmd_tx.clone();
            player.connect_pause(move |_| {
                let _ = tx.try_send(MprisCommand::Pause);
            });
            let tx = cmd_tx.clone();
            player.connect_next(move |_| {
                let _ = tx.try_send(MprisCommand::Next);
            });
            let tx = cmd_tx.clone();
            player.connect_previous(move |_| {
                let _ = tx.try_send(MprisCommand::Prev);
            });
            let tx = cmd_tx.clone();
            player.connect_stop(move |_| {
                let _ = tx.try_send(MprisCommand::Stop);
            });
            let tx = cmd_tx.clone();
            player.connect_seek(move |_, offset| {
                let _ = tx.try_send(MprisCommand::Seek(offset.as_micros() as i64));
            });
            let tx = cmd_tx.clone();
            player.connect_set_volume(move |_, vol| {
                let _ = tx.try_send(MprisCommand::SetVolume(vol));
            });
            let tx = cmd_tx.clone();
            player.connect_set_position(move |_, _, position| {
                let _ = tx.try_send(MprisCommand::SetPosition(position.as_micros() as i64));
            });
            ex.spawn(player.run()).detach();

            loop {
                smol::Timer::after(std::time::Duration::from_millis(100)).await;

                while let Ok(upd) = update_rx.try_recv() {
                    let status = if upd.playing {
                        PlaybackStatus::Playing
                    } else if upd.title.is_empty() {
                        PlaybackStatus::Stopped
                    } else {
                        PlaybackStatus::Paused
                    };

                    let mut meta = Metadata::new();
                    if !upd.title.is_empty() {
                        meta.set_title(Some(upd.title.clone()));
                        meta.set_artist(Some(vec![upd.artist.clone()]));
                        meta.set_album(Some(upd.album.clone()));
                        meta.set_length(Some(Time::from_micros(upd.duration_us)));
                        if let Some(ref url) = upd.cover_url {
                            meta.set_art_url(Some(url.clone()));
                        }
                    }

                    let _ = player.set_playback_status(status).await;
                    let _ = player.set_metadata(meta).await;
                    player.set_position(Time::from_micros(upd.position_us));
                    let _ = player.set_volume(upd.volume).await;
                    let _ = player.set_shuffle(upd.shuffle).await;
                }
            }
        }));
    });

    (cmd_rx, update_tx)
}
