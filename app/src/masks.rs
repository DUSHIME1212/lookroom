//! Edit layers and the masks that limit them. Each layer holds its own edit and is
//! blended over the picture through its masks. Rules and sources: MECHANICS.md.

use crate::crop::Geo;
use crate::pipeline::Grade;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

/// Long side of the grid that painted and curved masks are drawn on.
const RASTER_SIDE: usize = 640;
/// Colours paler than this have no reliable hue, so the qualifier ignores them.
const HUE_SAT_FLOOR: (f32, f32) = (0.02, 0.1);
const LINEAR_MIN_RAMP: f32 = 0.002;

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug, Default)]
pub enum Shape {
    #[default]
    Linear,
    Radial,
    Rectangle,
    Draw,
    Curve,
    Qualifier,
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug, Default)]
pub enum Combine {
    #[default]
    Add,
    Subtract,
    Intersect,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct Brush {
    pub points: Vec<[f32; 2]>,
    /// As a share of the photo's long side.
    pub radius: f32,
    pub softness: f32,
    pub opacity: f32,
    pub erase: bool,
}

/// A mask's place at one frame of a clip. Tracking makes one for every frame.
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug, Default)]
pub struct Key {
    pub frame: u32,
    pub x: f32,
    pub y: f32,
    pub size_x: f32,
    pub size_y: f32,
    pub angle: f32,
}

impl Key {
    pub fn of(m: &Mask, frame: u32) -> Self {
        Self { frame, x: m.x, y: m.y, size_x: m.size_x, size_y: m.size_y, angle: m.angle }
    }

    fn mix(a: &Key, b: &Key, t: f32) -> Key {
        let m = |x: f32, y: f32| x + (y - x) * t;
        Key {
            frame: a.frame,
            x: m(a.x, b.x),
            y: m(a.y, b.y),
            size_x: m(a.size_x, b.size_x),
            size_y: m(a.size_y, b.size_y),
            angle: m(a.angle, b.angle),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(default)]
pub struct Mask {
    pub shape: Shape,
    pub x: f32,
    pub y: f32,
    pub size_x: f32,
    pub size_y: f32,
    pub angle: f32,
    pub feather: f32,
    pub invert: bool,
    pub combine: Combine,
    pub strokes: Vec<Brush>,
    pub points: Vec<[f32; 2]>,
    pub hue_on: bool,
    pub hue_center: f32,
    pub hue_width: f32,
    pub hue_soft: f32,
    pub sat_on: bool,
    pub sat_low: f32,
    pub sat_high: f32,
    pub sat_soft: f32,
    pub lum_on: bool,
    pub lum_low: f32,
    pub lum_high: f32,
    pub lum_soft: f32,
    /// Sorted by frame. Empty for a mask that stays still.
    pub keys: Vec<Key>,
}

impl Default for Mask {
    fn default() -> Self {
        Self {
            shape: Shape::Linear,
            x: 0.5,
            y: 0.5,
            size_x: 0.25,
            size_y: 0.25,
            angle: 90.0,
            feather: 0.3,
            invert: false,
            combine: Combine::Add,
            strokes: Vec::new(),
            points: Vec::new(),
            hue_on: true,
            hue_center: 120.0,
            hue_width: 60.0,
            hue_soft: 30.0,
            sat_on: false,
            sat_low: 0.1,
            sat_high: 1.0,
            sat_soft: 0.1,
            lum_on: false,
            lum_low: 0.0,
            lum_high: 1.0,
            lum_soft: 0.1,
            keys: Vec::new(),
        }
    }
}

impl Mask {
    pub fn new(shape: Shape) -> Self {
        let angle = if shape == Shape::Linear { 90.0 } else { 0.0 };
        Self { shape, angle, ..Self::default() }
    }

    /// Points along the closed curve through the placed points.
    pub fn outline(&self) -> Vec<[f32; 2]> {
        smooth_loop(&self.points)
    }

    /// Where the mask sits at a frame, blending between the nearest keys.
    pub fn key_at(&self, frame: u32) -> Key {
        let (Some(first), Some(last)) = (self.keys.first(), self.keys.last()) else {
            return Key::of(self, frame);
        };
        if frame <= first.frame {
            return *first;
        }
        if frame >= last.frame {
            return *last;
        }
        let i = self.keys.partition_point(|k| k.frame <= frame);
        let (a, b) = (&self.keys[i - 1], &self.keys[i]);
        Key::mix(a, b, (frame - a.frame) as f32 / (b.frame - a.frame).max(1) as f32)
    }

    /// Adds a key at this frame, or moves the one already there.
    pub fn set_key(&mut self, key: Key) {
        match self.keys.binary_search_by_key(&key.frame, |k| k.frame) {
            Ok(i) => self.keys[i] = key,
            Err(i) => self.keys.insert(i, key),
        }
    }
}

/// Moves a picture's colours toward a reference photo's.
#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct ShotMatch {
    pub reference: String,
    pub from_mean: [f32; 3],
    pub from_spread: [f32; 3],
    pub to_mean: [f32; 3],
    pub to_spread: [f32; 3],
}

#[inline]
fn to_opponent(c: [f32; 3]) -> [f32; 3] {
    let y = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
    [y, c[2] - y, c[0] - y]
}

#[inline]
fn from_opponent(o: [f32; 3]) -> [f32; 3] {
    let (r, b) = (o[2] + o[0], o[1] + o[0]);
    [r, (o[0] - 0.2126 * r - 0.0722 * b) / 0.7152, b]
}

/// Mean and spread of a picture's light and two colour axes.
pub fn colour_stats(px: &[[f32; 3]]) -> ([f32; 3], [f32; 3]) {
    let n = (px.len() / 7 + 1) as f32;
    let mut mean = [0.0f32; 3];
    for p in px.iter().step_by(7) {
        let o = to_opponent(*p);
        for i in 0..3 {
            mean[i] += o[i] / n;
        }
    }
    let mut var = [0.0f32; 3];
    for p in px.iter().step_by(7) {
        let o = to_opponent(*p);
        for i in 0..3 {
            var[i] += (o[i] - mean[i]) * (o[i] - mean[i]) / n;
        }
    }
    (mean, var.map(f32::sqrt))
}

impl ShotMatch {
    pub fn new(reference: String, from: &[[f32; 3]], to: &[[f32; 3]]) -> Self {
        let (from_mean, from_spread) = colour_stats(from);
        let (to_mean, to_spread) = colour_stats(to);
        Self { reference, from_mean, from_spread, to_mean, to_spread }
    }

