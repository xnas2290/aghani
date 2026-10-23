
use anyhow::{Context, Result};
use libpulse_binding as pulse;
use pulse::context::Context as PulseContext;
use pulse::mainloop::threaded::Mainloop;
use pulse::volume::{ChannelVolumes, Volume};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub struct Player {
    current_child: Option<Arc<Mutex<Child>>>,
    pub volume: f32,
    started_at: Option<Instant>,
    paused_elapsed: Duration,
    current_path: Option<PathBuf>,
    current_gain: Option<f32>,
    paused: bool,
    pulse: Option<PulseHandle>,
    seeking: bool,
    seek_target: Duration,
    seek_last_update: Option<Instant>,
    seek_started_at: Option<Instant>,
}

struct PulseHandle {
    mainloop: Mainloop,
    context: PulseContext,
}

impl PulseHandle {
    fn new() -> Option<Self> {
        let mut mainloop = Mainloop::new()?;
        mainloop.start().ok()?;

        let mut context = PulseContext::new(&mainloop, "aghani")?;
        context
            .connect(None, pulse::context::FlagSet::NOFLAGS, None)
            .ok()?;

        loop {
            match context.get_state() {
                pulse::context::State::Ready => break,
                pulse::context::State::Failed
                | pulse::context::State::Terminated => return None,
                _ => thread::sleep(Duration::from_millis(10)),
            }
        }

        Some(Self { mainloop, context })
    }

    fn find_ffmpeg_sink_input(&mut self) -> Option<u32> {
        let result = Arc::new(Mutex::new(None::<u32>));
        let result_clone = Arc::clone(&result);
        let introspector = self.context.introspect();

        self.mainloop.lock();

        let op = introspector.get_sink_input_info_list(move |item| {
            if let pulse::callbacks::ListResult::Item(info) = item {
                if let Some(binary) =
                    info.proplist.get_str("application.process.binary")
                {
                    if binary == "ffmpeg" {
                        *result_clone.lock().unwrap() = Some(info.index);
                    }
                }
            }
        });

        self.mainloop.unlock();

        loop {
            self.mainloop.lock();
            let state = op.get_state();
            self.mainloop.unlock();

            match state {
                pulse::operation::State::Done => break,
                pulse::operation::State::Cancelled => return None,
                pulse::operation::State::Running => {
                    thread::sleep(Duration::from_millis(5));
                }
            }
        }

        let found = *result.lock().unwrap();
        found
    }

    fn set_volume(&mut self, index: u32, vol: f32) {
        let linear = (vol * Volume::NORMAL.0 as f32) as u32;
        let mut binding = ChannelVolumes::default();
        let channels = binding.set(
            2,
            Volume(linear.min(Volume::MAX.0)),
        );

        let mut introspector = self.context.introspect();
        self.mainloop.lock();
        introspector.set_sink_input_volume(index, channels, None);
        self.mainloop.unlock();
    }

    fn set_mute(&mut self, index: u32, mute: bool) {
        let mut introspector = self.context.introspect();
        self.mainloop.lock();
        introspector.set_sink_input_mute(index, mute, None);
        self.mainloop.unlock();
    }
}

impl Drop for PulseHandle {
    fn drop(&mut self) {
        self.context.disconnect();
        self.mainloop.stop();
    }
}

impl Player {
    pub fn new() -> Result<Self> {
        let pulse = PulseHandle::new();

        if pulse.is_none() {
            eprintln!(
                "Warning: could not connect to PulseAudio/PipeWire; \
                 volume control disabled"
            );
        }

        Ok(Self {
            current_child: None,
            volume: 1.0,
            started_at: None,
            paused_elapsed: Duration::ZERO,
            current_path: None,
            current_gain: None,
            paused: false,
            pulse,
            seeking: false,
            seek_target: Duration::ZERO,
            seek_last_update: None,
            seek_started_at: None,
        })
    }

    pub fn play(&mut self, path: &Path) -> Result<()> {
        self.seeking = false;
        self.seek_last_update = None;
        self.seek_started_at = None;
        self.play_from(path, Duration::ZERO, None)
    }

    pub fn play_with_gain(
        &mut self,
        path: &Path,
        gain_db: Option<f32>,
    ) -> Result<()> {
        self.seeking = false;
        self.seek_last_update = None;
        self.seek_started_at = None;
        self.play_from(path, Duration::ZERO, gain_db)
    }

    pub fn seek(&mut self, path: &Path, to: Duration) -> Result<()> {
        self.current_path = Some(path.to_path_buf());
        self.seek_target = to;
        self.seeking = true;
        let now = Instant::now();
        self.seek_last_update = Some(now);
        self.seek_started_at = Some(now);
        Ok(())
    }

    // Call on every press/repeat event while an arrow key is held.
    pub fn update_seek(&mut self, forward: bool, max: Option<Duration>) {
        let now = Instant::now();
        let first_update = !self.seeking;
        if !self.seeking {
            self.seek_target = self.elapsed();
            self.seeking = true;
            self.seek_started_at = Some(now);
        }

        let last = self.seek_last_update.replace(now).unwrap_or(now);
        let delta = now.duration_since(last);

        // Start at ten seconds per second and accelerate as the key remains
        // held, capped at one minute per second for controllable long seeks.
        let held_for = self
            .seek_started_at
            .map(|started| started.elapsed().as_secs_f64())
            .unwrap_or_default();
        let rate = (10.0 + held_for * 10.0).min(60.0);
        let step = if first_update {
            Duration::from_secs(5)
        } else {
            Duration::from_secs_f64(delta.as_secs_f64() * rate)
        };
        self.seek_target = if forward {
            self.seek_target.saturating_add(step)
        } else {
            self.seek_target.saturating_sub(step)
        };

        if let Some(max) = max {
            self.seek_target = self.seek_target.min(max);
        }
    }

