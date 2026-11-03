//! What happens to the picture after each clip is graded: transitions between clips,
//! timed effects, and text and stickers laid on top.

use crate::effects::noise01;
use crate::icons::{self, Icon};
use crate::pipeline::Picture;
use crate::store;
use crate::textanim::{Cell, Placed, SPARE_GLYPHS, Unit};
use crate::timeline::{Fx, Wipe};
use rayon::prelude::*;
use resvg::usvg::Options;
use std::path::Path;
use std::sync::{Arc, OnceLock, RwLock};

/// Space between the baselines of lines, in text heights.
const LINE_STEP: f32 = 1.25;
/// Height of the strip one line is drawn on, in text heights.
const LINE_BAND: f32 = 1.6;
/// Room allowed for each letter before trimming, in text heights.
const WIDEST_LETTER: f32 = 1.0;

pub const STICKERS: [Icon; 12] = [
    Icon::Star,
    Icon::Favourite,
    Icon::Sparkles,
    Icon::Sun03,
    Icon::Flash,
    Icon::Droplet,
    Icon::Leaf01,
    Icon::MaskTheater01,
    Icon::Film01,
    Icon::Aperture,
    Icon::Target02,
    Icon::Bulb,
];

const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];

#[inline]
fn luma(c: [f32; 3]) -> f32 {
    c[0] * LUMA[0] + c[1] * LUMA[1] + c[2] * LUMA[2]
}

#[inline]
fn sample(p: &Picture, x: f32, y: f32) -> [f32; 3] {
    let (w, h) = (p.w, p.h);
    let (x, y) = (x.clamp(0.0, (w - 1) as f32), y.clamp(0.0, (h - 1) as f32));
    let (x0, y0) = (x as usize, y as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (tx, ty) = (x - x0 as f32, y - y0 as f32);
    let (a, b, c, d) = (p.px[x0 + y0 * w], p.px[x1 + y0 * w], p.px[x0 + y1 * w], p.px[x1 + y1 * w]);
    std::array::from_fn(|i| (a[i] * (1.0 - tx) + b[i] * tx) * (1.0 - ty) + (c[i] * (1.0 - tx) + d[i] * tx) * ty)
}

/// Rebuilds the picture by looking up where each pixel comes from.
fn remap(p: &mut Picture, from: impl Fn(f32, f32) -> (f32, f32) + Sync) {
    let src = p.px.clone();
    let view = Picture { w: p.w, h: p.h, px: src };
    let w = p.w;
    p.px.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for (x, o) in row.iter_mut().enumerate() {
            let (sx, sy) = from(x as f32, y as f32);
            *o = sample(&view, sx, sy);
        }
    });
}

/// A soft copy: two box passes each way, close to a gaussian. Radius 0 gives it back.
pub fn blur(p: &Picture, r: usize) -> Picture {
    if r == 0 || p.w == 0 || p.h == 0 {
        return p.clone();
    }
    let along = |src: &[[f32; 3]], w: usize, h: usize| {
        let mut out = vec![[0.0f32; 3]; w * h];
        let norm = 1.0 / (r * 2 + 1) as f32;
        out.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            let line = &src[y * w..(y + 1) * w];
            let at = |i: isize| line[i.clamp(0, w as isize - 1) as usize];
            let mut sum = [0.0f32; 3];
            for i in -(r as isize)..=r as isize {
                let c = at(i);
                (0..3).for_each(|n| sum[n] += c[n]);
            }
            for (x, o) in row.iter_mut().enumerate() {
                *o = sum.map(|v| v * norm);
                let (add, off) = (at(x as isize + r as isize + 1), at(x as isize - r as isize));
                (0..3).for_each(|n| sum[n] += add[n] - off[n]);
            }
        });
        out
    };
    let turn = |src: &[[f32; 3]], w: usize, h: usize| {
        let mut out = vec![[0.0f32; 3]; w * h];
        out.par_chunks_mut(h).enumerate().for_each(|(x, col)| {
            for (y, o) in col.iter_mut().enumerate() {
                *o = src[x + y * w];
            }
        });
        out
    };
    let (w, h) = (p.w, p.h);
    let mut px = p.px.clone();
    for _ in 0..2 {
        px = along(&px, w, h);
        px = turn(&along(&turn(&px, w, h), h, w), h, w);
    }
    Picture { w, h, px }
}

/// Turns colours around the grey axis, keeping their brightness.
fn hue_turn(c: [f32; 3], angle: f32) -> [f32; 3] {
    let (s, k) = angle.sin_cos();
    let third = (1.0 - k) / 3.0;
    let q = (1.0f32 / 3.0).sqrt() * s;
    [
        c[0] * (k + third) + c[1] * (third - q) + c[2] * (third + q),
        c[0] * (third + q) + c[1] * (k + third) + c[2] * (third - q),
        c[0] * (third - q) + c[1] * (third + q) + c[2] * (k + third),
    ]
}

fn mixed(c: [f32; 3], d: [f32; 3], k: f32) -> [f32; 3] {
    std::array::from_fn(|i| c[i] + (d[i] - c[i]) * k)
}