    #[inline]
    pub fn apply(&self, c: [f32; 3]) -> [f32; 3] {
        let mut o = to_opponent(c);
        for i in 0..3 {
            let gain = (self.to_spread[i] / self.from_spread[i].max(1e-3)).clamp(0.25, 4.0);
            o[i] = (o[i] - self.from_mean[i]) * gain + self.to_mean[i];
        }
        from_opponent(o)
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(default)]
pub struct Layer {
    pub name: String,
    pub visible: bool,
    pub opacity: f32,
    pub grade: Grade,
    pub masks: Vec<Mask>,
}

impl Default for Layer {
    fn default() -> Self {
        Self { name: String::new(), visible: true, opacity: 1.0, grade: Grade::default(), masks: Vec::new() }
    }
}

struct Raster {
    w: usize,
    h: usize,
    px: Vec<f32>,
}

impl Raster {
    fn new(aspect: f32) -> Self {
        let (w, h) = if aspect >= 1.0 {
            (RASTER_SIDE, ((RASTER_SIDE as f32 / aspect) as usize).max(1))
        } else {
            (((RASTER_SIDE as f32 * aspect) as usize).max(1), RASTER_SIDE)
        };
        Self { w, h, px: vec![0.0; w * h] }
    }

    #[inline]
    fn sample(&self, u: f32, v: f32) -> f32 {
        let x = (u * self.w as f32 - 0.5).clamp(0.0, (self.w - 1) as f32);
        let y = (v * self.h as f32 - 0.5).clamp(0.0, (self.h - 1) as f32);
        let (x0, y0) = (x as usize, y as usize);
        let (x1, y1) = ((x0 + 1).min(self.w - 1), (y0 + 1).min(self.h - 1));
        let (tx, ty) = (x - x0 as f32, y - y0 as f32);
        let top = self.px[x0 + y0 * self.w] * (1.0 - tx) + self.px[x1 + y0 * self.w] * tx;
        let low = self.px[x0 + y1 * self.w] * (1.0 - tx) + self.px[x1 + y1 * self.w] * tx;
        top * (1.0 - ty) + low * ty
    }

