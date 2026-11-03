//! The Color tools that work one pixel at a time.

use crate::color::{from_hsv, from_oklab, hue_gap, hue_window, to_hsv, to_linear, to_oklab, to_srgb};
use serde::{Deserialize, Serialize};

const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];

#[inline]
fn luma(c: [f32; 3]) -> f32 {
    c[0] * LUMA[0] + c[1] * LUMA[1] + c[2] * LUMA[2]
}
#[inline]
fn mix3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t)
}
#[inline]
fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a).max(1e-5)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Lift, Gamma, Gain and Offset. Each holds red, green and blue, then the master.
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug)]
#[serde(default)]
pub struct Wheels {
    pub lift: [f32; 4],
    pub gamma: [f32; 4],
    pub gain: [f32; 4],
    pub offset: [f32; 4],
    pub lum_mix: f32,
}

impl Default for Wheels {
    fn default() -> Self {
        Self { lift: [0.0; 4], gamma: [0.0; 4], gain: [0.0; 4], offset: [0.0; 4], lum_mix: 1.0 }
    }
}

#[inline]
fn lgg(x: f32, lift: f32, gamma: f32, gain: f32, offset: f32) -> f32 {
    let v = x * (1.0 + gain) + lift * (1.0 - x);
    let v = if v > 0.0 { v.powf(1.0 / (1.0 + gamma * 0.9).max(0.1)) } else { v };
    v + offset
}

impl Wheels {
    pub fn is_neutral(&self) -> bool {
        self.lift == [0.0; 4] && self.gamma == [0.0; 4] && self.gain == [0.0; 4] && self.offset == [0.0; 4]
    }

    pub fn wheel_mut(&mut self, i: usize) -> &mut [f32; 4] {
        match i {
            0 => &mut self.lift,
            1 => &mut self.gamma,
            2 => &mut self.gain,
            _ => &mut self.offset,
        }
    }

    /// Colour first, then the master; Lum Mix decides whether the master moves
    /// each channel or only the light.
    #[inline]
    pub fn apply(&self, c: [f32; 3]) -> [f32; 3] {
        let col: [f32; 3] = std::array::from_fn(|i| lgg(c[i], self.lift[i], self.gamma[i], self.gain[i], self.offset[i]));
        let m = |x: f32| lgg(x, self.lift[3], self.gamma[3], self.gain[3], self.offset[3]);
        let channels = col.map(m);
        let l = luma(col);
        let d = m(l) - l;
        let light = col.map(|v| v + d);
        mix3(channels, light, self.lum_mix)
    }
}

/// Six brightness zones, in stops around middle grey.
pub const ZONES: [(&str, f32); 6] = [
    ("Black", -6.0),
    ("Dark", -4.0),
    ("Shadow", -2.4),
    ("Light", -0.8),
    ("Highlight", 0.9),
    ("Specular", 2.2),
];
const ZONE_WIDTH: f32 = 1.15;

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug, Default)]
#[serde(default)]
pub struct Zones {
    pub exposure: [f32; 6],
    pub saturation: [f32; 6],
}

impl Zones {
    pub fn is_neutral(&self) -> bool {
        *self == Self::default()
    }

    #[inline]
    pub fn apply(&self, c: [f32; 3]) -> [f32; 3] {
        let lin = c.map(to_linear);
        let stops = (luma(lin).max(1e-5) / 0.18).log2();
        let (mut ev, mut sat, mut total) = (0.0, 0.0, 0.0);
        for (i, (_, centre)) in ZONES.iter().enumerate() {
            let d = (stops - centre) / ZONE_WIDTH;
            let w = (-0.5 * d * d).exp();
            ev += self.exposure[i] * w;
            sat += self.saturation[i] * w;
            total += w;
        }
        let k = 2f32.powf(ev / total.max(1e-3));
        let s = 1.0 + sat / total.max(1e-3);
        let l = luma(lin);
        mix3([l; 3], lin, s).map(|v| to_srgb(v.max(0.0) * k))
    }
}

/// Seven hue slices; the last one is skin, which is narrower and needs real colour.
pub const SLICES: [(&str, f32, f32); 7] = [
    ("Red", 0.0, 42.0),
    ("Yellow", 58.0, 42.0),
    ("Green", 120.0, 48.0),
    ("Cyan", 180.0, 42.0),
    ("Blue", 240.0, 48.0),
    ("Magenta", 305.0, 42.0),
    ("Skin", 26.0, 24.0),
];
const SKIN: usize = 6;

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug, Default)]
#[serde(default)]
pub struct Slice {
    pub hue: f32,
    pub saturation: f32,
    pub density: f32,
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug, Default)]
#[serde(default)]
pub struct Slices(pub [Slice; 7]);

impl Slices {
    pub fn is_neutral(&self) -> bool {
        *self == Self::default()
    }

