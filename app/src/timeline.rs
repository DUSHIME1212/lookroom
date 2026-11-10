//! The project: media in the bin, clips along the main track, and everything placed
//! above and below them. Times are whole frames of the project's rate, except where a
//! clip reads from its source, which is in seconds because sources have their own rates.

use crate::color::Source;
use crate::pipeline::Grade;
use crate::shared::Shared;
use crate::textanim::Motion;
use crate::video::Info;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

pub const ROW_VIDEO: usize = 0;
pub const ROW_SOUND: usize = 1;
pub const ROW_FX: usize = 2;
pub const ROW_FX2: usize = 3;
pub const ROW_MUSIC: usize = 4;
pub const ROWS: usize = 5;
pub const PHOTO_SECONDS: f64 = 5.0;
pub const ITEM_SECONDS: f64 = 4.0;
pub const DEFAULT_FPS: f64 = 30.0;
pub const DEFAULT_SIZE: (usize, usize) = (1920, 1080);
pub const MIN_LEN: u32 = 2;
/// Slowest and fastest a clip can play.
pub const MIN_SPEED: f64 = 0.1;
pub const MAX_SPEED: f64 = 8.0;
/// The speeds offered as one click.
pub const SPEEDS: [f64; 9] = [0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 2.0, 3.0, 4.0];
/// A photo can be held for as long as anyone likes.
const ENDLESS: u32 = u32::MAX / 4;

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub enum Kind {
    Video,
    Photo,
    Audio,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Media {
    pub id: u32,
    /// Empty for the built-in sample scene.
    pub path: Option<PathBuf>,
    pub name: String,
    pub kind: Kind,
    pub seconds: f64,
    pub info: Option<Info>,
    pub has_audio: bool,
    pub size: (usize, usize),
}

pub use crate::transitions::Wipe;

/// How a clip takes over from the one before it. The two overlap for `len` frames.
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug)]
pub struct Transition {
    pub kind: Wipe,
    pub len: u32,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Clip {
    pub id: u32,
    pub media: u32,
    /// Where in the source this clip begins.
    pub start: f64,
    pub len: u32,
    pub grade: Shared<Grade>,
    pub transition: Option<Transition>,
    pub volume: f32,
    pub muted: bool,
    /// Its sound has moved to an audio item of its own.
    pub detached: bool,
    /// How fast the source plays: 2 covers twice the source in the same time.
    #[serde(default = "normal_speed")]
    pub speed: f64,
    /// A speed ramp: how fast the clip plays at points through it. Empty plays at `speed`.
    #[serde(default)]
    pub ramp: Vec<RampPoint>,
    /// J cut: seconds its sound starts before its picture.
    #[serde(default)]
    pub lead: f64,
    /// L cut: seconds its sound runs on after its picture.
    #[serde(default)]
    pub tail: f64,
    /// Punch in: how far the picture is enlarged, from its centre.
    #[serde(default = "normal_zoom")]
    pub zoom: f32,
    /// How hard its own sound's noise is cleaned up, 0 for not at all.
    #[serde(default)]
    pub denoise: f32,
}

fn normal_speed() -> f64 {
    1.0
}

fn normal_zoom() -> f32 {
    1.0
}

/// One point of a speed ramp: `at` is how far through the clip, from 0 to 1.
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug)]
pub struct RampPoint {
    pub at: f64,
    pub speed: f64,
}

/// Ramps people reach for, each a list of (how far through, speed).
pub const RAMPS: [(&str, &[(f64, f64)]); 6] = [
    ("Montage", &[(0.0, 3.0), (0.35, 3.0), (0.5, 0.35), (0.65, 3.0), (1.0, 3.0)]),
    ("Hero", &[(0.0, 1.0), (0.4, 0.3), (0.6, 0.3), (1.0, 1.0)]),
    ("Bullet", &[(0.0, 1.0), (0.3, 0.2), (0.7, 0.2), (1.0, 4.0)]),
    ("Speed Up", &[(0.0, 1.0), (1.0, 4.0)]),
    ("Slow Down", &[(0.0, 2.5), (1.0, 0.4)]),
    ("Flash In", &[(0.0, 5.0), (0.25, 1.0), (1.0, 1.0)]),
];

impl Clip {
    /// Seconds into the source, `offset` frames into the clip.
    pub fn source_at(&self, offset: f64, fps: f64) -> f64 {
        let fps = fps.max(1.0);
        if self.ramp.len() < 2 {
            return self.start + offset / fps * self.speed;
        }
        let seconds = self.len.max(1) as f64 / fps;
        self.start + seconds * ramp_area(&self.ramp, offset / self.len.max(1) as f64)
    }

