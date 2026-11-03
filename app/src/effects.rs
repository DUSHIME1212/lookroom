//! Effects that look at a pixel's neighbours or its place in the frame.
//! Strengths are this app's own; see MECHANICS.md.

use crate::pipeline::soft_blur;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];

#[inline]
fn luma(c: [f32; 3]) -> f32 {
    c[0] * LUMA[0] + c[1] * LUMA[1] + c[2] * LUMA[2]
}

#[inline]
fn screen(a: f32, b: f32) -> f32 {
    1.0 - (1.0 - a) * (1.0 - b).max(0.0)
}

#[inline]
fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a).max(1e-5)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[inline]
pub fn noise01(x: u32, y: u32, seed: u32) -> f32 {
    let mut h = x.wrapping_mul(0x9E37_79B1) ^ y.wrapping_mul(0x85EB_CA77) ^ seed.wrapping_mul(0x27D4_EB2F);
    h ^= h >> 15;
    h = h.wrapping_mul(0xC2B2_AE3D);
    h ^= h >> 13;
    (h & 0xFFFF) as f32 / 65535.0
}

/// Smooth noise over a plane, several sizes at once.
fn clouds(x: f32, y: f32, seed: u32) -> f32 {
    let mut sum = 0.0;
    let (mut scale, mut weight) = (1.0, 0.5);
    for o in 0..4u32 {
        let (fx, fy) = (x * scale, y * scale);
        let (ix, iy) = (fx.floor(), fy.floor());
        let (tx, ty) = (fx - ix, fy - iy);
        let (tx, ty) = (tx * tx * (3.0 - 2.0 * tx), ty * ty * (3.0 - 2.0 * ty));
        let (ix, iy) = (ix as i32 as u32, iy as i32 as u32);
        let n = |a: u32, b: u32| noise01(a, b, seed + o);
        let top = n(ix, iy) * (1.0 - tx) + n(ix + 1, iy) * tx;
        let low = n(ix, iy + 1) * (1.0 - tx) + n(ix + 1, iy + 1) * tx;
        sum += (top * (1.0 - ty) + low * ty) * weight;
        scale *= 2.0;
        weight *= 0.5;
    }
    sum
}

#[inline]
fn sample(px: &[[f32; 3]], w: usize, h: usize, x: f32, y: f32) -> [f32; 3] {
    let x = x.clamp(0.0, (w - 1) as f32);
    let y = y.clamp(0.0, (h - 1) as f32);
    let (x0, y0) = (x as usize, y as usize);
    let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1));
    let (tx, ty) = (x - x0 as f32, y - y0 as f32);
    let (a, b, c, d) = (px[x0 + y0 * w], px[x1 + y0 * w], px[x0 + y1 * w], px[x1 + y1 * w]);
    std::array::from_fn(|i| (a[i] * (1.0 - tx) + b[i] * tx) * (1.0 - ty) + (c[i] * (1.0 - tx) + d[i] * tx) * ty)
}

pub const DIFFUSERS: [&str; 4] = ["GlimmerGlass", "Black Pro-Mist", "Pro-Mist", "CineBloom"];

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug)]
#[serde(default)]
pub struct Diffusion {
    pub filter: usize,
    pub strength: f32,
    pub size: f32,
}

impl Default for Diffusion {
    fn default() -> Self {
        Self { filter: 1, strength: 0.0, size: 1.0 }
    }
}