/// Mixes the outgoing and incoming pictures, `t` running 0 to 1. Every kind shows
/// exactly the outgoing picture at 0 and exactly the incoming one at 1.
pub fn blend(a: &Picture, b: &Picture, kind: Wipe, t: f32) -> Picture {
    use std::f32::consts::{PI, TAU};
    let (w, h) = (b.w, b.h);
    let t = t.clamp(0.0, 1.0);
    let eased = t * t * (3.0 - 2.0 * t);
    // Strongest in the middle, nothing at either end.
    let swell = (PI * t).sin();
    let (fw, fh) = (w as f32, h as f32);
    let (cx, cy) = (fw * 0.5, fh * 0.5);
    let soft = match kind {
        Wipe::BlurDissolve => {
            let r = (swell * fw * 0.02) as usize;
            Some((blur(a, r), blur(b, r)))
        }
        _ => None,
    };
    let mut px = vec![[0.0f32; 3]; w * h];
    px.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for (x, o) in row.iter_mut().enumerate() {
            let (xf, yf) = (x as f32, y as f32);
            let at = |p: &Picture| p.px[x.min(p.w - 1) + y.min(p.h - 1) * p.w];
            let slide = |sx: f32| if sx < fw { sample(a, sx, yf) } else { sample(b, sx - fw, yf) };
            *o = match kind {
                Wipe::Dissolve => mixed(at(a), at(b), eased),
                Wipe::DipToBlack => {
                    let (c, k) = if t < 0.5 { (at(a), 1.0 - t * 2.0) } else { (at(b), t * 2.0 - 1.0) };
                    c.map(|v| v * k)
                }
                Wipe::Flash => {
                    let (c, k) = if t < 0.5 { (at(a), t * 2.0) } else { (at(b), 2.0 - t * 2.0) };
                    c.map(|v| v + (1.0 - v) * k)
                }
                Wipe::BlurDissolve => {
                    let (sa, sb) = soft.as_ref().expect("made above");
                    mixed(sa.px[x + y * w], sb.px[x + y * w], eased)
                }
                Wipe::WipeLeft => {
                    if xf >= fw * (1.0 - eased) { at(b) } else { at(a) }
                }
                Wipe::WipeRight => {
                    if xf < fw * eased { at(b) } else { at(a) }
                }
                Wipe::WipeUp => {
                    if yf >= fh * (1.0 - eased) { at(b) } else { at(a) }
                }
                Wipe::WipeDown => {
                    if yf < fh * eased { at(b) } else { at(a) }
                }
                Wipe::Diagonal => {
                    if (xf / fw + yf / fh) * 0.5 < eased { at(b) } else { at(a) }
                }
                Wipe::Clock => {
                    let turned = ((xf - cx).atan2(cy - yf) / TAU).rem_euclid(1.0);
                    if turned < eased { at(b) } else { at(a) }
                }
                Wipe::Iris => {
                    let reach = eased * ((cx * cx + cy * cy).sqrt() + 1.0);
                    if ((xf - cx).powi(2) + (yf - cy).powi(2)).sqrt() < reach { at(b) } else { at(a) }
                }
                Wipe::Blinds => {
                    let band = fh / 10.0;
                    if (yf % band) / band < eased { at(b) } else { at(a) }
                }
                Wipe::SlideLeft => slide(xf + fw * eased),
                Wipe::SlideRight => {
                    let sx = xf - fw * eased;
                    if sx >= 0.0 { sample(a, sx, yf) } else { sample(b, sx + fw, yf) }
                }
                Wipe::SlideUp => {
                    let sy = yf + fh * eased;
                    if sy < fh { sample(a, xf, sy) } else { sample(b, xf, sy - fh) }
                }
                Wipe::Cover => {
                    let edge = fw * (1.0 - eased);
                    if xf >= edge { sample(b, xf - edge, yf) } else { at(a) }
                }
                Wipe::WhipPan => {
                    // A fast slide, smeared along the way it moves.
                    const TAPS: usize = 9;
                    let (base, smear) = (xf + fw * eased, swell * fw * 0.12);
                    let mut sum = [0.0f32; 3];
                    for k in 0..TAPS {
                        let c = slide(base + smear * (k as f32 / (TAPS - 1) as f32 - 0.5));
                        (0..3).for_each(|n| sum[n] += c[n]);
                    }
                    sum.map(|v| v / TAPS as f32)
                }
                Wipe::ZoomIn => {
                    let z = 1.0 + eased * 0.6;
                    mixed(sample(a, cx + (xf - cx) / z, cy + (yf - cy) / z), at(b), eased)
                }
                Wipe::ZoomOut => {
                    let z = 1.0 + (1.0 - eased) * 0.6;
                    mixed(at(a), sample(b, cx + (xf - cx) / z, cy + (yf - cy) / z), eased)
                }
                Wipe::CrossZoom => {
                    // Rushes into the old picture and out of the new one.
                    const TAPS: usize = 8;
                    let s = swell * 0.3;
                    let rushed = |p: &Picture| {
                        let mut sum = [0.0f32; 3];
                        for k in 0..TAPS {
                            let z = 1.0 - s * k as f32 / TAPS as f32;
                            let c = sample(p, cx + (xf - cx) * z, cy + (yf - cy) * z);
                            (0..3).for_each(|n| sum[n] += c[n]);
                        }
                        sum.map(|v| v / TAPS as f32)
                    };
                    mixed(rushed(a), rushed(b), eased)
                }
                Wipe::Spin => {
                    let (from, u, turn) = if t < 0.5 { (a, t * 2.0, 1.0) } else { (b, (1.0 - t) * 2.0, -1.0) };
                    let s = u * u * (3.0 - 2.0 * u);
                    let (sin, cos) = (s * PI * 0.5 * turn).sin_cos();
                    let z = 1.0 + s * 0.6;
                    let (dx, dy) = ((xf - cx) / z, (yf - cy) / z);
                    sample(from, cx + dx * cos + dy * sin, cy - dx * sin + dy * cos)
                }
                Wipe::GlitchCut => {
                    let from = if t < 0.5 { a } else { b };
                    let band = (noise01(y as u32 / 16, (t * 24.0) as u32, 3) - 0.5) * fw * 0.1 * swell;
                    let split = fw * 0.012 * swell;
                    let sx = xf + band;
                    [sample(from, sx + split, yf)[0], sample(from, sx, yf)[1], sample(from, sx - split, yf)[2]]
                }
                Wipe::Pixelate => {
                    let from = if t < 0.5 { a } else { b };
                    let block = 1 + (swell * fw / 24.0) as usize;
                    let (bx, by) = ((x / block * block + block / 2).min(w - 1), (y / block * block + block / 2).min(h - 1));
                    from.px[bx.min(from.w - 1) + by.min(from.h - 1) * from.w]
                }
            };
        }
    });
    Picture { w, h, px }
}

