//! Sound output. The speakers run at a steady rate, so samples played is also the
//! playback clock: pictures follow it, and sound and picture can't drift apart.

use crate::timeline::{Part, mix_graph};
use crate::video;
use cpal::SampleFormat;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::collections::VecDeque;
use std::io::Read;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How much mixed sound to hold ready, in seconds.
const AHEAD: f32 = 0.4;
const LEVEL_FALL: f32 = 0.92;

#[derive(Default)]
struct Shared {
    queue: Mutex<VecDeque<f32>>,
    /// Stereo pairs that really reached the speakers.
    played: AtomicU64,
    /// Held shut until the pictures are ready, so both begin together.
    open: AtomicBool,
    /// Loudest recent sample, for the meter.
    level: AtomicU32,
}

impl Shared {
    fn fill(&self, out: &mut [f32], channels: usize) {
        if !self.open.load(Ordering::Relaxed) {
            out.fill(0.0);
            return;
        }
        let mut peak = f32::from_bits(self.level.load(Ordering::Relaxed)) * LEVEL_FALL;
        let Ok(mut q) = self.queue.lock() else {
            out.fill(0.0);
            return;
        };
        let mut pairs = 0;
        for frame in out.chunks_mut(channels) {
            if q.len() < 2 {
                frame.fill(0.0);
                continue;
            }
            let (l, r) = (q.pop_front().unwrap_or(0.0), q.pop_front().unwrap_or(0.0));
            peak = peak.max(l.abs()).max(r.abs());
            for (i, s) in frame.iter_mut().enumerate() {
                *s = if i % 2 == 0 { l } else { r };
            }
            pairs += 1;
        }
        drop(q);
        self.played.fetch_add(pairs, Ordering::Relaxed);
        self.level.store(peak.min(1.0).to_bits(), Ordering::Relaxed);
    }
}

/// The open sound device. Quietly absent on a machine with no output.
pub struct Output {
    shared: Arc<Shared>,
    pub rate: u32,
}