/// Each filter scatters light differently: how far, from how bright, and in what colour.
pub fn diffuse(px: &mut [[f32; 3]], w: usize, h: usize, d: &Diffusion, unit: f32) {
    let (radius, threshold, lift, tint) = match d.filter {
        0 => (9.0, 0.72, 0.0, [1.0, 1.0, 1.02]),
        1 => (20.0, 0.56, 0.0, [1.0, 0.99, 0.97]),
        2 => (32.0, 0.0, 0.34, [1.0, 1.0, 1.0]),
        _ => (54.0, 0.66, 0.08, [1.0, 0.95, 0.88]),
    };
    let glow: Vec<[f32; 3]> = px
        .par_iter()
        .map(|c| {
            let e = smooth(threshold, (threshold + 0.3).min(1.0), luma(*c));
            [c[0] * e, c[1] * e, c[2] * e]
        })
        .collect();
    let blur = soft_blur(&glow, w, h, radius * d.size * unit);
    let wide = if d.filter == 0 { Some(soft_blur(&glow, w, h, 34.0 * d.size * unit)) } else { None };
    let s = d.strength;
    px.par_iter_mut().enumerate().for_each(|(i, c)| {
        for ch in 0..3 {
            let mut g = blur[i][ch] * tint[ch];
            if let Some(far) = &wide {
                g += far[i][ch] * 0.35;
            }
            c[ch] = screen(c[ch], g * s * 1.1);
            // Mist that catches stray light lifts the blacks as well.
            c[ch] += (0.12 - c[ch] * 0.12).max(0.0) * lift * s;
        }
    });
}

pub fn soften(px: &mut [[f32; 3]], w: usize, h: usize, amount: f32, unit: f32) {
    let blur = soft_blur(px, w, h, amount * 3.6 * unit.max(0.6));
    px.par_iter_mut().zip(blur.par_iter()).for_each(|(c, b)| {
        for i in 0..3 {
            c[i] += (b[i] - c[i]) * amount.min(1.0);
        }
    });
}

/// Barrel or pincushion bend, with red and blue pulled apart toward the corners.
pub fn lens(px: &mut Vec<[f32; 3]>, w: usize, h: usize, bend: f32, fringe: f32) {
    let src = px.clone();
    let (cx, cy) = (w as f32 * 0.5, h as f32 * 0.5);
    let diag = cx * cx + cy * cy;
    let k = bend * 0.45;
    // Barrel bend pulls the frame in; zooming keeps its edges out of sight.
    let fit = if k > 0.0 { 1.0 / (1.0 + k) } else { 1.0 };
    px.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for (x, o) in row.iter_mut().enumerate() {
            let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
            let r2 = (dx * dx + dy * dy) / diag;
            let at = |s: f32, ch: usize| {
                let q = fit * (1.0 + k * r2) * s;
                sample(&src, w, h, cx + dx * q - 0.5, cy + dy * q - 0.5)[ch]
            };
            let spread = fringe * 0.012 * r2;
            *o = [at(1.0 + spread, 0), at(1.0, 1), at(1.0 - spread, 2)];
        }
    });
}

/// A small sideways and up-down wander of the film in the gate.
pub fn weave_offset(frame: u32, amount: f32, unit: f32) -> (f32, f32) {
    let walk = |seed: u32| {
        let t = frame as f32 * 0.42;
        let (i, f) = (t.floor() as u32, t.fract());
        let f = f * f * (3.0 - 2.0 * f);
        noise01(i, 7, seed) * (1.0 - f) + noise01(i + 1, 7, seed) * f - 0.5
    };
    (walk(11) * amount * 7.0 * unit, walk(23) * amount * 11.0 * unit)
}

pub fn shift(px: &mut Vec<[f32; 3]>, w: usize, h: usize, dx: f32, dy: f32) {
    if dx == 0.0 && dy == 0.0 {
        return;
    }
    let src = px.clone();
    px.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for (x, o) in row.iter_mut().enumerate() {
            *o = sample(&src, w, h, x as f32 - dx, y as f32 - dy);
        }
    });
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug)]
#[serde(default)]
pub struct Crt {
    pub amount: f32,
    pub scanlines: f32,
    pub mask: f32,
    pub curve: f32,
    pub glow: f32,
}

impl Default for Crt {
    fn default() -> Self {
        Self { amount: 0.0, scanlines: 0.6, mask: 0.5, curve: 0.35, glow: 0.4 }
    }
}

