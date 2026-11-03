//! The Film tools: stock and print emulation, plus the per-pixel looks.
//! Stock numbers are this app's own, shaped to behave like each stock's character;
//! they are not measured data. See MECHANICS.md.

use crate::color::{self, hue_window, to_linear, to_srgb};
use crate::lut::Lut3D;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];
const STOCK_TABLE: usize = 33;
/// Size of the table each clip's colour is baked to for playback.
pub const PLAY_TABLE: usize = 33;
const STOPS_ABOVE: f32 = 0.5;
const STOPS_BELOW: f32 = 0.21;

#[inline]
fn luma(c: [f32; 3]) -> f32 {
    c[0] * LUMA[0] + c[1] * LUMA[1] + c[2] * LUMA[2]
}
#[inline]
fn mix(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
#[inline]
fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [mix(a[0], b[0], t), mix(a[1], b[1], t), mix(a[2], b[2], t)]
}

/// How a camera negative answers light.
pub struct Stock {
    pub name: &'static str,
    /// Slope of each dye layer.
    gamma: [f32; 3],
    /// Light from one layer leaking into the others.
    crosstalk: color::M3,
    /// Colour of the base, added to the shadows.
    base: [f32; 3],
    /// Tungsten stocks shot in daylight lean warm, daylight stocks stay neutral.
    balance: [f32; 3],
    saturation: f32,
}

pub const STOCKS: [Stock; 6] = [
    Stock {
        name: "500T",
        gamma: [0.98, 1.0, 1.04],
        crosstalk: [[0.93, 0.06, 0.01], [0.04, 0.91, 0.05], [0.01, 0.08, 0.91]],
        base: [0.004, 0.006, 0.012],
        balance: [1.06, 1.0, 0.92],
        saturation: 0.96,
    },
    Stock {
        name: "250D",
        gamma: [1.0, 1.0, 1.0],
        crosstalk: [[0.95, 0.04, 0.01], [0.03, 0.94, 0.03], [0.01, 0.05, 0.94]],
        base: [0.004, 0.005, 0.008],
        balance: [1.02, 1.0, 0.98],
        saturation: 1.0,
    },
    Stock {
        name: "160T",
        gamma: [1.0, 1.02, 1.05],
        crosstalk: [[0.94, 0.05, 0.01], [0.03, 0.93, 0.04], [0.01, 0.06, 0.93]],
        base: [0.003, 0.004, 0.01],
        balance: [1.05, 1.0, 0.94],
        saturation: 1.02,
    },
    Stock {
        name: "Soft 16",
        gamma: [0.84, 0.86, 0.86],
        crosstalk: [[0.9, 0.08, 0.02], [0.05, 0.88, 0.07], [0.02, 0.09, 0.89]],
        base: [0.012, 0.012, 0.014],
        balance: [1.03, 1.0, 0.96],
        saturation: 0.88,
    },
    Stock {
        name: "Fuji Eterna 250D",
        gamma: [0.92, 0.94, 0.96],
        crosstalk: [[0.92, 0.07, 0.01], [0.03, 0.93, 0.04], [0.0, 0.07, 0.93]],
        base: [0.006, 0.009, 0.01],
        balance: [0.99, 1.01, 1.0],
        saturation: 0.9,
    },
    Stock {
        name: "Kodak Vision3 50D",
        gamma: [1.04, 1.04, 1.02],
        crosstalk: [[0.97, 0.025, 0.005], [0.02, 0.96, 0.02], [0.005, 0.03, 0.965]],
        base: [0.002, 0.003, 0.005],
        balance: [1.01, 1.0, 0.99],
        saturation: 1.06,
    },
];

/// How a print turns the negative into the final picture.
pub struct Print {
    pub name: &'static str,
    gamma: f32,
    /// Slope of the toe per channel; a higher red toe keeps red in the shadows.
    toe: [f32; 3],
    white: [f32; 3],
    black: [f32; 3],
}