    fn blur(&mut self, r: usize) {
        if r == 0 {
            return;
        }
        for _ in 0..3 {
            self.px = box_pass(&self.px, self.w, self.h, r);
            let flipped = transpose(&self.px, self.w, self.h);
            let done = box_pass(&flipped, self.h, self.w, r);
            self.px = transpose(&done, self.h, self.w);
        }
    }
}

fn box_pass(px: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    let mut out = vec![0.0; w * h];
    let norm = 1.0 / (2 * r + 1) as f32;
    out.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        let src = &px[y * w..(y + 1) * w];
        let mut sum: f32 = (0..=2 * r).map(|i| src[i.saturating_sub(r).min(w - 1)]).sum();
        for (x, o) in row.iter_mut().enumerate() {
            *o = sum * norm;
            sum += src[(x + r + 1).min(w - 1)] - src[x.saturating_sub(r)];
        }
    });
    out
}

fn transpose(px: &[f32], w: usize, h: usize) -> Vec<f32> {
    let mut out = vec![0.0; w * h];
    for y in 0..h {
        for x in 0..w {
            out[y + x * h] = px[x + y * w];
        }
    }
    out
}

#[inline]
fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a).max(1e-5)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A brush stroke is the distance to its line, so its width stays even; a hard
/// centre gives way to a smooth edge, and `softness` sets how much of the radius fades.
fn paint(r: &mut Raster, b: &Brush) {
    let long = r.w.max(r.h) as f32;
    let radius = (b.radius * long).max(0.5);
    let inner = (radius * (1.0 - b.softness)).min(radius - 1.0);
    let range = (radius - inner).max(0.01);
    let pts: Vec<[f32; 2]> = b.points.iter().map(|p| [p[0] * r.w as f32, p[1] * r.h as f32]).collect();
    let mut cover = vec![0.0f32; r.w * r.h];
    let segments = pts.len().saturating_sub(1).max(1);
    for i in 0..segments.min(pts.len()) {
        let a = pts[i];
        let c = pts[(i + 1).min(pts.len() - 1)];
        let x0 = (a[0].min(c[0]) - radius).floor().max(0.0) as usize;
        let y0 = (a[1].min(c[1]) - radius).floor().max(0.0) as usize;
        let x1 = ((a[0].max(c[0]) + radius).ceil() as usize + 1).min(r.w);
        let y1 = ((a[1].max(c[1]) + radius).ceil() as usize + 1).min(r.h);
        for y in y0..y1 {
            for x in x0..x1 {
                let d = dist_to_segment([x as f32 + 0.5, y as f32 + 0.5], a, c);
                if d > radius {
                    continue;
                }
                let t = ((d - inner) / range).clamp(0.0, 1.0);
                let v = 1.0 - t * t * (3.0 - 2.0 * t);
                let o = &mut cover[x + y * r.w];
                *o = o.max(v);
            }
        }
    }
    for (e, v) in r.px.iter_mut().zip(cover.iter()) {
        let amount = (v * b.opacity).clamp(0.0, 1.0);
        *e = if b.erase { (*e - amount).max(0.0) } else { e.max(amount) };
    }
}

#[inline]
fn dist_to_segment(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let denom = (ab[0] * ab[0] + ab[1] * ab[1]).max(1e-5);
    let t = (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / denom).clamp(0.0, 1.0);
    let (dx, dy) = (p[0] - (a[0] + t * ab[0]), p[1] - (a[1] + t * ab[1]));
    (dx * dx + dy * dy).sqrt()
}

