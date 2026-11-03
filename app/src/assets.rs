//! The libraries that come with the app: Fluent Emoji stickers (MIT, Microsoft), Kenney's
//! sound packs (CC0), and sound effects the app makes itself. Licences sit beside each
//! set in assets/. Sounds are written out to disk the first time they are used, because
//! the mixer reads files.

use crate::overlay::{Stamp, stamp_of};
use crate::sound_set::SOUND_FILES;
use crate::store;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

const EMOJI_TSV: &str = include_str!("../assets/stickers/fluent-emoji-flat.tsv");
pub const EMOJI_GROUPS: [&str; 9] = ["Smileys & Emotion", "People & Body", "Animals & Nature", "Food & Drink", "Activities", "Travel & Places", "Objects", "Symbols", "Flags"];
/// The emoji's own drawing area.
const EMOJI_BOX: u32 = 32;

pub struct Emoji {
    pub group: &'static str,
    /// Its name in the set, such as \`grinning-face\`.
    pub name: &'static str,
    body: &'static str,
}

impl Emoji {
    /// "grinning face", for showing and searching.
    pub fn label(&self) -> String {
        self.name.replace('-', " ")
    }

    pub fn svg(&self, px: u32) -> String {
        format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="{px}" height="{px}" viewBox="0 0 {EMOJI_BOX} {EMOJI_BOX}">{}</svg>"#, self.body)
    }
}

pub fn emoji() -> &'static [Emoji] {
    static ALL: OnceLock<Vec<Emoji>> = OnceLock::new();
    ALL.get_or_init(|| {
        EMOJI_TSV
            .lines()
            .filter_map(|l| {
                let mut parts = l.splitn(3, '\t');
                Some(Emoji { group: parts.next()?, name: parts.next()?, body: parts.next()? })
            })
            .collect()
    })
}

pub fn emoji_named(name: &str) -> Option<&'static Emoji> {
    static BY_NAME: OnceLock<HashMap<&'static str, usize>> = OnceLock::new();
    let map = BY_NAME.get_or_init(|| emoji().iter().enumerate().map(|(i, e)| (e.name, i)).collect());
    map.get(name).map(|i| &emoji()[*i])
}

/// An emoji drawn \`px\` pixels square, ready to lay on a picture.
pub fn emoji_stamp(name: &str, px: u32) -> Option<Stamp> {
    let e = emoji_named(name)?;
    let px = px.clamp(8, 4096);
    let tree = resvg::usvg::Tree::from_str(&e.svg(px), &resvg::usvg::Options::default()).ok()?;
    let mut map = resvg::tiny_skia::Pixmap::new(px, px)?;
    resvg::render(&tree, resvg::tiny_skia::Transform::identity(), &mut map.as_mut());
    Some(stamp_of(&map))
}

/// Sounds the app makes itself, so the editing staples need no licence at all.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Made {
    Whoosh,
    Swoosh,
    Riser,
    Boom,
    Hit,
    Pop,
    Ding,
    Chime,
    Censor,
    Tick,
    Heartbeat,
    Shutter,
    Glitch,
    BassDrop,
}

impl Made {
    pub const ALL: [Made; 14] = [
        Made::Whoosh,
        Made::Swoosh,
        Made::Riser,
        Made::Boom,
        Made::Hit,
        Made::Pop,
        Made::Ding,
        Made::Chime,
        Made::Censor,
        Made::Tick,
        Made::Heartbeat,
        Made::Shutter,
        Made::Glitch,
        Made::BassDrop,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Made::Whoosh => "Whoosh",
            Made::Swoosh => "Swoosh",
            Made::Riser => "Riser",
            Made::Boom => "Boom",
            Made::Hit => "Hit",
            Made::Pop => "Pop",
            Made::Ding => "Ding",
            Made::Chime => "Chime",
            Made::Censor => "Censor Beep",
            Made::Tick => "Tick",
            Made::Heartbeat => "Heartbeat",
            Made::Shutter => "Camera Shutter",
            Made::Glitch => "Glitch",
            Made::BassDrop => "Bass Drop",
        }
    }

    fn file(self) -> String {
        format!("made/{}.wav", self.name().to_lowercase().replace(' ', "-"))
    }
}

pub const MADE_PACK: &str = "For Editing";
pub const SOUND_PACKS: [&str; 4] = [MADE_PACK, "Interface", "Digital", "Foley"];

#[derive(Clone, Copy)]
pub enum Source {
    Made(Made),
    File(&'static str, &'static [u8]),
}

#[derive(Clone, Copy)]
pub struct Sound {
    pub pack: &'static str,
    pub name: &'static str,
    pub source: Source,
}