    /// The frame into the clip that shows `source` seconds of its source: the inverse of
    /// `source_at`, found by halving, since a ramp never runs backwards.
    pub fn offset_of(&self, source: f64, fps: f64) -> f64 {
        if self.ramp.len() < 2 {
            return (source - self.start) / self.speed * fps.max(1.0);
        }
        let (mut lo, mut hi) = (0.0, self.len as f64);
        for _ in 0..40 {
            let mid = (lo + hi) / 2.0;
            if self.source_at(mid, fps) < source {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        lo
    }

    /// How fast it plays at `t`, from 0 to 1 through the clip.
    pub fn speed_at(&self, t: f64) -> f64 {
        if self.ramp.len() < 2 { self.speed } else { ramp_speed(&self.ramp, t) }
    }
}

fn ramp_speed(r: &[RampPoint], t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    match r.iter().position(|p| p.at >= t) {
        Some(0) => r[0].speed,
        Some(i) => {
            let (a, b) = (r[i - 1], r[i]);
            let k = if b.at > a.at { (t - a.at) / (b.at - a.at) } else { 1.0 };
            a.speed + (b.speed - a.speed) * k
        }
        None => r[r.len() - 1].speed,
    }
}

/// The ramp's speed added up from the start to `t`: source covered, in clip lengths.
/// Exact, as the speed runs in straight lines between points.
pub fn ramp_area(r: &[RampPoint], t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    let mut area = 0.0;
    let mut last = (0.0, ramp_speed(r, 0.0));
    let stops = r.iter().map(|p| p.at).filter(|a| *a > 0.0 && *a < t).chain(std::iter::once(t));
    for at in stops {
        let s = ramp_speed(r, at);
        area += (at - last.0) * (s + last.1) / 2.0;
        last = (at, s);
    }
    area
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub enum Fx {
    BlackWhite,
    BlockFlashes,
    PullIn,
    Shake,
    Strobe,
    Glitch,
    FadeIn,
    FadeOut,
    Pixelate,
    RgbSplit,
    Vhs,
    OldFilm,
    Invert,
    Sepia,
    Blur,
    Mirror,
    Kaleidoscope,
    LightLeak,
    PushOut,
    Rainbow,
    Glow,
    Heartbeat,
    Duotone,
    Flicker,
    Flash,
    Shutter,
}

impl Fx {
    pub const ALL: [Fx; 26] = [
        Fx::FadeIn,
        Fx::FadeOut,
        Fx::BlackWhite,
        Fx::Sepia,
        Fx::Duotone,
        Fx::Invert,
        Fx::Rainbow,
        Fx::Glow,
        Fx::LightLeak,
        Fx::Blur,
        Fx::OldFilm,
        Fx::Vhs,
        Fx::Glitch,
        Fx::RgbSplit,
        Fx::Pixelate,
        Fx::BlockFlashes,
        Fx::Strobe,
        Fx::Flicker,
        Fx::Shutter,
        Fx::Flash,
        Fx::PullIn,
        Fx::PushOut,
        Fx::Heartbeat,
        Fx::Shake,
        Fx::Mirror,
        Fx::Kaleidoscope,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Fx::BlackWhite => "Black White Effect",
            Fx::BlockFlashes => "Block Flashes",
            Fx::PullIn => "Pull In Effect",
            Fx::Shake => "Camera Shake",
            Fx::Strobe => "Strobe",
            Fx::Glitch => "Colour Glitch",
            Fx::FadeIn => "Fade In",
            Fx::FadeOut => "Fade Out",
            Fx::Pixelate => "Pixelate",
            Fx::RgbSplit => "RGB Split",
            Fx::Vhs => "VHS",
            Fx::OldFilm => "Old Film",
            Fx::Invert => "Invert",
            Fx::Sepia => "Sepia",
            Fx::Blur => "Blur",
            Fx::Mirror => "Mirror",
            Fx::Kaleidoscope => "Kaleidoscope",
            Fx::LightLeak => "Light Leak",
            Fx::PushOut => "Push Out",
            Fx::Rainbow => "Rainbow",
            Fx::Glow => "Glow",
            Fx::Heartbeat => "Heartbeat",
            Fx::Duotone => "Duotone",
            Fx::Flicker => "Light Flicker",
            Fx::Flash => "White Flash",
            Fx::Shutter => "Shutter Flicker",
        }
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub enum Body {
    Audio {
        media: u32,
        start: f64,
        volume: f32,
        name: String,
        #[serde(default = "normal_speed")]
        speed: f64,
        /// Noise clean-up, 0 to 1.
        #[serde(default)]
        denoise: f32,
    },
    Effect { kind: Fx, amount: f32 },
    Text {
        text: String,
        size: f32,
        colour: [f32; 3],
        x: f32,
        y: f32,
        #[serde(default)]
        motion: Motion,
    },
    Sticker {
        icon: usize,
        /// An emoji from the sticker library, by name; empty for the icon stickers.
        #[serde(default)]
        emoji: String,
        size: f32,
        x: f32,
        y: f32,
        #[serde(default)]
        motion: Motion,
    },
    /// One layer of an imported design, such as a Figma frame saved as SVG. `size` 1
    /// draws the whole design as large as fits the picture.
    Graphic {
        file: String,
        layer: usize,
        name: String,
        size: f32,
        x: f32,
        y: f32,
        #[serde(default)]
        motion: Motion,
    },
    /// A video over the picture, for meme clips and picture in picture. `size` is its
    /// height as a share of the picture's. With a key colour, that colour turns see-through.
    Overlay {
        media: u32,
        start: f64,
        size: f32,
        x: f32,
        y: f32,
        key: Option<[f32; 3]>,
        tolerance: f32,
        volume: f32,
        #[serde(default)]
        motion: Motion,
    },
}

impl Body {
    /// Where it sits and how it moves, for the things that can be animated.
    pub fn placed_mut(&mut self) -> Option<(&mut f32, &mut f32, &mut Motion)> {
        match self {
            Body::Text { x, y, motion, .. } | Body::Sticker { x, y, motion, .. } | Body::Graphic { x, y, motion, .. } | Body::Overlay { x, y, motion, .. } => Some((x, y, motion)),
            _ => None,
        }
    }

    pub fn placed(&self) -> Option<(f32, f32, &Motion)> {
        match self {
            Body::Text { x, y, motion, .. } | Body::Sticker { x, y, motion, .. } | Body::Graphic { x, y, motion, .. } | Body::Overlay { x, y, motion, .. } => Some((*x, *y, motion)),
            _ => None,
        }
    }
}

/// Gives `to` whatever changed between `before` and `after`, one setting at a time, so
/// several items can be edited as one. What each says, and its keyframes, stay its own.
/// Places, sizes and turns move by the same amount, so a group keeps its arrangement;
/// fonts, colours and animations are copied as they are.
pub fn spread(before: &Body, after: &Body, to: &mut Body) {
    fn by(was: f32, now: f32, v: &mut f32) {
        if was != now {
            *v += now - was;
        }
    }
    fn times(was: f32, now: f32, v: &mut f32) {
        if was != now && was.abs() > 1e-6 {
            *v *= now / was;
        }
    }
    fn set<T: PartialEq + Clone>(was: &T, now: &T, v: &mut T) {
        if was != now {
            *v = now.clone();
        }
    }
    if let (Some((bx, by_, bm)), Some((ax, ay, am)), Some((x, y, m))) = (before.placed(), after.placed(), to.placed_mut()) {
        by(bx, ax, x);
        by(by_, ay, y);
        set(&bm.font, &am.font, &mut m.font);
        set(&bm.weight, &am.weight, &mut m.weight);
        set(&bm.italic, &am.italic, &mut m.italic);
        set(&bm.spacing, &am.spacing, &mut m.spacing);
        set(&bm.outline, &am.outline, &mut m.outline);
        times(bm.scale, am.scale, &mut m.scale);
        by(bm.rotate, am.rotate, &mut m.rotate);
        set(&bm.opacity, &am.opacity, &mut m.opacity);
        set(&bm.enter, &am.enter, &mut m.enter);
        set(&bm.leave, &am.leave, &mut m.leave);
        set(&bm.repeat, &am.repeat, &mut m.repeat);
        set(&bm.hidden, &am.hidden, &mut m.hidden);
        set(&bm.gradient, &am.gradient, &mut m.gradient);
        set(&bm.highlight, &am.highlight, &mut m.highlight);
        set(&bm.backing, &am.backing, &mut m.backing);
        set(&bm.backing_colour, &am.backing_colour, &mut m.backing_colour);
    }
    match (before, after, to) {
        (Body::Text { size: bs, colour: bc, .. }, Body::Text { size: s, colour: c, .. }, Body::Text { size, colour, .. }) => {
            times(*bs, *s, size);
            set(bc, c, colour);
        }
        (Body::Sticker { size: bs, .. }, Body::Sticker { size: s, .. }, Body::Sticker { size, .. }) | (Body::Graphic { size: bs, .. }, Body::Graphic { size: s, .. }, Body::Graphic { size, .. }) => times(*bs, *s, size),
        (Body::Overlay { size: bs, key: bk, tolerance: bt, volume: bv, .. }, Body::Overlay { size: s, key: k, tolerance: t, volume: v, .. }, Body::Overlay { size, key, tolerance, volume, .. }) => {
            times(*bs, *s, size);
            set(bk, k, key);
            set(bt, t, tolerance);
            set(bv, v, volume);
        }
        (Body::Effect { kind: bk, amount: ba }, Body::Effect { kind: k, amount: a }, Body::Effect { kind, amount }) => {
            set(bk, k, kind);
            set(ba, a, amount);
        }
        (Body::Audio { volume: bv, .. }, Body::Audio { volume: v, .. }, Body::Audio { volume, .. }) => set(bv, v, volume),
        _ => {}
    }
}

/// Anything placed freely on a track.
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Item {
    pub id: u32,
    pub row: usize,
    pub at: u32,
    pub len: u32,
    pub body: Body,
}

impl Item {
    pub fn end(&self) -> u32 {
        self.at + self.len
    }

    pub fn label(&self) -> String {
        match &self.body {
            Body::Audio { name, .. } => name.clone(),
            Body::Effect { kind, .. } => kind.name().to_string(),
            Body::Text { text, .. } => crate::overlay::plain(text).lines().next().unwrap_or("Text").to_string(),
            Body::Sticker { .. } => "Sticker".to_string(),
            Body::Graphic { name, .. } => name.clone(),
            Body::Overlay { .. } => "Video".to_string(),
        }
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug, Default)]
pub struct Track {
    pub locked: bool,
    pub muted: bool,
    pub hidden: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Picked {
    Clip(u32),
    Item(u32),
}

/// What the main track shows at one moment.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Shot {
    pub clip: usize,
    /// Frames into that clip.
    pub offset: u32,
    /// The clip being left, its offset, and how far through the transition this is.
    pub blend: Option<(usize, u32, f32)>,
}

/// One stretch of sound to mix, in seconds.
#[derive(Clone, PartialEq, Debug)]
pub struct Part {
    pub path: PathBuf,
    pub at: f64,
    pub start: f64,
    pub len: f64,
    pub volume: f32,
    pub speed: f64,
    /// Noise clean-up, 0 to 1.
    pub denoise: f32,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Project {
    pub name: String,
    pub fps: f64,
    pub size: (usize, usize),
    pub media: Shared<Vec<Media>>,
    pub clips: Shared<Vec<Clip>>,
    pub items: Shared<Vec<Item>>,
    pub tracks: [Track; ROWS],
    pub next_id: u32,
    /// The brand kit new text and captions follow, by id; empty for none.
    #[serde(default)]
    pub brand: String,
    /// Clips of another shape fill the picture, cropped, instead of fitting with bars.
    #[serde(default)]
    pub fill: bool,
}

impl Default for Project {
    fn default() -> Self {
        Self {
            name: "Untitled Project".to_string(),
            fps: DEFAULT_FPS,
            size: DEFAULT_SIZE,
            media: Shared::default(),
            clips: Shared::default(),
            items: Shared::default(),
            tracks: [Track::default(); ROWS],
            next_id: 1,
            brand: String::new(),
            fill: false,
        }
    }
}

impl Project {
    pub fn new_id(&mut self) -> u32 {
        self.next_id += 1;
        self.next_id - 1
    }

    pub fn frames(&self, seconds: f64) -> u32 {
        (seconds * self.fps).round().max(0.0) as u32
    }

    pub fn seconds(&self, frames: u32) -> f64 {
        frames as f64 / self.fps.max(1.0)
    }

    pub fn media_of(&self, id: u32) -> Option<&Media> {
        self.media.iter().find(|m| m.id == id)
    }

    /// Where each clip begins. A transition slides a clip back over the one before it.
    pub fn starts(&self) -> Vec<u32> {
        let mut out = Vec::with_capacity(self.clips.len());
        let mut at = 0u32;
        for c in self.clips.iter() {
            let lap = c.transition.map_or(0, |t| t.len).min(at);
            at -= lap;
            out.push(at);
            at += c.len;
        }
        out
    }

    pub fn video_end(&self) -> u32 {
        self.starts().last().zip(self.clips.last()).map_or(0, |(s, c)| s + c.len)
    }

    pub fn total(&self) -> u32 {
        self.items.iter().map(Item::end).fold(self.video_end(), u32::max)
    }

    /// The clip on screen at a frame. The incoming clip wins through a transition, and
    /// `blend` says what it is mixing with.
    pub fn at(&self, frame: u32) -> Option<Shot> {
        let starts = self.starts();
        let i = (0..self.clips.len()).rev().find(|i| frame >= starts[*i] && frame < starts[*i] + self.clips[*i].len)?;
        let offset = frame - starts[i];
        let blend = self.clips[i].transition.filter(|t| i > 0 && offset < t.len).map(|t| {
            let progress = (offset as f32 + 0.5) / t.len.max(1) as f32;
            (i - 1, frame - starts[i - 1], progress)
        });
        Some(Shot { clip: i, offset, blend })
    }

    /// The clip to work on at a frame, even in a gap or past the end.
    pub fn nearest(&self, frame: u32) -> Option<usize> {
        self.at(frame).map(|s| s.clip).or_else(|| self.clips.len().checked_sub(1))
    }

    /// How many frames a clip could run from where it begins.
    pub fn room(&self, c: &Clip) -> u32 {
        let pace = if c.ramp.len() < 2 { c.speed } else { ramp_area(&c.ramp, 1.0).max(1e-3) };
        match self.media_of(c.media) {
            Some(m) if m.kind == Kind::Video => self.frames((m.seconds - c.start).max(0.0) / pace),
            _ => ENDLESS,
        }
    }

    /// Gives a clip a speed ramp, or takes it away with an empty one. The clip keeps the
    /// stretch of source it showed, so it gets shorter or longer with the ramp's pace.
    pub fn set_ramp(&mut self, id: u32, ramp: Vec<RampPoint>) {
        let Some(i) = self.clip_index(id) else { return };
        let fps = self.fps;
        let c = &self.clips[i];
        let span = c.source_at(c.len as f64, fps) - c.start;
        let mut r: Vec<RampPoint> = ramp.into_iter().map(|p| RampPoint { at: p.at.clamp(0.0, 1.0), speed: p.speed.clamp(MIN_SPEED, MAX_SPEED) }).collect();
        r.sort_by(|a, b| a.at.total_cmp(&b.at));
        let pace = if r.len() < 2 { c.speed } else { ramp_area(&r, 1.0).max(1e-3) };
        let len = self.frames(span / pace);
        self.clips[i].ramp = r;
        let room = self.room(&self.clips[i]);
        self.clips[i].len = len.clamp(MIN_LEN, room.max(MIN_LEN));
        for n in [i, i + 1] {
            if let Some(t) = self.clips.get(n).and_then(|c| c.transition) {
                self.set_transition(n, t.kind, t.len);
            }
        }
    }

    /// Moves a ramp point while keeping the clip's length, for dragging on the curve.
    pub fn shape_ramp(&mut self, id: u32, ramp: Vec<RampPoint>) {
        let Some(i) = self.clip_index(id) else { return };
        let mut r: Vec<RampPoint> = ramp.into_iter().map(|p| RampPoint { at: p.at.clamp(0.0, 1.0), speed: p.speed.clamp(MIN_SPEED, MAX_SPEED) }).collect();
        r.sort_by(|a, b| a.at.total_cmp(&b.at));
        self.clips[i].ramp = r;
        let room = self.room(&self.clips[i]);
        let c = &mut self.clips[i];
        c.len = c.len.min(room.max(MIN_LEN));
    }

    pub fn clip_for(&mut self, media: u32) -> Option<Clip> {
        let m = self.media_of(media)?;
        if m.kind == Kind::Audio {
            return None;
        }
        let seconds = if m.kind == Kind::Photo { PHOTO_SECONDS } else { m.seconds };
        let len = self.frames(seconds).max(MIN_LEN);
        Some(Clip { id: self.new_id(), media, start: 0.0, len, grade: Shared::default(), transition: None, volume: 1.0, muted: false, detached: false, speed: 1.0, ramp: Vec::new(), lead: 0.0, tail: 0.0, zoom: 1.0, denoise: 0.0 })
    }

    pub fn insert_clip(&mut self, media: u32, index: usize) -> Option<usize> {
        let c = self.clip_for(media)?;
        let i = index.min(self.clips.len());
        self.clips.insert(i, c);
        Some(i)
    }

    fn clip_index(&self, id: u32) -> Option<usize> {
        self.clips.iter().position(|c| c.id == id)
    }

    /// Cuts the main-track clip under a frame in two. Both halves keep the grade.
    pub fn split_clip(&mut self, frame: u32) -> Option<usize> {
        let s = self.at(frame)?;
        if s.offset < MIN_LEN || self.clips[s.clip].len - s.offset < MIN_LEN {
            return None;
        }
        let id = self.new_id();
        let fps = self.fps.max(1.0);
        let c = &mut self.clips[s.clip];
        let mut tail = c.clone();
        tail.id = id;
        tail.start = c.source_at(s.offset as f64, fps);
        tail.len = c.len - s.offset;
        tail.transition = None;
        // The J cut stays with the start, the L cut goes with the end.
        tail.lead = 0.0;
        c.tail = 0.0;
        // A ramp's curve spans the whole clip; each piece keeps its average pace instead.
        if c.ramp.len() >= 2 {
            let at = s.offset as f64 / c.len.max(1) as f64;
            let first = ramp_area(&c.ramp, at) / at.max(1e-6);
            let second = (ramp_area(&c.ramp, 1.0) - ramp_area(&c.ramp, at)) / (1.0 - at).max(1e-6);
            c.ramp.clear();
            tail.ramp.clear();
            c.speed = first.clamp(MIN_SPEED, MAX_SPEED);
            tail.speed = second.clamp(MIN_SPEED, MAX_SPEED);
        }
        c.len = s.offset;
        self.clips.insert(s.clip + 1, tail);
        Some(s.clip + 1)
    }

    pub fn split_item(&mut self, id: u32, frame: u32) -> Option<u32> {
        let i = self.items.iter().position(|t| t.id == id)?;
        let at = self.items[i].at;
        let cut = frame.checked_sub(at).filter(|c| *c >= MIN_LEN && self.items[i].len - c >= MIN_LEN)?;
        let new = self.new_id();
        let fps = self.fps.max(1.0);
        let mut tail = self.items[i].clone();
        tail.id = new;
        tail.at = at + cut;
        tail.len -= cut;
        match &mut tail.body {
            Body::Audio { start, speed, .. } => *start += cut as f64 / fps * *speed,
            Body::Overlay { start, .. } => *start += cut as f64 / fps,
            _ => {}
        }
        if let Some((x, y, motion)) = self.items[i].body.placed_mut() {
            let rest = motion.split(cut, *x, *y);
            if let Some((_, _, motion)) = tail.body.placed_mut() {
                *motion = rest;
            }
        }
        self.items[i].len = cut;
        self.items.insert(i + 1, tail);
        Some(new)
    }

    pub fn duplicate(&mut self, p: Picked) -> Option<Picked> {
        match p {
            Picked::Clip(id) => {
                let i = self.clip_index(id)?;
                let mut c = self.clips[i].clone();
                c.id = self.new_id();
                c.transition = None;
                let new = c.id;
                self.clips.insert(i + 1, c);
                Some(Picked::Clip(new))
            }
            Picked::Item(id) => {
                let mut t = self.items.iter().find(|t| t.id == id)?.clone();
                t.at = t.end();
                t.id = self.new_id();
                let new = t.id;
                self.items.push(t);
                Some(Picked::Item(new))
            }
        }
    }

    pub fn delete(&mut self, p: Picked) {
        match p {
            Picked::Clip(id) => self.clips.retain(|c| c.id != id),
            Picked::Item(id) => self.items.retain(|t| t.id != id),
        }
        if let Some(c) = self.clips.first_mut() {
            c.transition = None;
        }
    }

    /// Moves a clip's start by `d` frames; forward shortens it, back reveals more.
    pub fn trim_clip_start(&mut self, id: u32, d: i64) {
        let Some(i) = self.clip_index(id) else { return };
        let fps = self.fps.max(1.0);
        let c = &mut self.clips[i];
        let most_back = -((c.start / c.speed * fps).floor() as i64);
        let d = d.clamp(most_back, c.len as i64 - MIN_LEN as i64);
        c.start = c.source_at(d as f64, fps).max(0.0);
        c.len = (c.len as i64 - d) as u32;
    }

    /// Changes how fast a clip plays. It keeps the same stretch of its source, so it
    /// gets shorter when faster, and everything after it moves up.
    pub fn set_speed(&mut self, id: u32, speed: f64) {
        let Some(i) = self.clip_index(id) else { return };
        let speed = speed.clamp(MIN_SPEED, MAX_SPEED);
        let c = &self.clips[i];
        let len = (c.len as f64 * c.speed / speed).round() as u32;
        self.clips[i].speed = speed;
        let room = self.room(&self.clips[i]);
        self.clips[i].len = len.clamp(MIN_LEN, room.max(MIN_LEN));
        // A transition can't be longer than the clips on either side of it.
        for n in [i, i + 1] {
            if let Some(t) = self.clips.get(n).and_then(|c| c.transition) {
                self.set_transition(n, t.kind, t.len);
            }
        }
    }

    pub fn trim_clip_end(&mut self, id: u32, d: i64) {
        let Some(i) = self.clip_index(id) else { return };
        let room = self.room(&self.clips[i]) as i64;
        let c = &mut self.clips[i];
        c.len = (c.len as i64 + d).clamp(MIN_LEN as i64, room.max(MIN_LEN as i64)) as u32;
    }

    pub fn move_clip(&mut self, from: usize, to: usize) {
        if from >= self.clips.len() {
            return;
        }
        let mut c = self.clips.remove(from);
        // A transition belongs to the cut it was on, and that cut has gone.
        c.transition = None;
        if let Some(n) = self.clips.get_mut(from) {
            n.transition = None;
        }
        let end = self.clips.len();
        self.clips.insert(to.min(end), c);
        if let Some(c) = self.clips.first_mut() {
            c.transition = None;
        }
    }

    /// Which position a clip dropped at this frame should take.
    pub fn slot_at(&self, frame: u32) -> usize {
        let starts = self.starts();
        (0..self.clips.len()).find(|i| frame < starts[*i] + self.clips[*i].len / 2).unwrap_or(self.clips.len())
    }

    /// Sets a transition on the cut before a clip, kept shorter than either side.
    pub fn set_transition(&mut self, index: usize, kind: Wipe, len: u32) -> bool {
        if index == 0 || index >= self.clips.len() {
            return false;
        }
        let room = self.clips[index].len.min(self.clips[index - 1].len).saturating_sub(1).max(1);
        self.clips[index].transition = Some(Transition { kind, len: len.clamp(1, room) });
        true
    }

    /// The cut nearest a frame, as the index of the clip after it.
    pub fn nearest_cut(&self, frame: u32) -> Option<usize> {
        let starts = self.starts();
        (1..self.clips.len()).min_by_key(|i| starts[*i].abs_diff(frame))
    }

    pub fn detach_audio(&mut self, id: u32) -> Option<u32> {
        let i = self.clip_index(id)?;
        let at = self.starts()[i];
        let c = &self.clips[i];
        let m = self.media_of(c.media)?;
        if !m.has_audio || c.detached {
            return None;
        }
        let body = Body::Audio { media: c.media, start: c.start, volume: c.volume, name: "Original Sound".to_string(), speed: c.speed, denoise: c.denoise };
        let len = c.len;
        let new = self.new_id();
        self.items.push(Item { id: new, row: ROW_SOUND, at, len, body });
        self.clips[i].detached = true;
        Some(new)
    }

    /// Every stretch of sound the listener should hear.
    pub fn parts(&self) -> Vec<Part> {
        let mut out = Vec::new();
        let sound = self.tracks[ROW_SOUND];
        if !sound.muted && !sound.hidden {
            let windows = self.sound_windows();
            for ((c, at), (from, to)) in self.clips.iter().zip(self.starts()).zip(windows) {
                // A ramped clip's own sound would warble through every change of pace, so
                // it stays quiet; music under a ramp is the usual way.
                if c.ramp.len() >= 2 || to <= from {
                    continue;
                }
                if let Some(path) = self.media_of(c.media).filter(|m| m.has_audio && !c.detached && !c.muted).and_then(|m| m.path.clone()) {
                    let start = (c.start + (from - self.seconds(at)) * c.speed).max(0.0);
                    out.push(Part { path, at: from, start, len: to - from, volume: c.volume, speed: c.speed, denoise: c.denoise });
                }
            }
        }
        for t in self.items.iter() {
            let track = self.tracks[t.row.min(ROWS - 1)];
            let sound = match &t.body {
                Body::Audio { media, start, volume, speed, denoise, .. } => Some((*media, *start, *volume, *speed, *denoise)),
                Body::Overlay { media, start, volume, .. } => Some((*media, *start, *volume, 1.0, 0.0)),
                _ => None,
            };
            if let (Some((media, start, volume, speed, denoise)), false) = (sound, track.muted || track.hidden)
                && let Some(path) = self.media_of(media).filter(|m| m.has_audio).and_then(|m| m.path.clone())
            {
                out.push(Part { path, at: self.seconds(t.at), start, len: self.seconds(t.len), volume, speed, denoise });
            }
        }
        out
    }

    /// When each clip's own sound plays, in timeline seconds. A J cut starts it early and
    /// ends the clip before it there; an L cut runs it on and starts the next one later.
    /// Neither reaches past the source's ends.
    pub fn sound_windows(&self) -> Vec<(f64, f64)> {
        let starts = self.starts();
        let mut w: Vec<(f64, f64)> = self
            .clips
            .iter()
            .zip(&starts)
            .map(|(c, at)| {
                let (a, b) = (self.seconds(*at), self.seconds(*at + c.len));
                let before = (c.start / c.speed).max(0.0);
                let after = self.media_of(c.media).map_or(0.0, |m| ((m.seconds - c.start) / c.speed - (b - a)).max(0.0));
                ((a - c.lead.min(before)).max(0.0), b + c.tail.min(after))
            })
            .collect();
        for i in 0..w.len() {
            if self.clips[i].lead > 0.0 && i > 0 {
                w[i - 1].1 = w[i - 1].1.min(w[i].0);
            }
            if self.clips[i].tail > 0.0 && i + 1 < w.len() {
                w[i + 1].0 = w[i + 1].0.max(w[i].1);
            }
        }
        w
    }

    /// The largest clip sets the picture size and the first video sets the rate.
    pub fn suit_first(&mut self, media: u32) {
        let Some(m) = self.media_of(media).cloned() else { return };
        if self.clips.len() <= 1 {
            let even = (m.size.0.max(2) & !1, m.size.1.max(2) & !1);
            self.size = even;
            if let Some(i) = &m.info {
                self.rate_to(i.fps);
            }
        }
    }

    /// Changes the frame rate, keeping every clip, item and keyframe at the same time in
    /// seconds. The first clip was measured at the old rate before this one was known.
    pub fn rate_to(&mut self, fps: f64) {
        if fps <= 0.0 || (fps - self.fps).abs() < 1e-9 {
            return;
        }
        let k = fps / self.fps.max(1.0);
        let at = |f: u32| (f as f64 * k).round() as u32;
        self.fps = fps;
        for i in 0..self.clips.len() {
            let room = self.room(&self.clips[i]);
            let c = &mut self.clips[i];
            c.len = at(c.len).clamp(MIN_LEN, room.max(MIN_LEN));
            if let Some(t) = &mut c.transition {
                t.len = at(t.len).max(1);
            }
        }
        for t in self.items.iter_mut() {
            t.at = at(t.at);
            t.len = at(t.len).max(MIN_LEN);
            if let Some((_, _, m)) = t.body.placed_mut() {
                for key in &mut m.keys {
                    key.frame = at(key.frame);
                }
                m.keys.dedup_by_key(|k| k.frame);
            }
        }
    }

    /// An HDR clip shows flat unless converted, so new clips get their own conversion.
    pub fn suit_clip(&mut self, index: usize) {
        let Some(c) = self.clips.get(index) else { return };
        let hdr = self.media_of(c.media).and_then(|m| m.info.as_ref()).and_then(|i| i.hdr);
        if let (Some(s), Some(c)) = (hdr, self.clips.get_mut(index))
            && c.grade.cst.source == Source::Rec709
        {
            c.grade.cst.source = s;
        }
    }
}

/// Lookups into a project, built once and reused until it changes. Clip starts only ever
/// rise, so a binary search finds a frame's clip. Items sit sorted by start, each with the
/// furthest end reached so far, which lets a search stop as soon as nothing can still
/// overlap. Both stay quick however long the project gets.
pub struct Index {
    pub starts: Vec<u32>,
    by_id: HashMap<u32, usize>,
    order: Vec<u32>,
    reach: Vec<u32>,
}

impl Index {
    pub fn of(p: &Project) -> Self {
        let mut order: Vec<u32> = (0..p.items.len() as u32).collect();
        order.sort_unstable_by_key(|i| p.items[*i as usize].at);
        let mut furthest = 0;
        let reach = order
            .iter()
            .map(|i| {
                furthest = furthest.max(p.items[*i as usize].end());
                furthest
            })
            .collect();
        let by_id = p.clips.iter().enumerate().map(|(i, c)| (c.id, i)).collect();
        Self { starts: p.starts(), by_id, order, reach }
    }

    pub fn clip_index(&self, id: u32) -> Option<usize> {
        self.by_id.get(&id).copied()
    }

    /// The same answer as `Project::at`, by binary search.
    pub fn shot(&self, p: &Project, frame: u32) -> Option<Shot> {
        let i = self.starts.partition_point(|s| *s <= frame).checked_sub(1)?;
        let offset = frame - self.starts[i];
        if offset >= p.clips[i].len {
            return None;
        }
        let blend = p.clips[i].transition.filter(|t| i > 0 && offset < t.len).map(|t| {
            let progress = (offset as f32 + 0.5) / t.len.max(1) as f32;
            (i - 1, frame - self.starts[i - 1], progress)
        });
        Some(Shot { clip: i, offset, blend })
    }

    /// Positions in `items` of everything overlapping these frames, in start order.
    pub fn items_in(&self, items: &[Item], from: u32, to: u32) -> Vec<usize> {
        let mut out = Vec::new();
        let mut n = self.order.partition_point(|i| items[*i as usize].at < to);
        while n > 0 && self.reach[n - 1] > from {
            n -= 1;
            let i = self.order[n] as usize;
            if items[i].end() > from {
                out.push(i);
            }
        }
        out.reverse();
        out
    }
}

/// Inputs and filter for mixing sound from `from` seconds onward. The filter's output is
/// `[mix]`. Silence underneath keeps the sound as long as the picture, so the mix can
/// serve as the clock. `first` is the number of the first input this adds.
/// Filters that change how fast sound plays without changing its pitch. One step only
/// reaches from half to double speed, so further takes several in a row.
fn tempo(speed: f64) -> String {
    if (speed - 1.0).abs() < 1e-6 {
        return String::new();
    }
    let mut left = speed.clamp(MIN_SPEED, MAX_SPEED);
    let mut out = String::new();
    while left > 2.0 {
        out += "atempo=2,";
        left /= 2.0;
    }
    while left < 0.5 {
        out += "atempo=0.5,";
        left /= 0.5;
    }
    out + &format!("atempo={left:.6},")
}

/// Noise clean-up for one part: a gentle cut of low rumble, then ffmpeg's FFT noise
/// reducer, which learns the steady hiss and hum and takes it down by up to 30 dB.
fn denoise(strength: f32) -> String {
    if strength <= 0.01 {
        return String::new();
    }
    let s = strength.clamp(0.0, 1.0);
    format!("highpass=f={:.0},afftdn=nr={:.1}:nf=-30:tn=1,", 60.0 + 30.0 * s, 6.0 + 24.0 * s)
}

/// The ffmpeg arguments that hand it a filter graph. The graph goes in a temporary file
/// (`-/filter_complex FILE`) because a project with many sound items makes one too long
/// for a Windows command line (32,767 characters); written inline it would fail to start.
pub fn graph_args(graph: &str) -> [String; 2] {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let file = std::env::temp_dir().join(format!("lookroom-graph-{}-{}.txt", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
    match std::fs::write(&file, graph) {
        Ok(()) => ["-/filter_complex".into(), file.to_string_lossy().to_string()],
        Err(_) => ["-filter_complex".into(), graph.to_string()],
    }
}

pub fn mix_graph(parts: &[Part], from: f64, seconds: f64, rate: u32, first: usize) -> (Vec<String>, String) {
    let mut inputs = Vec::new();
    let mut chains = format!("anullsrc=r={rate}:cl=stereo,atrim=0:{seconds:.6}[base];");
    let mut labels = "[base]".to_string();
    let mut n = 0;
    for p in parts {
        let skip = (from - p.at).max(0.0);
        if skip >= p.len {
            continue;
        }
        let delay = ((p.at - from).max(0.0) * 1000.0).round() as u64;
        inputs.extend([
            "-ss".to_string(),
            format!("{:.6}", p.start + skip * p.speed),
            "-t".to_string(),
            format!("{:.6}", (p.len - skip) * p.speed),
            "-i".to_string(),
            p.path.to_string_lossy().to_string(),
        ]);
        let index = first + n;
        chains += &format!(
            "[{index}:a:0]{}{}aresample={rate},aformat=channel_layouts=stereo,volume={:.3},adelay={delay}|{delay}[a{n}];",
            tempo(p.speed),
            denoise(p.denoise),
            p.volume
        );
        labels += &format!("[a{n}]");
        n += 1;
    }
    (inputs, format!("{chains}{labels}amix=inputs={}:normalize=0:duration=first[mix]", n + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn media(id: u32, kind: Kind, seconds: f64, audio: bool) -> Media {
        Media { id, path: Some(format!("m{id}.mp4").into()), name: format!("m{id}"), kind, seconds, info: None, has_audio: audio, size: (1920, 1080) }
    }

    fn ramp(points: &[(f64, f64)]) -> Vec<RampPoint> {
        points.iter().map(|(at, speed)| RampPoint { at: *at, speed: *speed }).collect()
    }

    #[test]
    fn a_ramp_covers_the_area_under_its_curve_and_runs_back_exactly() {
        let r = ramp(&[(0.0, 1.0), (1.0, 3.0)]);
        // From 1x to 3x in a straight line: on average 2x.
        assert!((ramp_area(&r, 1.0) - 2.0).abs() < 1e-9);
        assert!((ramp_area(&r, 0.5) - 0.75).abs() < 1e-9);
        let mut p = project();
        let id = p.clips[0].id;
        p.clips[0].len = 100;
        p.clips[0].ramp = r;
        let c = &p.clips[0];
        for f in [0.0, 13.0, 50.0, 99.0] {
            let back = c.offset_of(c.source_at(f, p.fps), p.fps);
            assert!((back - f).abs() < 1e-3, "{f} came back as {back}");
        }
        let _ = id;
    }

    #[test]
    fn a_ramp_keeps_the_footage_and_a_steady_speed_brings_it_back() {
        let mut p = project();
        let id = p.clips[0].id;
        p.clips[0].len = 60;
        let span = p.clips[0].source_at(60.0, p.fps) - p.clips[0].start;
        // Averaging 2x, the same footage takes half the time.
        p.set_ramp(id, ramp(&[(0.0, 1.0), (1.0, 3.0)]));
        assert_eq!(p.clips[0].len, 30);
        let now = p.clips[0].source_at(p.clips[0].len as f64, p.fps) - p.clips[0].start;
        assert!((now - span).abs() < 0.11, "{span} {now}");
        p.set_ramp(id, Vec::new());
        assert_eq!(p.clips[0].len, 60);
        // Its own sound stays quiet while ramped.
        p.set_ramp(id, ramp(&[(0.0, 1.0), (1.0, 3.0)]));
        assert!(p.parts().iter().all(|part| part.path.to_string_lossy() != "m1.mp4"));
    }

    #[test]
    fn j_and_l_cuts_move_sound_across_the_cut() {
        let mut p = project();
        // Room in the sources on both sides of the cut: more of the first after its end,
        // more of the second before its start.
        p.clips[0].len = 50;
        p.clips[1].start = 2.0;
        let plain = p.sound_windows();
        assert_eq!(plain[0].1, plain[1].0);
        p.clips[1].lead = 0.5;
        let j = p.sound_windows();
        assert!((j[1].0 - (plain[1].0 - 0.5)).abs() < 1e-9 && (j[0].1 - j[1].0).abs() < 1e-9);
        p.clips[1].lead = 0.0;
        p.clips[0].tail = 0.75;
        let l = p.sound_windows();
        assert!((l[0].1 - (plain[0].1 + 0.75)).abs() < 1e-9 && (l[1].0 - l[0].1).abs() < 1e-9);
        // A J cut can't start before the source does.
        p.clips[0].tail = 0.0;
        p.clips[1].start = 0.2;
        p.clips[1].lead = 1.0;
        assert!((p.sound_windows()[1].0 - (plain[1].0 - 0.2)).abs() < 1e-9);
    }

    #[test]
    fn cleaned_up_sound_goes_through_the_noise_filter() {
        let mut p = project();
        p.clips[0].denoise = 0.5;
        let parts = p.parts();
        assert!((parts[0].denoise - 0.5).abs() < 1e-6 && parts[1].denoise == 0.0);
        let (_, graph) = mix_graph(&parts, 0.0, 20.0, 48000, 1);
        assert_eq!(graph.matches("afftdn").count(), 1, "{graph}");
        assert!(graph.contains("highpass=f=75") && graph.contains("afftdn=nr=18.0"), "{graph}");
        // Detaching keeps it.
        let id = p.clips[0].id;
        p.detach_audio(id).unwrap();
        assert!(p.parts().iter().any(|part| (part.denoise - 0.5).abs() < 1e-6));
    }

    #[test]
    fn splitting_a_ramped_clip_keeps_each_halfs_pace() {
        let mut p = project();
        let id = p.clips[0].id;
        p.clips[0].len = 100;
        p.set_ramp(id, ramp(&[(0.0, 1.0), (1.0, 3.0)]));
        let len = p.clips[0].len;
        let i = p.split_clip(len / 2).unwrap();
        assert!(p.clips[0].ramp.is_empty() && p.clips[i].ramp.is_empty());
        assert!(p.clips[0].speed < p.clips[i].speed, "{} {}", p.clips[0].speed, p.clips[i].speed);
    }

    fn caption(text: &str, x: f32, size: f32) -> Body {
        Body::Text { text: text.into(), size, colour: [1.0; 3], x, y: 0.86, motion: Motion { group: "captions".into(), ..Motion::default() } }
    }

    #[test]
    fn a_change_to_one_reaches_the_others_but_their_words_stay() {
        let before = caption("Hello", 0.5, 64.0);
        let mut after = before.clone();
        if let Body::Text { text, size, colour, y, motion, .. } = &mut after {
            *text = "Changed".into();
            *size = 96.0;
            *colour = [1.0, 0.8, 0.0];
            *y = 0.80;
            motion.font = "Georgia".into();
            motion.outline = 0.08;
        }
        let mut other = caption("World", 0.3, 32.0);
        spread(&before, &after, &mut other);
        let Body::Text { text, size, colour, x, y, motion } = other else { panic!() };
        assert_eq!(text, "World");
        assert_eq!((size, colour, motion.font.as_str(), motion.outline), (48.0, [1.0, 0.8, 0.0], "Georgia", 0.08));
        assert!((x - 0.3).abs() < 1e-6 && (y - 0.80).abs() < 1e-6, "{x} {y}");
        assert_eq!(motion.group, "captions");
    }

    #[test]
    fn nothing_changed_means_nothing_copied_and_kinds_share_only_what_they_have() {
        let a = caption("A", 0.5, 64.0);
        let mut other = caption("B", 0.2, 40.0);
        let kept = other.clone();
        spread(&a, &a, &mut other);
        assert_eq!(other, kept);
        // A sticker in the group takes the new animation, but has no colour to take.
        let mut after = a.clone();
        if let Body::Text { colour, motion, .. } = &mut after {
            *colour = [0.0; 3];
            motion.enter = crate::textanim::Phase::preset(crate::textanim::Anim::Pop);
        }
        let mut sticker = Body::Sticker { icon: 0, emoji: String::new(), size: 200.0, x: 0.5, y: 0.5, motion: Motion::default() };
        spread(&a, &after, &mut sticker);
        let Body::Sticker { size, motion, .. } = sticker else { panic!() };
        assert_eq!(size, 200.0);
        assert_eq!(motion.enter.kind, crate::textanim::Anim::Pop);
    }

    /// Two ten-second clips at ten frames a second, so frames read easily.
    fn project() -> Project {
        let mut p = Project { fps: 10.0, media: vec![media(1, Kind::Video, 10.0, true), media(2, Kind::Video, 10.0, true), media(3, Kind::Photo, 0.0, false), media(4, Kind::Audio, 60.0, true)].into(), next_id: 100, ..Project::default() };
        p.insert_clip(1, 0);
        p.insert_clip(2, 1);
        p
    }

    #[test]
    fn clips_run_end_to_end() {
        let p = project();
        assert_eq!(p.starts(), [0, 100]);
        assert_eq!(p.total(), 200);
        assert_eq!(p.at(150).unwrap(), Shot { clip: 1, offset: 50, blend: None });
        assert!(p.at(200).is_none());
    }

    #[test]
    fn a_photo_gets_a_set_length_and_audio_is_no_clip() {
        let mut p = project();
        let i = p.insert_clip(3, 2).unwrap();
        assert_eq!(p.clips[i].len, 50);
        assert!(p.insert_clip(4, 3).is_none());
    }

    #[test]
    fn splitting_keeps_the_picture_in_the_same_place() {
        let mut p = project();
        p.clips[0].grade.exposure = 1.0;
        let i = p.split_clip(30).unwrap();
        assert_eq!((p.clips[0].len, p.clips[1].len), (30, 70));
        assert_eq!(p.clips[1].start, 3.0);
        assert_eq!(p.clips[1].grade.exposure, 1.0);
        assert_ne!(p.clips[0].id, p.clips[i].id);
        assert_eq!(p.total(), 200);
        assert_eq!(p.at(45).unwrap().clip, 1);
    }

    #[test]
    fn a_cut_right_at_an_edge_is_refused() {
        let mut p = project();
        assert!(p.split_clip(0).is_none());
        assert!(p.split_clip(99).is_none());
    }

    #[test]
    fn trimming_changes_where_a_clip_reads_from() {
        let mut p = project();
        let id = p.clips[0].id;
        p.trim_clip_start(id, 20);
        assert_eq!((p.clips[0].start, p.clips[0].len), (2.0, 80));
        p.trim_clip_start(id, -5);
        assert_eq!((p.clips[0].start, p.clips[0].len), (1.5, 85));
    }

    #[test]
    fn a_clip_cant_be_trimmed_past_its_source() {
        let mut p = project();
        let id = p.clips[0].id;
        p.trim_clip_start(id, -50);
        assert_eq!((p.clips[0].start, p.clips[0].len), (0.0, 100));
        p.trim_clip_end(id, 50);
        assert_eq!(p.clips[0].len, 100);
        p.trim_clip_end(id, -500);
        assert_eq!(p.clips[0].len, MIN_LEN);
    }

    #[test]
    fn a_photo_can_be_held_as_long_as_wanted() {
        let mut p = project();
        let i = p.insert_clip(3, 2).unwrap();
        let id = p.clips[i].id;
        p.trim_clip_end(id, 1000);
        assert_eq!(p.clips[i].len, 1050);
    }

    #[test]
    fn a_transition_overlaps_the_two_clips() {
        let mut p = project();
        assert!(p.set_transition(1, Wipe::Dissolve, 20));
        assert_eq!(p.starts(), [0, 80]);
        assert_eq!(p.total(), 180);
        let s = p.at(90).unwrap();
        assert_eq!(s.clip, 1);
        let (other, offset, t) = s.blend.unwrap();
        assert_eq!((other, offset), (0, 90));
        assert!((t - 0.525).abs() < 1e-5);
        assert!(p.at(120).unwrap().blend.is_none());
    }

    #[test]
    fn a_transition_needs_a_clip_on_each_side_and_room() {
        let mut p = project();
        assert!(!p.set_transition(0, Wipe::Dissolve, 20));
        assert!(p.set_transition(1, Wipe::Dissolve, 5000));
        assert_eq!(p.clips[1].transition.unwrap().len, 99);
    }

    #[test]
    fn reordering_drops_transitions_that_no_longer_have_a_cut() {
        let mut p = project();
        let (a, b) = (p.clips[0].id, p.clips[1].id);
        p.set_transition(1, Wipe::Dissolve, 20);
        p.move_clip(1, 0);
        assert_eq!((p.clips[0].id, p.clips[1].id), (b, a));
        assert!(p.clips.iter().all(|c| c.transition.is_none()));
    }

    #[test]
    fn a_drop_lands_between_the_clips_nearest_it() {
        let p = project();
        assert_eq!(p.slot_at(10), 0);
        assert_eq!(p.slot_at(120), 1);
        assert_eq!(p.slot_at(190), 2);
    }

    #[test]
    fn deleting_the_first_clip_leaves_no_transition_with_nothing_before_it() {
        let mut p = project();
        p.set_transition(1, Wipe::Dissolve, 20);
        let id = p.clips[0].id;
        p.delete(Picked::Clip(id));
        assert!(p.clips[0].transition.is_none());
    }

    #[test]
    fn detached_sound_replaces_the_clips_own() {
        let mut p = project();
        assert_eq!(p.parts().len(), 2);
        let id = p.clips[0].id;
        p.detach_audio(id).unwrap();
        assert!(p.clips[0].detached);
        assert_eq!(p.parts().len(), 2);
        assert!(p.detach_audio(id).is_none());
        assert_eq!(p.items[0].row, ROW_SOUND);
    }

    #[test]
    fn a_muted_track_goes_quiet() {
        let mut p = project();
        p.tracks[ROW_SOUND].muted = true;
        assert!(p.parts().is_empty());
    }

    #[test]
    fn splitting_sound_keeps_it_in_step_with_itself() {
        let mut p = project();
        let id = p.new_id();
        p.items.push(Item { id, row: ROW_MUSIC, at: 20, len: 100, body: Body::Audio { media: 4, start: 1.0, volume: 1.0, name: "m".into(), speed: 1.0, denoise: 0.0 } });
        let tail = p.split_item(id, 50).unwrap();
        let t = p.items.iter().find(|t| t.id == tail).unwrap();
        assert_eq!((t.at, t.len), (50, 70));
        assert!(matches!(t.body, Body::Audio { start, .. } if (start - 4.0).abs() < 1e-9));
    }

    #[test]
    fn items_extend_the_project_past_the_video() {
        let mut p = project();
        let id = p.new_id();
        p.items.push(Item { id, row: ROW_FX, at: 190, len: 40, body: Body::Effect { kind: Fx::Strobe, amount: 1.0 } });
        assert_eq!(p.total(), 230);
    }

    #[test]
    fn the_mix_skips_what_has_passed_and_delays_what_is_coming() {
        let parts = vec![
            Part { path: "a.mp4".into(), at: 0.0, start: 1.0, len: 4.0, volume: 1.0, speed: 1.0, denoise: 0.0 },
            Part { path: "b.mp4".into(), at: 10.0, start: 0.0, len: 4.0, volume: 0.5, speed: 1.0, denoise: 0.0 },
            Part { path: "c.mp4".into(), at: 0.0, start: 0.0, len: 1.0, volume: 1.0, speed: 1.0, denoise: 0.0 },
        ];
        let (inputs, filter) = mix_graph(&parts, 2.0, 20.0, 48000, 0);
        assert_eq!(inputs.iter().filter(|s| *s == "-i").count(), 2);
        assert!(inputs.windows(2).any(|w| w[0] == "-ss" && w[1] == "3.000000"));
        assert!(filter.contains("adelay=8000|8000"));
        assert!(filter.contains("amix=inputs=3"));
    }

    /// A project with transitions and overlapping items, to compare answers against.
    fn busy() -> Project {
        let mut p = project();
        for i in 0..40 {
            let at = p.clips.len();
            p.insert_clip(1 + i % 2, at);
            p.clips[at].len = 20 + (i * 13) % 50;
        }
        for i in (1..p.clips.len()).step_by(3) {
            p.set_transition(i, Wipe::Dissolve, 8);
        }
        for i in 0..60 {
            let id = p.new_id();
            p.items.push(Item { id, row: ROW_FX, at: (i * 37) % 900, len: 5 + (i * 11) % 200, body: Body::Effect { kind: Fx::Strobe, amount: 1.0 } });
        }
        p
    }

    #[test]
    fn the_index_finds_the_same_shot_as_walking_the_list() {
        let p = busy();
        let index = Index::of(&p);
        for f in 0..p.total() + 5 {
            assert_eq!(index.shot(&p, f), p.at(f), "frame {f}");
        }
    }

    #[test]
    fn the_index_finds_the_same_items_as_scanning_them() {
        let p = busy();
        let index = Index::of(&p);
        for f in 0..p.total() + 5 {
            let mut fast = index.items_in(&p.items, f, f + 1);
            let mut slow: Vec<usize> = (0..p.items.len()).filter(|i| f >= p.items[*i].at && f < p.items[*i].end()).collect();
            fast.sort_unstable();
            slow.sort_unstable();
            assert_eq!(fast, slow, "frame {f}");
        }
    }

    #[test]
    fn a_range_finds_everything_touching_it() {
        let p = busy();
        let index = Index::of(&p);
        let found = index.items_in(&p.items, 100, 200).len();
        let want = p.items.iter().filter(|t| t.at < 200 && t.end() > 100).count();
        assert_eq!(found, want);
    }

    #[test]
    fn the_index_finds_a_clip_by_id() {
        let p = busy();
        let index = Index::of(&p);
        let id = p.clips[7].id;
        assert_eq!(index.clip_index(id), Some(7));
        assert_eq!(index.clip_index(999_999), None);
    }

    #[test]
    fn a_copy_of_the_project_shares_its_data_until_edited() {
        let p = busy();
        let mut q = p.clone();
        assert!(p.clips.same(&q.clips) && p.items.same(&q.items));
        q.clips[3].grade.exposure = 1.0;
        assert!(!p.clips.same(&q.clips));
        assert_eq!(p.clips[3].grade.exposure, 0.0);
        // The clips nobody touched are still shared underneath.
        assert!(p.clips[4].grade.same(&q.clips[4].grade));
    }

    #[test]
    fn a_new_rate_keeps_everything_at_the_same_second() {
        let mut p = Project { fps: 30.0, ..project() };
        p.insert_clip(1, 0);
        assert_eq!(p.clips[0].len, 300);
        let id = p.new_id();
        let mut motion = Motion::default();
        motion.set_key(60, crate::textanim::Keyed { x: 0.5, y: 0.5, scale: 1.0, rotate: 0.0, opacity: 1.0 });
        p.items.push(Item { id, row: ROW_FX2, at: 30, len: 120, body: Body::Sticker { icon: 0, emoji: String::new(), size: 100.0, x: 0.5, y: 0.5, motion } });
        p.rate_to(24.0);
        assert_eq!(p.clips[0].len, 240);
        assert_eq!((p.items[0].at, p.items[0].len), (24, 96));
        assert_eq!(p.items[0].body.placed().unwrap().2.keys[0].frame, 48);
    }

    #[test]
    fn an_overlay_with_sound_is_heard_and_a_silent_one_is_not() {
        let mut p = project();
        let id = p.new_id();
        let overlay = |media| Body::Overlay { media, start: 2.0, size: 1.0, x: 0.5, y: 0.5, key: None, tolerance: 0.2, volume: 0.5, motion: Motion::default() };
        p.items.push(Item { id, row: ROW_FX2, at: 10, len: 30, body: overlay(1) });
        let id = p.new_id();
        p.items.push(Item { id, row: ROW_FX2, at: 10, len: 30, body: overlay(3) });
        // The two main-track clips are heard too; of the overlays, only the one with sound.
        let parts: Vec<Part> = p.parts().into_iter().filter(|q| (q.at - 1.0).abs() < 1e-9).collect();
        assert_eq!(parts.len(), 1);
        assert_eq!((parts[0].start, parts[0].volume), (2.0, 0.5));
    }

    #[test]
    fn splitting_an_overlay_continues_its_video() {
        let mut p = project();
        let id = p.new_id();
        p.items.push(Item { id, row: ROW_FX2, at: 0, len: 40, body: Body::Overlay { media: 1, start: 1.0, size: 1.0, x: 0.5, y: 0.5, key: None, tolerance: 0.2, volume: 1.0, motion: Motion::default() } });
        let tail = p.split_item(id, 20).unwrap();
        let t = p.items.iter().find(|t| t.id == tail).unwrap();
        assert!(matches!(t.body, Body::Overlay { start, .. } if (start - 3.0).abs() < 1e-9));
    }

    #[test]
    fn double_speed_halves_a_clip_and_keeps_its_source() {
        let mut p = project();
        p.insert_clip(1, 0);
        let id = p.clips[0].id;
        let len = p.clips[0].len;
        p.set_speed(id, 2.0);
        assert_eq!(p.clips[0].len, len / 2);
        let c = &p.clips[0];
        assert!((c.source_at(c.len as f64, p.fps) - len as f64 / p.fps).abs() < 0.11);
    }

    #[test]
    fn slowing_down_cant_run_past_the_source() {
        let mut p = project();
        p.insert_clip(1, 0);
        let id = p.clips[0].id;
        p.set_speed(id, 0.25);
        // Ten seconds at a quarter speed fill forty, and the source has no more to give.
        assert_eq!(p.clips[0].len, 400);
        p.trim_clip_end(id, 1000);
        assert_eq!(p.clips[0].len, 400);
    }

    #[test]
    fn splitting_a_fast_clip_continues_its_source() {
        let mut p = project();
        p.insert_clip(1, 0);
        let id = p.clips[0].id;
        p.set_speed(id, 2.0);
        p.split_clip(20);
        assert!((p.clips[1].start - 4.0).abs() < 1e-9, "{}", p.clips[1].start);
        assert_eq!(p.clips[1].speed, 2.0);
    }

    #[test]
    fn trimming_a_slow_clip_moves_its_source_slowly() {
        let mut p = project();
        p.insert_clip(1, 0);
        let id = p.clips[0].id;
        p.set_speed(id, 0.5);
        p.trim_clip_start(id, 10);
        assert!((p.clips[0].start - 0.5).abs() < 1e-9);
    }

    #[test]
    fn speed_stays_within_its_limits() {
        let mut p = project();
        p.insert_clip(1, 0);
        let id = p.clips[0].id;
        p.set_speed(id, 50.0);
        assert_eq!(p.clips[0].speed, MAX_SPEED);
        p.set_speed(id, 0.0);
        assert_eq!(p.clips[0].speed, MIN_SPEED);
    }

    #[test]
    fn fast_sound_reads_more_source_and_keeps_its_pitch() {
        let parts = [Part { path: "a.mp4".into(), at: 0.0, start: 1.0, len: 2.0, volume: 1.0, speed: 3.0, denoise: 0.0 }];
        let (inputs, filter) = mix_graph(&parts, 0.0, 4.0, 48000, 0);
        assert!(inputs.contains(&"6.000000".to_string()), "{inputs:?}");
        assert!(filter.contains("atempo=2,atempo=1.500000,"), "{filter}");
        assert_eq!(tempo(1.0), "");
        assert_eq!(tempo(0.25), "atempo=0.5,atempo=0.500000,");
    }

    #[test]
    fn a_project_saved_before_speed_still_plays_at_normal_speed() {
        let p = busy();
        let mut text = serde_json::to_string(&p).unwrap();
        text = text.replace(",\"speed\":1.0", "");
        assert!(!text.contains("speed"));
        let back: Project = serde_json::from_str(&text).unwrap();
        assert!(back.clips.iter().all(|c| c.speed == 1.0));
    }

    #[test]
    fn a_split_sticker_keeps_its_motion_continuous() {
        use crate::textanim::Keyed;
        let mut p = project();
        let id = p.new_id();
        let mut motion = Motion::default();
        motion.set_key(0, Keyed { x: 0.0, y: 0.5, scale: 1.0, rotate: 0.0, opacity: 1.0 });
        motion.set_key(100, Keyed { x: 1.0, y: 0.5, scale: 1.0, rotate: 0.0, opacity: 1.0 });
        let was = motion.at(40, 0.5, 0.5).x;
        p.items.push(Item { id, row: ROW_FX2, at: 0, len: 100, body: Body::Sticker { icon: 0, emoji: String::new(), size: 100.0, x: 0.5, y: 0.5, motion } });
        let tail = p.split_item(id, 40).unwrap();
        let t = p.items.iter().find(|t| t.id == tail).unwrap();
        let (_, _, m) = t.body.placed().unwrap();
        assert!((m.at(0, 0.5, 0.5).x - was).abs() < 1e-5);
    }

    #[test]
    fn a_saved_project_comes_back_the_same() {
        let p = project();
        let text = serde_json::to_string(&p).unwrap();
        assert_eq!(serde_json::from_str::<Project>(&text).unwrap(), p);
    }
}