fn smooth_loop(pts: &[[f32; 2]]) -> Vec<[f32; 2]> {
    let n = pts.len();
    if n < 3 {
        return pts.to_vec();
    }
    let mut out = Vec::with_capacity(n * 12);
    for i in 0..n {
        let (p0, p1, p2, p3) = (pts[(i + n - 1) % n], pts[i], pts[(i + 1) % n], pts[(i + 2) % n]);
        for s in 0..12 {
            let t = s as f32 / 12.0;
            let (t2, t3) = (t * t, t * t * t);
            let f = |a: f32, b: f32, c: f32, d: f32| {
                0.5 * (2.0 * b + (c - a) * t + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2 + (3.0 * b - a - 3.0 * c + d) * t3)
            };
            out.push([f(p0[0], p1[0], p2[0], p3[0]), f(p0[1], p1[1], p2[1], p3[1])]);
        }
    }
    out
}

fn fill(r: &mut Raster, outline: &[[f32; 2]]) {
    for y in 0..r.h {
        let sy = (y as f32 + 0.5) / r.h as f32;
        let mut cuts: Vec<f32> = Vec::new();
        for i in 0..outline.len() {
            let (a, b) = (outline[i], outline[(i + 1) % outline.len()]);
            if (a[1] <= sy) != (b[1] <= sy) {
                cuts.push(a[0] + (sy - a[1]) / (b[1] - a[1]) * (b[0] - a[0]));
            }
        }
        cuts.sort_by(f32::total_cmp);
        for pair in cuts.chunks_exact(2) {
            let x0 = (pair[0] * r.w as f32).max(0.0) as usize;
            let x1 = ((pair[1] * r.w as f32).max(0.0) as usize).min(r.w);
            for x in x0..x1 {
                r.px[x + y * r.w] = 1.0;
            }
        }
    }
}

/// A mask readied for one render; painted and curved shapes are drawn out once.
struct Ready<'a> {
    mask: &'a Mask,
    raster: Option<Raster>,
    aspect: f32,
    sin: f32,
    cos: f32,
    at: Key,
    /// Undoes tracking for shapes drawn once: pivot, home, zoom, sine and cosine of the turn.
    carried: Option<([f32; 2], [f32; 2], f32, f32, f32)>,
}

fn ready(mask: &Mask, aspect: f32, frame: u32) -> Ready<'_> {
    let at = mask.key_at(frame);
    let (sin, cos) = at.angle.to_radians().sin_cos();
    let carried = mask.keys.first().filter(|_| matches!(mask.shape, Shape::Draw | Shape::Curve)).map(|k| {
        let (s, c) = (k.angle - at.angle).to_radians().sin_cos();
        ([at.x, at.y], [k.x, k.y], k.size_x / at.size_x.max(1e-4), s, c)
    });
    let raster = match mask.shape {
        Shape::Draw => {
            let mut r = Raster::new(aspect);
            for b in mask.strokes.iter().filter(|b| !b.points.is_empty()) {
                paint(&mut r, b);
            }
            Some(r)
        }
        Shape::Curve => {
            let mut r = Raster::new(aspect);
            if mask.points.len() >= 3 {
                fill(&mut r, &mask.outline());
                r.blur((mask.feather * 40.0) as usize);
            }
            Some(r)
        }
        _ => None,
    };
    Ready { mask, raster, aspect, sin, cos, at, carried }
}

/// Hue in degrees, then saturation and lightness from 0 to 1.
#[inline]
pub fn hsl(c: [f32; 3]) -> (f32, f32, f32) {
    let mx = c[0].max(c[1]).max(c[2]);
    let mn = c[0].min(c[1]).min(c[2]);
    let l = (mx + mn) / 2.0;
    let d = mx - mn;
    if d < 1e-5 {
        return (0.0, 0.0, l);
    }
    let s = (d / (1.0 - (2.0 * l - 1.0).abs()).max(1e-5)).clamp(0.0, 1.0);
    let h = if mx == c[0] {
        ((c[1] - c[2]) / d).rem_euclid(6.0)
    } else if mx == c[1] {
        (c[2] - c[0]) / d + 2.0
    } else {
        (c[0] - c[1]) / d + 4.0
    };
    (h * 60.0, s, l)
}

#[inline]
fn band(v: f32, low: f32, high: f32, soft: f32) -> f32 {
    let s = soft.max(1e-3);
    smoothstep(low - s, low, v) * (1.0 - smoothstep(high, high + s, v))
}

