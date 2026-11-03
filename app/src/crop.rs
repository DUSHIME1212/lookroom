//! Cropping: the free rectangle, straightening, quarter turns and flips.
//! Positions on a photo are kept in 0..1 of the untouched photo, so masks stay on
//! the same part of the picture when the crop changes.

use crate::pipeline::Picture;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

/// The straighten slider's reach, in degrees.
pub const STRAIGHTEN_MAX: f32 = 45.0;
/// The crop box can't shrink past this share of the photo.
pub const MIN_SIDE: f32 = 0.05;

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(default)]
pub struct Crop {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub angle: f32,
    /// Clockwise quarter turns.
    pub turns: u8,
    pub flip_h: bool,
    pub flip_v: bool,
    /// Width over height of the box; 0 leaves it free.
    pub ratio: f32,
}

impl Default for Crop {
    fn default() -> Self {
        Self { x: 0.0, y: 0.0, w: 1.0, h: 1.0, angle: 0.0, turns: 0, flip_h: false, flip_v: false, ratio: 0.0 }
    }
}

impl Crop {
    pub fn rect(&self) -> [f32; 4] {
        [self.x, self.y, self.w, self.h]
    }

    pub fn set_rect(&mut self, r: [f32; 4]) {
        [self.x, self.y, self.w, self.h] = r;
    }

    pub fn turn_right(&mut self) {
        self.turns = (self.turns + 1) % 4;
        self.set_rect([1.0 - self.y - self.h, self.x, self.h, self.w]);
        self.ratio = if self.ratio > 0.0 { 1.0 / self.ratio } else { 0.0 };
    }

    pub fn turn_left(&mut self) {
        self.turns = (self.turns + 3) % 4;
        self.set_rect([self.y, 1.0 - self.x - self.w, self.h, self.w]);
        self.ratio = if self.ratio > 0.0 { 1.0 / self.ratio } else { 0.0 };
    }

    pub fn flip_horizontal(&mut self) {
        self.flip_h = !self.flip_h;
        self.x = 1.0 - self.x - self.w;
    }

    pub fn flip_vertical(&mut self) {
        self.flip_v = !self.flip_v;
        self.y = 1.0 - self.y - self.h;
    }

    /// Fits the largest box of `ratio` around the current centre. `fw` and `fh` are
    /// the size of the turned photo.
    pub fn apply_ratio(&mut self, ratio: f32, fw: f32, fh: f32) {
        self.ratio = ratio;
        if ratio <= 0.0 {
            return;
        }
        let (cx, cy) = (self.x + self.w / 2.0, self.y + self.h / 2.0);
        // The share of the photo's height a full-width box of this ratio takes.
        let h_for_w = fw / (ratio * fh);
        let (mut w, mut h) = if h_for_w <= 1.0 { (1.0, h_for_w) } else { (1.0 / h_for_w, 1.0) };
        let room = (cx.min(1.0 - cx) * 2.0 / w).min(cy.min(1.0 - cy) * 2.0 / h).clamp(0.5, 1.0);
        w *= room;
        h *= room;
        self.set_rect([(cx - w / 2.0).clamp(0.0, 1.0 - w), (cy - h / 2.0).clamp(0.0, 1.0 - h), w, h]);
    }

    pub fn geometry(&self, sw: usize, sh: usize, use_rect: bool) -> Geo {
        let (fw, fh) = if self.turns % 2 == 1 { (sh as f32, sw as f32) } else { (sw as f32, sh as f32) };
        let rect = if use_rect { self.rect() } else { [0.0, 0.0, 1.0, 1.0] };
        let a = self.angle.to_radians();
        let (sin, cos) = a.sin_cos();
        // Zooms in just enough that a straightened photo has no empty corners.
        let scale = cos.abs() + sin.abs() * (fw / fh).max(fh / fw);
        Geo {
            fw,
            fh,
            rect,
            sin,
            cos,
            scale,
            turns: self.turns,
            flip_h: self.flip_h,
            flip_v: self.flip_v,
            out_w: ((fw * rect[2]).round() as usize).max(1),
            out_h: ((fh * rect[3]).round() as usize).max(1),
        }
    }

    pub fn is_identity(&self, use_rect: bool) -> bool {
        let plain = self.angle == 0.0 && self.turns == 0 && !self.flip_h && !self.flip_v;
        plain && (!use_rect || self.rect() == [0.0, 0.0, 1.0, 1.0])
    }
}

/// Which sides of the crop box a drag moves.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Sides {
    pub left: bool,
    pub top: bool,
    pub right: bool,
    pub bottom: bool,
}