    #[inline]
    pub fn apply(&self, c: [f32; 3]) -> [f32; 3] {
        let [mut h, mut s, mut v] = to_hsv(c);
        let (h0, s0) = (h, s);
        for (i, (_, centre, half)) in SLICES.iter().enumerate() {
            let p = &self.0[i];
            if *p == Slice::default() {
                continue;
            }
            let mut w = hue_window(h0, *centre, *half) * smooth(0.02, 0.14, s0);
            if i == SKIN {
                w *= smooth(0.12, 0.24, s0) * (1.0 - smooth(0.62, 0.8, s0));
            }
            h += p.hue * 30.0 * w;
            s *= 1.0 + p.saturation * w;
            // Density weighs a colour down the way extra dye would.
            v *= 1.0 - p.density * 0.5 * w * s0;
        }
        from_hsv([h, s.clamp(0.0, 1.0), v])
    }
}

/// The eight named colours of the HSL and Color Luminance tools.
pub const BANDS: [(&str, f32); 8] = [
    ("Red", 0.0),
    ("Orange", 30.0),
    ("Yellow", 60.0),
    ("Green", 120.0),
    ("Aqua", 180.0),
    ("Blue", 225.0),
    ("Purple", 275.0),
    ("Magenta", 325.0),
];
const BAND_REACH: f32 = 38.0;

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug)]
#[serde(default)]
pub struct Band {
    pub hue: f32,
    pub saturation: f32,
    pub value: f32,
    pub range: f32,
}

impl Default for Band {
    fn default() -> Self {
        Self { hue: 0.0, saturation: 0.0, value: 0.0, range: 1.0 }
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug, Default)]
#[serde(default)]
pub struct HslMixer(pub [Band; 8]);

impl HslMixer {
    pub fn is_neutral(&self) -> bool {
        self.0.iter().all(|b| b.hue == 0.0 && b.saturation == 0.0 && b.value == 0.0)
    }

    #[inline]
    pub fn apply(&self, c: [f32; 3]) -> [f32; 3] {
        let [h0, s0, v0] = to_hsv(c);
        let (mut h, mut s, mut v) = (h0, s0, v0);
        for (i, (_, centre)) in BANDS.iter().enumerate() {
            let b = &self.0[i];
            if b.hue == 0.0 && b.saturation == 0.0 && b.value == 0.0 {
                continue;
            }
            let w = hue_window(h0, *centre, BAND_REACH * b.range) * smooth(0.02, 0.12, s0);
            h += b.hue * 45.0 * w;
            s *= 1.0 + b.saturation * w;
            v *= 1.0 + b.value * 0.6 * w;
        }
        from_hsv([h, s.clamp(0.0, 1.0), v])
    }
}

/// Moves a colour's lightness in Oklab, so its hue stays exactly where it was.
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug, Default)]
#[serde(default)]
pub struct ColorLuminance(pub [f32; 8]);

impl ColorLuminance {
    pub fn is_neutral(&self) -> bool {
        *self == Self::default()
    }

    #[inline]
    pub fn apply(&self, c: [f32; 3]) -> [f32; 3] {
        let [h, s, _] = to_hsv(c);
        let mut amount = 0.0;
        for (i, (_, centre)) in BANDS.iter().enumerate() {
            amount += self.0[i] * hue_window(h, *centre, BAND_REACH);
        }
        let w = smooth(0.03, 0.18, s);
        if amount * w == 0.0 {
            return c;
        }
        let [l, a, b] = to_oklab(c);
        from_oklab([(l * (1.0 + amount * w * 0.5)).clamp(0.0, 1.0), a, b])
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug)]
#[serde(default)]
pub struct Mixer {
    pub red: [f32; 3],
    pub green: [f32; 3],
    pub blue: [f32; 3],
    pub mono: bool,
    pub mono_mix: [f32; 3],
}

impl Default for Mixer {
    fn default() -> Self {
        Self {
            red: [1.0, 0.0, 0.0],
            green: [0.0, 1.0, 0.0],
            blue: [0.0, 0.0, 1.0],
            mono: false,
            mono_mix: [0.3, 0.59, 0.11],
        }
    }
}

impl Mixer {
    pub fn is_neutral(&self) -> bool {
        *self == Self::default()
    }

    #[inline]
    pub fn apply(&self, c: [f32; 3]) -> [f32; 3] {
        let dot = |m: [f32; 3]| m[0] * c[0] + m[1] * c[1] + m[2] * c[2];
        if self.mono {
            return [dot(self.mono_mix); 3];
        }
        [dot(self.red), dot(self.green), dot(self.blue)]
    }
}

/// Keeps the subject's colour and pushes the rest of the palette away from it.
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug)]
#[serde(default)]
pub struct Separation {
    pub amount: f32,
    pub anchor: f32,
    pub width: f32,
    pub saturation: f32,
}

impl Default for Separation {
    fn default() -> Self {
        Self { amount: 0.0, anchor: 28.0, width: 30.0, saturation: 0.0 }
    }
}