impl Output {
    pub fn open() -> Option<Self> {
        let shared = Arc::new(Shared::default());
        let (tx, rx) = std::sync::mpsc::channel();
        let inner = shared.clone();
        // The stream can't cross threads on Windows, so it lives on one of its own.
        std::thread::spawn(move || {
            let made = (|| {
                let device = cpal::default_host().default_output_device()?;
                let config = device.default_output_config().ok()?;
                let channels = config.channels() as usize;
                let rate = config.sample_rate().0;
                let format = config.sample_format();
                let stream_config = config.config();
                let oops = |_| {};
                let s = inner.clone();
                let stream = match format {
                    SampleFormat::F32 => device.build_output_stream(&stream_config, move |out: &mut [f32], _| s.fill(out, channels), oops, None),
                    SampleFormat::I16 => device.build_output_stream(
                        &stream_config,
                        move |out: &mut [i16], _| {
                            let mut f = vec![0.0; out.len()];
                            s.fill(&mut f, channels);
                            for (o, v) in out.iter_mut().zip(&f) {
                                *o = (v.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
                            }
                        },
                        oops,
                        None,
                    ),
                    _ => return None,
                }
                .ok()?;
                stream.play().ok()?;
                Some((stream, rate))
            })();
            match made {
                Some((stream, rate)) => {
                    let _ = tx.send(Some(rate));
                    // Held here for as long as the app runs.
                    loop {
                        std::thread::park();
                        let _ = &stream;
                    }
                }
                None => {
                    let _ = tx.send(None);
                }
            }
        });
        let rate = rx.recv_timeout(Duration::from_secs(3)).ok().flatten()?;
        Some(Self { shared, rate })
    }

    pub fn level(&self) -> f32 {
        f32::from_bits(self.shared.level.load(Ordering::Relaxed))
    }

    pub fn fade_level(&self) {
        let v = self.level() * LEVEL_FALL;
        self.shared.level.store(v.to_bits(), Ordering::Relaxed);
    }

    /// Seconds played since the current run began.
    pub fn seconds(&self) -> f64 {
        self.shared.played.load(Ordering::Relaxed) as f64 / self.rate.max(1) as f64
    }

    pub fn start_together(&self) {
        self.shared.open.store(true, Ordering::Relaxed);
    }

    pub fn has_sound_ready(&self) -> bool {
        self.shared.queue.lock().is_ok_and(|q| q.len() as f32 > self.rate as f32 * 0.1)
    }

    fn reset(&self) {
        self.shared.open.store(false, Ordering::Relaxed);
        self.shared.played.store(0, Ordering::Relaxed);
        if let Ok(mut q) = self.shared.queue.lock() {
            q.clear();
        }
    }
}

/// Mixes the project's sound from a point onward and feeds it to the speakers.
pub struct Feed {
    stop: Arc<AtomicBool>,
}

impl Feed {
    pub fn start(out: &Output, parts: Vec<Part>, from: f64, seconds: f64) -> Self {
        out.reset();
        let stop = Arc::new(AtomicBool::new(false));
        let (flag, shared, rate) = (stop.clone(), out.shared.clone(), out.rate);
        std::thread::spawn(move || {
            let (inputs, filter) = mix_graph(&parts, from, seconds, rate, 0);
            let child = video::tool("ffmpeg")
                .args(["-v", "error", "-nostdin"])
                .args(&inputs)
                .args(["-filter_complex", &filter, "-map", "[mix]", "-f", "f32le", "-ac", "2", "-ar"])
                .arg(rate.to_string())
                .arg("pipe:1")
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn();
            let Ok(mut child) = child else { return };
            let Some(mut pipe) = child.stdout.take() else { return };
            let mut buf = vec![0u8; 8192];
            let room = (rate as f32 * AHEAD * 2.0) as usize;
            let mut spare: Vec<u8> = Vec::new();
            while !flag.load(Ordering::Relaxed) {
                let Ok(n) = pipe.read(&mut buf) else { break };
                if n == 0 {
                    break;
                }
                spare.extend_from_slice(&buf[..n]);
                let whole = spare.len() / 4 * 4;
                if let Ok(mut q) = shared.queue.lock() {
                    q.extend(spare[..whole].chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])));
                }
                spare.drain(..whole);
                while !flag.load(Ordering::Relaxed) && shared.queue.lock().is_ok_and(|q| q.len() > room) {
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
            let _ = child.kill();
            let _ = child.wait();
        });
        Self { stop }
    }
}

impl Drop for Feed {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_plays_until_the_gate_opens() {
        let s = Shared::default();
        s.queue.lock().unwrap().extend([0.5, 0.5]);
        let mut out = [1.0; 2];
        s.fill(&mut out, 2);
        assert_eq!(out, [0.0, 0.0]);
        assert_eq!(s.played.load(Ordering::Relaxed), 0);
        s.open.store(true, Ordering::Relaxed);
        s.fill(&mut out, 2);
        assert_eq!(out, [0.5, 0.5]);
        assert_eq!(s.played.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn the_clock_stops_when_the_sound_runs_out() {
        let s = Shared::default();
        s.open.store(true, Ordering::Relaxed);
        let mut out = [1.0; 4];
        s.fill(&mut out, 2);
        assert_eq!(out, [0.0; 4]);
        assert_eq!(s.played.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn extra_speakers_get_the_stereo_pair_in_turn() {
        let s = Shared::default();
        s.open.store(true, Ordering::Relaxed);
        s.queue.lock().unwrap().extend([0.1, 0.9]);
        let mut out = [0.0; 4];
        s.fill(&mut out, 4);
        assert_eq!(out, [0.1, 0.9, 0.1, 0.9]);
    }

    #[test]
    fn the_meter_catches_a_peak() {
        let s = Shared::default();
        s.open.store(true, Ordering::Relaxed);
        s.queue.lock().unwrap().extend([0.2, -0.7]);
        let mut out = [0.0; 2];
        s.fill(&mut out, 2);
        assert!((f32::from_bits(s.level.load(Ordering::Relaxed)) - 0.7).abs() < 1e-5);
    }
}