pub const PRINTS: [Print; 5] = [
    Print {
        name: "Kodak 2383 D65",
        gamma: 1.32,
        toe: [0.9, 1.0, 1.08],
        white: [0.985, 0.985, 0.97],
        black: [0.012, 0.012, 0.014],
    },
    Print {
        name: "Kodak 2383 D60",
        gamma: 1.32,
        toe: [0.9, 1.0, 1.1],
        white: [1.0, 0.975, 0.93],
        black: [0.013, 0.012, 0.012],
    },
    Print {
        name: "Kodak 2393",
        gamma: 1.44,
        toe: [0.92, 1.0, 1.06],
        white: [0.99, 0.98, 0.96],
        black: [0.008, 0.008, 0.01],
    },
    Print {
        name: "Fuji 3510",
        gamma: 1.26,
        toe: [1.02, 0.96, 1.0],
        white: [0.97, 0.985, 0.98],
        black: [0.012, 0.015, 0.016],
    },
    Print {
        name: "Fuji 3514",
        gamma: 1.2,
        toe: [1.02, 0.96, 1.02],
        white: [0.96, 0.98, 0.985],
        black: [0.016, 0.018, 0.02],
    },
];

pub const GAUGES: [&str; 4] = ["65mm", "35mm", "16mm", "8mm"];
pub const OUTPUTS: [&str; 3] = ["Print", "Negative", "Rec.709"];

/// What each gauge does to grain, softness and halation.
pub const GAUGE_SCALE: [(f32, f32, f32); 4] = [(0.55, 0.3, 0.7), (1.0, 0.6, 1.0), (1.8, 1.2, 1.25), (2.8, 2.2, 1.5)];

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug)]
#[serde(default)]
pub struct FilmLook {
    pub on: bool,
    pub stock: usize,
    pub gauge: usize,
    pub output: usize,
    pub print: usize,
    pub intensity: f32,
    pub halation: f32,
    pub grain: f32,
    pub softness: f32,
    pub weave: f32,
    pub dust: f32,
}

impl Default for FilmLook {
    fn default() -> Self {
        Self {
            on: false,
            stock: 4,
            gauge: 1,
            output: 0,
            print: 0,
            intensity: 0.77,
            halation: 0.25,
            grain: 0.2,
            softness: 0.15,
            weave: 0.0,
            dust: 0.0,
        }
    }
}

#[inline]
fn soft_s(x: f32, gamma: f32, toe: f32) -> f32 {
    // Log-exposure in stops around grey, squeezed through a toe and shoulder.
    let t = (x * gamma).tanh();
    0.5 + 0.5 * if t < 0.0 { -(-t).powf(toe) } else { t }
}

/// One colour through the negative and then the print.
fn develop(c: [f32; 3], s: &Stock, p: &Print, output: usize) -> [f32; 3] {
    let lin = c.map(to_linear);
    let exposed = color::apply(&s.crosstalk, [lin[0] * s.balance[0], lin[1] * s.balance[1], lin[2] * s.balance[2]]);
    let stops: [f32; 3] = std::array::from_fn(|i| ((exposed[i] + s.base[i]).max(1e-4) / 0.18).log2());
    if output == 1 {
        // The scanned negative: flat, log-looking and without the print's contrast.
        return std::array::from_fn(|i| (0.42 + stops[i] * s.gamma[i] * 0.075).clamp(0.0, 1.0));
    }
    let mut out = [0.0; 3];
    for i in 0..3 {
        let g = if output == 2 { 1.0 } else { p.gamma };
        // A display white sits under 3 stops above grey and black over 5 below, so
        // each side has its own scale or whites would never reach the top of the curve.
        let scale = if stops[i] > 0.0 { STOPS_ABOVE } else { STOPS_BELOW };
        let v = soft_s(stops[i] * s.gamma[i] * scale, g, p.toe[i]);
        out[i] = mix(p.black[i], p.white[i], v);
    }
    let l = luma(out);
    let out = mix3([l; 3], out, s.saturation);
    // A stops curve centred on grey lands grey at 0.5 in display terms.
    out.map(|v| to_srgb(to_linear(v.clamp(0.0, 1.0)).powf(1.04)))
}