impl Separation {
    #[inline]
    pub fn apply(&self, c: [f32; 3]) -> [f32; 3] {
        const REACH: f32 = 60.0;
        let [h, s, v] = to_hsv(c);
        let d = hue_gap(h, self.anchor);
        let away = smooth(self.width * 0.5, self.width + REACH, d.abs()) * (1.0 - smooth(120.0, 180.0, d.abs()));
        let h2 = h + d.signum() * self.amount * 50.0 * away;
        let s2 = (s * (1.0 + self.saturation * away)).clamp(0.0, 1.0);
        from_hsv([h2, s2, v])
    }
}

/// Shadow toe and highlight shoulder of the film curve.
#[inline]
pub fn toe_shoulder(c: [f32; 3], toe: f32, shoulder: f32) -> [f32; 3] {
    c.map(|v| {
        let x = v.clamp(0.0, 1.0);
        let low = (1.0 - x).powi(3) * x * toe * 0.9;
        let high = x.powi(3) * (1.0 - x) * shoulder * 0.9;
        v - low + high
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untouched_tools_do_nothing() {
        let c = [0.31, 0.52, 0.74];
        let same = |o: [f32; 3]| (0..3).all(|i| (o[i] - c[i]).abs() < 2e-3);
        assert!(same(Wheels::default().apply(c)));
        assert!(same(Mixer::default().apply(c)));
        assert!(same(Slices::default().apply(c)));
        assert!(same(HslMixer::default().apply(c)));
        assert!(same(Zones::default().apply(c)));
        assert!(same(Separation::default().apply(c)));
    }

    #[test]
    fn lift_moves_shadows_and_gain_moves_highlights() {
        let lift = Wheels { lift: [0.0, 0.0, 0.0, 0.2], ..Wheels::default() };
        assert!(lift.apply([0.0; 3])[0] > 0.15);
        assert!((lift.apply([1.0; 3])[0] - 1.0).abs() < 0.02);
        let gain = Wheels { gain: [0.0, 0.0, 0.0, -0.3], ..Wheels::default() };
        assert!(gain.apply([1.0; 3])[0] < 0.75);
        assert!(gain.apply([0.0; 3])[0].abs() < 0.01);
    }

    #[test]
    fn lum_mix_keeps_colour_apart_when_the_master_moves() {
        let w = Wheels { offset: [0.0, 0.0, 0.0, 0.1], lum_mix: 1.0, ..Wheels::default() };
        let c = [0.6, 0.3, 0.2];
        let o = w.apply(c);
        assert!(((o[0] - o[2]) - (c[0] - c[2])).abs() < 1e-3);
    }

    #[test]
    fn a_zone_only_touches_its_own_light() {
        let mut z = Zones::default();
        z.exposure[5] = -2.0;
        let bright = z.apply([0.95; 3])[0];
        let dark = z.apply([0.05; 3])[0];
        assert!(bright < 0.85);
        assert!((dark - 0.05).abs() < 0.01);
    }

    #[test]
    fn the_skin_slice_needs_skin_like_saturation() {
        let mut s = Slices::default();
        s.0[SKIN].saturation = 1.0;
        assert!(to_hsv(s.apply([0.8, 0.58, 0.46]))[1] > to_hsv([0.8, 0.58, 0.46])[1]);
        // Vivid orange is near skin's hue, but far too saturated to be skin.
        let vivid = [0.95, 0.5, 0.02];
        let o = s.apply(vivid);
        assert!((0..3).all(|i| (o[i] - vivid[i]).abs() < 1e-3), "{o:?}");
    }

    #[test]
    fn colour_luminance_moves_light_and_keeps_hue() {
        let mut l = ColorLuminance::default();
        l.0[5] = 0.8;
        let c = [0.1, 0.25, 0.7];
        let o = l.apply(c);
        assert!(luma(o) > luma(c));
        assert!((hue_gap(to_hsv(o)[0], to_hsv(c)[0])).abs() < 6.0);
    }

    #[test]
    fn the_mixer_goes_mono() {
        let m = Mixer { mono: true, ..Mixer::default() };
        let o = m.apply([0.2, 0.6, 0.9]);
        assert!(o[0] == o[1] && o[1] == o[2]);
    }

    #[test]
    fn separation_leaves_the_anchor_alone() {
        let s = Separation { amount: 1.0, anchor: 30.0, ..Separation::default() };
        let skin = from_hsv([30.0, 0.5, 0.7]);
        let o = s.apply(skin);
        assert!((0..3).all(|i| (o[i] - skin[i]).abs() < 1e-3));
        let far = from_hsv([100.0, 0.5, 0.7]);
        assert!(hue_gap(to_hsv(s.apply(far))[0], 100.0) > 10.0);
    }

    #[test]
    fn toe_darkens_shadows_and_shoulder_lifts_highlights() {
        assert!(toe_shoulder([0.2; 3], 1.0, 0.0)[0] < 0.2);
        assert!(toe_shoulder([0.8; 3], 0.0, 1.0)[0] > 0.8);
    }
}