impl Ready<'_> {
    #[inline]
    fn value(&self, u: f32, v: f32, colour: [f32; 3]) -> f32 {
        let m = self.mask;
        let at = &self.at;
        let (dx, dy) = ((u - at.x) * self.aspect, v - at.y);
        // Position along the mask's own turned axes.
        let (ax, ay) = (dx * self.cos + dy * self.sin, dy * self.cos - dx * self.sin);
        let w = match m.shape {
            Shape::Linear => {
                let half = (m.feather * 0.5).max(LINEAR_MIN_RAMP);
                1.0 - smoothstep(-half, half, ax)
            }
            Shape::Radial => {
                let r = ((ax / at.size_x.max(1e-3)).powi(2) + (ay / at.size_y.max(1e-3)).powi(2)).sqrt();
                1.0 - smoothstep((1.0 - m.feather).max(0.0), 1.0 + 1e-3, r)
            }
            Shape::Rectangle => {
                let (qx, qy) = (ax.abs() - at.size_x, ay.abs() - at.size_y);
                let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt();
                let d = outside + qx.max(qy).min(0.0);
                let f = (m.feather * 0.2).max(LINEAR_MIN_RAMP);
                1.0 - smoothstep(-f, f, d)
            }
            Shape::Draw | Shape::Curve => {
                let (su, sv) = match self.carried {
                    Some((pivot, home, zoom, s, c)) => {
                        let (px, py) = ((u - pivot[0]) * self.aspect * zoom, (v - pivot[1]) * zoom);
                        (home[0] + (px * c - py * s) / self.aspect, home[1] + px * s + py * c)
                    }
                    None => (u, v),
                };
                self.raster.as_ref().map_or(0.0, |r| r.sample(su, sv))
            }
            Shape::Qualifier => qualify(m, colour),
        };
        if m.invert { 1.0 - w } else { w }
    }
}

#[inline]
fn qualify(m: &Mask, c: [f32; 3]) -> f32 {
    let (h, s, l) = hsl(c);
    let mut w = 1.0;
    if m.hue_on {
        let d = (h - m.hue_center).rem_euclid(360.0);
        let d = d.min(360.0 - d);
        let half = m.hue_width / 2.0;
        w *= (1.0 - smoothstep(half, half + m.hue_soft.max(0.1), d)) * smoothstep(HUE_SAT_FLOOR.0, HUE_SAT_FLOOR.1, s);
    }
    if m.sat_on {
        w *= band(s, m.sat_low, m.sat_high, m.sat_soft);
    }
    if m.lum_on {
        w *= band(l, m.lum_low, m.lum_high, m.lum_soft);
    }
    w
}