/// Stock and print baked to a table, kept so each pairing is only worked out once.
pub fn table(stock: usize, print: usize, output: usize) -> Arc<Lut3D> {
    static CACHE: OnceLock<Mutex<HashMap<(usize, usize, usize), Arc<Lut3D>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = (stock.min(STOCKS.len() - 1), print.min(PRINTS.len() - 1), output.min(OUTPUTS.len() - 1));
    if let Ok(mut c) = cache.lock() {
        return c
            .entry(key)
            .or_insert_with(|| Arc::new(Lut3D::from_fn(STOCK_TABLE, |x| develop(x, &STOCKS[key.0], &PRINTS[key.1], key.2))))
            .clone();
    }
    Arc::new(Lut3D::from_fn(STOCK_TABLE, |x| develop(x, &STOCKS[key.0], &PRINTS[key.1], key.2)))
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug)]
#[serde(default)]
pub struct Super8 {
    pub on: bool,
    pub amount: f32,
    pub flicker: f32,
    pub dust: f32,
    pub softness: f32,
    pub frame: bool,
}

impl Default for Super8 {
    fn default() -> Self {
        Self { on: false, amount: 0.7, flicker: 0.3, dust: 0.3, softness: 0.4, frame: true }
    }
}

/// Faded dyes of small-gauge reversal stock.
#[inline]
pub fn super8_colour(c: [f32; 3], amount: f32) -> [f32; 3] {
    let l = luma(c);
    let warm = [
        (c[0] * 1.06 + 0.035).min(1.0),
        c[1] * 0.98 + 0.018,
        c[2] * 0.82 + 0.05,
    ];
    let faded = mix3([l; 3], warm, 0.82);
    mix3(c, faded, amount)
}

/// Dye-layer vertices, one nudge for each corner of the colour cube.
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug, Default)]
#[serde(default)]
pub struct Tetra {
    pub red: [f32; 3],
    pub green: [f32; 3],
    pub blue: [f32; 3],
    pub cyan: [f32; 3],
    pub magenta: [f32; 3],
    pub yellow: [f32; 3],
}

impl Tetra {
    pub fn is_neutral(&self) -> bool {
        *self == Self::default()
    }

    pub fn corner_mut(&mut self, i: usize) -> &mut [f32; 3] {
        match i {
            0 => &mut self.red,
            1 => &mut self.green,
            2 => &mut self.blue,
            3 => &mut self.cyan,
            4 => &mut self.magenta,
            _ => &mut self.yellow,
        }
    }

    /// Splits the colour cube into six wedges and blends across the one holding `c`.
    #[inline]
    pub fn apply(&self, c: [f32; 3]) -> [f32; 3] {
        let v = |base: [f32; 3], d: [f32; 3]| [base[0] + d[0], base[1] + d[1], base[2] + d[2]];
        let r = v([1.0, 0.0, 0.0], self.red);
        let g = v([0.0, 1.0, 0.0], self.green);
        let b = v([0.0, 0.0, 1.0], self.blue);
        let cy = v([0.0, 1.0, 1.0], self.cyan);
        let m = v([1.0, 0.0, 1.0], self.magenta);
        let y = v([1.0, 1.0, 0.0], self.yellow);
        let [cr, cg, cb] = c;
        let w = |p: [f32; 3], a: f32, q: [f32; 3], d: f32, e: f32| -> [f32; 3] {
            std::array::from_fn(|i| p[i] * a + q[i] * d + e)
        };
        if cr > cg {
            if cg > cb {
                w(r, cr - cg, y, cg - cb, cb)
            } else if cr > cb {
                w(r, cr - cb, m, cb - cg, cg)
            } else {
                w(b, cb - cr, m, cr - cg, cg)
            }
        } else if cb > cg {
            w(b, cb - cg, cy, cg - cr, cr)
        } else if cb > cr {
            w(g, cg - cb, cy, cb - cr, cr)
        } else {
            w(g, cg - cr, y, cr - cb, cb)
        }
    }
}