pub fn crt(px: &mut Vec<[f32; 3]>, w: usize, h: usize, c: &Crt, frame: u32) {
    const LINES: f32 = 240.0;
    let bloom = soft_blur(px, w, h, 3.0 + 10.0 * c.glow);
    let src = px.clone();
    let (cx, cy) = (w as f32 * 0.5, h as f32 * 0.5);
    let k = c.curve * 0.24;
    let period = (h as f32 / LINES).max(2.0);
    let amount = c.amount;
    px.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for (x, o) in row.iter_mut().enumerate() {
            let (u, v) = ((x as f32 + 0.5 - cx) / cx, (y as f32 + 0.5 - cy) / cy);
            let r2 = u * u + v * v;
            let (su, sv) = (u * (1.0 + k * r2), v * (1.0 + k * r2));
            if su.abs() > 1.0 || sv.abs() > 1.0 {
                *o = [0.0; 3];
                continue;
            }
            let (sx, sy) = (su * cx + cx - 0.5, sv * cy + cy - 0.5);
            let mut p = sample(&src, w, h, sx, sy);
            let g = sample(&bloom, w, h, sx, sy);
            for i in 0..3 {
                p[i] = screen(p[i], g[i] * c.glow * 0.6);
            }
            let line = (sy / period * std::f32::consts::TAU).sin() * 0.5 + 0.5;
            let scan = 1.0 - c.scanlines * 0.55 * (1.0 - line) * (1.0 - luma(p) * 0.5);
            let phase = ((sx / (period * 0.5)) as usize) % 3;
            let edge = 1.0 - smooth(0.82, 1.0, su.abs().max(sv.abs())) * 0.6;
            let hiss = (noise01(x as u32, y as u32, frame) - 0.5) * 0.025;
            for i in 0..3 {
                let m = if i == phase { 1.0 + c.mask * 0.28 } else { 1.0 - c.mask * 0.18 };
                p[i] = p[i] * scan * m * edge * 1.06 + hiss;
            }
            let was = src[x + y * w];
            *o = std::array::from_fn(|i| was[i] + (p[i] - was[i]) * amount);
        }
    });
}

pub const TEXTURES: [&str; 6] = ["Paper", "Canvas", "Dust", "Scratches", "Light Leak", "Film Burn"];

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug)]
#[serde(default)]
pub struct Texture {
    pub kind: usize,
    pub amount: f32,
    pub scale: f32,
}

impl Default for Texture {
    fn default() -> Self {
        Self { kind: 0, amount: 0.0, scale: 1.0 }
    }
}

pub fn texture(px: &mut [[f32; 3]], w: usize, h: usize, t: &Texture, frame: u32) {
    let long = w.max(h) as f32;
    let (kind, amount, seed) = (t.kind, t.amount, 3 + frame / 2);
    let unit = long / 1200.0 * t.scale.max(0.1);
    px.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        let fy = y as f32;
        for (x, c) in row.iter_mut().enumerate() {
            let fx = x as f32;
            let (u, v) = (fx / w as f32, fy / h as f32);
            match kind {
                0 => {
                    let f = clouds(fx / (2.4 * unit), fy / (2.4 * unit), 5) * 0.6 + clouds(fx / (46.0 * unit), fy / (46.0 * unit), 8) * 0.4;
                    let m = 1.0 - amount * 0.42 * (1.0 - f * 1.5).clamp(0.0, 1.0);
                    *c = c.map(|a| a * m);
                }
                1 => {
                    let cell = 6.0 * unit;
                    let (a, b) = ((fx / cell * 3.14).sin(), (fy / cell * 3.14).sin());
                    let over = ((fx / cell) as i32 + (fy / cell) as i32) % 2 == 0;
                    let thread = if over { a } else { b };
                    let m = 1.0 + amount * 0.14 * thread + amount * 0.1 * (clouds(fx / (30.0 * unit), fy / (30.0 * unit), 2) - 0.5);
                    *c = c.map(|a| a * m);
                }
                2 => {
                    let f = clouds(fx / (3.0 * unit), fy / (3.0 * unit), seed);
                    let speck = smooth(0.74, 0.8, f) * amount;
                    *c = c.map(|a| screen(a, speck * 0.85));
                }
                3 => {
                    let drift = (clouds(0.0, fy / (260.0 * unit), 4) - 0.5) * 30.0 * unit;
                    let col = ((fx + drift) / (3.0 * unit)) as u32;
                    let on = noise01(col, 1, 9 + frame / 3) > 0.985;
                    let broken = clouds(fx / (90.0 * unit), fy / (18.0 * unit), col) > 0.42;
                    if on && broken {
                        *c = c.map(|a| screen(a, amount * 0.7));
                    }
                }
                4 => {
                    let wave = clouds(v * 2.5, frame as f32 * 0.02, 6);
                    let reach = (1.0 - u * (1.6 + wave)).clamp(0.0, 1.0).powi(2);
                    let leak = [reach, reach * 0.42 * (0.6 + v * 0.6), reach * 0.12];
                    for i in 0..3 {
                        c[i] = screen(c[i], leak[i] * amount);
                    }
                }
                _ => {
                    let edge = (u.min(1.0 - u) * 1.6).min(v.min(1.0 - v) * 2.2);
                    let f = clouds(u * 3.0, v * 3.0 + frame as f32 * 0.03, 12);
                    let burn = (1.0 - edge * 3.4 - f * 0.9 + 0.5).clamp(0.0, 1.0);
                    let glow = [burn, burn * burn * 0.62, burn * burn * burn * 0.2];
                    for i in 0..3 {
                        c[i] = screen(c[i], glow[i] * amount);
                    }
                }
            }
        }
    });
}