/// Drags the chosen sides, keeping `ratio` when one is set (0 leaves it free).
/// A drag that would leave the photo with a ratio set is ignored.
pub fn resize_box(r: [f32; 4], s: Sides, dx: f32, dy: f32, ratio: f32, fw: f32, fh: f32) -> [f32; 4] {
    let (mut x0, mut y0, mut x1, mut y1) = (r[0], r[1], r[0] + r[2], r[1] + r[3]);
    if s.left {
        x0 = (x0 + dx).clamp(0.0, x1 - MIN_SIDE);
    }
    if s.right {
        x1 = (x1 + dx).clamp(x0 + MIN_SIDE, 1.0);
    }
    if s.top {
        y0 = (y0 + dy).clamp(0.0, y1 - MIN_SIDE);
    }
    if s.bottom {
        y1 = (y1 + dy).clamp(y0 + MIN_SIDE, 1.0);
    }
    if ratio > 0.0 {
        let k = fw / (ratio * fh);
        if s.left || s.right {
            let h = (x1 - x0) * k;
            if s.top {
                y0 = y1 - h;
            } else if s.bottom {
                y1 = y0 + h;
            } else {
                let c = (y0 + y1) / 2.0;
                y0 = c - h / 2.0;
                y1 = c + h / 2.0;
            }
        } else {
            let w = (y1 - y0) / k;
            let c = (x0 + x1) / 2.0;
            x0 = c - w / 2.0;
            x1 = c + w / 2.0;
        }
        if x0 < 0.0 || y0 < 0.0 || x1 > 1.0 || y1 > 1.0 {
            return r;
        }
    }
    [x0, y0, x1 - x0, y1 - y0]
}

/// Maps between positions on the shown picture and on the untouched photo.
#[derive(Clone)]
pub struct Geo {
    fw: f32,
    fh: f32,
    rect: [f32; 4],
    sin: f32,
    cos: f32,
    scale: f32,
    turns: u8,
    flip_h: bool,
    flip_v: bool,
    pub out_w: usize,
    pub out_h: usize,
}

impl Geo {
    /// From 0..1 on the shown picture to 0..1 on the untouched photo.
    #[inline]
    pub fn to_source(&self, u: f32, v: f32) -> (f32, f32) {
        let a = self.rect[0] + u * self.rect[2];
        let b = self.rect[1] + v * self.rect[3];
        let (px, py) = ((a - 0.5) * self.fw, (b - 0.5) * self.fh);
        let rx = (px * self.cos - py * self.sin) / self.scale;
        let ry = (px * self.sin + py * self.cos) / self.scale;
        let (mut a, mut b) = (rx / self.fw + 0.5, ry / self.fh + 0.5);
        if self.flip_h {
            a = 1.0 - a;
        }
        if self.flip_v {
            b = 1.0 - b;
        }
        match self.turns {
            1 => (b, 1.0 - a),
            2 => (1.0 - a, 1.0 - b),
            3 => (1.0 - b, a),
            _ => (a, b),
        }
    }

    /// The reverse of `to_source`.
    pub fn from_source(&self, sx: f32, sy: f32) -> (f32, f32) {
        let (mut a, mut b) = match self.turns {
            1 => (1.0 - sy, sx),
            2 => (1.0 - sx, 1.0 - sy),
            3 => (sy, 1.0 - sx),
            _ => (sx, sy),
        };
        if self.flip_h {
            a = 1.0 - a;
        }
        if self.flip_v {
            b = 1.0 - b;
        }
        let (rx, ry) = ((a - 0.5) * self.fw * self.scale, (b - 0.5) * self.fh * self.scale);
        let px = rx * self.cos + ry * self.sin;
        let py = -rx * self.sin + ry * self.cos;
        let (a, b) = (px / self.fw + 0.5, py / self.fh + 0.5);
        ((a - self.rect[0]) / self.rect[2], (b - self.rect[1]) / self.rect[3])
    }

    pub fn apply(&self, src: &Picture) -> Picture {
        let (w, h) = (self.out_w, self.out_h);
        let mut px = vec![[0.0f32; 3]; w * h];
        px.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            for (x, o) in row.iter_mut().enumerate() {
                let (u, v) = ((x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32);
                let (sx, sy) = self.to_source(u, v);
                *o = sample(src, sx, sy);
            }
        });
        Picture { w, h, px }
    }
}

