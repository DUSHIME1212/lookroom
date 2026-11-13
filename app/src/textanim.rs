//! How text moves: In, Out and Loop animations that run across letters, words, lines
//! or the whole text with a delay between them, on top of keyframes for position,
//! scale, rotation and opacity. Nothing here draws; it says where each piece goes.
//! Rules and sources: MECHANICS.md.

use serde::{Deserialize, Serialize};

/// Furthest pieces travel in the Slide animations, in text heights.
const SLIDE: f32 = 0.6;
const DROP: f32 = 1.4;
const SCALE_FROM: f32 = 0.6;
const BLUR_FROM: f32 = 0.3;
const SWING_FROM: f32 = 35.0;
const CASCADE_TURN: f32 = 8.0;
const TUMBLE_TURN: f32 = 25.0;
/// Pop reaches full opacity in this share of its time.
const POP_FADE: f32 = 0.3;
/// Scrambled letters change this many times a second.
const SCRAMBLE_RATE: f32 = 15.0;
pub const SPARE_GLYPHS: &str = "ABDEGHKMNPRSTXZ0123456789#$%&";

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug, Default)]
pub enum Ease {
    Linear,
    In,
    #[default]
    Out,
    InOut,
    Back,
    Expo,
    Circ,
    Elastic,
    Bounce,
    Spring,
}

impl Ease {
    pub const ALL: [Ease; 10] = [Ease::Linear, Ease::In, Ease::Out, Ease::InOut, Ease::Back, Ease::Expo, Ease::Circ, Ease::Elastic, Ease::Bounce, Ease::Spring];

    pub fn name(self) -> &'static str {
        match self {
            Ease::Linear => "Linear",
            Ease::In => "Ease In",
            Ease::Out => "Ease Out",
            Ease::InOut => "Ease In Out",
            Ease::Back => "Back",
            Ease::Expo => "Expo",
            Ease::Circ => "Circ",
            Ease::Elastic => "Elastic",
            Ease::Bounce => "Bounce",
            Ease::Spring => "Spring",
        }
    }

    /// 0 at the start and 1 at the end; some curves overshoot on the way.
    pub fn at(self, t: f32) -> f32 {
        if t <= 0.0 {
            return 0.0;
        }
        if t >= 1.0 {
            return 1.0;
        }
        let u = 1.0 - t;
        match self {
            Ease::Linear => t,
            Ease::In => t * t * t,
            Ease::Out => 1.0 - u * u * u,
            Ease::InOut => {
                if t < 0.5 { 4.0 * t * t * t } else { 1.0 - 4.0 * u * u * u }
            }
            Ease::Back => {
                const C: f32 = 1.70158;
                1.0 - u * u * ((C + 1.0) * u - C)
            }
            Ease::Expo => 1.0 - 2f32.powf(-10.0 * t),
            Ease::Circ => (1.0 - u * u).sqrt(),
            Ease::Elastic => 2f32.powf(-10.0 * t) * ((t * 10.0 - 0.75) * std::f32::consts::TAU / 3.0).sin() + 1.0,
            Ease::Bounce => {
                const N: f32 = 7.5625;
                const D: f32 = 2.75;
                if t < 1.0 / D {
                    N * t * t
                } else if t < 2.0 / D {
                    let v = t - 1.5 / D;
                    N * v * v + 0.75
                } else if t < 2.5 / D {
                    let v = t - 2.25 / D;
                    N * v * v + 0.9375
                } else {
                    let v = t - 2.625 / D;
                    N * v * v + 0.984375
                }
            }
            Ease::Spring => 1.0 - (-8.0 * t).exp() * (14.0 * t).cos(),
        }
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug, Default)]
pub enum Unit {
    #[default]
    Whole,
    Line,
    Word,
    Letter,
}

impl Unit {
    pub const ALL: [Unit; 4] = [Unit::Whole, Unit::Line, Unit::Word, Unit::Letter];
    pub const NAMES: [&'static str; 4] = ["Whole", "Line", "Word", "Letter"];

    fn slot(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug, Default)]
pub enum Order {
    #[default]
    Forward,
    Reverse,
    Random,
}

impl Order {
    pub const ALL: [Order; 3] = [Order::Forward, Order::Reverse, Order::Random];
    pub const NAMES: [&'static str; 3] = ["Forward", "Reverse", "Random"];
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug, Default)]
pub enum Anim {
    #[default]
    None,
    Fade,
    SlideUp,
    SlideDown,
    SlideLeft,
    SlideRight,
    Scale,
    Pop,
    Drop,
    Blur,
    Reveal,
    Typewriter,
    Scramble,
    Cascade,
    Bounce,
    WordRise,
    Tumble,
    Flip,
    Swing,
    /// Described by a plugin, through the phase's `from`.
    Custom,
}

impl Anim {
    pub const ALL: [Anim; 19] = [
        Anim::None,
        Anim::Fade,
        Anim::SlideUp,
        Anim::SlideDown,
        Anim::SlideLeft,
        Anim::SlideRight,
        Anim::Scale,
        Anim::Pop,
        Anim::Drop,
        Anim::Blur,
        Anim::Reveal,
        Anim::Typewriter,
        Anim::Scramble,
        Anim::Cascade,
        Anim::Bounce,
        Anim::WordRise,
        Anim::Tumble,
        Anim::Flip,
        Anim::Swing,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Anim::None => "None",
            Anim::Fade => "Fade",
            Anim::SlideUp => "Slide Up",
            Anim::SlideDown => "Slide Down",
            Anim::SlideLeft => "Slide Left",
            Anim::SlideRight => "Slide Right",
            Anim::Scale => "Scale",
            Anim::Pop => "Pop",
            Anim::Drop => "Drop In",
            Anim::Blur => "Blur Focus",
            Anim::Reveal => "Reveal",
            Anim::Typewriter => "Typewriter",
            Anim::Scramble => "Scramble",
            Anim::Cascade => "Cascade",
            Anim::Bounce => "Bounce Letters",
            Anim::WordRise => "Word Fade Up",
            Anim::Tumble => "3D Tumble",
            Anim::Flip => "Flip",
            Anim::Swing => "Swing",
            Anim::Custom => "Custom",
        }
    }
}