/// One timed effect. `t` runs 0 to 1 across the effect; `frame` drives anything random.
pub fn effect(p: &mut Picture, kind: Fx, amount: f32, t: f32, frame: u32) {
    let (w, h) = (p.w, p.h);
    let (fw, fh) = (w as f32, h as f32);
    match kind {
        Fx::BlackWhite => p.px.par_iter_mut().for_each(|c| {
            let l = luma(*c);
            *c = c.map(|v| v + (l - v) * amount);
        }),
        Fx::BlockFlashes => {
            const BLOCKS: u32 = 8;
            let seed = frame / 3;
            p.px.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
                for (x, c) in row.iter_mut().enumerate() {
                    let cell = ((x * BLOCKS as usize / w) as u32, (y * BLOCKS as usize / h) as u32);
                    if noise01(cell.0, cell.1, seed) > 0.8 {
                        *c = c.map(|v| v + (1.0 - v) * 0.75 * amount);
                    }
                }
            });
        }
        Fx::PullIn => {
            let z = 1.0 + t * 0.3 * amount;
            remap(p, move |x, y| (fw * 0.5 + (x - fw * 0.5) / z, fh * 0.5 + (y - fh * 0.5) / z));
        }
        Fx::Shake => {
            let reach = fw * 0.02 * amount;
            let (dx, dy) = ((noise01(frame, 3, 1) - 0.5) * reach, (noise01(frame, 7, 2) - 0.5) * reach);
            let z = 1.0 + 0.04 * amount;
            remap(p, move |x, y| (fw * 0.5 + (x - fw * 0.5) / z + dx, fh * 0.5 + (y - fh * 0.5) / z + dy));
        }
        Fx::Strobe => {
            if frame % 4 < 2 {
                p.px.par_iter_mut().for_each(|c| *c = c.map(|v| v + (1.0 - v) * 0.7 * amount));
            }
        }
        Fx::Glitch => {
            let reach = fw * 0.012 * amount * (0.4 + noise01(frame, 1, 9) * 1.2);
            let src = Picture { w, h, px: p.px.clone() };
            p.px.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
                let band = noise01(y as u32 / 24, frame / 2, 5) - 0.5;
                for (x, c) in row.iter_mut().enumerate() {
                    let s = x as f32 + band * reach * 3.0;
                    c[0] = sample(&src, s + reach, y as f32)[0];
                    c[1] = sample(&src, s, y as f32)[1];
                    c[2] = sample(&src, s - reach, y as f32)[2];
                }
            });
        }
        Fx::FadeIn => {
            let k = 1.0 - (1.0 - t) * amount;
            p.px.par_iter_mut().for_each(|c| *c = c.map(|v| v * k));
        }
        Fx::FadeOut => {
            let k = 1.0 - t * amount;
            p.px.par_iter_mut().for_each(|c| *c = c.map(|v| v * k));
        }
        Fx::Pixelate => {
            let block = (2.0 + amount * fw / 40.0) as usize;
            let src = p.px.clone();
            p.px.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
                let by = (y / block * block + block / 2).min(h - 1);
                for (x, c) in row.iter_mut().enumerate() {
                    *c = src[(x / block * block + block / 2).min(w - 1) + by * w];
                }
            });
        }
        Fx::RgbSplit => {
            let reach = fw * 0.008 * amount * (1.0 + 0.5 * (frame as f32 * 0.7).sin());
            let src = Picture { w, h, px: p.px.clone() };
            p.px.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
                for (x, c) in row.iter_mut().enumerate() {
                    c[0] = sample(&src, x as f32 + reach, y as f32)[0];
                    c[2] = sample(&src, x as f32 - reach, y as f32)[2];
                }
            });
        }
        Fx::Vhs => {
            let src = Picture { w, h, px: p.px.clone() };
            p.px.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
                let jitter = (noise01(y as u32 / 3, frame, 4) - 0.5) * fw * 0.004 * amount;
                let line = if y % 2 == 0 { 1.0 - 0.18 * amount } else { 1.0 };
                for (x, c) in row.iter_mut().enumerate() {
                    let sx = x as f32 + jitter;
                    let mut v = sample(&src, sx, y as f32);
                    v[0] = sample(&src, sx + 3.0 * amount, y as f32)[0];
                    let l = luma(v);
                    let grain = (noise01(x as u32, y as u32, frame) - 0.5) * 0.08 * amount;
                    *c = v.map(|k| ((k + (l - k) * 0.25 * amount) * line + grain).max(0.0));
                }
            });
        }
        Fx::OldFilm => {
            let flicker = 1.0 + (noise01(frame, 1, 7) - 0.5) * 0.12 * amount;
            p.px.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
                for (x, c) in row.iter_mut().enumerate() {
                    let (dx, dy) = (x as f32 / fw - 0.5, y as f32 / fh - 0.5);
                    let edge = 1.0 - 0.45 * amount * (dx * dx + dy * dy) * 2.0;
                    let mut v = mixed(*c, sepia(*c), amount).map(|k| k * flicker * edge);
                    if noise01(x as u32 / 2, frame / 2, 11) > 0.996 {
                        v = v.map(|k| k + 0.25 * amount);
                    }
                    if noise01(x as u32 / 3, y as u32 / 3 + frame * 7, 13) > 0.9993 {
                        v = v.map(|k| k * (1.0 - 0.8 * amount));
                    }
                    *c = v;
                }
            });
        }
        Fx::Invert => p.px.par_iter_mut().for_each(|c| *c = mixed(*c, c.map(|v| 1.0 - v), amount)),
        Fx::Sepia => p.px.par_iter_mut().for_each(|c| *c = mixed(*c, sepia(*c), amount)),
        Fx::Blur => *p = blur(p, (amount * fw * 0.012) as usize),
        Fx::Mirror => {
            p.px.par_chunks_mut(w).for_each(|row| {
                for x in w / 2..w {
                    row[x] = row[w - 1 - x];
                }
            });
        }
        Fx::Kaleidoscope => {
            let src = p.px.clone();
            p.px.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
                let sy = if y < h / 2 { y } else { h - 1 - y };
                for (x, c) in row.iter_mut().enumerate() {
                    let sx = if x < w / 2 { x } else { w - 1 - x };
                    *c = src[sx + sy * w];
                }
            });
        }
        Fx::LightLeak => {
            let (lx, ly, reach) = (fw * (-0.2 + 1.4 * t), fh * 0.3, fw * 0.7);
            const LEAK: [f32; 3] = [1.0, 0.55, 0.2];
            p.px.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
                for (x, c) in row.iter_mut().enumerate() {
                    let d = ((x as f32 - lx).powi(2) + (y as f32 - ly).powi(2)).sqrt() / reach;
                    let k = (1.0 - d).max(0.0).powi(2) * amount * 0.9;
                    *c = std::array::from_fn(|i| 1.0 - (1.0 - c[i]) * (1.0 - LEAK[i] * k));
                }
            });
        }
        Fx::PushOut => {
            let z = 1.0 + (1.0 - t) * 0.3 * amount;
            remap(p, move |x, y| (fw * 0.5 + (x - fw * 0.5) / z, fh * 0.5 + (y - fh * 0.5) / z));
        }
        Fx::Rainbow => {
            let angle = t * std::f32::consts::TAU * amount;
            p.px.par_iter_mut().for_each(|c| *c = hue_turn(*c, angle).map(|v| v.max(0.0)));
        }
        Fx::Glow => {
            let bright = Picture { w, h, px: p.px.iter().map(|c| c.map(|v| (v - 0.55).max(0.0) * 2.0)).collect() };
            let halo = blur(&bright, (fw * 0.02) as usize);
            p.px.par_iter_mut().zip(halo.px.par_iter()).for_each(|(c, g)| *c = std::array::from_fn(|i| c[i] + g[i] * amount * 0.8));
        }
        Fx::Heartbeat => {
            let z = 1.0 + 0.07 * amount * (-((frame % 20) as f32) / 3.0).exp();
            remap(p, move |x, y| (fw * 0.5 + (x - fw * 0.5) / z, fh * 0.5 + (y - fh * 0.5) / z));
        }
        Fx::Duotone => {
            const DARK: [f32; 3] = [0.1, 0.05, 0.35];
            const LIGHT: [f32; 3] = [1.0, 0.6, 0.45];
            p.px.par_iter_mut().for_each(|c| *c = mixed(*c, mixed(DARK, LIGHT, luma(*c).clamp(0.0, 1.0)), amount));
        }
    }
}

