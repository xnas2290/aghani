// use mpris_server::{
//     PlayerInterface, RootInterface, Server, Time, TrackId, zbus, zbus::fdo,
//     zbus::zvariant::ObjectPath,
// };
// use std::sync::{Arc, Mutex};
// use tokio::sync::mpsc::UnboundedSender;
// // use zvariant;
// #[derive(Debug)]
// pub enum MprisCommand {
//     Play,
//     Pause,
//     PlayPause,
//     Next,
//     Prev,
//     Stop,
//     Seek(i64), // microseconds
//     SetVolume(f64),
// }

// #[derive(Clone)]
// pub struct MprisState {
//     pub title: String,
//     pub artist: String,
//     pub album: String,
//     pub duration_us: i64, // microseconds
//     pub position_us: i64,
//     pub playing: bool,
//     pub volume: f64,
//     pub shuffle: bool,
//     pub can_go_next: bool,
//     pub can_go_prev: bool,
// }

// impl Default for MprisState {
//     fn default() -> Self {
//         MprisState {
//             title: String::new(),
//             artist: String::new(),
//             album: String::new(),
//             duration_us: 0,
//             position_us: 0,
//             playing: false,
//             volume: 1.0,
//             shuffle: false,
//             can_go_next: true,
//             can_go_prev: true,
//         }
//     }
// }

// pub type SharedMprisState = Arc<Mutex<MprisState>>;

// struct AghaniPlayer {
//     state: SharedMprisState,
//     cmd_tx: UnboundedSender<MprisCommand>,
// }

// impl RootInterface for AghaniPlayer {
//     async fn raise(&self) -> fdo::Result<()> {
//         Ok(())
//     }
//     async fn quit(&self) -> fdo::Result<()> {
//         Ok(())
//     }
//     async fn can_quit(&self) -> fdo::Result<bool> {
//         Ok(false)
//     }
//     async fn can_raise(&self) -> fdo::Result<bool> {
//         Ok(false)
//     }
//     async fn has_track_list(&self) -> fdo::Result<bool> {
//         Ok(false)
//     }
//     async fn identity(&self) -> fdo::Result<String> {
//         Ok("Aghani".into())
//     }
//     async fn supported_uri_schemes(&self) -> fdo::Result<Vec<String>> {
//         Ok(vec![])
//     }
//     async fn supported_mime_types(&self) -> fdo::Result<Vec<String>> {
//         Ok(vec![])
//     }
//     async fn fullscreen(&self) -> fdo::Result<bool> {
//         Ok(false)
//     }
//     async fn set_fullscreen(&self, _: bool) -> zbus::Result<()> {
//         Ok(())
//     }
//     async fn desktop_entry(&self) -> fdo::Result<String> {
//         Ok("aghani".into())
//     }
//     fn can_set_fullscreen(
//         &self,
//     ) -> impl std::future::Future<Output = Result<bool, mpris_server::zbus::fdo::Error>> + Send + Sync
//     {
//         async {
//             Ok(false) // Or true if your app supports it
//         }
//     }
// }

// impl PlayerInterface for AghaniPlayer {
//     async fn next(&self) -> fdo::Result<()> {
//         let _ = self.cmd_tx.send(MprisCommand::Next);
//         Ok(())
//     }

//     async fn previous(&self) -> fdo::Result<()> {
//         let _ = self.cmd_tx.send(MprisCommand::Prev);
//         Ok(())
//     }

//     async fn pause(&self) -> fdo::Result<()> {
//         let _ = self.cmd_tx.send(MprisCommand::Pause);
//         Ok(())
//     }

//     async fn play_pause(&self) -> fdo::Result<()> {
//         let _ = self.cmd_tx.send(MprisCommand::PlayPause);
//         Ok(())
//     }

//     async fn stop(&self) -> fdo::Result<()> {
//         let _ = self.cmd_tx.send(MprisCommand::Stop);
//         Ok(())
//     }

//     async fn play(&self) -> fdo::Result<()> {
//         let _ = self.cmd_tx.send(MprisCommand::Play);
//         Ok(())
//     }

//     async fn seek(&self, offset: Time) -> fdo::Result<()> {
//         let _ = self
//             .cmd_tx
//             .send(MprisCommand::Seek(offset.as_micros() as i64));
//         Ok(())
//     }

//     async fn set_position(&self, _track_id: TrackId, position: Time) -> fdo::Result<()> {
//         let _ = self
//             .cmd_tx
//             .send(MprisCommand::Seek(position.as_micros() as i64));
//         Ok(())
//     }