/// Specks and hairs caught in the gate; different every frame.
pub fn dust(px: &mut [[f32; 3]], w: usize, h: usize, amount: f32, frame: u32, unit: f32) {
    const SPECKS: f32 = 70.0;
    const HAIRS: f32 = 3.0;
    let count = (amount * SPECKS) as u32;
    let mut dot = |x: f32, y: f32, r: f32, dark: bool| {
        let (x0, x1) = ((x - r).max(0.0) as usize, ((x + r) as usize + 1).min(w));
        let (y0, y1) = ((y - r).max(0.0) as usize, ((y + r) as usize + 1).min(h));
        for py in y0..y1 {
            for px_x in x0..x1 {
                let d = ((px_x as f32 - x).powi(2) + (py as f32 - y).powi(2)).sqrt();
                let a = 1.0 - smooth(r * 0.5, r, d);
                let p = &mut px[px_x + py * w];
                for v in p.iter_mut() {
                    *v = if dark { *v * (1.0 - a * 0.85) } else { *v + (0.92 - *v) * a * 0.8 };
                }
            }
        }
    };
    for i in 0..count {
        let x = noise01(i, 1, frame) * w as f32;
        let y = noise01(i, 2, frame) * h as f32;
        let r = (0.6 + noise01(i, 3, frame).powi(3) * 2.6) * unit.max(0.6);
        dot(x, y, r, i % 3 == 0);
    }
    for i in 0..(amount * HAIRS + noise01(frame, 9, 4)) as u32 {
        let (mut x, mut y) = (noise01(i, 4, frame) * w as f32, noise01(i, 5, frame) * h as f32);
        let mut a = noise01(i, 6, frame) * std::f32::consts::TAU;
        let bend = (noise01(i, 7, frame) - 0.5) * 0.3;
        for _ in 0..(40.0 * unit.max(0.6)) as u32 {
            dot(x, y, 0.7 * unit.max(0.7), true);
            x += a.cos() * 1.2;
            y += a.sin() * 1.2;
            a += bend;
        }
    }
}

/// Exposure drift between frames, from uneven shutters and old lamps.
pub fn flicker(px: &mut [[f32; 3]], amount: f32, frame: u32) {
    let k = 1.0 + (noise01(frame, 3, 17) - 0.5) * amount * 0.22;
    px.par_iter_mut().for_each(|c| *c = c.map(|v| v * k));
}