fn sepia(c: [f32; 3]) -> [f32; 3] {
    [
        c[0] * 0.393 + c[1] * 0.769 + c[2] * 0.189,
        c[0] * 0.349 + c[1] * 0.686 + c[2] * 0.168,
        c[0] * 0.272 + c[1] * 0.534 + c[2] * 0.131,
    ]
}

/// A picture with its own transparency, ready to lay over another.
#[derive(Clone, Default)]
pub struct Stamp {
    pub w: usize,
    pub h: usize,
    /// Premultiplied colour, then coverage.
    pub px: Vec<[f32; 4]>,
}

fn loaded_fonts() -> &'static RwLock<Arc<Options<'static>>> {
    static OPTIONS: OnceLock<RwLock<Arc<Options<'static>>>> = OnceLock::new();
    OPTIONS.get_or_init(|| {
        let mut o = Options::default();
        let db = o.fontdb_mut();
        db.load_system_fonts();
        db.load_fonts_dir(store::fonts_dir());
        // Text with no font of its own takes the system's usual sans.
        let installed = |n: &str| db.faces().any(|f| f.families.iter().any(|(name, _)| name == n));
        let family = crate::platform::SANS.iter().find(|n| installed(n)).map_or("sans-serif".to_string(), |n| n.to_string());
        if family != "sans-serif" {
            db.set_sans_serif_family(family.clone());
        }
        o.font_family = family;
        RwLock::new(Arc::new(o))
    })
}

pub(crate) fn fonts() -> Arc<Options<'static>> {
    loaded_fonts().read().map_or_else(|e| e.into_inner().clone(), |o| o.clone())
}

/// The face text uses when it names none: Segoe UI, the Mac's own, or a Linux sans.
pub fn default_font() -> String {
    fonts().font_family.clone()
}

/// Every font family on this computer, in alphabetical order.
pub fn font_families() -> Vec<String> {
    let mut names: Vec<String> = fonts().fontdb.faces().filter_map(|f| f.families.first().map(|n| n.0.clone())).filter(|n| !n.starts_with(['@', '.'])).collect();
    names.sort_by_key(|n| n.to_lowercase());
    names.dedup();
    names
}

