use anyhow::{Context, Result};
use rodio::{OutputStream, OutputStreamHandle, Sink, Source};
use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub struct Player {
    _stream: OutputStream,
    _handle: OutputStreamHandle,
    sink: Sink,
    current_child: Option<Arc<Mutex<Child>>>,
    pub volume: f32,
    started_at: Option<Instant>,
    paused_elapsed: Duration,
    current_gain: Option<f32>,
}

impl Player {
    pub fn new() -> Result<Self> {
        let (stream, handle) = OutputStream::try_default()?;
        let sink = Sink::try_new(&handle)?;
        Ok(Self {
            _stream: stream,
            _handle: handle,
            sink,
            current_child: None,
            volume: 1.0,
            started_at: None,
            paused_elapsed: Duration::ZERO,
            current_gain: None,
        })
    }

    pub fn play(&mut self, path: &Path) -> Result<()> {
        self.play_from(path, Duration::ZERO, None)
    }

    pub fn play_with_gain(&mut self, path: &Path, gain_db: Option<f32>) -> Result<()> {
        self.play_from(path, Duration::ZERO, gain_db)
    }

    pub fn seek(&mut self, path: &Path, to: Duration) -> Result<()> {
        self.play_from(path, to, self.current_gain)
    }

    fn play_from(&mut self, path: &Path, offset: Duration, gain_db: Option<f32>) -> Result<()> {
        self.kill_child();
        self.sink.stop();
        self.current_gain = gain_db;

        // Convert dB gain to ffmpeg volume filter
        // ReplayGain formula: linear = 10^(dB/20) * preamp
        // We use 89dB reference (standard ReplayGain preamp)
        let volume_arg = if let Some(db) = gain_db {
            let linear = 10f32.powf((db + 89.0 - 89.0) / 20.0);
            // Clamp to prevent clipping
            let clamped = linear.clamp(0.0, 4.0);
            format!("{:.6}", clamped)
        } else {
            "1.0".to_string()
        };

        let mut child = Command::new("ffmpeg")
            .args([
                "-ss",
                &format!("{:.3}", offset.as_secs_f64()),
                "-i",
                path.to_str().context("invalid path")?,
                "-af",
                &format!("volume={}", volume_arg),
                "-f",
                "f32le",
                "-ar",
                "44100",
                "-ac",
                "2",
                "-loglevel",
                "quiet",
                "pipe:1",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .stdin(Stdio::null())
            .spawn()
            .context("ffmpeg not found")?;

        let stdout = child.stdout.take().context("no stdout")?;
        let child_arc = Arc::new(Mutex::new(child));
        self.current_child = Some(Arc::clone(&child_arc));

        let source = FfmpegSource {
            reader: std::io::BufReader::with_capacity(256 * 1024, stdout),
            child: child_arc,
            sample_buf: Vec::with_capacity(4096),
            sample_pos: 0,
        };

        self.sink = Sink::try_new(&self._handle)?;
        self.sink.set_volume(self.volume);
        self.sink.append(source);

        self.started_at = Some(Instant::now());
        self.paused_elapsed = offset;

        Ok(())
    }

    fn kill_child(&mut self) {
        if let Some(child) = self.current_child.take() {
            if let Ok(mut c) = child.lock() {
                let _ = c.kill();
                let _ = c.wait();
            }
        }
    }

    pub fn pause(&mut self) {
        if !self.sink.is_paused() {
            self.sink.pause();
            if let Some(started) = self.started_at.take() {
                self.paused_elapsed += started.elapsed();
            }
        }
    }

    pub fn resume(&mut self) {
        if self.sink.is_paused() {
            self.sink.play();
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

    #[allow(dead_code)]
    pub fn stop(&mut self) {
        self.kill_child();
        self.sink.stop();
        self.started_at = None;
        self.paused_elapsed = Duration::ZERO;
    }

    pub fn is_paused(&self) -> bool {
        self.sink.is_paused()
    }
    pub fn is_finished(&self) -> bool {
        self.sink.empty()
    }

    pub fn elapsed(&self) -> Duration {
        let running = self.started_at.map(|t| t.elapsed()).unwrap_or_default();
        self.paused_elapsed + running
    }

    pub fn set_volume(&mut self, vol: f32) {
        self.volume = vol.clamp(0.0, 2.0);
        self.sink.set_volume(self.volume);
    }

    pub fn volume_up(&mut self) {
        self.set_volume(self.volume + 0.05);
    }
    pub fn volume_down(&mut self) {
        self.set_volume(self.volume - 0.05);
    }
}

// ── FFmpeg PCM source ─────────────────────────────────────────────────────────

struct FfmpegSource {
    reader: std::io::BufReader<std::process::ChildStdout>,
    child: Arc<Mutex<Child>>,
    sample_buf: Vec<f32>,
    sample_pos: usize,
}

impl Iterator for FfmpegSource {
    type Item = f32;

    fn next(&mut self) -> Option<f32> {
        if self.sample_pos >= self.sample_buf.len() {
            let mut bytes = vec![0u8; 8192 * 4];
            match self.reader.read(&mut bytes) {
                Ok(0) | Err(_) => return None,
                Ok(n) => {
                    self.sample_buf = bytes[..n]
                        .chunks_exact(4)
                        .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                        .collect();
                    self.sample_pos = 0;
                }
            }
        }

        if self.sample_pos < self.sample_buf.len() {
            let s = self.sample_buf[self.sample_pos];
            self.sample_pos += 1;
            Some(s)
        } else {
            None
        }
    }
}

impl Source for FfmpegSource {
    fn current_frame_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> u16 {
        2
    }
    fn sample_rate(&self) -> u32 {
        44100
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

impl Drop for FfmpegSource {
    fn drop(&mut self) {
        if let Ok(mut c) = self.child.lock() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}