/// How strongly the layer's masks let its edit through at one point.
#[inline]
fn combined(masks: &[Ready<'_>], u: f32, v: f32, colour: [f32; 3]) -> f32 {
    let mut base = None::<f32>;
    for m in masks {
        let child = m.value(u, v, colour);
        base = Some(match (base, m.mask.combine) {
            (None, _) => child,
            (Some(b), Combine::Add) => b.max(child),
            (Some(b), Combine::Subtract) => b * (1.0 - child),
            (Some(b), Combine::Intersect) => b * child,
        });
    }
    base.unwrap_or(1.0)
}

/// One strength for each pixel of the picture. A layer with no masks covers it all.
pub fn layer_weights(layer: &Layer, px: &[[f32; 3]], w: usize, geo: &Geo, aspect: f32, frame: u32) -> Vec<f32> {
    if layer.masks.is_empty() {
        return vec![1.0; px.len()];
    }
    let masks: Vec<Ready<'_>> = layer.masks.iter().map(|m| ready(m, aspect, frame)).collect();
    let mut out = vec![0.0f32; px.len()];
    out.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
        let h = px.len() / w;
        for (x, o) in row.iter_mut().enumerate() {
            let (u, v) = ((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
            let (su, sv) = geo.to_source(u, v);
            *o = combined(&masks, su, sv, px[x + y * w]);
        }
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(m: Mask, u: f32, v: f32) -> f32 {
        combined(&[ready(&m, 1.5, 0)], u, v, [0.5; 3])
    }

    #[test]
    fn radial_is_full_inside_and_empty_outside() {
        let m = Mask::new(Shape::Radial);
        assert!(at(m.clone(), 0.5, 0.5) > 0.99);
        assert!(at(m, 0.05, 0.05) < 0.01);
    }

    #[test]
    fn invert_swaps_inside_and_outside() {
        let m = Mask { invert: true, ..Mask::new(Shape::Radial) };
        assert!(at(m, 0.5, 0.5) < 0.01);
    }

    #[test]
    fn linear_fades_across_its_line() {
        let m = Mask::new(Shape::Linear);
        assert!(at(m.clone(), 0.5, 0.05) > 0.99);
        assert!(at(m.clone(), 0.5, 0.95) < 0.01);
        assert!((at(m, 0.5, 0.5) - 0.5).abs() < 0.01);
    }

    #[test]
    fn rectangle_covers_only_its_box() {
        let m = Mask { feather: 0.0, ..Mask::new(Shape::Rectangle) };
        assert!(at(m.clone(), 0.5, 0.5) > 0.99);
        assert!(at(m.clone(), 0.5, 0.74) > 0.99);
        assert!(at(m, 0.5, 0.76) < 0.01);
    }

    #[test]
    fn a_turned_rectangle_turns_with_its_angle() {
        let m = Mask { feather: 0.0, size_x: 0.3, size_y: 0.05, ..Mask::new(Shape::Rectangle) };
        let turned = Mask { angle: 90.0, ..m.clone() };
        assert!(at(m, 0.5, 0.7) < 0.01);
        assert!(at(turned, 0.5, 0.7) > 0.99);
    }

    #[test]
    fn a_brush_paints_and_the_eraser_removes() {
        let stroke = |erase| Brush { points: vec![[0.2, 0.5], [0.8, 0.5]], radius: 0.05, softness: 0.0, opacity: 1.0, erase };
        let m = Mask { strokes: vec![stroke(false)], ..Mask::new(Shape::Draw) };
        assert!(at(m.clone(), 0.5, 0.5) > 0.9);
        assert!(at(m.clone(), 0.5, 0.9) < 0.01);
        let wiped = Mask { strokes: vec![stroke(false), stroke(true)], ..m };
        assert!(at(wiped, 0.5, 0.5) < 0.01);
    }

    #[test]
    fn a_curve_fills_what_it_encloses() {
        let m = Mask {
            feather: 0.0,
            points: vec![[0.3, 0.3], [0.7, 0.3], [0.7, 0.7], [0.3, 0.7]],
            ..Mask::new(Shape::Curve)
        };
        assert!(at(m.clone(), 0.5, 0.5) > 0.99);
        assert!(at(m, 0.05, 0.05) < 0.01);
    }

    #[test]
    fn the_qualifier_picks_a_hue_and_skips_pale_colours() {
        let m = Mask::new(Shape::Qualifier);
        assert!(qualify(&m, [0.1, 0.8, 0.1]) > 0.99);
        assert!(qualify(&m, [0.8, 0.1, 0.1]) < 0.01);
        assert!(qualify(&m, [0.5, 0.505, 0.5]) < 0.05);
    }

    #[test]
    fn the_qualifier_can_limit_light() {
        let m = Mask { hue_on: false, lum_on: true, lum_low: 0.6, lum_high: 1.0, lum_soft: 0.01, ..Mask::new(Shape::Qualifier) };
        assert!(qualify(&m, [0.9; 3]) > 0.99);
        assert!(qualify(&m, [0.2; 3]) < 0.01);
    }

    #[test]
    fn masks_combine_by_the_rules() {
        let a = Mask { feather: 0.0, size_x: 0.3, size_y: 0.3, ..Mask::new(Shape::Rectangle) };
        let b = Mask { feather: 0.0, x: 0.8, size_x: 0.3, size_y: 0.3, ..Mask::new(Shape::Rectangle) };
        let with = |c| combined(&[ready(&a, 1.0, 0), ready(&Mask { combine: c, ..b.clone() }, 1.0, 0)], 0.6, 0.5, [0.5; 3]);
        // Both shapes cover this point.
        assert!(with(Combine::Add) > 0.99);
        assert!(with(Combine::Intersect) > 0.99);
        assert!(with(Combine::Subtract) < 0.01);
    }

    #[test]
    fn a_layer_without_masks_covers_everything() {
        let g = crate::crop::Crop::default().geometry(4, 4, true);
        let w = layer_weights(&Layer::default(), &[[0.5; 3]; 16], 4, &g, 1.0, 0);
        assert!(w.iter().all(|v| *v == 1.0));
    }

    #[test]
    fn masks_stay_on_the_photo_when_it_is_cropped() {
        let crop = crate::crop::Crop { x: 0.5, w: 0.5, ..Default::default() };
        let g = crop.geometry(100, 100, true);
        let layer = Layer { masks: vec![Mask { x: 0.75, ..Mask::new(Shape::Radial) }], ..Default::default() };
        let px = vec![[0.5; 3]; 5000];
        let w = layer_weights(&layer, &px, 50, &g, 1.0, 0);
        // The photo's 0.75 is now the middle of the shown picture.
        assert!(w[25 + 50 * 50] > 0.99);
    }

    #[test]
    fn keys_move_a_mask_between_frames() {
        let mut m = Mask::new(Shape::Radial);
        m.set_key(Key { frame: 10, x: 0.2, ..Key::of(&m, 10) });
        m.set_key(Key { frame: 20, x: 0.6, ..Key::of(&m, 20) });
        assert!((m.key_at(15).x - 0.4).abs() < 1e-5);
        assert!((m.key_at(0).x - 0.2).abs() < 1e-5);
        assert!((m.key_at(99).x - 0.6).abs() < 1e-5);
        let w = |f: u32, u: f32| combined(&[ready(&m, 1.0, f)], u, 0.5, [0.5; 3]);
        assert!(w(10, 0.2) > 0.99 && w(20, 0.2) < 0.5 && w(20, 0.6) > 0.99);
    }

    #[test]
    fn a_drawn_mask_travels_with_its_keys() {
        let mut m = Mask {
            feather: 0.0,
            x: 0.3,
            points: vec![[0.2, 0.4], [0.4, 0.4], [0.4, 0.6], [0.2, 0.6]],
            ..Mask::new(Shape::Curve)
        };
        m.set_key(Key { frame: 0, ..Key::of(&m, 0) });
        m.set_key(Key { frame: 10, x: 0.7, ..Key::of(&m, 10) });
        let w = |f: u32, u: f32| combined(&[ready(&m, 1.0, f)], u, 0.5, [0.5; 3]);
        assert!(w(0, 0.3) > 0.9 && w(0, 0.7) < 0.1);
        assert!(w(10, 0.7) > 0.9 && w(10, 0.3) < 0.1);
    }

    #[test]
    fn setting_a_key_twice_replaces_it() {
        let mut m = Mask::new(Shape::Radial);
        m.set_key(Key { frame: 4, x: 0.1, ..Key::default() });
        m.set_key(Key { frame: 4, x: 0.2, ..Key::default() });
        assert_eq!(m.keys.len(), 1);
        assert_eq!(m.keys[0].x, 0.2);
    }

    #[test]
    fn shot_match_moves_colours_to_the_reference() {
        let from = vec![[0.2, 0.3, 0.5]; 700];
        let to: Vec<[f32; 3]> = (0..700).map(|i| [0.7 + (i % 2) as f32 * 0.1, 0.5, 0.2]).collect();
        let s = ShotMatch::new("ref".into(), &from, &to);
        let out = s.apply([0.2, 0.3, 0.5]);
        let (mean, _) = colour_stats(&to);
        let o = to_opponent(out);
        assert!((o[0] - mean[0]).abs() < 0.02 && (o[2] - mean[2]).abs() < 0.02, "{o:?} {mean:?}");
    }

    #[test]
    fn opponent_colours_convert_back() {
        let c = [0.3, 0.6, 0.9];
        let back = from_opponent(to_opponent(c));
        assert!((0..3).all(|i| (back[i] - c[i]).abs() < 1e-5));
    }
}