fn sample(p: &Picture, u: f32, v: f32) -> [f32; 3] {
    let x = (u * p.w as f32 - 0.5).clamp(0.0, (p.w - 1) as f32);
    let y = (v * p.h as f32 - 0.5).clamp(0.0, (p.h - 1) as f32);
    let (x0, y0) = (x as usize, y as usize);
    let (x1, y1) = ((x0 + 1).min(p.w - 1), (y0 + 1).min(p.h - 1));
    let (tx, ty) = (x - x0 as f32, y - y0 as f32);
    let (a, b) = (p.px[x0 + y0 * p.w], p.px[x1 + y0 * p.w]);
    let (c, d) = (p.px[x0 + y1 * p.w], p.px[x1 + y1 * p.w]);
    let mut o = [0.0; 3];
    for i in 0..3 {
        let top = a[i] + (b[i] - a[i]) * tx;
        let low = c[i] + (d[i] - c[i]) * tx;
        o[i] = top + (low - top) * ty;
    }
    o
}

/// Finds the framing that keeps the most detail and colour, favouring a subject on
/// the thirds. `ratio` is width over height of the picture; 0 keeps the photo's shape.
pub fn auto_crop(p: &Picture, ratio: f32) -> [f32; 4] {
    const GRID: usize = 96;
    let gw = GRID;
    let gh = ((GRID * p.h / p.w).max(4)).min(GRID * 3);
    let at = |x: usize, y: usize| p.px[(x * p.w / gw).min(p.w - 1) + (y * p.h / gh).min(p.h - 1) * p.w];
    let luma = |c: [f32; 3]| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
    // Running totals of interest, so any box can be summed at once.
    let mut sum = vec![0.0f32; (gw + 1) * (gh + 1)];
    let mut col = vec![0.0f32; (gw + 1) * (gh + 1)];
    let mut row = vec![0.0f32; (gw + 1) * (gh + 1)];
    for y in 0..gh {
        for x in 0..gw {
            let c = at(x, y);
            let (r, d) = (at((x + 1).min(gw - 1), y), at(x, (y + 1).min(gh - 1)));
            let edge = (luma(c) - luma(r)).abs() + (luma(c) - luma(d)).abs();
            let sat = c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2]);
            let e = edge * 3.0 + sat * 0.35 + 0.002;
            let i = x + 1 + (y + 1) * (gw + 1);
            let up = i - (gw + 1);
            sum[i] = e + sum[i - 1] + sum[up] - sum[up - 1];
            col[i] = e * x as f32 + col[i - 1] + col[up] - col[up - 1];
            row[i] = e * y as f32 + row[i - 1] + row[up] - row[up - 1];
        }
    }
    let total = sum[gw + gh * (gw + 1)].max(1e-6);
    let boxed = |t: &[f32], x0: usize, y0: usize, x1: usize, y1: usize| {
        t[x1 + y1 * (gw + 1)] - t[x0 + y1 * (gw + 1)] - t[x1 + y0 * (gw + 1)] + t[x0 + y0 * (gw + 1)]
    };
    let shape = if ratio > 0.0 { ratio } else { p.w as f32 / p.h as f32 };
    let h_for_w = (p.w as f32) / (shape * p.h as f32);
    let (fit_w, fit_h) = if h_for_w <= 1.0 { (1.0, h_for_w) } else { (1.0 / h_for_w, 1.0) };
    let mut best = ([0.0, 0.0, fit_w, fit_h], f32::MIN);
    for scale in [1.0f32, 0.92, 0.84, 0.76, 0.68] {
        let (w, h) = (fit_w * scale, fit_h * scale);
        for iy in 0..=10 {
            for ix in 0..=10 {
                let (x, y) = ((1.0 - w) * ix as f32 / 10.0, (1.0 - h) * iy as f32 / 10.0);
                let (x0, y0) = ((x * gw as f32) as usize, (y * gh as f32) as usize);
                let x1 = (((x + w) * gw as f32) as usize).clamp(x0 + 1, gw);
                let y1 = (((y + h) * gh as f32) as usize).clamp(y0 + 1, gh);
                let inside = boxed(&sum, x0, y0, x1, y1).max(1e-6);
                let cx = boxed(&col, x0, y0, x1, y1) / inside / gw as f32;
                let cy = boxed(&row, x0, y0, x1, y1) / inside / gh as f32;
                let (ux, uy) = ((cx - x) / w, (cy - y) / h);
                let third = |t: f32| (t - 1.0 / 3.0).abs().min((t - 2.0 / 3.0).abs());
                let score = inside / total - 0.25 * (third(ux) + third(uy)) - 0.05 * (1.0 - w * h);
                if score > best.1 {
                    best = ([x, y, w, h], score);
                }
            }
        }
    }
    best.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gradient(w: usize, h: usize) -> Picture {
        let px = (0..h)
            .flat_map(|y| (0..w).map(move |x| [x as f32 / (w - 1) as f32, y as f32 / (h - 1) as f32, 0.0]))
            .collect();
        Picture { w, h, px }
    }

    #[test]
    fn positions_map_there_and_back() {
        for turns in 0..4 {
            let c = Crop {
                x: 0.1, y: 0.2, w: 0.6, h: 0.5, angle: 12.0, turns, flip_h: turns % 2 == 0, flip_v: turns == 3,
                ratio: 0.0,
            };
            let g = c.geometry(600, 400, true);
            let (sx, sy) = g.to_source(0.3, 0.7);
            let (u, v) = g.from_source(sx, sy);
            assert!((u - 0.3).abs() < 1e-3 && (v - 0.7).abs() < 1e-3, "turns {turns}: {u} {v}");
        }
    }

    #[test]
    fn crop_keeps_the_chosen_part() {
        let c = Crop { x: 0.5, y: 0.5, w: 0.5, h: 0.5, ..Crop::default() };
        let out = c.geometry(100, 100, true).apply(&gradient(100, 100));
        assert_eq!((out.w, out.h), (50, 50));
        assert!(out.px[0][0] > 0.48 && out.px[0][1] > 0.48);
    }

    #[test]
    fn a_quarter_turn_swaps_the_sides() {
        let mut c = Crop::default();
        c.turn_right();
        let out = c.geometry(120, 80, true).apply(&gradient(120, 80));
        assert_eq!((out.w, out.h), (80, 120));
        // The photo's top-left corner lands at the top right.
        assert!(out.px[79][0] < 0.05 && out.px[79][1] < 0.05);
    }

    #[test]
    fn turning_four_times_restores_the_crop() {
        let mut c = Crop { x: 0.1, y: 0.2, w: 0.3, h: 0.6, ..Crop::default() };
        let before = c.clone();
        for _ in 0..4 {
            c.turn_right();
        }
        assert!((c.x - before.x).abs() < 1e-5 && (c.h - before.h).abs() < 1e-5 && c.turns == 0);
    }

    #[test]
    fn a_flip_mirrors_the_picture() {
        let c = Crop { flip_h: true, ..Crop::default() };
        let out = c.geometry(100, 100, true).apply(&gradient(100, 100));
        assert!(out.px[0][0] > 0.95);
    }

    #[test]
    fn a_ratio_gives_a_box_of_that_shape() {
        let mut c = Crop::default();
        c.apply_ratio(1.0, 1800.0, 1200.0);
        assert!((c.w * 1800.0 - c.h * 1200.0).abs() < 1.0);
    }

    const CORNER: Sides = Sides { left: false, top: false, right: true, bottom: true };

    #[test]
    fn dragging_a_corner_resizes_the_box() {
        let r = resize_box([0.1, 0.1, 0.5, 0.5], CORNER, 0.1, 0.2, 0.0, 100.0, 100.0);
        assert!((r[2] - 0.6).abs() < 1e-5 && (r[3] - 0.7).abs() < 1e-5 && r[0] == 0.1);
    }

    #[test]
    fn the_box_keeps_its_ratio_and_stays_on_the_photo() {
        let r = resize_box([0.1, 0.1, 0.5, 0.5], CORNER, 0.2, 0.0, 1.0, 100.0, 100.0);
        assert!((r[2] - r[3]).abs() < 1e-5);
        // Growing to the right edge would push the bottom off the photo.
        let start = [0.1, 0.3, 0.5, 0.5];
        assert_eq!(resize_box(start, CORNER, 0.45, 0.0, 1.0, 100.0, 100.0), start);
    }

    #[test]
    fn the_box_cannot_collapse() {
        let r = resize_box([0.4, 0.4, 0.2, 0.2], CORNER, -1.0, -1.0, 0.0, 100.0, 100.0);
        assert!(r[2] >= MIN_SIDE - 1e-5 && r[3] >= MIN_SIDE - 1e-5);
    }

    #[test]
    fn straightening_leaves_no_empty_corners() {
        let g = Crop { angle: 20.0, ..Crop::default() }.geometry(600, 400, true);
        for (u, v) in [(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)] {
            let (x, y) = g.to_source(u, v);
            assert!((-0.001..=1.001).contains(&x) && (-0.001..=1.001).contains(&y), "{x} {y}");
        }
    }

    #[test]
    fn auto_crop_keeps_the_busy_part() {
        let mut p = Picture { w: 300, h: 200, px: vec![[0.2; 3]; 60000] };
        for y in 40..90 {
            for x in 200..260 {
                p.px[x + y * 300] = if (x + y) % 2 == 0 { [1.0, 0.1, 0.1] } else { [0.0, 0.0, 1.0] };
            }
        }
        let r = auto_crop(&p, 1.0);
        let (cx, cy) = (230.0 / 300.0, 65.0 / 200.0);
        assert!(r[0] <= cx && cx <= r[0] + r[2] && r[1] <= cy && cy <= r[1] + r[3], "{r:?}");
    }
}