pub fn sounds() -> &'static [Sound] {
    static ALL: OnceLock<Vec<Sound>> = OnceLock::new();
    ALL.get_or_init(|| {
        let made = Made::ALL.iter().map(|m| Sound { pack: MADE_PACK, name: m.name(), source: Source::Made(*m) });
        let files = SOUND_FILES.iter().map(|(pack, name, file, bytes)| Sound { pack, name, source: Source::File(file, bytes) });
        made.chain(files).collect()
    })
}

/// Where a sound lives on disk, writing it there first if it isn't yet.
pub fn sound_path(s: &Sound) -> Result<PathBuf, String> {
    let rel = match s.source {
        Source::Made(m) => m.file(),
        Source::File(f, _) => f.to_string(),
    };
    let path = store::data_dir().join("Sounds").join(rel);
    if path.exists() {
        return Ok(path);
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let bytes = match s.source {
        Source::Made(m) => wav(&synth(m)),
        Source::File(_, b) => b.to_vec(),
    };
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(path)
}

const RATE: u32 = 48_000;

/// A repeatable noise, so a made sound is the same every time.
struct Noise(u32);

impl Noise {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (self.0 >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0
    }
}

/// Noise through a low-pass whose corner follows \`corner\` (Hz) over time.
fn swept_noise(seconds: f32, seed: u32, corner: impl Fn(f32) -> f32, level: impl Fn(f32) -> f32) -> Vec<f32> {
    let n = (seconds * RATE as f32) as usize;
    let mut noise = Noise(seed);
    let (mut low, mut band) = (0.0f32, 0.0f32);
    (0..n)
        .map(|i| {
            let t = i as f32 / n as f32;
            let k = (std::f32::consts::TAU * corner(t) / RATE as f32).min(0.9);
            low += k * (noise.next() - low);
            band += k * (low - band);
            (low - band) * 3.0 * level(t)
        })
        .collect()
}

/// A sine whose pitch follows \`pitch\` (Hz) over time.
fn tone(seconds: f32, pitch: impl Fn(f32) -> f32, level: impl Fn(f32) -> f32) -> Vec<f32> {
    let n = (seconds * RATE as f32) as usize;
    let mut phase = 0.0f32;
    (0..n)
        .map(|i| {
            let t = i as f32 / n as f32;
            phase += std::f32::consts::TAU * pitch(t) / RATE as f32;
            phase.sin() * level(t)
        })
        .collect()
}

fn mix(a: &mut Vec<f32>, b: &[f32], at: f32) {
    let from = (at * RATE as f32) as usize;
    if a.len() < from + b.len() {
        a.resize(from + b.len(), 0.0);
    }
    for (o, v) in a[from..].iter_mut().zip(b) {
        *o += v;
    }
}

/// Rises quickly, then falls away.
fn swell(peak: f32) -> impl Fn(f32) -> f32 {
    move |t: f32| if t < peak { (t / peak).powi(2) } else { (1.0 - (t - peak) / (1.0 - peak)).max(0.0).powi(2) }
}

fn decay(rate: f32) -> impl Fn(f32) -> f32 {
    move |t: f32| (-t * rate).exp()
}

pub fn synth(m: Made) -> Vec<f32> {
    let mut s = match m {
        Made::Whoosh => swept_noise(0.9, 7, |t| 300.0 + 2600.0 * (std::f32::consts::PI * t).sin(), swell(0.55)),
        Made::Swoosh => swept_noise(0.45, 11, |t| 900.0 + 4500.0 * t, swell(0.35)),
        Made::Riser => {
            let mut v = swept_noise(2.5, 3, |t| 200.0 + 6000.0 * t * t, |t| t * t * 0.7);
            mix(&mut v, &tone(2.5, |t| 180.0 * 2f32.powf(t * 2.5), |t| t * t * 0.35), 0.0);
            v
        }
        Made::Boom => {
            let mut v = tone(1.6, |t| 62.0 - 28.0 * t, decay(3.5));
            mix(&mut v, &swept_noise(0.4, 5, |_| 400.0, decay(9.0)), 0.0);
            v
        }
        Made::Hit => {
            let mut v = tone(0.5, |t| 110.0 - 50.0 * t, decay(9.0));
            mix(&mut v, &swept_noise(0.12, 9, |_| 3000.0, decay(30.0)), 0.0);
            v
        }
        Made::Pop => tone(0.14, |t| 700.0 - 450.0 * t, decay(14.0)),
        Made::Ding => {
            let mut v = tone(1.5, |_| 1318.5, |t| decay(4.0)(t) * 0.6);
            mix(&mut v, &tone(1.5, |_| 2637.0, |t| decay(7.0)(t) * 0.2), 0.0);
            v
        }
        Made::Chime => {
            let mut v = Vec::new();
            for (i, f) in [1046.5f32, 1318.5, 1568.0].iter().enumerate() {
                mix(&mut v, &tone(1.3, move |_| *f, |t| decay(4.0)(t) * 0.45), i as f32 * 0.12);
            }
            v
        }
        Made::Censor => tone(1.0, |_| 1000.0, |t| if !(0.01..=0.99).contains(&t) { 0.0 } else { 0.5 }),
        Made::Tick => swept_noise(0.05, 13, |_| 5000.0, decay(60.0)),
        Made::Heartbeat => {
            let mut v = Vec::new();
            for at in [0.0, 0.28] {
                mix(&mut v, &tone(0.25, |t| 60.0 - 20.0 * t, decay(12.0)), at);
            }
            v.resize((1.2 * RATE as f32) as usize, 0.0);
            v
        }
        Made::Shutter => {
            let mut v = swept_noise(0.06, 17, |_| 4000.0, decay(40.0));
            mix(&mut v, &swept_noise(0.08, 19, |_| 2500.0, decay(30.0)), 0.12);
            v
        }
        Made::Glitch => {
            let mut noise = Noise(23);
            let n = (0.6 * RATE as f32) as usize;
            let mut level = 0.0;
            (0..n)
                .map(|i| {
                    if i % 1200 == 0 {
                        level = if noise.next() > 0.0 { 0.5 } else { 0.0 };
                    }
                    let square = if (i / (20 + (i / 2400) * 7)) % 2 == 0 { 1.0 } else { -1.0 };
                    square * level
                })
                .collect()
        }
        Made::BassDrop => tone(1.6, |t| 140.0 * (1.0 - t).powi(2) + 38.0, |t| (decay(1.5)(t) * 1.6).tanh() * 0.8),
    };
    // Every sound peaks at the same height and fades its last few milliseconds.
    let peak = s.iter().fold(0.0f32, |m, v| m.max(v.abs())).max(1e-6);
    let fade = (RATE / 200) as usize;
    let len = s.len();
    for (i, v) in s.iter_mut().enumerate() {
        let tail = ((len - i) as f32 / fade as f32).min(1.0);
        *v = (*v / peak * 0.85 * tail).clamp(-1.0, 1.0);
    }
    s
}

/// 16-bit mono WAV.
pub fn wav(samples: &[f32]) -> Vec<u8> {
    let data = samples.len() as u32 * 2;
    let mut out = Vec::with_capacity(44 + data as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data.to_le_bytes());
    for v in samples {
        out.extend_from_slice(&((v * 32767.0) as i16).to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_emoji_loads_and_the_groups_are_all_used() {
        let all = emoji();
        assert!(all.len() > 1500);
        assert!(EMOJI_GROUPS.iter().all(|g| all.iter().any(|e| e.group == *g)));
        assert!(all.iter().all(|e| EMOJI_GROUPS.contains(&e.group) && e.body.starts_with('<')));
    }

    #[test]
    fn an_emoji_draws_in_colour() {
        let s = emoji_stamp("grinning-face", 64).unwrap();
        assert_eq!((s.w, s.h), (64, 64));
        assert!(s.px.iter().any(|c| c[3] > 0.9 && c[0] > 0.5 && c[2] < 0.3), "a yellow face");
        assert!(emoji_stamp("no-such-emoji", 64).is_none());
    }

    #[test]
    fn skin_tone_copies_are_left_out() {
        assert!(emoji().iter().all(|e| !e.name.ends_with("-dark") && !e.name.ends_with("-light")));
    }

    #[test]
    fn every_made_sound_is_audible_and_within_range() {
        for m in Made::ALL {
            let s = synth(m);
            assert!(s.len() > RATE as usize / 40, "{m:?}");
            let peak = s.iter().fold(0.0f32, |p, v| p.max(v.abs()));
            assert!(peak > 0.5 && peak <= 1.0 && s.iter().all(|v| v.is_finite()), "{m:?} {peak}");
            assert!(s.last().unwrap().abs() < 0.01, "{m:?} ends without a click");
        }
    }

    #[test]
    fn a_wav_says_how_long_it_is() {
        let w = wav(&[0.0; 480]);
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(u32::from_le_bytes([w[40], w[41], w[42], w[43]]), 960);
        assert_eq!(w.len(), 44 + 960);
    }

    #[test]
    fn the_library_holds_every_pack() {
        let all = sounds();
        assert!(SOUND_PACKS.iter().all(|p| all.iter().any(|s| s.pack == *p)));
        assert_eq!(all.len(), Made::ALL.len() + SOUND_FILES.len());
    }
}