/// Where a piece starts for an animation a plugin describes. It eases from these to rest:
/// full opacity, no offset, full size, upright and sharp. Offsets are in text heights.
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug)]
#[serde(default)]
pub struct Origin {
    pub opacity: f32,
    pub dx: f32,
    pub dy: f32,
    pub scale: f32,
    pub rotate: f32,
    pub blur: f32,
}

impl Default for Origin {
    fn default() -> Self {
        Self { opacity: 0.0, dx: 0.0, dy: 0.0, scale: 1.0, rotate: 0.0, blur: 0.0 }
    }
}

impl Origin {
    /// Keeps a plugin's numbers within what draws sensibly.
    pub fn tamed(self) -> Self {
        let keep = |v: f32, lo: f32, hi: f32, rest: f32| if v.is_finite() { v.clamp(lo, hi) } else { rest };
        Self {
            opacity: keep(self.opacity, 0.0, 1.0, 0.0),
            dx: keep(self.dx, -8.0, 8.0, 0.0),
            dy: keep(self.dy, -8.0, 8.0, 0.0),
            scale: keep(self.scale, 0.0, 8.0, 1.0),
            rotate: keep(self.rotate, -720.0, 720.0, 0.0),
            blur: keep(self.blur, 0.0, 1.0, 0.0),
        }
    }
}

/// One In or Out animation.
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug)]
#[serde(default)]
pub struct Phase {
    pub kind: Anim,
    pub unit: Unit,
    pub order: Order,
    /// Seconds each piece takes.
    pub seconds: f32,
    /// Seconds between one piece starting and the next.
    pub delay: f32,
    pub ease: Ease,
    /// Only for `Anim::Custom`. Saved with the project, so it plays without the plugin.
    pub from: Origin,
}

impl Default for Phase {
    fn default() -> Self {
        Self { kind: Anim::None, unit: Unit::Whole, order: Order::Forward, seconds: 0.6, delay: 0.0, ease: Ease::Out, from: Origin::default() }
    }
}

impl Phase {
    /// An animation with the settings that suit it.
    pub fn preset(kind: Anim) -> Self {
        let (unit, seconds, delay, ease) = match kind {
            Anim::None => (Unit::Whole, 0.6, 0.0, Ease::Out),
            Anim::Fade => (Unit::Whole, 0.6, 0.0, Ease::InOut),
            Anim::SlideUp | Anim::SlideDown | Anim::SlideLeft | Anim::SlideRight => (Unit::Whole, 0.6, 0.0, Ease::Expo),
            Anim::Scale => (Unit::Whole, 0.6, 0.0, Ease::Expo),
            Anim::Pop => (Unit::Whole, 0.5, 0.0, Ease::Back),
            Anim::Drop => (Unit::Letter, 0.7, 0.04, Ease::Bounce),
            Anim::Blur => (Unit::Word, 0.7, 0.08, Ease::Out),
            Anim::Reveal => (Unit::Line, 0.7, 0.1, Ease::Expo),
            Anim::Typewriter => (Unit::Letter, 0.02, 0.06, Ease::Linear),
            Anim::Scramble => (Unit::Letter, 0.5, 0.05, Ease::Linear),
            Anim::Cascade => (Unit::Letter, 0.6, 0.035, Ease::Back),
            Anim::Bounce => (Unit::Letter, 0.8, 0.04, Ease::Bounce),
            Anim::WordRise => (Unit::Word, 0.6, 0.12, Ease::Expo),
            Anim::Tumble => (Unit::Letter, 0.7, 0.04, Ease::Back),
            Anim::Flip => (Unit::Word, 0.6, 0.1, Ease::Back),
            Anim::Swing => (Unit::Word, 0.7, 0.1, Ease::Elastic),
            Anim::Custom => (Unit::Letter, 0.6, 0.04, Ease::Out),
        };
        Self { kind, unit, order: Order::Forward, seconds, delay, ease, from: Origin::default() }
    }

    /// How long the whole animation runs across `count` pieces.
    fn span(&self, count: usize) -> f32 {
        if self.kind == Anim::None { 0.0 } else { self.seconds + self.delay * count.saturating_sub(1) as f32 }
    }

    fn rank(&self, group: usize, count: usize) -> usize {
        match self.order {
            Order::Forward => group,
            Order::Reverse => count.saturating_sub(1 + group),
            Order::Random => shuffled(group, count),
        }
    }

    /// Time run through by one piece, from 0 to 1, `t` seconds in.
    fn run(&self, group: usize, count: usize, t: f32, stretch: f32) -> f32 {
        let start = self.rank(group, count) as f32 * self.delay * stretch;
        ((t - start) / (self.seconds * stretch).max(1e-4)).clamp(0.0, 1.0)
    }
}

