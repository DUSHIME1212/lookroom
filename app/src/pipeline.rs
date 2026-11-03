//! The colour engine: all sliders, the order they run in, and the pixel work.
//! Ranges and neutral values are listed in MECHANICS.md.

use crate::color::{self, Cst, CstStep, to_linear, to_srgb};
use crate::crop::Crop;
use crate::curves::{Baked, Curves, HUE_REACH, LIGHT_REACH, Table};
use crate::effects::{self, Crt, Diffusion, Texture};
use crate::film::{self, FilmLook, Foliage, PrintSim, Split, Super8, Tetra};
use crate::lut::{Lut3D, LutBank};
use crate::masks::{self, Layer, ShotMatch};
use crate::tools::{self, ColorLuminance, HslMixer, Mixer, Separation, Slices, Wheels, Zones};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];

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

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug)]
pub struct Tint {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub opacity: f32,
}
impl Default for Tint {
    fn default() -> Self {
        Self { r: 1.0, g: 1.0, b: 1.0, opacity: 0.0 }
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct LutRef {
    pub id: String,
    pub name: String,
    pub intensity: f32,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(default)]
pub struct Grade {
    pub luts: Vec<LutRef>,
    pub exposure: f32,
    pub contrast: f32,
    pub brightness: f32,
    pub highlight: f32,
    pub shadow: f32,
    pub whites: f32,
    pub blacks: f32,
    pub gamma: f32,
    pub saturation: f32,
    pub vibrance: f32,
    pub hue: f32,
    pub temp: f32,
    pub tint: f32,
    pub fade: f32,
    pub clarity: f32,
    pub sharpness: f32,
    pub vignette: f32,
    pub grain_intensity: f32,
    pub grain_size: f32,
    pub grain_roughness: f32,
    pub halation_intensity: f32,
    pub halation_threshold: f32,
    pub halation_glow: f32,
    pub bloom_intensity: f32,
    pub bloom_threshold: f32,
    pub film_contrast: f32,
    pub film_density: f32,
    pub film_saturation: f32,
    pub shadows_tint: Tint,
    pub midtones_tint: Tint,
    pub highlights_tint: Tint,
    pub frame_ratio: f32,
    pub crop: Crop,
    pub layers: Vec<Layer>,
    pub shot: Option<ShotMatch>,
    pub cst: Cst,
    pub toe: f32,
    pub shoulder: f32,
    pub structure: f32,
    pub dehaze: f32,
    pub curves: Curves,
    pub wheels: Wheels,
    pub zones: Zones,
    pub global_tint: Tint,
    pub mixer: Mixer,
    pub slices: Slices,
    pub hsl: HslMixer,
    pub color_lum: ColorLuminance,
    pub separation: Separation,
    pub density_rgb: [f32; 3],
    pub tetra: Tetra,
    pub film: FilmLook,
    pub super8: Super8,
    pub split: Split,
    pub bleach: f32,
    pub saturate: f32,
    pub technicolor: f32,
    pub foliage: Foliage,
    pub print: PrintSim,
    pub diffusion: Diffusion,
    pub gate_weave: f32,
    pub crt: Crt,
    pub texture: Texture,
    pub aberration: f32,
    pub lens: f32,
}

impl Default for Grade {
    fn default() -> Self {
        Self {
            luts: Vec::new(),
            exposure: 0.0,
            contrast: 1.0,
            brightness: 0.0,
            highlight: 0.0,
            shadow: 0.0,
            whites: 0.0,
            blacks: 0.0,
            gamma: 0.0,
            saturation: 1.0,
            vibrance: 0.0,
            hue: 0.0,
            temp: 0.0,
            tint: 0.0,
            fade: 0.0,
            clarity: 0.0,
            sharpness: 0.0,
            vignette: 0.0,
            grain_intensity: 0.0,
            grain_size: 1.0,
            grain_roughness: 1.0,
            halation_intensity: 0.0,
            halation_threshold: 0.7,
            halation_glow: 50.0,
            bloom_intensity: 0.0,
            bloom_threshold: 0.75,
            film_contrast: 0.0,
            film_density: 0.0,
            film_saturation: 1.0,
            shadows_tint: Tint::default(),
            midtones_tint: Tint::default(),
            highlights_tint: Tint::default(),
            frame_ratio: 0.0,
            crop: Crop::default(),
            layers: Vec::new(),
            shot: None,
            cst: Cst::default(),
            toe: 0.0,
            shoulder: 0.0,
            structure: 0.0,
            dehaze: 0.0,
            curves: Curves::default(),
            wheels: Wheels::default(),
            zones: Zones::default(),
            global_tint: Tint::default(),
            mixer: Mixer::default(),
            slices: Slices::default(),
            hsl: HslMixer::default(),
            color_lum: ColorLuminance::default(),
            separation: Separation::default(),
            density_rgb: [1.0; 3],
            tetra: Tetra::default(),
            film: FilmLook::default(),
            super8: Super8::default(),
            split: Split::default(),
            bleach: 0.0,
            saturate: 0.0,
            technicolor: 0.0,
            foliage: Foliage::default(),
            print: PrintSim::default(),
            diffusion: Diffusion::default(),
            gate_weave: 0.0,
            crt: Crt::default(),
            texture: Texture::default(),
            aberration: 0.0,
            lens: 0.0,
        }
    }
}

/// A picture held as display-encoded colour from 0 to 1.
#[derive(Clone)]
pub struct Picture {
    pub w: usize,
    pub h: usize,
    pub px: Vec<[f32; 3]>,
}

impl Picture {
    /// Keeps whatever depth the file had; 8-bit files simply land on fewer values.
    pub fn from_rgb32f(w: usize, h: usize, values: &[f32]) -> Self {
        let px = values.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
        Self { w, h, px }
    }

    pub fn to_rgba8(&self) -> Vec<u8> {
        let mut out = vec![255u8; self.px.len() * 4];
        out.par_chunks_mut(4).zip(self.px.par_iter()).for_each(|(o, p)| {
            o[0] = (p[0].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            o[1] = (p[1].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            o[2] = (p[2].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        });
        out
    }

    pub fn to_rgb8(&self) -> Vec<u8> {
        let mut out = vec![0u8; self.px.len() * 3];
        out.par_chunks_mut(3).zip(self.px.par_iter()).for_each(|(o, p)| {
            o[0] = (p[0].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            o[1] = (p[1].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
            o[2] = (p[2].clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        });
        out
    }

    pub fn to_rgb16(&self) -> Vec<u16> {
        let mut out = vec![0u16; self.px.len() * 3];
        out.par_chunks_mut(3).zip(self.px.par_iter()).for_each(|(o, p)| {
            o[0] = (p[0].clamp(0.0, 1.0) * 65535.0 + 0.5) as u16;
            o[1] = (p[1].clamp(0.0, 1.0) * 65535.0 + 0.5) as u16;
            o[2] = (p[2].clamp(0.0, 1.0) * 65535.0 + 0.5) as u16;
        });
        out
    }

    /// Averaging shrink so that the longest side fits `max_side`.
    pub fn fit(&self, max_side: usize) -> Picture {
        let longest = self.w.max(self.h);
        if longest <= max_side {
            return self.clone();
        }
        let s = longest as f32 / max_side as f32;
        let w = ((self.w as f32 / s).round() as usize).max(1);
        let h = ((self.h as f32 / s).round() as usize).max(1);
        self.resample(0, 0, self.w, self.h, w, h)
    }

    /// Centre crop to an exact size, for the preset tiles.
    pub fn cover(&self, w: usize, h: usize) -> Picture {
        let want = w as f32 / h as f32;
        let have = self.w as f32 / self.h as f32;
        let (cw, ch) = if have > want {
            ((self.h as f32 * want) as usize, self.h)
        } else {
            (self.w, (self.w as f32 / want) as usize)
        };
        self.resample((self.w - cw) / 2, (self.h - ch) / 2, cw, ch, w, h)
    }

    fn resample(&self, x0: usize, y0: usize, cw: usize, ch: usize, w: usize, h: usize) -> Picture {
        let mut px = vec![[0.0f32; 3]; w * h];
        px.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            let ya = y0 + y * ch / h;
            let yb = (y0 + (y + 1) * ch / h).max(ya + 1).min(self.h);
            for (x, o) in row.iter_mut().enumerate() {
                let xa = x0 + x * cw / w;
                let xb = (x0 + (x + 1) * cw / w).max(xa + 1).min(self.w);
                let mut sum = [0.0; 3];
                let mut n = 0.0;
                for yy in ya..yb {
                    for xx in xa..xb {
                        let p = self.px[xx + yy * self.w];
                        sum[0] += p[0];
                        sum[1] += p[1];
                        sum[2] += p[2];
                        n += 1.0;
                    }
                }
                *o = [sum[0] / n, sum[1] / n, sum[2] / n];
            }
        });
        Picture { w, h, px }
    }
}

/// Steps that only depend on one pixel. Baked once for each render.
pub struct ColorStep {
    luts: Vec<(Arc<Lut3D>, f32)>,
    g: Grade,
    gain: [f32; 3],
    exposure: f32,
    gamma_pow: f32,
    hue_cos: f32,
    hue_sin: f32,
    has_tone: bool,
    cst: CstStep,
    cst_on: bool,
    curves: Baked,
    wheels_on: bool,
    zones_on: bool,
    mixer_on: bool,
    slices_on: bool,
    hsl_on: bool,
    lum_on: bool,
    tetra_on: bool,
    foliage_on: bool,
    stock: Option<(Arc<Lut3D>, f32)>,
}

impl ColorStep {
    pub fn new(g: &Grade, bank: &LutBank) -> Self {
        let luts = g
            .luts
            .iter()
            .filter_map(|l| bank.get(&l.id).map(|t| (t.clone(), l.intensity)))
            .collect();
        // White balance: the slider is an offset in kelvin around 6500 K.
        let t = g.temp / 3000.0;
        let m = g.tint / 100.0;
        let gain = [1.0 + 0.28 * t + 0.06 * m, 1.0 - 0.14 * m, 1.0 - 0.34 * t + 0.08 * m];
        let (hue_sin, hue_cos) = g.hue.to_radians().sin_cos();
        let has_tone = g.shadows_tint.opacity > 0.0
            || g.midtones_tint.opacity > 0.0
            || g.highlights_tint.opacity > 0.0
            || g.global_tint.opacity > 0.0;
        let stock = g.film.on.then(|| (film::table(g.film.stock, g.film.print, g.film.output), g.film.intensity));
        Self {
            luts,
            g: g.clone(),
            gain,
            exposure: 2f32.powf(g.exposure),
            gamma_pow: 2f32.powf(-g.gamma),
            hue_cos,
            hue_sin,
            has_tone,
            cst: CstStep::new(&g.cst),
            cst_on: !g.cst.is_neutral(),
            curves: Baked::new(&g.curves),
            wheels_on: !g.wheels.is_neutral(),
            zones_on: !g.zones.is_neutral(),
            mixer_on: !g.mixer.is_neutral(),
            slices_on: !g.slices.is_neutral(),
            hsl_on: !g.hsl.is_neutral(),
            lum_on: !g.color_lum.is_neutral(),
            tetra_on: !g.tetra.is_neutral(),
            foliage_on: !g.foliage.is_neutral(),
            stock,
        }
    }

    /// Every colour step, for the tiles and for exporting a table.
    #[inline]
    pub fn apply(&self, c: [f32; 3]) -> [f32; 3] {
        self.print(self.fade(self.late(self.early(c))))
    }

    /// White balance through brightness: the steps before clarity.
    #[inline]
    fn early(&self, mut c: [f32; 3]) -> [f32; 3] {
        let g = &self.g;
        if self.cst_on {
            c = self.cst.input(c);
        }
        if let Some(s) = &g.shot {
            c = s.apply(c);
        }
        if self.gain != [1.0; 3] {
            for i in 0..3 {
                c[i] = to_srgb(to_linear(c[i]) * self.gain[i]);
            }
        }
        if g.hue != 0.0 {
            let y = 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2];
            let i = 0.596 * c[0] - 0.274 * c[1] - 0.322 * c[2];
            let q = 0.211 * c[0] - 0.523 * c[1] + 0.312 * c[2];
            let (i2, q2) = (i * self.hue_cos - q * self.hue_sin, i * self.hue_sin + q * self.hue_cos);
            c = [
                y + 0.956 * i2 + 0.621 * q2,
                y - 0.272 * i2 - 0.647 * q2,
                y - 1.106 * i2 + 1.703 * q2,
            ];
        }
        if self.exposure != 1.0 {
            for v in &mut c {
                *v = to_srgb(to_linear(*v) * self.exposure);
            }
        }
        if g.whites != 0.0 || g.blacks != 0.0 {
            let l = luma(c).clamp(0.0, 1.0);
            let s = (1.0 - l) * (1.0 - l);
            let h = l * l;
            c = lift(c, g.whites * 0.14 * h * h + g.blacks * 0.14 * s * s);
        }
        if g.highlight != 0.0 || g.shadow != 0.0 {
            let l = luma(c).clamp(0.0, 1.0);
            let s = (1.0 - l) * (1.0 - l);
            let h = l * l;
            c = lift(c, g.shadow * 0.32 * s * (1.0 - s * 0.35) + g.highlight * 0.32 * h * (1.0 - h * 0.35));
        }
        if g.contrast != 1.0 {
            for v in &mut c {
                *v = (*v - 0.5) * g.contrast + 0.5;
            }
        }
        if g.brightness != 0.0 {
            for v in &mut c {
                *v += g.brightness;
            }
        }
        if g.gamma != 0.0 {
            for v in &mut c {
                *v = v.max(0.0).powf(self.gamma_pow);
            }
        }
        c
    }

    /// Film contrast through the colour tables: the steps after clarity.
    #[inline]
    fn late(&self, mut c: [f32; 3]) -> [f32; 3] {
        let g = &self.g;
        if g.film_contrast != 0.0 {
            for v in &mut c {
                let x = v.clamp(0.0, 1.0);
                let s = x * x * (3.0 - 2.0 * x);
                *v = mix(*v, s, g.film_contrast);
            }
        }
        if g.toe != 0.0 || g.shoulder != 0.0 {
            c = tools::toe_shoulder(c, g.toe, g.shoulder);
        }
        if let Some(t) = &self.curves.tone {
            c = tone_curves(t, c);
        }
        if let Some(t) = &self.curves.colour {
            c = colour_curves(t, c);
        }
        if self.wheels_on {
            c = g.wheels.apply(c);
        }
        if self.zones_on {
            c = g.zones.apply(c);
        }
        if self.has_tone {
            let l = luma(c).clamp(0.0, 1.0);
            let s = (1.0 - l) * (1.0 - l);
            let h = l * l;
            let m = 4.0 * l * (1.0 - l);
            for (t, w) in [(g.highlights_tint, h), (g.midtones_tint, m), (g.shadows_tint, s), (g.global_tint, 1.0)] {
                if t.opacity > 0.0 {
                    let tint = [t.r, t.g, t.b];
                    for i in 0..3 {
                        // Soft light keeps the picture's own contrast under the tint.
                        let a = c[i].clamp(0.0, 1.0);
                        let b = tint[i];
                        let soft = (1.0 - 2.0 * b) * a * a + 2.0 * b * a;
                        c[i] = mix(c[i], soft, t.opacity * w);
                    }
                }
            }
        }
        if g.vibrance != 0.0 {
            let mx = c[0].max(c[1]).max(c[2]);
            let mn = c[0].min(c[1]).min(c[2]);
            let sat = (mx - mn).clamp(0.0, 1.0);
            // Warm mid-saturated colours (skin) get less of the boost.
            let skin = ((c[0] - c[1]).clamp(0.0, 0.3) / 0.3) * ((c[1] - c[2]).clamp(0.0, 0.3) / 0.3);
            let amount = g.vibrance * (1.0 - sat) * (1.0 - 0.6 * skin);
            let l = luma(c);
            c = mix3([l; 3], c, 1.0 + amount);
        }
        if g.saturation != 1.0 {
            let l = luma(c);
            c = mix3([l; 3], c, g.saturation);
        }
        if self.mixer_on {
            c = g.mixer.apply(c);
        }
        if self.slices_on {
            c = g.slices.apply(c);
        }
        if self.hsl_on {
            c = g.hsl.apply(c);
        }
        if self.lum_on {
            c = g.color_lum.apply(c);
        }
        if self.tetra_on {
            c = g.tetra.apply(c);
        }
        if g.separation.amount != 0.0 {
            c = g.separation.apply(c);
        }
        if g.film_density != 0.0 {
            let mx = c[0].max(c[1]).max(c[2]);
            let mn = c[0].min(c[1]).min(c[2]);
            let k = g.film_density * (mx - mn).clamp(0.0, 1.0) * 0.35;
            for i in 0..3 {
                c[i] *= 1.0 - k * g.density_rgb[i];
            }
        }
        if g.film_saturation != 1.0 {
            c = log_saturation(c, g.film_saturation);
        }
        if let Some((table, i)) = &self.stock {
            let out = table.sample(c);
            c = mix3(c, out, *i);
        }
        if g.bleach != 0.0 {
            c = film::bleach_bypass(c, g.bleach);
        }
        if g.saturate != 0.0 {
            c = film::saturate(c, g.saturate);
        }
        if g.technicolor != 0.0 {
            c = film::technicolor(c, g.technicolor);
        }
        if self.foliage_on {
            c = g.foliage.apply(c);
        }
        if g.split.amount != 0.0 {
            c = g.split.apply(c);
        }
        if g.super8.on {
            c = film::super8_colour(c, g.super8.amount);
        }
        for (lut, i) in &self.luts {
            let out = lut.sample(c);
            c = mix3(c, out, *i);
        }
        if self.cst_on {
            c = self.cst.output(c);
        }
        c
    }

    /// Fade runs after the tables, bloom and sharpness.
    #[inline]
    fn fade(&self, mut c: [f32; 3]) -> [f32; 3] {
        let f = self.g.fade;
        if f != 0.0 {
            for v in &mut c {
                *v = *v * (1.0 - f * 0.22) + f * 0.11;
            }
        }
        c
    }

    /// Paper comes last of all, after grain and halation.
    #[inline]
    fn print(&self, c: [f32; 3]) -> [f32; 3] {
        if self.g.print.amount != 0.0 { self.g.print.apply(c) } else { c }
    }
}

#[inline]
fn tone_curves(t: &[Table; 4], c: [f32; 3]) -> [f32; 3] {
    let m = c.map(|v| t[0].get(v));
    [t[1].get(m[0]), t[2].get(m[1]), t[3].get(m[2])]
}

/// The six hue and saturation curves, with 0.5 meaning no change.
#[inline]
fn colour_curves(t: &[Table; 6], c: [f32; 3]) -> [f32; 3] {
    let [h, s, v] = color::to_hsv(c);
    let hue = h / 360.0;
    let l = luma(c).clamp(0.0, 1.0);
    let h2 = h + (t[0].get(hue) - 0.5) * HUE_REACH;
    let s2 = s * t[1].get(hue) * 2.0 * t[3].get(l) * 2.0 * t[4].get(s) * 2.0;
    let v2 = v * (1.0 + (t[2].get(hue) - 0.5) * LIGHT_REACH) * (1.0 + (t[5].get(s) - 0.5) * LIGHT_REACH);
    color::from_hsv([h2, s2.clamp(0.0, 1.0), v2])
}

/// Saturation worked out on a log scale, so strong colours compress instead of clipping.
#[inline]
fn log_saturation(c: [f32; 3], amount: f32) -> [f32; 3] {
    const SQUEEZE: f32 = 8.0;
    let top = (1.0 + SQUEEZE).ln();
    let enc = |v: f32| (1.0 + v.max(0.0) * SQUEEZE).ln();
    let l = enc(luma(c));
    c.map(|v| {
        let e = mix(l, enc(v), amount);
        (e.clamp(0.0, top + 0.2).exp() - 1.0) / SQUEEZE
    })
}

/// Removes the veil of scattered light, using the dark channel as a measure of haze.
fn dehaze(px: &mut [[f32; 3]], w: usize, h: usize, amount: f32, unit: f32) {
    const KEEP: f32 = 0.95;
    const MIN_LIGHT: f32 = 0.2;
    let dark: Vec<[f32; 3]> = px.par_iter().map(|c| [c[0].min(c[1]).min(c[2]); 3]).collect();
    let veil = soft_blur(&dark, w, h, 24.0 * unit);
    let air = veil.iter().map(|v| v[0]).fold(0.0f32, f32::max).clamp(0.5, 1.0);
    px.par_iter_mut().zip(veil.par_iter()).for_each(|(c, v)| {
        if amount > 0.0 {
            let t = (1.0 - KEEP * amount * v[0] / air).max(MIN_LIGHT);
            *c = c.map(|x| (x - air) / t + air);
        } else {
            *c = c.map(|x| x + (air - x) * -amount * 0.45);
        }
    });
}

/// Lifts or lowers a colour by a light amount, mostly keeping its hue.
#[inline]
fn lift(c: [f32; 3], d: f32) -> [f32; 3] {
    let l = luma(c).clamp(0.0, 1.0);
    let k = (l + d).max(0.0) / l.max(0.02);
    mix3([c[0] + d, c[1] + d, c[2] + d], [c[0] * k, c[1] * k, c[2] * k], 0.7)
}

/// Three box passes on a shrunken copy, enough for wide glows and local contrast.
pub(crate) fn soft_blur(px: &[[f32; 3]], w: usize, h: usize, radius: f32) -> Vec<[f32; 3]> {
    let down = ((radius / 3.0).floor() as usize).clamp(1, 16);
    let sw = (w / down).max(1);
    let sh = (h / down).max(1);
    let mut small = vec![[0.0f32; 3]; sw * sh];
    small.par_chunks_mut(sw).enumerate().for_each(|(y, row)| {
        for (x, o) in row.iter_mut().enumerate() {
            *o = px[(x * w / sw) + (y * h / sh) * w];
        }
    });
    let r = ((radius / down as f32).round() as usize).max(1);
    for _ in 0..3 {
        small = box_h(&small, sw, sh, r);
        small = transpose(&small, sw, sh);
        small = box_h(&small, sh, sw, r);
        small = transpose(&small, sh, sw);
    }
    let mut out = vec![[0.0f32; 3]; w * h];
    out.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        let fy = (y as f32 + 0.5) * sh as f32 / h as f32 - 0.5;
        let y0 = (fy.floor().max(0.0) as usize).min(sh - 1);
        let y1 = (y0 + 1).min(sh - 1);
        let ty = (fy - y0 as f32).clamp(0.0, 1.0);
        for (x, o) in row.iter_mut().enumerate() {
            let fx = (x as f32 + 0.5) * sw as f32 / w as f32 - 0.5;
            let x0 = (fx.floor().max(0.0) as usize).min(sw - 1);
            let x1 = (x0 + 1).min(sw - 1);
            let tx = (fx - x0 as f32).clamp(0.0, 1.0);
            let a = mix3(small[x0 + y0 * sw], small[x1 + y0 * sw], tx);
            let b = mix3(small[x0 + y1 * sw], small[x1 + y1 * sw], tx);
            *o = mix3(a, b, ty);
        }
    });
    out
}

fn box_h(px: &[[f32; 3]], w: usize, h: usize, r: usize) -> Vec<[f32; 3]> {
    let mut out = vec![[0.0f32; 3]; w * h];
    let norm = 1.0 / (2 * r + 1) as f32;
    out.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        let src = &px[y * w..(y + 1) * w];
        let mut sum = [0.0f32; 3];
        for i in 0..=(2 * r) {
            let p = src[i.saturating_sub(r).min(w - 1)];
            sum[0] += p[0];
            sum[1] += p[1];
            sum[2] += p[2];
        }
        for (x, o) in row.iter_mut().enumerate() {
            *o = [sum[0] * norm, sum[1] * norm, sum[2] * norm];
            let add = src[(x + r + 1).min(w - 1)];
            let rem = src[x.saturating_sub(r)];
            for i in 0..3 {
                sum[i] += add[i] - rem[i];
            }
        }
    });
    out
}

fn transpose(px: &[[f32; 3]], w: usize, h: usize) -> Vec<[f32; 3]> {
    let mut out = vec![[0.0f32; 3]; w * h];
    out.par_chunks_mut(h).enumerate().for_each(|(x, col)| {
        for (y, o) in col.iter_mut().enumerate() {
            *o = px[x + y * w];
        }
    });
    out
}

#[inline]
fn hash(x: u32, y: u32) -> f32 {
    let mut h = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 13;
    (h & 0xFFFF) as f32 / 65535.0 - 0.5
}

/// Blended noise so grain has clumps as well as fine detail.
#[inline]
fn grain_at(x: f32, y: f32, size: f32, rough: f32) -> f32 {
    let fx = x / size;
    let fy = y / size;
    let (ix, iy) = (fx.floor(), fy.floor());
    let (tx, ty) = (fx - ix, fy - iy);
    let (ix, iy) = (ix as i32 as u32, iy as i32 as u32);
    let smooth = mix(
        mix(hash(ix, iy), hash(ix + 1, iy), tx),
        mix(hash(ix, iy + 1), hash(ix + 1, iy + 1), tx),
        ty,
    );
    mix(smooth * 1.7, hash(x as u32, y as u32), rough * 0.65)
}

/// The finished picture: crop, then the base edit, then each layer through its masks.
pub fn render(src: &Picture, g: &Grade, bank: &LutBank, full: bool) -> Picture {
    render_framed(src, g, bank, full, true)
}

/// As `render`; while the crop box is being moved the whole photo is shown instead.
pub fn render_framed(src: &Picture, g: &Grade, bank: &LutBank, full: bool, use_rect: bool) -> Picture {
    render_at(src, g, bank, full, use_rect, 0)
}

/// As `render_framed`, at one frame of a clip; effects that change over time use it.
pub fn render_at(src: &Picture, g: &Grade, bank: &LutBank, full: bool, use_rect: bool, frame: u32) -> Picture {
    render_with(src, g, bank, full, use_rect, frame, None)
}

/// As `render_at`, with the colour steps already baked to a table. Playback uses this:
/// a table lookup costs a fraction of working every step out for every pixel.
pub fn render_play(src: &Picture, g: &Grade, table: &Lut3D, bank: &LutBank, frame: u32) -> Picture {
    render_with(src, g, bank, true, true, frame, Some(table))
}

fn render_with(src: &Picture, g: &Grade, bank: &LutBank, full: bool, use_rect: bool, frame: u32, table: Option<&Lut3D>) -> Picture {
    let framed = if g.crop.is_identity(use_rect) {
        None
    } else {
        Some(g.crop.geometry(src.w, src.h, use_rect).apply(src))
    };
    let mut out = grade_picture(framed.as_ref().unwrap_or(src), g, bank, full, frame, table);
    let geo = g.crop.geometry(src.w, src.h, use_rect);
    let aspect = src.w as f32 / src.h as f32;
    for layer in g.layers.iter().filter(|l| l.visible) {
        let weights = masks::layer_weights(layer, &out.px, out.w, &geo, aspect, frame);
        let edited = grade_picture(&out, &layer.grade, bank, full, frame, None);
        out.px
            .par_iter_mut()
            .zip(edited.px.par_iter().zip(weights.par_iter()))
            .for_each(|(c, (e, w))| *c = mix3(*c, *e, w * layer.opacity));
    }
    out
}

/// Red over the part of the picture that one layer's masks let through.
pub fn tint_mask(out: &mut Picture, src: &Picture, g: &Grade, layer: usize, use_rect: bool, frame: u32) {
    let Some(l) = g.layers.get(layer) else { return };
    let geo = g.crop.geometry(src.w, src.h, use_rect);
    let weights = masks::layer_weights(l, &out.px, out.w, &geo, src.w as f32 / src.h as f32, frame);
    out.px
        .par_iter_mut()
        .zip(weights.par_iter())
        .for_each(|(c, w)| *c = mix3(*c, MASK_TINT, w * MASK_TINT_STRENGTH));
}

const MASK_TINT: [f32; 3] = [1.0, 0.12, 0.2];
const MASK_TINT_STRENGTH: f32 = 0.55;

/// Runs every step in the original's order (see MECHANICS.md).
/// True for the full pipeline; tile previews skip the pixel-neighbour effects.
fn grade_picture(src: &Picture, g: &Grade, bank: &LutBank, full: bool, frame: u32, table: Option<&Lut3D>) -> Picture {
    // With a table, every step that looks at one pixel at a time is already in it.
    let step = table.is_none().then(|| ColorStep::new(g, bank));
    let (w, h) = (src.w, src.h);
    let mut px: Vec<[f32; 3]> = match (&step, table) {
        (Some(s), _) => src.px.par_iter().map(|p| s.early(*p)).collect(),
        (None, Some(t)) => src.px.par_iter().map(|p| t.sample(*p)).collect(),
        (None, None) => src.px.clone(),
    };
    let unit = w.max(h) as f32 / 1200.0;
    let (gauge_grain, gauge_soft, gauge_glow) = film::GAUGE_SCALE[g.film.gauge.min(film::GAUGE_SCALE.len() - 1)];
    let f = if g.film.on { g.film.intensity } else { 0.0 };

    if full && g.dehaze != 0.0 {
        dehaze(&mut px, w, h, g.dehaze, unit);
    }

    if full && g.clarity != 0.0 {
        let blur = soft_blur(&px, w, h, 28.0 * unit);
        px.par_iter_mut().zip(blur.par_iter()).for_each(|(c, b)| {
            let d = (luma(*c) - luma(*b)) * g.clarity * 0.9;
            for v in c.iter_mut() {
                *v += d;
            }
        });
    }
    if full && g.structure != 0.0 {
        let blur = soft_blur(&px, w, h, 6.0 * unit);
        px.par_iter_mut().zip(blur.par_iter()).for_each(|(c, b)| {
            let d = (luma(*c) - luma(*b)) * g.structure * 1.5;
            for v in c.iter_mut() {
                *v += d;
            }
        });
    }
    if let Some(s) = &step {
        px.par_iter_mut().for_each(|c| *c = s.late(*c));
    }

    if full && g.diffusion.strength > 0.0 {
        effects::diffuse(&mut px, w, h, &g.diffusion, unit);
    }

    if full && g.bloom_intensity > 0.0 {
        let t = g.bloom_threshold;
        let glow: Vec<[f32; 3]> = px
            .par_iter()
            .map(|c| {
                let e = ((luma(*c) - t) / (1.0 - t).max(0.05)).clamp(0.0, 1.0);
                [c[0] * e, c[1] * e, c[2] * e]
            })
            .collect();
        let blur = soft_blur(&glow, w, h, 36.0 * unit);
        px.par_iter_mut().zip(blur.par_iter()).for_each(|(c, b)| {
            for i in 0..3 {
                c[i] = 1.0 - (1.0 - c[i]) * (1.0 - b[i] * g.bloom_intensity).max(0.0);
            }
        });
    }
    let soft = g.film.softness * gauge_soft * f + if g.super8.on { g.super8.softness * 0.8 } else { 0.0 };
    if full && soft > 0.0 {
        effects::soften(&mut px, w, h, soft, unit);
    }
    if full && g.sharpness != 0.0 {
        let blur = soft_blur(&px, w, h, 1.6 * unit.max(1.0));
        px.par_iter_mut().zip(blur.par_iter()).for_each(|(c, b)| {
            for i in 0..3 {
                c[i] += (c[i] - b[i]) * g.sharpness * 1.6;
            }
        });
    }
    if let Some(s) = &step {
        px.par_iter_mut().for_each(|c| *c = s.fade(*c));
    }

    if full && (g.lens != 0.0 || g.aberration != 0.0) {
        effects::lens(&mut px, w, h, g.lens, g.aberration);
    }
    // The gate only moves between frames, so a still photo stays put.
    let wander = g.gate_weave + g.film.weave * f + if g.super8.on { 0.5 * g.super8.amount } else { 0.0 };
    if full && frame > 0 && wander > 0.0 {
        let (dx, dy) = effects::weave_offset(frame, wander, unit);
        effects::shift(&mut px, w, h, dx, dy);
    }
    if full && g.crt.amount > 0.0 {
        effects::crt(&mut px, w, h, &g.crt, frame);
    }

    let grain = g.grain_intensity + g.film.grain * gauge_grain * f + if g.super8.on { 0.3 * g.super8.amount } else { 0.0 };
    if grain > 0.0 {
        let size = (g.grain_size * unit * if f > 0.0 { gauge_grain.max(1.0) } else { 1.0 }).max(0.75);
        // A new grain pattern for each frame, as each frame is a new piece of film.
        let (ox, oy) = ((frame % 977) as f32 * 37.0, (frame % 883) as f32 * 91.0);
        px.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            for (x, c) in row.iter_mut().enumerate() {
                let n = grain_at(x as f32 + ox, y as f32 + oy, size, g.grain_roughness);
                let l = luma(*c).clamp(0.0, 1.0);
                // Grain shows most in the midtones, as on a film print.
                let amount = grain * 0.24 * (0.35 + 0.65 * 4.0 * l * (1.0 - l));
                for v in c.iter_mut() {
                    *v += n * amount;
                }
            }
        });
    }
    let halation = g.halation_intensity + g.film.halation * f;
    if full && halation > 0.0 {
        let t = g.halation_threshold;
        let glow: Vec<[f32; 3]> = px
            .par_iter()
            .map(|c| {
                let e = ((luma(*c) - t) / (1.0 - t).max(0.05)).clamp(0.0, 1.0);
                [e, e * 0.12, e * 0.03]
            })
            .collect();
        let blur = soft_blur(&glow, w, h, g.halation_glow * 0.5 * unit * if f > 0.0 { gauge_glow } else { 1.0 });
        px.par_iter_mut().zip(blur.par_iter()).for_each(|(c, b)| {
            for i in 0..3 {
                c[i] = 1.0 - (1.0 - c[i]) * (1.0 - b[i] * halation * 1.8).max(0.0);
            }
        });
    }
    if g.vignette != 0.0 {
        px.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            for (x, c) in row.iter_mut().enumerate() {
                let (u, v) = ((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
                let (dx, dy) = (u - 0.5, v - 0.5);
                let r = ((dx * dx + dy * dy) * 2.0).min(1.0);
                let k = (1.0 - g.vignette * r * r * 1.4).max(0.0);
                for i in 0..3 {
                    c[i] *= k;
                }
            }
        });
    }
    let specks = g.film.dust * f + if g.super8.on { g.super8.dust } else { 0.0 };
    if full && specks > 0.0 {
        effects::dust(&mut px, w, h, specks, frame, unit);
    }
    if full && g.super8.on && frame > 0 && g.super8.flicker > 0.0 {
        effects::flicker(&mut px, g.super8.flicker, frame);
    }
    if full && g.texture.amount > 0.0 {
        effects::texture(&mut px, w, h, &g.texture, frame);
    }
    if let Some(s) = step.as_ref().filter(|_| g.print.amount != 0.0) {
        px.par_iter_mut().for_each(|c| *c = s.print(*c));
    }
    if full && g.super8.on && g.super8.frame {
        effects::super8_gate(&mut px, w, h, frame);
    }
    Picture { w, h, px }
}