/// Copies a font file in with the app's own, so it is there next time too. Gives its name.
pub fn add_font(path: &Path) -> Result<String, String> {
    let before = font_families();
    let dir = store::fonts_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let name = path.file_name().ok_or("That is not a font file.")?;
    let kept = dir.join(name);
    std::fs::copy(path, &kept).map_err(|e| e.to_string())?;
    let bytes = std::fs::read(&kept).map_err(|e| e.to_string())?;
    let lock = loaded_fonts();
    let mut slot = lock.write().unwrap_or_else(|e| e.into_inner());
    let mut db = (*slot.fontdb).clone();
    db.load_font_data(bytes);
    let mut o = Options { font_family: slot.font_family.clone(), ..Default::default() };
    o.fontdb = Arc::new(db);
    *slot = Arc::new(o);
    drop(slot);
    font_families().into_iter().find(|f| !before.contains(f)).ok_or_else(|| "That font is already here, or the file could not be read.".to_string())
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

pub(crate) fn stamp_of(map: &resvg::tiny_skia::Pixmap) -> Stamp {
    let (w, h) = (map.width() as usize, map.height() as usize);
    let px = map.data().chunks_exact(4).map(|c| [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, c[3] as f32 / 255.0]).collect();
    Stamp { w, h, px }
}

/// How a text is set.
#[derive(Clone, PartialEq, Debug)]
pub struct Setting {
    pub text: String,
    pub size: f32,
    pub colour: [f32; 3],
    pub font: String,
    pub weight: u16,
    pub italic: bool,
    pub spacing: f32,
    pub outline: f32,
    pub finest: Unit,
    pub scrambles: bool,
}

/// A text cut into pieces, ready to animate. Each piece is drawn in the place the
/// whole line gives it, so spacing is the font's own.
#[derive(Clone, Default)]
pub struct TextCells {
    pub stamps: Vec<Stamp>,
    pub cells: Vec<Cell>,
    /// Stand-in letters for Scramble.
    pub spares: Vec<Stamp>,
}

/// One part of one line, in letters.
struct Piece {
    line: usize,
    from: usize,
    to: usize,
    groups: [usize; 4],
}

fn pieces(lines: &[Vec<char>], finest: Unit) -> Vec<Piece> {
    let (mut word, mut letter) = (0, 0);
    let mut out = Vec::new();
    for (n, l) in lines.iter().enumerate() {
        if l.iter().all(|c| c.is_whitespace()) {
            continue;
        }
        if finest == Unit::Whole || finest == Unit::Line {
            out.push(Piece { line: n, from: 0, to: l.len(), groups: [0, n, 0, 0] });
            continue;
        }
        let mut i = 0;
        while i < l.len() {
            if l[i].is_whitespace() {
                i += 1;
                continue;
            }
            let end = (i..l.len()).find(|j| l[*j].is_whitespace()).unwrap_or(l.len());
            if finest == Unit::Word {
                out.push(Piece { line: n, from: i, to: end, groups: [0, n, word, word] });
            } else {
                for j in i..end {
                    out.push(Piece { line: n, from: j, to: j + 1, groups: [0, n, word, letter] });
                    letter += 1;
                }
            }
            word += 1;
            i = end;
        }
    }
    out
}

/// The room a line is drawn in, before it is trimmed to its ink.
fn band(size: f32) -> (f32, f32) {
    (size * LINE_BAND, size * 1.05)
}

/// Draws one stretch of a line on an empty strip the height of a line. The rest of the
/// line is laid out but left invisible, which keeps the stretch in its true place.
fn strip(line: &[char], from: usize, to: usize, s: &Setting, width: u32) -> Option<Stamp> {
    let (h, baseline) = band(s.size);
    let fill = format!("#{:02x}{:02x}{:02x}", (s.colour[0] * 255.0) as u8, (s.colour[1] * 255.0) as u8, (s.colour[2] * 255.0) as u8);
    let run = |a: usize, b: usize| escape(&line[a..b].iter().collect::<String>());
    let hidden = |t: String| if t.is_empty() { t } else { format!(r#"<tspan fill-opacity="0" stroke-opacity="0">{t}</tspan>"#) };
    // The edge is drawn under the letter, so it never eats into it.
    let edge = if s.outline > 0.0 { format!(r##" stroke="#000000" stroke-width="{}" stroke-linejoin="round" paint-order="stroke""##, s.outline * s.size * 2.0) } else { String::new() };
    let (before, shown, after) = (hidden(run(0, from)), run(from, to), hidden(run(to, line.len())));
    let fallback = default_font();
    let family = if s.font.is_empty() { fallback.as_str() } else { &s.font };
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{}"><text xml:space="preserve" x="{}" y="{baseline}" font-family="{}" font-weight="{}" font-style="{}" letter-spacing="{}" font-size="{}" fill="{fill}"{edge} text-anchor="middle">{before}<tspan>{shown}</tspan>{after}</text></svg>"#,
        h.ceil(),
        width / 2,
        escape(family),
        s.weight,
        if s.italic { "italic" } else { "normal" },
        s.spacing * s.size,
        s.size,
    );
    let tree = resvg::usvg::Tree::from_str(&svg, &fonts()).ok()?;
    let mut map = resvg::tiny_skia::Pixmap::new(width, h.ceil() as u32)?;
    resvg::render(&tree, resvg::tiny_skia::Transform::identity(), &mut map.as_mut());
    Some(stamp_of(&map))
}

/// Trims empty space from the sides. Gives the stamp and how far in it started.
fn trimmed(s: &Stamp) -> Option<(Stamp, usize)> {
    let inked = |x: usize, y: usize| s.px[x + y * s.w][3] > 0.01;
    let left = (0..s.w).find(|x| (0..s.h).any(|y| inked(*x, y)))?;
    let right = (0..s.w).rev().find(|x| (0..s.h).any(|y| inked(*x, y)))?;
    let (nw, mut px) = (right - left + 1, Vec::with_capacity((right - left + 1) * s.h));
    for y in 0..s.h {
        px.extend_from_slice(&s.px[left + y * s.w..=right + y * s.w]);
    }
    Some((Stamp { w: nw, h: s.h, px }, left))
}

/// Cuts a text into the pieces its animations need, and draws them.
pub fn text_cells(s: &Setting) -> Option<TextCells> {
    let lines: Vec<Vec<char>> = s.text.lines().map(|l| l.chars().collect()).collect();
    let longest = lines.iter().map(|l| l.len()).max()?.max(1) as f32;
    let width = ((longest * s.size * (WIDEST_LETTER + s.spacing.max(0.0)) + s.size) as u32).max(8);
    let (line_h, (strip_h, _)) = (s.size * LINE_STEP, band(s.size));
    let list = pieces(&lines, s.finest);
    let drawn: Vec<(Stamp, usize, &Piece)> = list
        .par_iter()
        .filter_map(|p| {
            let (stamp, left) = trimmed(&strip(&lines[p.line], p.from, p.to, s, width)?)?;
            Some((stamp, left, p))
        })
        .collect();
    let left = drawn.iter().map(|d| d.1).min()?;
    let right = drawn.iter().map(|d| d.1 + d.0.w).max()?;
    let (mid_x, tall) = ((left + right) as f32 / 2.0, line_h * (lines.len() - 1) as f32 + strip_h);
    let mut out = TextCells::default();
    for (stamp, x, p) in drawn {
        out.cells.push(Cell {
            x: x as f32 + stamp.w as f32 / 2.0 - mid_x,
            y: line_h * p.line as f32 + strip_h / 2.0 - tall / 2.0,
            w: stamp.w as f32,
            h: stamp.h as f32,
            groups: p.groups,
        });
        out.stamps.push(stamp);
    }
    if s.scrambles {
        let spare = Setting { spacing: 0.0, ..s.clone() };
        let glyphs: Vec<char> = SPARE_GLYPHS.chars().collect();
        out.spares = glyphs.par_iter().filter_map(|g| Some(trimmed(&strip(&[*g], 0, 1, &spare, (s.size * 2.0).max(8.0) as u32)?)?.0)).collect();
    }
    Some(out)
}

/// Softens a stamp. It grows by the blur's reach on every side.
pub fn soften(s: &Stamp, reach: usize) -> Stamp {
    let pad = reach * 2;
    let (w, h) = (s.w + pad * 2, s.h + pad * 2);
    let mut px = vec![[0.0f32; 4]; w * h];
    for y in 0..s.h {
        px[pad + (y + pad) * w..pad + (y + pad) * w + s.w].copy_from_slice(&s.px[y * s.w..(y + 1) * s.w]);
    }
    let along = |src: &[[f32; 4]], w: usize, h: usize| {
        let mut out = vec![[0.0f32; 4]; w * h];
        let norm = 1.0 / (reach * 2 + 1) as f32;
        out.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            let line = &src[y * w..(y + 1) * w];
            let mut sum = [0.0f32; 4];
            for i in 0..=reach * 2 {
                let c = line[i.saturating_sub(reach).min(w - 1)];
                (0..4).for_each(|n| sum[n] += c[n]);
            }
            for (x, o) in row.iter_mut().enumerate() {
                *o = sum.map(|v| v * norm);
                let (add, off) = (line[(x + reach + 1).min(w - 1)], line[x.saturating_sub(reach)]);
                (0..4).for_each(|n| sum[n] += add[n] - off[n]);
            }
        });
        out
    };
    let turn = |src: &[[f32; 4]], w: usize, h: usize| {
        let mut out = vec![[0.0f32; 4]; w * h];
        for y in 0..h {
            for x in 0..w {
                out[y + x * h] = src[x + y * w];
            }
        }
        out
    };
    for _ in 0..2 {
        px = along(&px, w, h);
        px = turn(&along(&turn(&px, w, h), h, w), h, w);
    }
    Stamp { w, h, px }
}

/// Lays a stamp over the picture, scaled, turned and faded. `sunk` slides it down
/// inside its own box, cutting off whatever leaves it, for the Reveal animation.
pub fn draw(p: &mut Picture, s: &Stamp, q: &Placed) {
    if q.scale_x.abs() < 1e-3 || q.scale_y.abs() < 1e-3 || s.w == 0 || s.h == 0 {
        return;
    }
    let (sin, cos) = q.rotate.to_radians().sin_cos();
    let (hw, hh) = (s.w as f32 / 2.0, s.h as f32 / 2.0);
    let (ax, ay) = ((hw * q.scale_x).abs(), (hh * q.scale_y).abs());
    let reach_x = ax * cos.abs() + ay * sin.abs() + 1.0;
    let reach_y = ax * sin.abs() + ay * cos.abs() + 1.0;
    let (x0, x1) = (((q.x - reach_x).floor().max(0.0)) as usize, ((q.x + reach_x).ceil().max(0.0) as usize).min(p.w));
    let (y0, y1) = (((q.y - reach_y).floor().max(0.0)) as usize, ((q.y + reach_y).ceil().max(0.0) as usize).min(p.h));
    if x0 >= x1 || y0 >= y1 {
        return;
    }
    let drop = q.sunk * s.h as f32;
    let w = p.w;
    p.px[y0 * w..y1 * w].par_chunks_mut(w).enumerate().for_each(|(row, line)| {
        let dy = (y0 + row) as f32 + 0.5 - q.y;
        for (x, o) in line.iter_mut().enumerate().take(x1).skip(x0) {
            let dx = x as f32 + 0.5 - q.x;
            let lx = (dx * cos + dy * sin) / q.scale_x + hw;
            let ly = (dy * cos - dx * sin) / q.scale_y + hh;
            if lx < 0.0 || ly < 0.0 || lx >= s.w as f32 || ly >= s.h as f32 {
                continue;
            }
            let c = stamp_at(s, lx - 0.5, ly - drop - 0.5);
            let a = c[3] * q.opacity;
            if a > 0.0 {
                for i in 0..3 {
                    o[i] = c[i] * q.opacity + o[i] * (1.0 - a);
                }
            }
        }
    });
}

#[inline]
fn stamp_at(s: &Stamp, x: f32, y: f32) -> [f32; 4] {
    let (fx, fy) = (x.floor(), y.floor());
    let (tx, ty) = (x - fx, y - fy);
    let at = |ix: isize, iy: isize| {
        if ix < 0 || iy < 0 || ix >= s.w as isize || iy >= s.h as isize { [0.0; 4] } else { s.px[ix as usize + iy as usize * s.w] }
    };
    let (ix, iy) = (fx as isize, fy as isize);
    let (a, b, c, d) = (at(ix, iy), at(ix + 1, iy), at(ix, iy + 1), at(ix + 1, iy + 1));
    std::array::from_fn(|i| (a[i] * (1.0 - tx) + b[i] * tx) * (1.0 - ty) + (c[i] * (1.0 - tx) + d[i] * tx) * ty)
}

pub fn sticker_stamp(icon: usize, size: u32) -> Option<Stamp> {
    let img = icons::rasterize(*STICKERS.get(icon)?, size.max(8))?;
    let px = img.pixels.iter().map(|c| [c.r() as f32 / 255.0, c.g() as f32 / 255.0, c.b() as f32 / 255.0, c.a() as f32 / 255.0]).collect();
    Some(Stamp { w: img.size[0], h: img.size[1], px })
}

/// Lays a stamp over the picture with its centre at (x, y), each from 0 to 1.
pub fn place(p: &mut Picture, s: &Stamp, x: f32, y: f32) {
    let left = (x * p.w as f32 - s.w as f32 * 0.5) as isize;
    let top = (y * p.h as f32 - s.h as f32 * 0.5) as isize;
    let w = p.w as isize;
    for sy in 0..s.h as isize {
        let py = top + sy;
        if py < 0 || py >= p.h as isize {
            continue;
        }
        for sx in 0..s.w as isize {
            let px = left + sx;
            if px < 0 || px >= w {
                continue;
            }
            let c = s.px[sx as usize + sy as usize * s.w];
            if c[3] > 0.0 {
                let o = &mut p.px[(px + py * w) as usize];
                for i in 0..3 {
                    o[i] = c[i] + o[i] * (1.0 - c[3]);
                }
            }
        }
    }
}

/// Fits a picture inside a frame without stretching it, black filling what's left.
pub fn letterbox(p: Picture, w: usize, h: usize) -> Picture {
    if p.w == w && p.h == h {
        return p;
    }
    let s = (w as f32 / p.w as f32).min(h as f32 / p.h as f32);
    let (iw, ih) = ((p.w as f32 * s).round().max(1.0) as usize, (p.h as f32 * s).round().max(1.0) as usize);
    let (ox, oy) = ((w - iw.min(w)) / 2, (h - ih.min(h)) / 2);
    let scale = 1.0 / s;
    let mut px = vec![[0.0f32; 3]; w * h];
    px.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        if y < oy || y >= oy + ih {
            return;
        }
        for (x, o) in row.iter_mut().enumerate().skip(ox).take(iw) {
            *o = sample(&p, (x - ox) as f32 * scale, (y - oy) as f32 * scale);
        }
    });
    Picture { w, h, px }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(v: f32) -> Picture {
        Picture { w: 32, h: 18, px: vec![[v; 3]; 32 * 18] }
    }

    #[test]
    fn every_transition_starts_on_the_old_picture_and_ends_on_the_new() {
        let (a, b) = (flat(0.2), flat(0.8));
        for k in Wipe::ALL {
            let early = blend(&a, &b, k, 0.0);
            let late = blend(&a, &b, k, 1.0);
            assert!((early.px[100][0] - 0.2).abs() < 0.03, "{k:?} {:?}", early.px[100]);
            assert!((late.px[100][0] - 0.8).abs() < 0.03, "{k:?} {:?}", late.px[100]);
        }
    }

    fn ramp(across: bool) -> Picture {
        let (w, h) = (48, 27);
        Picture { w, h, px: (0..w * h).map(|i| if across { [(i % w) as f32 / w as f32, 0.3, 0.6] } else { [0.7, (i / w) as f32 / h as f32, 0.1] }).collect() }
    }

    #[test]
    fn every_transition_is_exact_at_both_ends_on_every_pixel() {
        let (a, b) = (ramp(true), ramp(false));
        for k in Wipe::ALL {
            let off = |p: &Picture, q: &Picture| p.px.iter().zip(&q.px).flat_map(|(x, y)| (0..3).map(move |i| (x[i] - y[i]).abs())).fold(0.0f32, f32::max);
            assert!(off(&blend(&a, &b, k, 0.0), &a) < 1e-4, "{k:?} at the start");
            assert!(off(&blend(&a, &b, k, 1.0), &b) < 1e-4, "{k:?} at the end");
        }
    }

    #[test]
    fn every_transition_is_part_way_in_the_middle() {
        let (a, b) = (flat(0.0), flat(1.0));
        for k in Wipe::ALL.into_iter().filter(|k| *k != Wipe::DipToBlack) {
            let m = blend(&a, &b, k, 0.5);
            let mean = m.px.iter().map(|c| c[0]).sum::<f32>() / m.px.len() as f32;
            assert!(mean > 0.02 && mean.is_finite(), "{k:?} {mean}");
        }
    }

    #[test]
    fn a_blur_keeps_the_light_and_softens_edges() {
        let mut p = flat(0.0);
        p.px[16 + 9 * 32] = [1.0; 3];
        let s = blur(&p, 2);
        let sum = |q: &Picture| q.px.iter().map(|c| c[0]).sum::<f32>();
        assert!((sum(&s) - 1.0).abs() < 0.05);
        assert!(s.px[16 + 9 * 32][0] < 0.5 && s.px[17 + 9 * 32][0] > 0.0);
    }

    #[test]
    fn a_full_hue_turn_comes_back() {
        let c = [0.8, 0.3, 0.1];
        let back = hue_turn(c, std::f32::consts::TAU);
        assert!((0..3).all(|i| (back[i] - c[i]).abs() < 1e-4));
        let half = hue_turn(c, std::f32::consts::PI);
        assert!((luma(half) - luma(c)).abs() < 0.08);
    }

    #[test]
    fn effects_at_nothing_leave_the_picture_alone() {
        let p = ramp(true);
        for k in [Fx::Invert, Fx::Sepia, Fx::Duotone, Fx::Rainbow, Fx::Blur, Fx::Glow, Fx::LightLeak] {
            let mut q = p.clone();
            effect(&mut q, k, 0.0, 0.5, 3);
            let off = p.px.iter().zip(&q.px).map(|(x, y)| (x[0] - y[0]).abs()).fold(0.0f32, f32::max);
            assert!(off < 1e-3, "{k:?} {off}");
        }
    }

    #[test]
    fn a_dip_goes_through_black() {
        let m = blend(&flat(0.5), &flat(0.5), Wipe::DipToBlack, 0.5);
        assert!(m.px[50][0] < 0.02);
    }

    #[test]
    fn black_and_white_removes_colour() {
        let mut p = Picture { w: 2, h: 1, px: vec![[0.9, 0.2, 0.1]; 2] };
        effect(&mut p, Fx::BlackWhite, 1.0, 0.5, 0);
        assert!((p.px[0][0] - p.px[0][2]).abs() < 1e-5);
    }

    #[test]
    fn fades_reach_black_at_the_right_end() {
        let (mut a, mut b) = (flat(0.6), flat(0.6));
        effect(&mut a, Fx::FadeIn, 1.0, 0.0, 0);
        effect(&mut b, Fx::FadeOut, 1.0, 1.0, 0);
        assert!(a.px[0][0] < 1e-5 && b.px[0][0] < 1e-5);
    }

    #[test]
    fn pull_in_grows_over_time() {
        let mut p = Picture { w: 40, h: 40, px: (0..1600).map(|i| [(i % 40) as f32 / 40.0; 3]).collect() };
        let was = p.px[2 + 800][0];
        effect(&mut p, Fx::PullIn, 1.0, 1.0, 0);
        assert!(p.px[2 + 800][0] > was);
    }

    #[test]
    fn every_effect_at_nothing_or_full_stays_finite() {
        for k in Fx::ALL {
            let mut p = flat(0.5);
            effect(&mut p, k, 1.0, 0.5, 5);
            assert!(p.px.iter().flatten().all(|v| v.is_finite()), "{k:?}");
        }
    }

    #[test]
    fn stickers_draw_something() {
        for i in 0..STICKERS.len() {
            let s = sticker_stamp(i, 48).unwrap();
            assert!(s.px.iter().any(|c| c[3] > 0.5), "{i}");
        }
        assert!(sticker_stamp(99, 48).is_none());
    }

    fn set(text: &str, finest: Unit) -> Setting {
        Setting { text: text.into(), size: 48.0, colour: [1.0, 0.0, 0.0], font: String::new(), weight: 600, italic: false, spacing: 0.0, outline: 0.0, finest, scrambles: false }
    }

    fn still(c: &Cell, i: usize) -> Placed {
        Placed { cell: i, x: 200.0 + c.x, y: 60.0 + c.y, scale_x: 1.0, scale_y: 1.0, rotate: 0.0, opacity: 1.0, blur: 0.0, sunk: 0.0, spare: None, settled: true }
    }

    fn ink(p: &Picture) -> f32 {
        p.px.iter().map(|c| c[0]).sum()
    }

    fn painted(cells: &TextCells) -> Picture {
        let mut p = Picture { w: 400, h: 120, px: vec![[0.0; 3]; 48000] };
        for (i, c) in cells.cells.iter().enumerate() {
            draw(&mut p, &cells.stamps[i], &still(c, i));
        }
        p
    }

    #[test]
    fn text_draws_and_is_trimmed_to_its_ink() {
        let t = text_cells(&set("Hello", Unit::Line)).unwrap();
        assert_eq!(t.stamps.len(), 1);
        assert!(t.stamps[0].px.iter().any(|c| c[3] > 0.5));
        assert!(t.stamps[0].w < 300, "{}", t.stamps[0].w);
    }

    #[test]
    fn awkward_text_is_handled() {
        assert!(text_cells(&set("a < b & c", Unit::Letter)).is_some());
        assert!(text_cells(&set("\"quoted\" 'text'", Unit::Word)).is_some());
        assert!(text_cells(&set("", Unit::Line)).is_none());
        assert!(text_cells(&set("   ", Unit::Letter)).is_none());
        assert_eq!(text_cells(&set("two\nlines", Unit::Line)).unwrap().stamps.len(), 2);
    }

    #[test]
    fn pieces_are_cut_as_finely_as_asked() {
        let count = |u| text_cells(&set("one two\nthree", u)).unwrap().stamps.len();
        assert_eq!(count(Unit::Line), 2);
        assert_eq!(count(Unit::Word), 3);
        assert_eq!(count(Unit::Letter), 11);
    }

    #[test]
    fn letters_land_exactly_where_the_whole_word_would() {
        let whole = painted(&text_cells(&set("AVATAR type", Unit::Line)).unwrap());
        let cut = painted(&text_cells(&set("AVATAR type", Unit::Letter)).unwrap());
        let off: f32 = whole.px.iter().zip(&cut.px).map(|(a, b)| (a[0] - b[0]).abs()).sum();
        assert!(off < ink(&whole) * 0.05, "{off} of {}", ink(&whole));
    }

    #[test]
    fn letters_know_their_word_and_line() {
        let t = text_cells(&set("ab c\nd", Unit::Letter)).unwrap();
        let g: Vec<[usize; 4]> = t.cells.iter().map(|c| c.groups).collect();
        assert_eq!(g, [[0, 0, 0, 0], [0, 0, 0, 1], [0, 0, 1, 2], [0, 1, 2, 3]]);
    }

    #[test]
    fn a_second_line_sits_below_the_first() {
        let t = text_cells(&set("top\nlow", Unit::Line)).unwrap();
        assert!(t.cells[1].y > t.cells[0].y + 40.0);
    }

    #[test]
    fn weight_italic_and_spacing_change_the_drawing() {
        let plain = text_cells(&set("Weight", Unit::Line)).unwrap();
        let bold = text_cells(&Setting { weight: 900, ..set("Weight", Unit::Line) }).unwrap();
        let wide = text_cells(&Setting { spacing: 0.3, ..set("Weight", Unit::Line) }).unwrap();
        assert!(ink(&painted(&bold)) > ink(&painted(&plain)));
        assert!(wide.stamps[0].w > plain.stamps[0].w + 40);
    }

    #[test]
    fn an_outline_adds_a_dark_edge_outside_the_letters() {
        let plain = text_cells(&Setting { colour: [1.0; 3], ..set("Meme", Unit::Line) }).unwrap();
        let edged = text_cells(&Setting { colour: [1.0; 3], outline: 0.06, ..set("Meme", Unit::Line) }).unwrap();
        assert!(edged.stamps[0].w > plain.stamps[0].w);
        // Dark but solid pixels: the edge.
        assert!(edged.stamps[0].px.iter().any(|c| c[3] > 0.9 && c[0] < 0.1));
        assert!(!plain.stamps[0].px.iter().any(|c| c[3] > 0.9 && c[0] < 0.1));
    }

    #[test]
    fn another_font_draws_differently() {
        let serif = font_families().into_iter().find(|f| f == "Georgia" || f == "Times New Roman");
        let Some(font) = serif else { return };
        let a = painted(&text_cells(&set("Rag", Unit::Line)).unwrap());
        let b = painted(&text_cells(&Setting { font, ..set("Rag", Unit::Line) }).unwrap());
        assert!(a.px.iter().zip(&b.px).any(|(p, q)| (p[0] - q[0]).abs() > 0.5));
    }

    #[test]
    fn the_font_list_is_sorted_and_has_no_repeats() {
        let f = font_families();
        assert!(f.windows(2).all(|w| w[0].to_lowercase() <= w[1].to_lowercase() && w[0] != w[1]));
    }

    #[test]
    fn scramble_gets_stand_in_letters() {
        let t = text_cells(&Setting { scrambles: true, ..set("Hi", Unit::Letter) }).unwrap();
        assert_eq!(t.spares.len(), SPARE_GLYPHS.chars().count());
    }

    fn block() -> Stamp {
        Stamp { w: 40, h: 40, px: vec![[1.0, 1.0, 1.0, 1.0]; 1600] }
    }

    fn drawn(q: Placed) -> Picture {
        let mut p = Picture { w: 100, h: 100, px: vec![[0.0; 3]; 10000] };
        draw(&mut p, &block(), &q);
        p
    }

    fn centred() -> Placed {
        Placed { cell: 0, x: 50.0, y: 50.0, scale_x: 1.0, scale_y: 1.0, rotate: 0.0, opacity: 1.0, blur: 0.0, sunk: 0.0, spare: None, settled: false }
    }

    #[test]
    fn scale_changes_how_much_it_covers() {
        let (one, two) = (ink(&drawn(centred())), ink(&drawn(Placed { scale_x: 2.0, scale_y: 2.0, ..centred() })));
        assert!((two / one - 4.0).abs() < 0.3, "{one} {two}");
    }

    #[test]
    fn opacity_mixes_with_the_picture() {
        let p = drawn(Placed { opacity: 0.5, ..centred() });
        assert!((p.px[50 + 50 * 100][0] - 0.5).abs() < 0.01);
    }

    #[test]
    fn a_turned_square_reaches_its_corners_outward() {
        let p = drawn(Placed { rotate: 45.0, ..centred() });
        assert!(p.px[50 + 22 * 100][0] > 0.5);
        assert!(p.px[22 + 22 * 100][0] < 0.5);
    }

    #[test]
    fn reveal_cuts_off_at_its_own_box() {
        let p = drawn(Placed { sunk: 0.5, ..centred() });
        // The top half of the box is empty; nothing spills under the bottom edge.
        assert!(p.px[50 + 35 * 100][0] < 0.01 && p.px[50 + 60 * 100][0] > 0.9 && p.px[50 + 72 * 100][0] < 0.01);
    }

    #[test]
    fn pieces_off_the_picture_are_skipped() {
        let p = drawn(Placed { x: -500.0, y: 900.0, ..centred() });
        assert_eq!(ink(&p), 0.0);
    }

    #[test]
    fn softening_spreads_without_gaining_or_losing_light() {
        let b = block();
        let s = soften(&b, 4);
        assert!(s.w > b.w);
        let sum = |t: &Stamp| t.px.iter().map(|c| c[3]).sum::<f32>();
        assert!((sum(&s) - sum(&b)).abs() < sum(&b) * 0.02);
    }

    #[test]
    fn a_stamp_lands_centred_and_can_hang_off_the_edge() {
        let mut p = flat(0.0);
        let s = Stamp { w: 4, h: 4, px: vec![[1.0, 1.0, 1.0, 1.0]; 16] };
        place(&mut p, &s, 0.5, 0.5);
        assert_eq!(p.px[16 + 9 * 32][0], 1.0);
        assert_eq!(p.px[0][0], 0.0);
        place(&mut p, &s, 0.0, 0.0);
        assert_eq!(p.px[0][0], 1.0);
    }

    #[test]
    fn a_tall_picture_gets_bars_at_the_sides() {
        let tall = Picture { w: 10, h: 40, px: vec![[1.0; 3]; 400] };
        let out = letterbox(tall, 80, 40);
        assert_eq!((out.w, out.h), (80, 40));
        assert_eq!(out.px[20 * 80][0], 0.0);
        assert_eq!(out.px[40 + 20 * 80][0], 1.0);
    }
}