/// A fixed shuffle: each place gets its own rank, the same on every frame.
fn shuffled(group: usize, count: usize) -> usize {
    let mut ranks: Vec<usize> = (0..count).collect();
    let mut seed = 0x9E37_79B9u32 ^ count as u32;
    for i in (1..count).rev() {
        seed = seed.wrapping_mul(0x85EB_CA77).wrapping_add(0x2F);
        ranks.swap(i, (seed >> 7) as usize % (i + 1));
    }
    ranks.get(group).copied().unwrap_or(0)
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug, Default)]
pub enum Sway {
    #[default]
    None,
    Pulse,
    Float,
    Wave,
    Shake,
    Wobble,
    Flicker,
}

impl Sway {
    pub const ALL: [Sway; 7] = [Sway::None, Sway::Pulse, Sway::Float, Sway::Wave, Sway::Shake, Sway::Wobble, Sway::Flicker];

    pub fn name(self) -> &'static str {
        match self {
            Sway::None => "None",
            Sway::Pulse => "Pulse",
            Sway::Float => "Float",
            Sway::Wave => "Wave",
            Sway::Shake => "Shake",
            Sway::Wobble => "Wobble",
            Sway::Flicker => "Flicker",
        }
    }
}

/// A movement repeating for as long as the text is on screen.
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug)]
#[serde(default)]
pub struct Repeat {
    pub kind: Sway,
    pub unit: Unit,
    /// Rounds a second.
    pub speed: f32,
    pub amount: f32,
}

impl Default for Repeat {
    fn default() -> Self {
        Self { kind: Sway::None, unit: Unit::Whole, speed: 1.0, amount: 0.5 }
    }
}

impl Repeat {
    pub fn preset(kind: Sway) -> Self {
        let unit = if kind == Sway::Wave { Unit::Letter } else { Unit::Whole };
        Self { kind, unit, ..Self::default() }
    }
}

/// What keyframes can change.
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug)]
pub struct Keyed {
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    pub rotate: f32,
    pub opacity: f32,
}

impl Keyed {
    fn mix(a: Keyed, b: Keyed, t: f32) -> Keyed {
        let m = |p: f32, q: f32| p + (q - p) * t;
        Keyed { x: m(a.x, b.x), y: m(a.y, b.y), scale: m(a.scale, b.scale), rotate: m(a.rotate, b.rotate), opacity: m(a.opacity, b.opacity).clamp(0.0, 1.0) }
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug)]
pub struct TextKey {
    /// Frames from the start of the text item.
    pub frame: u32,
    pub value: Keyed,
    /// The curve on the way to the next keyframe.
    pub ease: Ease,
}

/// Everything about a text item past its words, size and colour.
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(default)]
pub struct Motion {
    /// A font family; empty means the app's own.
    pub font: String,
    pub weight: u16,
    pub italic: bool,
    /// Extra room between letters, in text heights.
    pub spacing: f32,
    /// A dark edge around each letter, in text heights. Memes use it.
    pub outline: f32,
    pub scale: f32,
    pub rotate: f32,
    pub opacity: f32,
    pub enter: Phase,
    pub leave: Phase,
    pub repeat: Repeat,
    pub keys: Vec<TextKey>,
    /// Switched off in the layers list: kept, but not drawn.
    pub hidden: bool,
    /// A set this belongs to, such as the captions Auto Captions wrote.
    pub group: String,
    /// A second colour the letters blend into, top to bottom; None keeps one colour.
    pub gradient: Option<[f32; 3]>,
    /// Words written between *stars* take this colour and are drawn bold.
    pub highlight: [f32; 3],
    /// A shape behind the text: a box, a pill, a marker stroke and so on.
    pub backing: Backing,
    pub backing_colour: [f32; 3],
}

/// What sits behind a text, as in title and callout templates.
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug, Default)]
pub enum Backing {
    #[default]
    None,
    Box,
    Pill,
    /// A rough brush stroke, like a highlighter or marker.
    Marker,
    /// A hand-drawn line under the words.
    Underline,
    /// An outlined box, as in callouts.
    Frame,
}

impl Backing {
    pub const ALL: [Backing; 6] = [Backing::None, Backing::Box, Backing::Pill, Backing::Marker, Backing::Underline, Backing::Frame];

    pub fn name(self) -> &'static str {
        match self {
            Backing::None => "None",
            Backing::Box => "Box",
            Backing::Pill => "Pill",
            Backing::Marker => "Marker",
            Backing::Underline => "Underline",
            Backing::Frame => "Frame",
        }
    }
}

pub const DEFAULT_WEIGHT: u16 = 600;

impl Default for Motion {
    fn default() -> Self {
        Self {
            font: String::new(),
            weight: DEFAULT_WEIGHT,
            italic: false,
            spacing: 0.0,
            outline: 0.0,
            scale: 1.0,
            rotate: 0.0,
            opacity: 1.0,
            enter: Phase::default(),
            leave: Phase::default(),
            repeat: Repeat::default(),
            keys: Vec::new(),
            hidden: false,
            group: String::new(),
            gradient: None,
            highlight: [0.48, 0.38, 1.0],
            backing: Backing::None,
            backing_colour: [0.83, 1.0, 0.36],
        }
    }
}