/// One way a corner of the colour cube can lean. Primaries lean by gaining another
/// channel; secondaries already have two, so they lean by losing one.
pub struct Lean {
    pub name: &'static str,
    /// The hue this lean heads toward.
    pub toward: f32,
    channel: usize,
    sign: f32,
}

pub struct TetraChip {
    pub label: &'static str,
    /// The hue of the corner itself.
    pub hue: f32,
    pub corner: usize,
    pub leans: [Lean; 2],
}

const fn lean(name: &'static str, toward: f32, channel: usize, sign: f32) -> Lean {
    Lean { name, toward, channel, sign }
}

/// The six corners in panel order. `corner` follows `Tetra::corner_mut`.
pub const TETRA: [TetraChip; 6] = [
    TetraChip { label: "R->G/B", hue: 0.0, corner: 0, leans: [lean("Red-Green", 120.0, 1, 1.0), lean("Red-Blue", 240.0, 2, 1.0)] },
    TetraChip { label: "G->R/B", hue: 120.0, corner: 1, leans: [lean("Green-Red", 0.0, 0, 1.0), lean("Green-Blue", 240.0, 2, 1.0)] },
    TetraChip { label: "B->R/G", hue: 240.0, corner: 2, leans: [lean("Blue-Red", 0.0, 0, 1.0), lean("Blue-Green", 120.0, 1, 1.0)] },
    TetraChip { label: "C->G/B", hue: 180.0, corner: 3, leans: [lean("Cyan-Green", 120.0, 2, -1.0), lean("Cyan-Blue", 240.0, 1, -1.0)] },
    TetraChip { label: "M->R/B", hue: 300.0, corner: 4, leans: [lean("Magenta-Red", 0.0, 2, -1.0), lean("Magenta-Blue", 240.0, 0, -1.0)] },
    TetraChip { label: "Y->R/G", hue: 60.0, corner: 5, leans: [lean("Yellow-Red", 0.0, 1, -1.0), lean("Yellow-Green", 120.0, 0, -1.0)] },
];

impl Tetra {
    pub fn lean_of(&self, chip: &TetraChip, l: &Lean) -> f32 {
        let mut t = *self;
        t.corner_mut(chip.corner)[l.channel] * l.sign
    }

    pub fn set_lean(&mut self, chip: &TetraChip, l: &Lean, v: f32) {
        self.corner_mut(chip.corner)[l.channel] = v * l.sign;
    }
}

/// Split-tone styles: shadow tint, then highlight tint.
pub const SPLITS: [(&str, [f32; 3], [f32; 3]); 8] = [
    ("Negative", [0.40, 0.52, 0.60], [0.66, 0.56, 0.42]),
    ("Print", [0.34, 0.50, 0.56], [0.70, 0.60, 0.44]),
    ("Slide", [0.32, 0.42, 0.70], [0.68, 0.62, 0.34]),
    ("Chrome", [0.30, 0.48, 0.66], [0.64, 0.58, 0.46]),
    ("Cross Process", [0.36, 0.60, 0.46], [0.70, 0.66, 0.30]),
    ("Bronze", [0.42, 0.42, 0.50], [0.74, 0.54, 0.34]),
    ("Cyanotype", [0.26, 0.44, 0.62], [0.54, 0.62, 0.68]),
    ("Sepia", [0.50, 0.44, 0.38], [0.72, 0.58, 0.40]),
];

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug, Default)]
#[serde(default)]
pub struct Split {
    pub style: usize,
    pub amount: f32,
    pub balance: f32,
}

impl Split {
    #[inline]
    pub fn apply(&self, c: [f32; 3]) -> [f32; 3] {
        let (_, shadow, high) = SPLITS[self.style.min(SPLITS.len() - 1)];
        let l = luma(c).clamp(0.0, 1.0);
        let pivot = 0.5 + self.balance * 0.35;
        let t = ((l - pivot + 0.5) * 1.1).clamp(0.0, 1.0);
        let tint = mix3(shadow, high, t * t * (3.0 - 2.0 * t));
        let toned: [f32; 3] = std::array::from_fn(|i| {
            let a = c[i].clamp(0.0, 1.0);
            let b = tint[i];
            (1.0 - 2.0 * b) * a * a + 2.0 * b * a
        });
        mix3(c, toned, self.amount)
    }
}