    // Call when the arrow key is released.
    pub fn finish_seek(&mut self) -> Result<()> {
        if self.seeking == false {
            return Ok(());
        }

        let target = self.seek_target;
        let path = self
            .current_path
            .clone()
            .context("No track selected")?;
        let gain = self.current_gain;
        let was_paused = self.paused;

        self.seeking = false;
        self.seek_last_update = None;
        self.seek_started_at = None;

        self.play_from(&path, target, gain)?;

        if was_paused {
            self.pause();
        }

        Ok(())
    }

    pub fn finish_seek_if_idle(&mut self, idle_for: Duration) -> Result<()> {
        if self.seeking
            && self
                .seek_last_update
                .map(|last| last.elapsed() >= idle_for)
                .unwrap_or(false)
        {
            self.finish_seek()?;
        }
        Ok(())
    }

    fn play_from(
        &mut self,
        path: &Path,
        offset: Duration,
        gain_db: Option<f32>,
    ) -> Result<()> {
        self.kill_child();

        self.current_path = Some(path.to_path_buf());
        self.current_gain = gain_db;

        let offset_str = format!("{:.3}", offset.as_secs_f64());
        let path_str = path.to_str().context("Invalid path")?;
        let volume_str = format!("{:.6}", gain_to_linear(gain_db));

        let child = Command::new("ffmpeg")
            .args([
                "-nostdin",
                "-ss",
                &offset_str,
                "-i",
                path_str,
                "-af",
                &format!("volume={}", volume_str),
                "-f",
                "pulse",
                "-device",
                "default",
                "-loglevel",
                "quiet",
                "default",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .context("ffmpeg not found — please install ffmpeg")?;

        self.current_child = Some(Arc::new(Mutex::new(child)));
        self.started_at = Some(Instant::now());
        self.paused_elapsed = offset;
        self.paused = false;

        let vol = self.volume;

        if let Some(ref mut pulse) = self.pulse {
            // PulseAudio may need a moment to register the new ffmpeg sink,
            // but don't block playback/seek for half a second waiting for it.
            for _ in 0..3 {
                thread::sleep(Duration::from_millis(10));
                if let Some(index) = pulse.find_ffmpeg_sink_input() {
                    pulse.set_volume(index, vol);
                    break;
                }
            }
        }

        Ok(())
    }

    fn kill_child(&mut self) {
        if let Some(child) = self.current_child.take() {
            if let Ok(mut child) = child.lock() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }

    pub fn pause(&mut self) {
        if self.paused {
            return;
        }

        if self.seeking && self.finish_seek().is_err() {
            return;
        }

        if let Some(ref child) = self.current_child {
            if let Ok(child) = child.lock() {
                unsafe {
                    libc::kill(child.id() as i32, libc::SIGSTOP);
                }
            }
        }

        // SIGSTOP prevents more samples from being produced, but PulseAudio
        // may still have buffered samples queued. Mute the sink so pause is
        // audible immediately instead of waiting for that buffer to drain.
        if let Some(ref mut pulse) = self.pulse {
            if let Some(index) = pulse.find_ffmpeg_sink_input() {
                pulse.set_mute(index, true);
            }
        }

        if let Some(started) = self.started_at.take() {
            self.paused_elapsed += started.elapsed();
        }

        self.paused = true;
    }

    pub fn resume(&mut self) {
        if self.paused==false {
            return;
        }

        if let Some(ref child) = self.current_child {
            if let Ok(child) = child.lock() {
                unsafe {
                    libc::kill(child.id() as i32, libc::SIGCONT);
                }
            }
        }

        if let Some(ref mut pulse) = self.pulse {
            if let Some(index) = pulse.find_ffmpeg_sink_input() {
                pulse.set_mute(index, false);
            }
        }

        self.started_at = Some(Instant::now());
        self.paused = false;
    }

    pub fn toggle_pause(&mut self) {
        if self.paused {
            self.resume();
        } else {
            self.pause();
        }
    }

    pub fn stop(&mut self) {
        self.kill_child();
        self.started_at = None;
        self.paused_elapsed = Duration::ZERO;
        self.paused = false;
        self.seeking = false;
        self.seek_last_update = None;
        self.seek_started_at = None;
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn is_finished(&self) -> bool {
        match &self.current_child {
            None => true,
            Some(child) => match child.lock() {
                Ok(mut child) => match child.try_wait() {
                    Ok(Some(_)) => true,
                    Ok(None) => false,
                    Err(_) => false,
                },
                Err(_) => false,
            },
        }
    }

    pub fn elapsed(&self) -> Duration {
        if self.seeking {
            return self.seek_target;
        }

        self.paused_elapsed
            + self
                .started_at
                .map(|started| started.elapsed())
                .unwrap_or_default()
    }

    pub fn set_volume(&mut self, vol: f32) {
        self.volume = vol.clamp(0.0, 1.0);

        if let Some(ref mut pulse) = self.pulse {
            if let Some(index) = pulse.find_ffmpeg_sink_input() {
                pulse.set_volume(index, self.volume);
            }
        }
    }

    pub fn volume_up(&mut self) {
        self.set_volume(self.volume + 0.05);
    }

    pub fn volume_down(&mut self) {
        self.set_volume(self.volume - 0.05);
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop();
    }
}

fn gain_to_linear(gain_db: Option<f32>) -> f32 {
    match gain_db {
        Some(db) => 10f32.powf(db / 20.0).clamp(0.0, 4.0),
        None => 1.0,
    }
}