impl Motion {
    /// The values at a frame: from the keyframes, or the plain settings without any.
    pub fn at(&self, frame: u32, x: f32, y: f32) -> Keyed {
        let plain = Keyed { x, y, scale: self.scale, rotate: self.rotate, opacity: self.opacity };
        let Some(first) = self.keys.first() else { return plain };
        if frame <= first.frame {
            return first.value;
        }
        for pair in self.keys.windows(2) {
            let (a, b) = (&pair[0], &pair[1]);
            if frame < b.frame {
                let t = (frame - a.frame) as f32 / (b.frame - a.frame).max(1) as f32;
                return Keyed::mix(a.value, b.value, a.ease.at(t));
            }
        }
        self.keys[self.keys.len() - 1].value
    }

    /// Places or updates a keyframe. A new one keeps the curve the keyframes around it use.
    pub fn set_key(&mut self, frame: u32, value: Keyed) {
        match self.keys.binary_search_by_key(&frame, |k| k.frame) {
            Ok(i) => self.keys[i].value = value,
            Err(i) => {
                let ease = i.checked_sub(1).map_or(Ease::InOut, |p| self.keys[p].ease);
                self.keys.insert(i, TextKey { frame, value, ease });
            }
        }
    }

    pub fn remove_key(&mut self, frame: u32) {
        self.keys.retain(|k| k.frame != frame);
    }

    /// Moves a keyframe in time. It stops short of its neighbours, keeping their order.
    pub fn retime_key(&mut self, index: usize, frame: u32, len: u32) -> u32 {
        let low = index.checked_sub(1).map_or(0, |p| self.keys[p].frame + 1);
        let high = self.keys.get(index + 1).map_or(len.saturating_sub(1), |n| n.frame.saturating_sub(1));
        let f = frame.clamp(low, high.max(low));
        self.keys[index].frame = f;
        f
    }

    pub fn next_key(&self, frame: u32) -> Option<u32> {
        self.keys.iter().map(|k| k.frame).find(|f| *f > frame)
    }

    pub fn previous_key(&self, frame: u32) -> Option<u32> {
        self.keys.iter().rev().map(|k| k.frame).find(|f| *f < frame)
    }

    /// Keeps keyframes on the same moment of the picture when the item's start moves.
    pub fn shift_keys(&mut self, frames: i64) {
        for k in &mut self.keys {
            k.frame = (k.frame as i64 - frames).max(0) as u32;
        }
        self.keys.dedup_by_key(|k| k.frame);
    }

    /// Cuts the motion in two at `cut`, leaving the first half here. An In belongs to
    /// the start and an Out to the end, so neither plays at the cut.
    pub fn split(&mut self, cut: u32, x: f32, y: f32) -> Motion {
        let mut tail = self.clone();
        tail.enter.kind = Anim::None;
        self.leave.kind = Anim::None;
        if self.keys.is_empty() {
            return tail;
        }
        let there = self.at(cut, x, y);
        tail.keys.retain(|k| k.frame >= cut);
        for k in &mut tail.keys {
            k.frame -= cut;
        }
        tail.set_key(0, there);
        self.keys.retain(|k| k.frame < cut);
        self.set_key(cut, there);
        tail
    }

    /// How finely the text must be cut into pieces to run these animations.
    pub fn finest(&self) -> Unit {
        let wanted = [
            (self.enter.kind != Anim::None).then_some(self.enter.unit),
            (self.leave.kind != Anim::None).then_some(self.leave.unit),
            (self.repeat.kind != Sway::None).then_some(self.repeat.unit),
            self.scrambles().then_some(Unit::Letter),
        ];
        wanted.into_iter().flatten().max_by_key(|u| u.slot()).unwrap_or(Unit::Line)
    }

    pub fn scrambles(&self) -> bool {
        self.enter.kind == Anim::Scramble || self.leave.kind == Anim::Scramble
    }

    /// When an In and an Out would overlap, both are shortened to fit the item.
    fn timing(&self, seconds: f32, counts: &[usize; 4]) -> (f32, f32) {
        let in_span = self.enter.span(counts[self.enter.unit.slot()]);
        let out_span = self.leave.span(counts[self.leave.unit.slot()]);
        let all = in_span + out_span;
        let stretch = if all > seconds && all > 0.0 { seconds / all } else { 1.0 };
        (stretch, seconds - out_span * stretch)
    }


}

/// One drawn piece of a text, and where it sits in the text.
#[derive(Clone, Copy, Debug)]
pub struct Cell {
    /// Centre, in pixels from the centre of the text.
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Which whole, line, word and letter it belongs to.
    pub groups: [usize; 4],
}

/// One piece as it appears on one frame.
#[derive(Clone, Copy, Debug)]
pub struct Placed {
    pub cell: usize,
    /// Centre, in pixels on the picture.
    pub x: f32,
    pub y: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub rotate: f32,
    pub opacity: f32,
    pub blur: f32,
    /// How far it has sunk inside its own box, as a share of the box's height.
    pub sunk: f32,
    /// A stand-in letter to show, for Scramble.
    pub spare: Option<usize>,
    /// Nothing moved it, so it can be drawn straight.
    pub settled: bool,
}

#[derive(Clone, Copy)]
struct Pose {
    opacity: f32,
    dx: f32,
    dy: f32,
    scale_x: f32,
    scale_y: f32,
    rotate: f32,
    blur: f32,
    sunk: f32,
    scramble: bool,
}

const REST: Pose = Pose { opacity: 1.0, dx: 0.0, dy: 0.0, scale_x: 1.0, scale_y: 1.0, rotate: 0.0, blur: 0.0, sunk: 0.0, scramble: false };