/// Skipped silver: lower saturation, harder light.
#[inline]
pub fn bleach_bypass(c: [f32; 3], amount: f32) -> [f32; 3] {
    let l = luma(c).clamp(0.0, 1.0);
    let over: [f32; 3] = std::array::from_fn(|i| {
        let a = c[i].clamp(0.0, 1.0);
        if l < 0.5 { 2.0 * a * l } else { 1.0 - 2.0 * (1.0 - a) * (1.0 - l) }
    });
    mix3(c, mix3([l; 3], over, 0.55), amount)
}

/// Saturation that adds density, as richer dyes do, so colours deepen rather than glow.
#[inline]
pub fn saturate(c: [f32; 3], amount: f32) -> [f32; 3] {
    let l = luma(c);
    let boosted = mix3([l; 3], c, 1.0 + amount * 0.9);
    let spread = (c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2])).clamp(0.0, 1.0);
    let k = 1.0 - amount.max(0.0) * spread * 0.22;
    boosted.map(|v| v * k)
}

/// Three separate strips, each dyed in its complement and laid over the others.
#[inline]
pub fn technicolor(c: [f32; 3], amount: f32) -> [f32; 3] {
    let cyan = 1.0 - (c[0] - (c[1] + c[2]) * 0.5).clamp(-1.0, 1.0);
    let magenta = 1.0 - (c[1] - (c[0] + c[2]) * 0.5).clamp(-1.0, 1.0);
    let yellow = 1.0 - (c[2] - (c[0] + c[1]) * 0.5).clamp(-1.0, 1.0);
    let out = [
        c[0] * magenta.min(1.0) * yellow.min(1.0) * 1.08,
        c[1] * cyan.min(1.0) * yellow.min(1.0) * 1.04,
        c[2] * cyan.min(1.0) * magenta.min(1.0) * 1.06,
    ];
    mix3(c, out, amount)
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug, Default)]
#[serde(default)]
pub struct Foliage {
    pub hue: f32,
    pub saturation: f32,
    pub lightness: f32,
}

impl Foliage {
    pub fn is_neutral(&self) -> bool {
        *self == Self::default()
    }

    #[inline]
    pub fn apply(&self, c: [f32; 3]) -> [f32; 3] {
        const GREEN: f32 = 95.0;
        const REACH: f32 = 65.0;
        let [h, s, v] = color::to_hsv(c);
        let w = hue_window(h, GREEN, REACH) * (s * 5.0).min(1.0);
        if w <= 0.0 {
            return c;
        }
        color::from_hsv([
            h + self.hue * 40.0 * w,
            (s * (1.0 + self.saturation * w)).clamp(0.0, 1.0),
            v * (1.0 + self.lightness * 0.5 * w),
        ])
    }
}

pub const PAPERS: [&str; 3] = ["Matte", "Neutral", "Glossy"];

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug)]
#[serde(default)]
pub struct PrintSim {
    pub paper: usize,
    pub amount: f32,
}

impl Default for PrintSim {
    fn default() -> Self {
        Self { paper: 1, amount: 0.0 }
    }
}