/// Bars for a chosen framing ratio, as (left, top, right, bottom) in pixels.
pub fn frame_bars(w: f32, h: f32, ratio: f32) -> (f32, f32) {
    if ratio <= 0.0 {
        return (0.0, 0.0);
    }
    let have = w / h;
    if ratio > have {
        (0.0, (h - w / ratio) * 0.5)
    } else {
        ((w - h * ratio) * 0.5, 0.0)
    }
}

pub fn apply_frame(p: &mut Picture, ratio: f32) {
    let (bx, by) = frame_bars(p.w as f32, p.h as f32, ratio);
    let (bx, by) = (bx as usize, by as usize);
    if bx == 0 && by == 0 {
        return;
    }
    let (w, h) = (p.w, p.h);
    for y in 0..h {
        for x in 0..w {
            if y < by || y >= h - by || x < bx || x >= w - bx {
                p.px[x + y * w] = [0.0; 3];
            }
        }
    }
}

/// Turns the colour steps of the current edit into a table for use elsewhere.
pub fn bake_lut(g: &Grade, bank: &LutBank, size: usize) -> Lut3D {
    let step = ColorStep::new(g, bank);
    let mut lut = Lut3D::from_fn(size, |c| c);
    lut.data.par_iter_mut().for_each(|p| *p = step.apply(*p));
    lut
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(g: &Grade) -> ColorStep {
        ColorStep::new(g, &LutBank::default())
    }

    #[test]
    fn neutral_edit_leaves_colours_alone() {
        let s = step(&Grade::default());
        let c = [0.2, 0.5, 0.8];
        assert_eq!(s.apply(c), c);
    }

    #[test]
    fn warmer_temperature_raises_red_over_blue() {
        let s = step(&Grade { temp: 1500.0, ..Grade::default() });
        let c = s.apply([0.5, 0.5, 0.5]);
        assert!(c[0] > 0.5 && c[2] < 0.5);
    }

    #[test]
    fn exposure_one_stop_doubles_linear_light() {
        let s = step(&Grade { exposure: 1.0, ..Grade::default() });
        let c = s.apply([0.5, 0.5, 0.5]);
        assert!((to_linear(c[0]) - to_linear(0.5) * 2.0).abs() < 1e-3);
    }

    #[test]
    fn baked_table_matches_the_colour_steps() {
        let g = Grade { contrast: 1.2, saturation: 0.8, ..Grade::default() };
        let bank = LutBank::default();
        let lut = bake_lut(&g, &bank, 33);
        let want = step(&g).apply([0.25, 0.5, 0.75]);
        let got = lut.sample([0.25, 0.5, 0.75]);
        assert!((want[0] - got[0]).abs() < 0.01);
    }

    fn flat(c: [f32; 3]) -> Picture {
        Picture { w: 40, h: 40, px: vec![c; 1600] }
    }

    #[test]
    fn a_layer_only_changes_what_its_mask_covers() {
        let layer = Layer {
            grade: Grade { brightness: 0.3, ..Grade::default() },
            masks: vec![masks::Mask::new(masks::Shape::Radial)],
            ..Layer::default()
        };
        let g = Grade { layers: vec![layer], ..Grade::default() };
        let out = render(&flat([0.4; 3]), &g, &LutBank::default(), true);
        assert!((out.px[20 + 20 * 40][0] - 0.7).abs() < 0.01);
        assert!((out.px[0][0] - 0.4).abs() < 0.01);
    }

    #[test]
    fn a_hidden_layer_does_nothing() {
        let layer = Layer { visible: false, grade: Grade { brightness: 0.3, ..Grade::default() }, ..Layer::default() };
        let g = Grade { layers: vec![layer], ..Grade::default() };
        let out = render(&flat([0.4; 3]), &g, &LutBank::default(), true);
        assert!((out.px[0][0] - 0.4).abs() < 1e-5);
    }

    #[test]
    fn layer_opacity_blends_the_edit() {
        let layer = Layer { opacity: 0.5, grade: Grade { brightness: 0.4, ..Grade::default() }, ..Layer::default() };
        let g = Grade { layers: vec![layer], ..Grade::default() };
        let out = render(&flat([0.4; 3]), &g, &LutBank::default(), true);
        assert!((out.px[0][0] - 0.6).abs() < 1e-3);
    }

    #[test]
    fn crop_shrinks_the_finished_picture() {
        let g = Grade { crop: Crop { w: 0.5, ..Crop::default() }, ..Grade::default() };
        let out = render(&flat([0.4; 3]), &g, &LutBank::default(), false);
        assert_eq!((out.w, out.h), (20, 40));
    }

    #[test]
    fn sliders_run_before_the_colour_table() {
        // A table that turns everything black must win over a brightness lift.
        let mut bank = LutBank::default();
        bank.luts.insert("black".into(), Arc::new(Lut3D::from_fn(5, |_| [0.0; 3])));
        let g = Grade {
            brightness: 0.4,
            luts: vec![LutRef { id: "black".into(), name: "Black".into(), intensity: 1.0 }],
            ..Grade::default()
        };
        assert_eq!(ColorStep::new(&g, &bank).apply([0.5, 0.5, 0.5]), [0.0; 3]);
    }

    #[test]
    fn every_new_tool_is_silent_until_it_is_used() {
        let out = render(&flat([0.31, 0.5, 0.74]), &Grade::default(), &LutBank::default(), true);
        assert!(out.px.iter().all(|c| (c[0] - 0.31).abs() < 1e-5 && (c[2] - 0.74).abs() < 1e-5));
    }

    #[test]
    fn a_log_clip_looks_normal_once_transformed() {
        let flatness = |p: &Picture| p.px[0][1];
        let g = Grade { cst: Cst { source: color::Source::SLog3, ..Cst::default() }, ..Grade::default() };
        let black = render(&flat([0.094; 3]), &g, &LutBank::default(), true);
        let white = render(&flat([0.6; 3]), &g, &LutBank::default(), true);
        assert!(flatness(&black) < 0.05 && flatness(&white) > 0.75, "{:?} {:?}", flatness(&black), flatness(&white));
    }

    #[test]
    fn exported_tables_carry_the_colour_tools() {
        let g = Grade { technicolor: 0.8, ..Grade::default() };
        let bank = LutBank::default();
        let lut = bake_lut(&g, &bank, 33);
        let want = step(&g).apply([0.4, 0.5, 0.3]);
        let got = lut.sample([0.4, 0.5, 0.3]);
        assert!((want[1] - got[1]).abs() < 0.02);
    }

    #[test]
    fn film_saturation_compresses_rather_than_clips() {
        let o = log_saturation([0.95, 0.6, 0.1], 1.8);
        assert!(o[0] <= 1.3, "{o:?}");
        let same = log_saturation([0.3, 0.5, 0.7], 1.0);
        assert!((same[0] - 0.3).abs() < 1e-4);
    }

    #[test]
    fn dehaze_deepens_a_hazy_picture() {
        let mut p = vec![[0.7, 0.72, 0.75]; 1600];
        p[820] = [0.3, 0.32, 0.35];
        let before = p[820][0];
        dehaze(&mut p, 40, 40, 0.8, 1.0);
        assert!(p[820][0] < before);
    }

    #[test]
    fn grain_changes_with_each_frame() {
        let g = Grade { grain_intensity: 0.8, ..Grade::default() };
        let a = render_at(&flat([0.5; 3]), &g, &LutBank::default(), true, true, 1);
        let b = render_at(&flat([0.5; 3]), &g, &LutBank::default(), true, true, 2);
        assert!(a.px.iter().zip(&b.px).any(|(x, y)| (x[0] - y[0]).abs() > 0.01));
    }

    #[test]
    fn weave_never_moves_a_still() {
        let p = Picture { w: 40, h: 40, px: (0..1600).map(|i| [(i % 40) as f32 / 40.0; 3]).collect() };
        let g = Grade { gate_weave: 1.0, ..Grade::default() };
        let out = render_at(&p, &g, &LutBank::default(), true, true, 0);
        assert!(out.px.iter().zip(&p.px).all(|(a, b)| (a[0] - b[0]).abs() < 1e-5));
    }

    #[test]
    fn the_baked_path_matches_the_full_colour_path() {
        let g = Grade { exposure: 0.4, contrast: 1.2, saturation: 0.8, temp: 500.0, technicolor: 0.3, ..Grade::default() };
        let bank = LutBank::default();
        let p = Picture { w: 32, h: 32, px: (0..1024).map(|i| [(i % 32) as f32 / 31.0, (i / 32) as f32 / 31.0, 0.4]).collect() };
        let table = bake_lut(&g, &bank, 33);
        let (a, b) = (render(&p, &g, &bank, true), render_play(&p, &g, &table, &bank, 0));
        let worst = a.px.iter().zip(&b.px).flat_map(|(x, y)| (0..3).map(move |i| (x[i].clamp(0.0, 1.0) - y[i].clamp(0.0, 1.0)).abs())).fold(0.0, f32::max);
        assert!(worst < 0.03, "{worst}");
    }

    #[test]
    fn every_saved_field_survives_json() {
        let g = Grade { temp: -870.0, fade: 0.3, bleach: 0.4, ..Grade::default() };
        let t = serde_json::to_string(&g).unwrap();
        assert_eq!(serde_json::from_str::<Grade>(&t).unwrap(), g);
    }

    #[test]
    fn presets_saved_before_the_new_tools_still_load() {
        let old = serde_json::from_str::<Grade>("{\"exposure\": 0.5}").unwrap();
        assert_eq!(old.exposure, 0.5);
        assert_eq!(old.density_rgb, [1.0; 3]);
        assert_eq!(old.wheels.lum_mix, 1.0);
    }
}
