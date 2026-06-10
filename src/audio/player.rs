use anyhow::{Context, Result};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use libpulse_binding as pulse;
use pulse::context::Context as PulseContext;
use pulse::mainloop::threaded::Mainloop;
use pulse::volume::{ChannelVolumes, Volume};

pub struct Player {
    current_child: Option<Arc<Mutex<std::process::Child>>>,
    pub volume: f32,
    started_at: Option<Instant>,
    paused_elapsed: Duration,
    current_path: Option<std::path::PathBuf>,
    current_gain: Option<f32>,
    paused: bool,
    pulse: Option<PulseHandle>,
}

/// Holds a connected PulseAudio mainloop + context
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

        // Wait for connection
        loop {
            match context.get_state() {
                pulse::context::State::Ready => break,
                pulse::context::State::Failed
                | pulse::context::State::Terminated => return None,
                _ => {}
            }

            std::thread::sleep(Duration::from_millis(10));
        }

        Some(Self { mainloop, context })
    }

    /// Find the sink-input index for ffmpeg
    fn find_ffmpeg_sink_input(&mut self) -> Option<u32> {
        let result = Arc::new(Mutex::new(None::<u32>));
        let result_clone = Arc::clone(&result);

        let introspector = self.context.introspect();

        self.mainloop.lock();

        let op = introspector.get_sink_input_info_list(move |item| {
            if let pulse::callbacks::ListResult::Item(info) = item {
                if let Some(props) =
                    info.proplist.get_str("application.process.binary")
                {
                    if props == "ffmpeg" {
                        *result_clone.lock().unwrap() =
                            Some(info.index);
                    }
                }
            }
        });

        self.mainloop.unlock();

        // Wait for operation to complete
        loop {
            self.mainloop.lock();
            let state = op.get_state();
            self.mainloop.unlock();

            match state {
                pulse::operation::State::Done => break,
                pulse::operation::State::Cancelled => return None,
                pulse::operation::State::Running => {
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
        }

        let res = *result.lock().unwrap();
        res
    }

    fn set_volume(&mut self, index: u32, vol: f32) {
        let linear =
            (vol * pulse::volume::Volume::NORMAL.0 as f32) as u32;

        let mut binding = ChannelVolumes::default();

        let cv = binding.set(
            2,
            Volume(linear.min(pulse::volume::Volume::MAX.0)),
        );

        let mut introspector = self.context.introspect();

        self.mainloop.lock();
        introspector.set_sink_input_volume(index, cv, None);
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
                "Warning: could not connect to PulseAudio/PipeWire — volume control disabled"
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
        })
    }

    pub fn play(&mut self, path: &Path) -> Result<()> {
        self.play_from(path, Duration::ZERO, None)
    }

    pub fn play_with_gain(
        &mut self,
        path: &Path,
        gain_db: Option<f32>,
    ) -> Result<()> {
        self.play_from(path, Duration::ZERO, gain_db)
    }

    pub fn seek(
        &mut self,
        path: &Path,
        to: Duration,
    ) -> Result<()> {
        self.play_from(path, to, self.current_gain)
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
        let path_str = path.to_str().context("invalid path")?;

        let rg_linear = gain_to_linear(gain_db);
        let volume_str = format!("{:.6}", rg_linear);

        let child = Command::new("ffmpeg")
            .args([
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
            for _ in 0..10 {
                std::thread::sleep(Duration::from_millis(50));

                if let Some(idx) =
                    pulse.find_ffmpeg_sink_input()
                {
                    pulse.set_volume(idx, vol);
                    break;
                }
            }
        }

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
        if !self.paused {
            if let Some(ref mut pulse) = self.pulse {
                if let Some(idx) =
                    pulse.find_ffmpeg_sink_input()
                {
                    pulse.set_mute(idx, true);
                }
            }

            if let Some(ref child) = self.current_child {
                if let Ok(c) = child.lock() {
                    unsafe {
                        libc::kill(
                            c.id() as i32,
                            libc::SIGSTOP,
                        );
                    }
                }
            }

            if let Some(started) = self.started_at.take() {
                self.paused_elapsed += started.elapsed();
            }

            self.paused = true;
        }
    }

    pub fn resume(&mut self) {
        if self.paused {
            if let Some(ref child) = self.current_child {
                if let Ok(c) = child.lock() {
                    unsafe {
                        libc::kill(
                            c.id() as i32,
                            libc::SIGCONT,
                        );
                    }
                }
            }

            if let Some(ref mut pulse) = self.pulse {
                if let Some(idx) =
                    pulse.find_ffmpeg_sink_input()
                {
                    pulse.set_mute(idx, false);
                }
            }

            self.started_at = Some(Instant::now());
            self.paused = false;
        }
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
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    pub fn is_finished(&self) -> bool {
        match &self.current_child {
            None => true,
            Some(child) => match child.lock() {
                Ok(mut c) => {
                    matches!(c.try_wait(), Ok(Some(_)))
                }
                Err(_) => false,
            },
        }
    }

    pub fn elapsed(&self) -> Duration {
        let running = self
            .started_at
            .map(|t| t.elapsed())
            .unwrap_or_default();

        self.paused_elapsed + running
    }

    pub fn set_volume(&mut self, vol: f32) {
        self.volume = vol.clamp(0.0, 1.0);

        if let Some(ref mut pulse) = self.pulse {
            if let Some(idx) =
                pulse.find_ffmpeg_sink_input()
            {
                pulse.set_volume(idx, self.volume);
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
        Some(db) => {
            10f32.powf(db / 20.0).clamp(0.0, 4.0)
        }
        None => 1.0,
    }
}