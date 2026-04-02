use std::fs::File;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use crossbeam_channel::{Receiver, bounded};
use rodio::{OutputStream, OutputStreamHandle, Sink, Source};

use symphonia::core::{
    audio::SampleBuffer, codecs::DecoderOptions, formats::FormatOptions, io::MediaSourceStream,
    meta::MetadataOptions, probe::Hint,
};
use symphonia::default::{get_codecs, get_probe};

pub struct Player {
    _stream: OutputStream,
    _handle: OutputStreamHandle,
    sink: Arc<Mutex<Sink>>,
    pub volume: f32,
    started_at: Option<Instant>,
    paused_elapsed: Duration,
}

impl Player {
    pub fn new() -> Result<Self> {
        let (stream, handle) = OutputStream::try_default()?;
        let sink = Sink::try_new(&handle)?;
        Ok(Self {
            _stream: stream,
            _handle: handle,
            sink: Arc::new(Mutex::new(sink)),
            volume: 1.0,
            started_at: None,
            paused_elapsed: Duration::ZERO,
        })
    }

    pub fn play(&mut self, path: &Path) -> Result<()> {
        self.play_from(path, Duration::ZERO, false)
    }

    pub fn seek(&mut self, path: &Path, to: Duration) -> Result<()> {
        self.play_from(path, to, true)
    }

    fn play_from(&mut self, path: &Path, offset: Duration, is_seek: bool) -> Result<()> {
        let file = File::open(path).context("open failed")?;
        let (tx, rx) = bounded::<Vec<f32>>(64); // large buffer

        let path_owned = path.to_owned();
        let offset_secs = offset.as_secs_f64();

        thread::spawn(move || {
            let mss = MediaSourceStream::new(Box::new(file), Default::default());
            let mut hint = Hint::new();
            if let Some(ext) = path_owned.extension().and_then(|e| e.to_str()) {
                hint.with_extension(ext);
            }

            let probed = match get_probe().format(
                &hint,
                mss,
                &FormatOptions::default(),
                &MetadataOptions::default(),
            ) {
                Ok(p) => p,
                Err(_) => return,
            };

            let mut format = probed.format;
            let track = match format.default_track() {
                Some(t) => t,
                None => return,
            };

            let sample_rate = track.codec_params.sample_rate.unwrap_or(44100) as f64;
            let channels = track.codec_params.channels.map(|c| c.count()).unwrap_or(2) as f64;

            let mut decoder =
                match get_codecs().make(&track.codec_params, &DecoderOptions::default()) {
                    Ok(d) => d,
                    Err(_) => return,
                };

            let mut samples_to_skip = (offset_secs * sample_rate * channels) as usize;

            loop {
                let packet = match format.next_packet() {
                    Ok(p) => p,
                    Err(_) => break,
                };
                let decoded = match decoder.decode(&packet) {
                    Ok(d) => d,
                    Err(_) => continue,
                };
                let spec = *decoded.spec();
                let mut buffer = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
                buffer.copy_interleaved_ref(decoded);
                let samples = buffer.samples().to_vec();

                if samples_to_skip > 0 {
                    if samples_to_skip >= samples.len() {
                        samples_to_skip -= samples.len();
                        continue;
                    } else {
                        let remaining = samples[samples_to_skip..].to_vec();
                        samples_to_skip = 0;
                        if tx.send(remaining).is_err() {
                            break;
                        }
                        continue;
                    }
                }

                if tx.send(samples).is_err() {
                    break;
                }
            }
        });

        // On seek: pause sink first, let decoder buffer up, then swap + resume
        // On normal play: swap immediately
        if is_seek {
            thread::sleep(Duration::from_millis(120)); // let decoder skip to offset
        }

        let mut sink_guard = self.sink.lock().unwrap();
        *sink_guard = Sink::try_new(&self._handle)?;
        sink_guard.set_volume(self.volume);
        sink_guard.append(StreamingSource {
            rx,
            current: Vec::new().into_iter(),
            sample_rate: 44100,
            channels: 2,
        });

        self.started_at = Some(Instant::now());
        self.paused_elapsed = offset;

        Ok(())
    }

    pub fn pause(&mut self) {
        let sink = self.sink.lock().unwrap();
        if !sink.is_paused() {
            sink.pause();
            if let Some(started) = self.started_at.take() {
                self.paused_elapsed += started.elapsed();
            }
        }
    }

    pub fn resume(&mut self) {
        let sink = self.sink.lock().unwrap();
        if sink.is_paused() {
            sink.play();
            drop(sink);
            self.started_at = Some(Instant::now());
        }
    }

    pub fn toggle_pause(&mut self) {
        if self.is_paused() {
            self.resume();
        } else {
            self.pause();
        }
    }

    // pub fn stop(&mut self) {
    //     self.sink.lock().unwrap().stop();
    //     self.started_at = None;
    //     self.paused_elapsed = Duration::ZERO;
    // }

    pub fn is_paused(&self) -> bool {
        self.sink.lock().unwrap().is_paused()
    }

    pub fn is_finished(&self) -> bool {
        self.sink.lock().unwrap().empty()
    }

    pub fn elapsed(&self) -> Duration {
        let running = self.started_at.map(|t| t.elapsed()).unwrap_or_default();
        self.paused_elapsed + running
    }

    pub fn set_volume(&mut self, vol: f32) {
        self.volume = vol.clamp(0.0, 2.0);
        self.sink.lock().unwrap().set_volume(self.volume);
    }

    pub fn volume_up(&mut self) {
        self.set_volume(self.volume + 0.05);
    }
    pub fn volume_down(&mut self) {
        self.set_volume(self.volume - 0.05);
    }
}

// ── Streaming Source ──────────────────────────────────────────────────────────

pub struct StreamingSource {
    rx: Receiver<Vec<f32>>,
    current: std::vec::IntoIter<f32>,
    sample_rate: u32,
    channels: u16,
}

impl Iterator for StreamingSource {
    type Item = f32;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(s) = self.current.next() {
                return Some(s);
            }
            match self.rx.recv() {
                Ok(chunk) => self.current = chunk.into_iter(),
                Err(_) => return None,
            }
        }
    }
}

impl Source for StreamingSource {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        self.channels
    }
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}