impl PrintSim {
    /// Paper limits how black and how white a print can go.
    #[inline]
    pub fn apply(&self, c: [f32; 3]) -> [f32; 3] {
        let (black, white, tint, gloss) = match self.paper {
            0 => (0.075, 0.93, [1.0, 0.99, 0.965], 0.93),
            2 => (0.018, 0.985, [1.0, 1.0, 1.0], 1.1),
            _ => (0.042, 0.96, [1.0, 0.995, 0.985], 1.0),
        };
        let out: [f32; 3] = std::array::from_fn(|i| {
            let a = (c[i].clamp(0.0, 1.0) - 0.5) * gloss + 0.5;
            (black + a.clamp(0.0, 1.0) * (white - black)) * tint[i]
        });
        mix3(c, out, self.amount)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grey_stays_roughly_grey_on_every_daylight_stock() {
        let t = table(1, 0, 0);
        let c = t.sample([0.46; 3]);
        assert!((c[0] - c[1]).abs() < 0.04 && (c[1] - c[2]).abs() < 0.05, "{c:?}");
        assert!((0.3..0.62).contains(&c[1]), "{c:?}");
    }

    #[test]
    fn the_print_keeps_blacks_above_zero_and_whites_below_one() {
        let t = table(1, 0, 0);
        let (k, w) = (t.sample([0.0; 3]), t.sample([1.0; 3]));
        assert!(k.iter().all(|v| *v > 0.0) && w.iter().all(|v| *v < 1.0), "{k:?} {w:?}");
        assert!(w[1] > 0.88 && k[1] < 0.12, "{k:?} {w:?}");
    }

    #[test]
    fn the_negative_output_is_flatter_than_the_print() {
        let (print, neg) = (table(1, 0, 0), table(1, 0, 1));
        let range = |t: &Lut3D| t.sample([0.9; 3])[1] - t.sample([0.1; 3])[1];
        assert!(range(&neg) < range(&print));
    }

    #[test]
    fn tetra_at_rest_changes_nothing() {
        let t = Tetra::default();
        for c in [[0.2, 0.5, 0.8], [0.9, 0.1, 0.4], [0.3, 0.3, 0.3], [0.7, 0.6, 0.2]] {
            let o = t.apply(c);
            assert!((0..3).all(|i| (o[i] - c[i]).abs() < 1e-5), "{c:?} {o:?}");
        }
    }

    #[test]
    fn tetra_moves_only_colours_near_its_corner() {
        let t = Tetra { red: [0.0, 0.4, 0.0], ..Tetra::default() };
        assert!(t.apply([0.9, 0.05, 0.05])[1] > 0.3);
        let grey = t.apply([0.5, 0.5, 0.5]);
        assert!((grey[1] - 0.5).abs() < 1e-5);
    }

    #[test]
    fn every_lean_moves_its_corner_toward_the_named_hue() {
        for chip in &TETRA {
            for l in &chip.leans {
                let mut t = Tetra::default();
                t.set_lean(chip, l, 0.5);
                assert_eq!(t.lean_of(chip, l), 0.5, "{}", l.name);
                let start = crate::color::from_hsv([chip.hue, 1.0, 1.0]);
                let moved = crate::color::to_hsv(t.apply(start))[0];
                let was = crate::color::hue_gap(l.toward, chip.hue).abs();
                let now = crate::color::hue_gap(l.toward, moved).abs();
                assert!(now < was - 5.0, "{} went from {was} to {now}", l.name);
            }
        }
    }

    #[test]
    fn leans_leave_grey_alone() {
        for chip in &TETRA {
            let mut t = Tetra::default();
            t.set_lean(chip, &chip.leans[0], 1.0);
            t.set_lean(chip, &chip.leans[1], -1.0);
            let g = t.apply([0.4; 3]);
            assert!(g.iter().all(|v| (v - 0.4).abs() < 1e-5), "{}: {g:?}", chip.label);
        }
    }

    #[test]
    fn bleach_lowers_saturation() {
        let c = [0.7, 0.3, 0.2];
        let o = bleach_bypass(c, 1.0);
        assert!(o[0] - o[2] < c[0] - c[2]);
    }

    #[test]
    fn foliage_leaves_skin_alone() {
        let f = Foliage { hue: 1.0, saturation: 1.0, lightness: 1.0 };
        let skin = [0.8, 0.55, 0.45];
        assert_eq!(f.apply(skin), skin);
        assert_ne!(f.apply([0.2, 0.6, 0.15]), [0.2, 0.6, 0.15]);
    }

    #[test]
    fn matte_paper_has_lifted_blacks() {
        let m = PrintSim { paper: 0, amount: 1.0 }.apply([0.0; 3]);
        let g = PrintSim { paper: 2, amount: 1.0 }.apply([0.0; 3]);
        assert!(m[0] > g[0]);
    }
}