/// The scanned frame: rounded gate, black surround and a sprocket hole on the left.
pub fn super8_gate(px: &mut [[f32; 3]], w: usize, h: usize, frame: u32) {
    const INSET: (f32, f32) = (0.11, 0.05);
    const CORNER: f32 = 0.05;
    let (fw, fh) = (w as f32, h as f32);
    let long = fw.min(fh);
    let (ix, iy) = (fw * INSET.0, fh * INSET.1);
    let wobble = (noise01(frame, 5, 2) - 0.5) * long * 0.004;
    let (half_x, half_y) = ((fw - ix * 1.4) * 0.5, (fh - iy * 2.0) * 0.5);
    let (cx, cy) = (ix * 1.15 + half_x + wobble, fh * 0.5);
    let r = long * CORNER;
    let hole = (ix * 0.42, fh * 0.5, long * 0.055, long * 0.075);
    px.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        for (x, c) in row.iter_mut().enumerate() {
            let (dx, dy) = ((x as f32 - cx).abs() - half_x + r, (y as f32 - cy).abs() - half_y + r);
            let d = (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt() + dx.max(dy).min(0.0) - r;
            let keep = 1.0 - smooth(-1.5, 1.5, d);
            let (hx, hy) = (x as f32 - hole.0, y as f32 - hole.1);
            let inside = (hx.abs() < hole.2 && hy.abs() < hole.3) as u8 as f32;
            let surround = [0.012 + inside * 0.86, 0.01 + inside * 0.82, 0.008 + inside * 0.7];
            *c = std::array::from_fn(|i| surround[i] + (c[i] - surround[i]) * keep);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(v: f32) -> Vec<[f32; 3]> {
        vec![[v; 3]; 64 * 64]
    }

    #[test]
    fn a_straight_lens_changes_nothing_much() {
        let mut p: Vec<[f32; 3]> = (0..64 * 64).map(|i| [(i % 64) as f32 / 64.0; 3]).collect();
        let was = p.clone();
        lens(&mut p, 64, 64, 0.0, 0.0);
        assert!(p.iter().zip(&was).all(|(a, b)| (a[0] - b[0]).abs() < 0.02));
    }

    #[test]
    fn fringe_moves_red_and_blue_but_not_green() {
        let mut p: Vec<[f32; 3]> = (0..64 * 64).map(|i| [(i % 64) as f32 / 64.0; 3]).collect();
        lens(&mut p, 64, 64, 0.0, 1.0);
        let corner = p[63 + 40 * 64];
        assert!(corner[0] != corner[2]);
    }

    #[test]
    fn diffusion_leaves_black_alone_with_black_mist() {
        let mut p = flat(0.0);
        diffuse(&mut p, 64, 64, &Diffusion { filter: 1, strength: 1.0, size: 1.0 }, 1.0);
        assert!(p.iter().all(|c| c[0] < 1e-4));
    }

    #[test]
    fn plain_mist_lifts_black() {
        let mut p = flat(0.0);
        diffuse(&mut p, 64, 64, &Diffusion { filter: 2, strength: 1.0, size: 1.0 }, 1.0);
        assert!(p[0][0] > 0.01);
    }

    #[test]
    fn crt_darkens_outside_the_curved_screen() {
        let mut p = flat(0.8);
        crt(&mut p, 64, 64, &Crt { amount: 1.0, curve: 1.0, ..Crt::default() }, 0);
        assert!(p[0][0] < 0.05);
        assert!(p[32 + 32 * 64][0] > 0.3);
    }

    #[test]
    fn weave_changes_between_frames_and_stays_small() {
        let (a, b) = (weave_offset(3, 1.0, 1.0), weave_offset(9, 1.0, 1.0));
        assert_ne!(a, b);
        assert!(a.0.abs() < 8.0 && a.1.abs() < 8.0);
    }

    #[test]
    fn dust_marks_differ_from_frame_to_frame() {
        let (mut a, mut b) = (flat(0.5), flat(0.5));
        dust(&mut a, 64, 64, 1.0, 1, 1.0);
        dust(&mut b, 64, 64, 1.0, 2, 1.0);
        assert!(a.iter().zip(&b).any(|(x, y)| (x[0] - y[0]).abs() > 0.1));
    }

    #[test]
    fn the_gate_blacks_out_the_surround() {
        let mut p = flat(0.7);
        super8_gate(&mut p, 64, 64, 0);
        assert!(p[0][0] < 0.05);
        assert!((p[36 + 32 * 64][0] - 0.7).abs() < 0.02);
    }

    #[test]
    fn every_texture_stays_in_range() {
        for kind in 0..TEXTURES.len() {
            let mut p = flat(0.5);
            texture(&mut p, 64, 64, &Texture { kind, amount: 1.0, scale: 1.0 }, 0);
            assert!(p.iter().all(|c| c.iter().all(|v| v.is_finite() && *v >= -0.01 && *v <= 1.5)), "{kind}");
        }
    }
}