//     async fn open_uri(&self, _uri: String) -> fdo::Result<()> {
//         Ok(())
//     }

//     async fn playback_status(&self) -> fdo::Result<mpris_server::PlaybackStatus> {
//         let s = self.state.lock().unwrap();
//         Ok(if s.playing {
//             mpris_server::PlaybackStatus::Playing
//         } else {
//             mpris_server::PlaybackStatus::Paused
//         })
//     }

//     async fn loop_status(&self) -> fdo::Result<mpris_server::LoopStatus> {
//         Ok(mpris_server::LoopStatus::None)
//     }

//     async fn set_loop_status(&self, _: mpris_server::LoopStatus) -> zbus::Result<()> {
//         Ok(())
//     }

//     async fn rate(&self) -> fdo::Result<f64> {
//         Ok(1.0)
//     }
//     async fn set_rate(&self, _: f64) -> zbus::Result<()> {
//         Ok(())
//     }
//     async fn minimum_rate(&self) -> fdo::Result<f64> {
//         Ok(1.0)
//     }
//     async fn maximum_rate(&self) -> fdo::Result<f64> {
//         Ok(1.0)
//     }

//     async fn shuffle(&self) -> fdo::Result<bool> {
//         Ok(self.state.lock().unwrap().shuffle)
//     }

//     async fn set_shuffle(&self, shuffle: bool) -> zbus::Result<()> {
//         self.state.lock().unwrap().shuffle = shuffle;
//         Ok(())
//     }

//     async fn metadata(&self) -> fdo::Result<mpris_server::Metadata> {
//         let s = self.state.lock().unwrap();
//         let mut meta = mpris_server::Metadata::new();
//         meta.set_title(Some(s.title.clone()));
//         meta.set_artist(Some(vec![s.artist.clone()]));
//         meta.set_album(Some(s.album.clone()));
//         meta.set_length(Some(Time::from_micros(s.duration_us)));
//         meta.set_trackid(Some(TrackId(
//             ObjectPath::try_from("/org/aghani/track/1").unwrap(),
//         )));
//         Ok(meta)
//     }

//     async fn volume(&self) -> fdo::Result<f64> {
//         Ok(self.state.lock().unwrap().volume)
//     }

//     async fn set_volume(&self, vol: f64) -> zbus::Result<()> {
//         let _ = self.cmd_tx.send(MprisCommand::SetVolume(vol));
//         Ok(())
//     }

//     async fn position(&self) -> fdo::Result<Time> {
//         Ok(Time::from_micros(self.state.lock().unwrap().position_us))
//     }

//     async fn can_go_next(&self) -> fdo::Result<bool> {
//         Ok(self.state.lock().unwrap().can_go_next)
//     }

//     async fn can_go_previous(&self) -> fdo::Result<bool> {
//         Ok(self.state.lock().unwrap().can_go_prev)
//     }

//     async fn can_play(&self) -> fdo::Result<bool> {
//         Ok(true)
//     }
//     async fn can_pause(&self) -> fdo::Result<bool> {
//         Ok(true)
//     }
//     async fn can_seek(&self) -> fdo::Result<bool> {
//         Ok(true)
//     }
//     async fn can_control(&self) -> fdo::Result<bool> {
//         Ok(true)
//     }
// }

// /// Spawn the MPRIS server in a background tokio runtime.
// /// Returns the shared state (write to it from the main thread)
// /// and a receiver for commands sent by external clients.
// pub fn start_mpris() -> (SharedMprisState, std::sync::mpsc::Receiver<MprisCommand>) {
//     let state = Arc::new(Mutex::new(MprisState::default()));
//     let state_clone = Arc::clone(&state);

//     let (tokio_tx, mut tokio_rx) = tokio::sync::mpsc::unbounded_channel::<MprisCommand>();
//     let (std_tx, std_rx) = std::sync::mpsc::channel::<MprisCommand>();

//     std::thread::spawn(move || {
//         let rt = tokio::runtime::Runtime::new().unwrap();
//         rt.block_on(async move {
//             let player = AghaniPlayer {
//                 state: state_clone,
//                 cmd_tx: tokio_tx,
//             };

//             let server = Server::new("aghani", player).await.unwrap();

//             // Bridge tokio channel → std channel so main thread can receive
//             tokio::spawn(async move {
//                 while let Some(cmd) = tokio_rx.recv().await {
//                     let _ = std_tx.send(cmd);
//                 }
//             });

//             // Keep server alive
//             server.run().await.unwrap();
//         });
//     });

//     (state, std_rx)
// }