/// A piece partway through an animation: `p` is eased, `raw` is plain time.
fn pose(kind: Anim, from: &Origin, p: f32, raw: f32) -> Pose {
    let gone = 1.0 - p;
    let fade = p.clamp(0.0, 1.0);
    match kind {
        Anim::None => REST,
        Anim::Fade => Pose { opacity: fade, ..REST },
        Anim::SlideUp => Pose { opacity: fade, dy: gone * SLIDE, ..REST },
        Anim::SlideDown => Pose { opacity: fade, dy: -gone * SLIDE, ..REST },
        Anim::SlideLeft => Pose { opacity: fade, dx: gone * SLIDE, ..REST },
        Anim::SlideRight => Pose { opacity: fade, dx: -gone * SLIDE, ..REST },
        Anim::Scale => {
            let s = SCALE_FROM + (1.0 - SCALE_FROM) * p;
            Pose { opacity: fade, scale_x: s, scale_y: s, ..REST }
        }
        Anim::Pop => Pose { opacity: (raw / POP_FADE).min(1.0), scale_x: p, scale_y: p, ..REST },
        Anim::Drop | Anim::Bounce => Pose { opacity: (raw / POP_FADE).min(1.0), dy: -gone * DROP, ..REST },
        Anim::Blur => Pose { opacity: fade, blur: gone.max(0.0) * BLUR_FROM, ..REST },
        Anim::Reveal => Pose { sunk: gone, ..REST },
        Anim::Typewriter => Pose { opacity: if raw > 0.0 { 1.0 } else { 0.0 }, ..REST },
        Anim::Scramble => Pose { opacity: if raw > 0.0 { 1.0 } else { 0.0 }, scramble: raw > 0.0 && raw < 1.0, ..REST },
        Anim::Cascade => Pose { opacity: fade, dy: -gone * SLIDE * 1.5, rotate: -gone * CASCADE_TURN, ..REST },
        Anim::WordRise => Pose { opacity: fade, dy: gone * SLIDE, ..REST },
        Anim::Tumble => Pose { opacity: fade, scale_y: (gone * std::f32::consts::FRAC_PI_2).cos(), rotate: -gone * TUMBLE_TURN, ..REST },
        Anim::Flip => Pose { opacity: fade, scale_y: (gone * std::f32::consts::FRAC_PI_2).cos(), ..REST },
        Anim::Swing => Pose { opacity: (raw / POP_FADE).min(1.0), rotate: -gone * SWING_FROM, ..REST },
        Anim::Custom => {
            let s = from.scale + (1.0 - from.scale) * p;
            Pose {
                opacity: (from.opacity + (1.0 - from.opacity) * p).clamp(0.0, 1.0),
                dx: from.dx * gone,
                dy: from.dy * gone,
                scale_x: s,
                scale_y: s,
                rotate: from.rotate * gone,
                blur: from.blur * gone.max(0.0),
                ..REST
            }
        }
    }
}

fn sway(r: &Repeat, t: f32, group: usize, count: usize) -> Pose {
    use std::f32::consts::TAU;
    let at = t * r.speed * TAU;
    let a = r.amount;
    match r.kind {
        Sway::None => REST,
        Sway::Pulse => {
            let s = 1.0 + 0.1 * a * (at.sin() * 0.5 + 0.5);
            Pose { scale_x: s, scale_y: s, ..REST }
        }
        Sway::Float => Pose { dy: at.sin() * 0.12 * a, ..REST },
        Sway::Wave => Pose { dy: (at - group as f32 * TAU / count.max(1) as f32).sin() * 0.25 * a, ..REST },
        Sway::Shake => Pose { dx: (at * 9.0).sin() * 0.06 * a, dy: (at * 7.0).cos() * 0.04 * a, ..REST },
        Sway::Wobble => Pose { rotate: at.sin() * 10.0 * a, ..REST },
        Sway::Flicker => Pose { opacity: 1.0 - a * 0.7 * ((at * 3.0).sin() * (at * 7.3).sin()).abs(), ..REST },
    }
}

#[inline]
fn hash(a: u32, b: u32) -> u32 {
    let mut h = a.wrapping_mul(0x9E37_79B1) ^ b.wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h.wrapping_mul(0xC2B2_AE3D) >> 8
}

/// Where a piece and its running totals are, part way through being posed.
struct Moving {
    x: f32,
    y: f32,
    scale_x: f32,
    scale_y: f32,
    rotate: f32,
    opacity: f32,
    blur: f32,
    sunk: f32,
    scramble: bool,
}

impl Moving {
    /// Turns and scales around the centre of its group, then moves.
    fn apply(&mut self, o: &Pose, pivot: (f32, f32), em: f32) {
        let (sin, cos) = o.rotate.to_radians().sin_cos();
        let (rx, ry) = ((self.x - pivot.0) * o.scale_x, (self.y - pivot.1) * o.scale_y);
        self.x = pivot.0 + rx * cos - ry * sin + o.dx * em;
        self.y = pivot.1 + rx * sin + ry * cos + o.dy * em;
        self.scale_x *= o.scale_x;
        self.scale_y *= o.scale_y;
        self.rotate += o.rotate;
        self.opacity *= o.opacity;
        self.blur += o.blur * em;
        self.sunk += o.sunk;
        self.scramble |= o.scramble;
    }
}

/// The middle of each group of pieces, by unit.
fn pivots(cells: &[Cell]) -> ([usize; 4], Vec<Vec<(f32, f32)>>) {
    let mut counts = [0usize; 4];
    for c in cells {
        for u in 0..4 {
            counts[u] = counts[u].max(c.groups[u] + 1);
        }
    }
    let mut out = Vec::with_capacity(4);
    for (u, count) in counts.iter().enumerate() {
        let mut bounds = vec![(f32::MAX, f32::MAX, f32::MIN, f32::MIN); *count];
        for c in cells {
            let b = &mut bounds[c.groups[u]];
            b.0 = b.0.min(c.x - c.w / 2.0);
            b.1 = b.1.min(c.y - c.h / 2.0);
            b.2 = b.2.max(c.x + c.w / 2.0);
            b.3 = b.3.max(c.y + c.h / 2.0);
        }
        out.push(bounds.iter().map(|b| ((b.0 + b.2) / 2.0, (b.1 + b.3) / 2.0)).collect());
    }
    (counts, out)
}

/// Where every piece goes on one frame. `em` is the text's height in pixels.
#[allow(clippy::too_many_arguments)]
pub fn compose(cells: &[Cell], m: &Motion, x: f32, y: f32, frame: u32, len: u32, fps: f32, em: f32, size: (f32, f32), spares: usize) -> Vec<Placed> {
    if m.hidden {
        return Vec::new();
    }
    let fps = fps.max(1.0);
    let (t, seconds) = (frame as f32 / fps, len as f32 / fps);
    let (counts, centres) = pivots(cells);
    let (stretch, out_from) = m.timing(seconds, &counts);
    let whole = m.at(frame, x, y);
    let (sin, cos) = whole.rotate.to_radians().sin_cos();
    let still = m.keys.is_empty() && m.scale == 1.0 && m.rotate == 0.0 && m.opacity == 1.0 && m.repeat.kind == Sway::None;
    let mut out = Vec::with_capacity(cells.len());
    for (i, c) in cells.iter().enumerate() {
        let mut now = Moving { x: c.x, y: c.y, scale_x: 1.0, scale_y: 1.0, rotate: 0.0, opacity: 1.0, blur: 0.0, sunk: 0.0, scramble: false };
        let mut settled = still;
        if m.enter.kind != Anim::None {
            let u = m.enter.unit.slot();
            let raw = m.enter.run(c.groups[u], counts[u], t, stretch);
            settled &= raw >= 1.0;
            now.apply(&pose(m.enter.kind, &m.enter.from, m.enter.ease.at(raw), raw), centres[u][c.groups[u]], em);
        }
        if m.repeat.kind != Sway::None {
            let u = m.repeat.unit.slot();
            now.apply(&sway(&m.repeat, t, c.groups[u], counts[u]), centres[u][c.groups[u]], em);
        }
        if m.leave.kind != Anim::None && t >= out_from {
            let u = m.leave.unit.slot();
            let raw = m.leave.run(c.groups[u], counts[u], t - out_from, stretch);
            settled &= raw <= 0.0;
            now.apply(&pose(m.leave.kind, &m.leave.from, 1.0 - m.leave.ease.at(raw), 1.0 - raw), centres[u][c.groups[u]], em);
        }
        let opacity = (now.opacity * whole.opacity).clamp(0.0, 1.0);
        if opacity <= 0.0 {
            continue;
        }
        let (gx, gy) = (now.x * whole.scale, now.y * whole.scale);
        let spare = (now.scramble && spares > 0).then(|| hash(i as u32, (t * SCRAMBLE_RATE) as u32) as usize % spares);
        out.push(Placed {
            cell: i,
            x: whole.x * size.0 + gx * cos - gy * sin,
            y: whole.y * size.1 + gx * sin + gy * cos,
            scale_x: now.scale_x * whole.scale,
            scale_y: now.scale_y * whole.scale,
            rotate: now.rotate + whole.rotate,
            opacity,
            blur: now.blur,
            sunk: now.sunk.clamp(0.0, 1.0),
            spare,
            settled,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn letters(n: usize) -> Vec<Cell> {
        (0..n)
            .map(|i| Cell { x: i as f32 * 40.0 - 20.0 * (n as f32 - 1.0), y: 0.0, w: 30.0, h: 80.0, groups: [0, 0, i / 2, i] })
            .collect()
    }

    fn at(m: &Motion, frame: u32) -> Vec<Placed> {
        compose(&letters(6), m, 0.5, 0.5, frame, 120, 30.0, 80.0, (1000.0, 500.0), 4)
    }

    fn with(kind: Anim) -> Motion {
        Motion { enter: Phase::preset(kind), ..Motion::default() }
    }

    #[test]
    fn every_curve_starts_at_zero_and_ends_at_one() {
        for e in Ease::ALL {
            assert_eq!(e.at(0.0), 0.0, "{e:?}");
            assert_eq!(e.at(1.0), 1.0, "{e:?}");
            assert!((0..=20).all(|i| e.at(i as f32 / 20.0).is_finite()), "{e:?}");
        }
    }

    #[test]
    fn back_goes_past_the_end_before_settling() {
        assert!((1..20).any(|i| Ease::Back.at(i as f32 / 20.0) > 1.0));
    }

    #[test]
    fn every_animation_ends_exactly_where_the_text_belongs() {
        let plain = at(&Motion::default(), 60);
        for a in Anim::ALL {
            let done = at(&with(a), 100);
            assert_eq!(done.len(), plain.len(), "{a:?}");
            for (p, q) in done.iter().zip(&plain) {
                assert!((p.x - q.x).abs() < 1e-3 && (p.y - q.y).abs() < 1e-3, "{a:?} {p:?}");
                assert!((p.opacity - 1.0).abs() < 1e-3 && p.spare.is_none() && p.settled, "{a:?}");
                assert!(p.blur.abs() < 1e-3 && p.sunk.abs() < 1e-3, "{a:?}");
            }
        }
    }

    #[test]
    fn every_animation_except_reveal_and_blur_hides_or_moves_at_the_start() {
        for a in Anim::ALL.into_iter().filter(|a| *a != Anim::None) {
            let plain = at(&Motion::default(), 0);
            let first = at(&with(a), 0);
            let differs = first.len() != plain.len()
                || first.iter().zip(&plain).any(|(p, q)| (p.x - q.x).abs() > 1.0 || (p.y - q.y).abs() > 1.0 || p.opacity < 1.0 || p.sunk > 0.0 || p.scale_y < 1.0);
            assert!(differs, "{a:?}");
        }
    }

    #[test]
    fn letters_arrive_one_after_another() {
        let m = Motion { enter: Phase { unit: Unit::Letter, delay: 0.1, seconds: 0.1, ..Phase::preset(Anim::Fade) }, ..Motion::default() };
        let seen = at(&m, 3);
        assert!(seen.iter().any(|p| p.cell == 0) && seen.iter().all(|p| p.cell <= 1), "{seen:?}");
    }

    #[test]
    fn reverse_starts_from_the_last_piece() {
        let mut m = Motion { enter: Phase { unit: Unit::Letter, delay: 0.1, seconds: 0.1, ..Phase::preset(Anim::Fade) }, ..Motion::default() };
        m.enter.order = Order::Reverse;
        assert_eq!(at(&m, 2).first().map(|p| p.cell), Some(5));
    }

    #[test]
    fn random_order_uses_every_place_once() {
        let mut ranks: Vec<usize> = (0..12).map(|g| shuffled(g, 12)).collect();
        assert_ne!(ranks, (0..12).collect::<Vec<_>>());
        ranks.sort();
        assert_eq!(ranks, (0..12).collect::<Vec<_>>());
    }

    #[test]
    fn typewriter_shows_more_letters_as_time_goes_on() {
        let m = with(Anim::Typewriter);
        assert!(at(&m, 0).len() <= 1);
        assert!(at(&m, 5).len() > at(&m, 1).len());
    }

    #[test]
    fn scramble_shows_stand_ins_until_each_letter_lands() {
        let m = with(Anim::Scramble);
        assert!(at(&m, 4).iter().any(|p| p.spare.is_some()));
        assert!(at(&m, 100).iter().all(|p| p.spare.is_none()));
    }

    #[test]
    fn an_out_ends_with_the_text_gone() {
        let m = Motion { leave: Phase::preset(Anim::Fade), ..Motion::default() };
        assert_eq!(at(&m, 60).len(), 6);
        assert!(at(&m, 119).iter().all(|p| p.opacity < 0.1));
    }

    #[test]
    fn a_short_item_squeezes_in_and_out_so_both_play() {
        let m = Motion { enter: Phase::preset(Anim::Fade), leave: Phase::preset(Anim::Fade), ..Motion::default() };
        let (stretch, from) = m.timing(0.6, &[1; 4]);
        assert!((stretch - 0.5).abs() < 1e-5 && (from - 0.3).abs() < 1e-5);
    }

    #[test]
    fn a_word_turns_around_its_own_middle() {
        let cells = letters(6);
        let m = Motion { repeat: Repeat { kind: Sway::Wobble, unit: Unit::Word, speed: 0.25, amount: 1.0 }, ..Motion::default() };
        let placed = compose(&cells, &m, 0.5, 0.5, 30, 120, 30.0, 80.0, (1000.0, 500.0), 0);
        // The first word's two letters stay equally far from the word's middle.
        let (a, b) = (placed[0], placed[1]);
        let mid = (500.0 - 80.0, 250.0);
        let d = |p: Placed| ((p.x - mid.0).powi(2) + (p.y - mid.1).powi(2)).sqrt();
        assert!((d(a) - d(b)).abs() < 1.0 && (a.y - b.y).abs() > 1.0, "{a:?} {b:?}");
    }

    fn key(x: f32) -> Keyed {
        Keyed { x, y: 0.5, scale: 1.0, rotate: 0.0, opacity: 1.0 }
    }

    #[test]
    fn keyframes_run_between_their_values_and_hold_beyond_them() {
        let mut m = Motion::default();
        m.set_key(10, key(0.0));
        m.set_key(20, key(1.0));
        m.keys[0].ease = Ease::Linear;
        assert_eq!(m.at(0, 0.5, 0.5).x, 0.0);
        assert!((m.at(15, 0.5, 0.5).x - 0.5).abs() < 1e-5);
        assert_eq!(m.at(40, 0.5, 0.5).x, 1.0);
    }

    #[test]
    fn without_keyframes_the_plain_settings_apply() {
        let m = Motion { scale: 2.0, ..Motion::default() };
        let v = m.at(5, 0.2, 0.3);
        assert_eq!((v.x, v.y, v.scale), (0.2, 0.3, 2.0));
    }

    #[test]
    fn a_key_at_the_same_frame_is_updated_not_doubled() {
        let mut m = Motion::default();
        m.set_key(10, key(0.0));
        m.set_key(10, key(0.7));
        assert_eq!(m.keys.len(), 1);
        assert_eq!(m.keys[0].value.x, 0.7);
    }

    #[test]
    fn keys_stay_in_order_however_they_are_added() {
        let mut m = Motion::default();
        for f in [30, 10, 20] {
            m.set_key(f, key(0.0));
        }
        assert_eq!(m.keys.iter().map(|k| k.frame).collect::<Vec<_>>(), [10, 20, 30]);
        assert_eq!((m.next_key(10), m.previous_key(20)), (Some(20), Some(10)));
        m.remove_key(20);
        assert_eq!(m.keys.len(), 2);
    }

    #[test]
    fn a_key_cant_be_dragged_past_its_neighbour() {
        let mut m = Motion::default();
        for f in [10, 20, 30] {
            m.set_key(f, key(0.0));
        }
        assert_eq!(m.retime_key(1, 50, 100), 29);
        assert_eq!(m.retime_key(0, 99, 100), 28);
        assert_eq!(m.retime_key(2, 500, 100), 99);
    }

    #[test]
    fn keyframes_move_the_whole_text() {
        let mut m = Motion::default();
        m.set_key(0, key(0.2));
        let p = at(&m, 0);
        let mid = (p[0].x + p[5].x) / 2.0;
        assert!((mid - 200.0).abs() < 1e-2, "{mid}");
    }

    #[test]
    fn keyframed_opacity_hides_the_text() {
        let mut m = Motion::default();
        m.set_key(0, Keyed { opacity: 0.0, ..key(0.5) });
        assert!(at(&m, 0).is_empty());
    }

    #[test]
    fn rotation_turns_the_whole_text_around_its_centre() {
        let m = Motion { rotate: 90.0, ..Motion::default() };
        let p = at(&m, 0);
        assert!((p[0].x - 500.0).abs() < 1e-2 && p[0].y < 250.0 && p[5].y > 250.0, "{p:?}");
    }

    #[test]
    fn splitting_keeps_the_motion_continuous() {
        let mut m = Motion { enter: Phase::preset(Anim::Fade), leave: Phase::preset(Anim::Fade), ..Motion::default() };
        m.set_key(0, key(0.0));
        m.set_key(100, key(1.0));
        let was = m.at(40, 0.5, 0.5);
        let tail = m.split(40, 0.5, 0.5);
        assert_eq!(m.at(40, 0.5, 0.5), was);
        assert_eq!(tail.at(0, 0.5, 0.5), was);
        assert_eq!(tail.keys.last().map(|k| k.frame), Some(60));
        assert_eq!(m.leave.kind, Anim::None);
        assert_eq!(tail.enter.kind, Anim::None);
    }

    #[test]
    fn trimming_the_start_keeps_keys_on_the_same_moment() {
        let mut m = Motion::default();
        m.set_key(30, key(0.0));
        m.shift_keys(10);
        assert_eq!(m.keys[0].frame, 20);
    }

    #[test]
    fn pieces_are_only_as_fine_as_the_animations_need() {
        assert_eq!(Motion::default().finest(), Unit::Line);
        assert_eq!(with(Anim::WordRise).finest(), Unit::Word);
        let m = Motion { enter: Phase::preset(Anim::WordRise), leave: Phase::preset(Anim::Bounce), ..Motion::default() };
        assert_eq!(m.finest(), Unit::Letter);
        let s = Motion { enter: Phase { unit: Unit::Whole, ..Phase::preset(Anim::Scramble) }, ..Motion::default() };
        assert_eq!(s.finest(), Unit::Letter);
    }

    fn custom(from: Origin) -> Motion {
        Motion { enter: Phase { from, ..Phase::preset(Anim::Custom) }, ..Motion::default() }
    }

    #[test]
    fn a_custom_animation_starts_where_it_says_and_ends_at_rest() {
        let m = custom(Origin { opacity: 1.0, dy: 1.0, rotate: 30.0, ..Origin::default() });
        let plain = at(&Motion::default(), 0);
        let first = at(&m, 0);
        assert!((first[0].y - plain[0].y - 80.0).abs() < 1e-3 && (first[0].rotate - 30.0).abs() < 1e-3, "{:?}", first[0]);
        for (p, q) in at(&m, 100).iter().zip(&plain) {
            assert!((p.x - q.x).abs() < 1e-3 && (p.y - q.y).abs() < 1e-3 && p.rotate.abs() < 1e-3 && p.settled);
        }
    }

    #[test]
    fn a_custom_animation_also_plays_as_an_out() {
        let m = Motion { leave: Phase { from: Origin { scale: 0.0, ..Origin::default() }, ..Phase::preset(Anim::Custom) }, ..Motion::default() };
        assert!(at(&m, 119).iter().all(|p| p.opacity < 0.2 || p.scale_x < 0.2));
    }

    #[test]
    fn wild_plugin_numbers_are_tamed() {
        let t = Origin { opacity: 9.0, dy: f32::NAN, scale: -3.0, rotate: 1e9, blur: 4.0, dx: 0.5 }.tamed();
        assert_eq!((t.opacity, t.dy, t.scale, t.rotate, t.blur, t.dx), (1.0, 0.0, 0.0, 720.0, 1.0, 0.5));
    }

    #[test]
    fn a_saved_project_from_before_still_loads() {
        let m: Motion = serde_json::from_str("{}").unwrap();
        assert_eq!(m, Motion::default());
    }
}
