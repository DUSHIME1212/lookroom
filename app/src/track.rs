//! Motion tracking: follows patches of the picture from frame to frame and fits the
//! pan, tilt, zoom and rotate that best explain how they moved.

use crate::masks::{Key, Mask};
use crate::pipeline::Picture;
use crate::video::{self, Info};
use rayon::prelude::*;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;

/// Long side of the frames the tracker looks at.
const SIDE: usize = 640;
const PATCH: i32 = 7;
const SEARCH: i32 = 14;
/// Frames between fresh anchors. Long enough to stop drift, short enough to follow change.
const ANCHOR_SPAN: u32 = 16;
/// How long the subject may stay hidden before tracking gives up.
const MAX_COAST: u32 = 48;
/// Share of an anchor's points that must match for a frame to count as seen.
const SEEN_SHARE: usize = 3;
const MIN_SEEN: usize = 3;
/// Frames held in memory at a time when walking backward through a clip.
const BACK_CHUNK: u32 = 48;
/// How much of each new frame's speed goes into the running average.
const SPEED_MIX: f32 = 0.3;
const GRID: usize = 7;
const MAX_POINTS: usize = 24;
/// Patches flatter than this have nothing to lock on to.
const MIN_TEXTURE: f32 = 0.0015;
const FIT_PASSES: usize = 3;
const OUTLIER_SCALE: f32 = 2.5;
/// Zoom and rotation need points spread around, or noise reads as a real move.
const MIN_POINTS_FOR_SHAPE: usize = 6;
/// Moves this small between frames are noise; letting them add up makes a mask drift.
const ZOOM_QUIET: f32 = 0.0015;
const TURN_QUIET: f32 = 0.08;
/// No real subject scales or turns this far between two anchors.
const ZOOM_LIMIT: f32 = 0.3;
const TURN_LIMIT: f32 = 30.0;

pub struct Gray {
    pub w: usize,
    pub h: usize,
    pub px: Vec<f32>,
}

impl Gray {
    /// Lightly smoothed, so sensor noise and film grain don't read as texture.
    pub fn of(p: &Picture) -> Self {
        let (w, h) = (p.w, p.h);
        let raw: Vec<f32> = p.px.iter().map(|c| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]).collect();
        let at = |x: isize, y: isize| {
            let (x, y) = (x.clamp(0, w as isize - 1) as usize, y.clamp(0, h as isize - 1) as usize);
            raw[x + y * w]
        };
        let mut px = vec![0.0; w * h];
        px.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            for (x, o) in row.iter_mut().enumerate() {
                let (x, y) = (x as isize, y as isize);
                let side = at(x - 1, y) + at(x + 1, y) + at(x, y - 1) + at(x, y + 1);
                let corner = at(x - 1, y - 1) + at(x + 1, y - 1) + at(x - 1, y + 1) + at(x + 1, y + 1);
                *o = (at(x, y) * 4.0 + side * 2.0 + corner) / 16.0;
            }
        });
        Self { w, h, px }
    }

    #[inline]
    fn at(&self, x: i32, y: i32) -> f32 {
        let (x, y) = (x.clamp(0, self.w as i32 - 1) as usize, y.clamp(0, self.h as i32 - 1) as usize);
        self.px[x + y * self.w]
    }
}

/// How unevenly a patch is lit: an empty sky scores near zero.
fn texture(f: &Gray, cx: i32, cy: i32) -> f32 {
    let (mut sum, mut sq, mut n) = (0.0, 0.0, 0.0);
    for y in -PATCH..=PATCH {
        for x in -PATCH..=PATCH {
            let v = f.at(cx + x, cy + y);
            sum += v;
            sq += v * v;
            n += 1.0;
        }
    }
    sq / n - (sum / n) * (sum / n)
}

/// The best-textured spots inside a region, spread across a grid.
pub fn pick(f: &Gray, region: [f32; 4]) -> Vec<[f32; 2]> {
    let [x0, y0, x1, y1] = region;
    let mut found = Vec::new();
    for gy in 0..GRID {
        for gx in 0..GRID {
            let (fx, fy) = ((gx as f32 + 0.5) / GRID as f32, (gy as f32 + 0.5) / GRID as f32);
            let (cx, cy) = (x0 + (x1 - x0) * fx, y0 + (y1 - y0) * fy);
            let mut best = (0.0, cx, cy);
            for dy in [-3, 0, 3] {
                for dx in [-3, 0, 3] {
                    let t = texture(f, cx as i32 + dx, cy as i32 + dy);
                    if t > best.0 {
                        best = (t, cx + dx as f32, cy + dy as f32);
                    }
                }
            }
            if best.0 > MIN_TEXTURE {
                found.push(best);
            }
        }
    }
    found.sort_by(|a, b| b.0.total_cmp(&a.0));
    found.truncate(MAX_POINTS);
    found.into_iter().map(|(_, x, y)| [x, y]).collect()
}

/// Sum of squared differences, whose curve around a match is smooth enough to read
/// a fraction of a pixel from.
fn cost(a: &Gray, ax: i32, ay: i32, b: &Gray, bx: i32, by: i32) -> f32 {
    let mut s = 0.0;
    for y in -PATCH..=PATCH {
        for x in -PATCH..=PATCH {
            let d = a.at(ax + x, ay + y) - b.at(bx + x, by + y);
            s += d * d;
        }
    }
    s
}

/// Finds the patch at `p` in the anchor frame, searching around `guess` in the new one.
/// Gives the point's new place to a fraction of a pixel, or nothing if the match is poor.
fn follow(anchor: &Gray, frame: &Gray, p: [f32; 2], guess: [f32; 2]) -> Option<[f32; 2]> {
    let (ax, ay) = (p[0].round() as i32, p[1].round() as i32);
    let (gx, gy) = (guess[0].round() as i32, guess[1].round() as i32);
    let mut best = (f32::MAX, 0, 0);
    let mut mean = 0.0;
    for dy in -SEARCH..=SEARCH {
        for dx in -SEARCH..=SEARCH {
            let c = cost(anchor, ax, ay, frame, gx + dx, gy + dy);
            mean += c;
            if c < best.0 {
                best = (c, dx, dy);
            }
        }
    }
    mean /= ((2 * SEARCH + 1) * (2 * SEARCH + 1)) as f32;
    if best.0 > mean * 0.6 && mean > 1e-4 {
        return None;
    }
    let (dx, dy) = (best.1, best.2);
    let parabola = |l: f32, c: f32, r: f32| {
        let d = l - 2.0 * c + r;
        if d.abs() < 1e-7 { 0.0 } else { (0.5 * (l - r) / d).clamp(-0.5, 0.5) }
    };
    let (cx, cy) = (gx + dx, gy + dy);
    let sx = parabola(cost(anchor, ax, ay, frame, cx - 1, cy), best.0, cost(anchor, ax, ay, frame, cx + 1, cy));
    let sy = parabola(cost(anchor, ax, ay, frame, cx, cy - 1), best.0, cost(anchor, ax, ay, frame, cx, cy + 1));
    // The patch sat on whole pixels, so the point keeps its own fraction.
    Some([cx as f32 + sx + (p[0] - ax as f32), cy as f32 + sy + (p[1] - ay as f32)])
}

/// Pan in pixels, zoom as a ratio, rotate in degrees, all around `pivot`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Motion {
    pub dx: f32,
    pub dy: f32,
    pub zoom: f32,
    pub rot: f32,
    pub pivot: [f32; 2],
}

impl Motion {
    pub const NONE: Motion = Motion { dx: 0.0, dy: 0.0, zoom: 1.0, rot: 0.0, pivot: [0.0; 2] };

    /// Where a point ends up, so a mask away from the pivot still lands correctly.
    pub fn carry(&self, p: [f32; 2]) -> [f32; 2] {
        let (s, c) = self.rot.to_radians().sin_cos();
        let (x, y) = (p[0] - self.pivot[0], p[1] - self.pivot[1]);
        [
            self.pivot[0] + self.dx + self.zoom * (c * x - s * y),
            self.pivot[1] + self.dy + self.zoom * (s * x + c * y),
        ]
    }
}

/// Best-fit move between two point sets, leaving out points that disagree with the rest.
pub fn fit(pairs: &[([f32; 2], [f32; 2])]) -> Option<Motion> {
    let mut used: Vec<([f32; 2], [f32; 2])> = pairs.to_vec();
    let mut motion = None;
    for _ in 0..FIT_PASSES {
        if used.len() < 2 {
            break;
        }
        let n = used.len() as f32;
        let centre = |side: usize| {
            let (mut x, mut y) = (0.0, 0.0);
            for u in &used {
                let p = if side == 0 { u.0 } else { u.1 };
                x += p[0] / n;
                y += p[1] / n;
            }
            [x, y]
        };
        let (ca, cb) = (centre(0), centre(1));
        let (mut a, mut b, mut d) = (0.0, 0.0, 0.0);
        for (p, q) in &used {
            let (px, py) = (p[0] - ca[0], p[1] - ca[1]);
            let (qx, qy) = (q[0] - cb[0], q[1] - cb[1]);
            a += px * qx + py * qy;
            b += px * qy - py * qx;
            d += px * px + py * py;
        }
        let (a, b) = (a / d.max(1e-3), b / d.max(1e-3));
        let (mut zoom, mut rot) = ((a * a + b * b).sqrt(), b.atan2(a).to_degrees());
        if used.len() < MIN_POINTS_FOR_SHAPE || (zoom - 1.0).abs() < ZOOM_QUIET {
            zoom = 1.0;
        }
        if used.len() < MIN_POINTS_FOR_SHAPE || rot.abs() < TURN_QUIET {
            rot = 0.0;
        }
        let zoom = zoom.clamp(1.0 - ZOOM_LIMIT, 1.0 + ZOOM_LIMIT);
        let rot = rot.clamp(-TURN_LIMIT, TURN_LIMIT);
        motion = Some(Motion { dx: cb[0] - ca[0], dy: cb[1] - ca[1], zoom, rot, pivot: ca });
        let residual = |p: [f32; 2], q: [f32; 2]| {
            let (px, py) = (p[0] - ca[0], p[1] - ca[1]);
            let (ex, ey) = (cb[0] + a * px - b * py - q[0], cb[1] + b * px + a * py - q[1]);
            (ex * ex + ey * ey).sqrt()
        };
        let mut errs: Vec<f32> = used.iter().map(|(p, q)| residual(*p, *q)).collect();
        errs.sort_by(f32::total_cmp);
        let limit = (errs[errs.len() / 2] * OUTLIER_SCALE).max(0.75);
        used.retain(|(p, q)| residual(*p, *q) <= limit);
    }
    if motion.is_none() && pairs.len() == 1 {
        let (p, q) = pairs[0];
        motion = Some(Motion { dx: q[0] - p[0], dy: q[1] - p[1], pivot: p, ..Motion::NONE });
    }
    motion
}

/// One frame's step: each point's place in the anchor and now, and the move from one
/// to the other. Measuring from the anchor is what stops small errors adding up.
pub fn step(anchor: &Gray, frame: &Gray, from: &[[f32; 2]], now: &[[f32; 2]]) -> (Vec<([f32; 2], [f32; 2])>, Motion) {
    let moved: Vec<Option<[f32; 2]>> = from.par_iter().zip(now.par_iter()).map(|(p, g)| follow(anchor, frame, *p, *g)).collect();
    let pairs: Vec<([f32; 2], [f32; 2])> = from.iter().zip(&moved).filter_map(|(p, q)| q.map(|q| (*p, q))).collect();
    let m = fit(&pairs).unwrap_or(Motion::NONE);
    (pairs, m)
}

/// The area of the photo a mask covers, as (left, top, right, bottom) from 0 to 1.
fn region_of(m: &Mask) -> [f32; 4] {
    const LINEAR_REACH: f32 = 0.2;
    match m.shape {
        crate::masks::Shape::Linear => [m.x - LINEAR_REACH, m.y - LINEAR_REACH, m.x + LINEAR_REACH, m.y + LINEAR_REACH],
        crate::masks::Shape::Radial | crate::masks::Shape::Rectangle => {
            [m.x - m.size_x, m.y - m.size_y, m.x + m.size_x, m.y + m.size_y]
        }
        _ => {
            let mut b = [1.0f32, 1.0, 0.0, 0.0];
            let mut grow = |p: [f32; 2]| {
                b = [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])];
            };
            m.points.iter().copied().for_each(&mut grow);
            m.strokes.iter().flat_map(|s| s.points.iter().copied()).for_each(&mut grow);
            if b[0] < b[2] { b } else { [0.25, 0.25, 0.75, 0.75] }
        }
    }
}

pub enum Update {
    Key(Key),
    Finished(Result<(), String>),
}

pub struct Run {
    pub total: u32,
    pub cancel: Arc<AtomicBool>,
    pub done: Arc<AtomicU32>,
    pub updates: Receiver<Update>,
}

/// Follows a mask from `from` to `to` in the background, giving a key for each frame.
pub fn start(info: Info, mask: Mask, from: u32, to: u32) -> Run {
    let cancel = Arc::new(AtomicBool::new(false));
    let done = Arc::new(AtomicU32::new(0));
    let (tx, updates) = mpsc::channel();
    let total = to.abs_diff(from) + 1;
    let (stop, count) = (cancel.clone(), done.clone());
    std::thread::spawn(move || {
        let r = if to < from { run_back(&info, &mask, from, to, &stop, &count, &tx) } else { run(&info, &mask, from, to, &stop, &count, &tx) };
        let _ = tx.send(Update::Finished(r));
    });
    Run { total, cancel, done, updates }
}

fn run(
    info: &Info,
    mask: &Mask,
    from: u32,
    to: u32,
    stop: &AtomicBool,
    count: &AtomicU32,
    tx: &mpsc::Sender<Update>,
) -> Result<(), String> {
    let (w, h) = info.size_within(SIDE);
    let n = to.saturating_sub(from) + 1;
    let mut reader = video::Reader::open(info, from, w, h, Some(n))?;
    let first = Gray::of(&reader.next().ok_or("The first frame could not be read.")?);
    let mut walk = Walker::new(first, mask, from)?;
    let _ = tx.send(Update::Key(walk.key));
    for f in from + 1..=to {
        if stop.load(Ordering::Relaxed) {
            return Err("Cancelled".into());
        }
        let Some(next) = reader.next() else { break };
        let _ = tx.send(Update::Key(walk.advance(Gray::of(&next), f)?));
        count.store(f - from, Ordering::Relaxed);
    }
    Ok(())
}

/// The same walk toward the start of the clip. Video only decodes forward, so frames
/// are read in short runs and taken from each run's end.
fn run_back(
    info: &Info,
    mask: &Mask,
    from: u32,
    to: u32,
    stop: &AtomicBool,
    count: &AtomicU32,
    tx: &mpsc::Sender<Update>,
) -> Result<(), String> {
    let (w, h) = info.size_within(SIDE);
    let first = video::grab(info, from, w, h)?;
    let mut walk = Walker::new(Gray::of(&first), mask, from)?;
    let _ = tx.send(Update::Key(walk.key));
    let mut end = from;
    while end > to {
        let start = end.saturating_sub(BACK_CHUNK).max(to);
        let mut reader = video::Reader::open(info, start, w, h, Some(end - start))?;
        let mut run = Vec::new();
        while let Some(p) = reader.next() {
            run.push(Gray::of(&p));
        }
        drop(reader);
        for (i, g) in run.into_iter().enumerate().rev() {
            if stop.load(Ordering::Relaxed) {
                return Err("Cancelled".into());
            }
            let f = start + i as u32;
            let _ = tx.send(Update::Key(walk.advance(g, f)?));
            count.store(from - f, Ordering::Relaxed);
        }
        end = start;
    }
    Ok(())
}

/// Follows one subject from frame to frame, in whichever direction it is fed them.
struct Walker {
    anchor: Gray,
    anchor_frame: u32,
    from_pts: Vec<[f32; 2]>,
    now_pts: Vec<[f32; 2]>,
    start: Key,
    base: Key,
    key: Key,
    region: [f32; 4],
    need: usize,
    /// Pixels a frame, averaged, for carrying on while the subject is hidden.
    speed: [f32; 2],
    hidden: u32,
}

impl Walker {
    fn new(first: Gray, mask: &Mask, frame: u32) -> Result<Self, String> {
        let (fw, fh) = (first.w as f32, first.h as f32);
        let r = region_of(mask);
        let region = [r[0] * fw, r[1] * fh, r[2] * fw, r[3] * fh];
        let from_pts = pick(&first, region);
        if from_pts.len() < 2 {
            return Err("That area is too plain to track. Try a spot with more detail.".into());
        }
        let start = Key::of(mask, frame);
        Ok(Self {
            need: (from_pts.len() / SEEN_SHARE).max(MIN_SEEN).min(from_pts.len()),
            now_pts: from_pts.clone(),
            from_pts,
            anchor: first,
            anchor_frame: frame,
            start,
            base: start,
            key: start,
            region,
            speed: [0.0; 2],
            hidden: 0,
        })
    }

    fn advance(&mut self, frame: Gray, f: u32) -> Result<Key, String> {
        let (fw, fh) = (frame.w as f32, frame.h as f32);
        let (pairs, m) = step(&self.anchor, &frame, &self.from_pts, &self.now_pts);
        if pairs.len() < self.need {
            // Hidden: keep moving at the last speed and keep looking for the same
            // patches, so the subject can be picked up where it comes back out.
            self.hidden += 1;
            if self.hidden > MAX_COAST {
                return Err(format!("Tracking lost the subject at frame {f}."));
            }
            for p in &mut self.now_pts {
                p[0] += self.speed[0];
                p[1] += self.speed[1];
            }
            self.key.x += self.speed[0] / fw;
            self.key.y += self.speed[1] / fh;
            self.key.frame = f;
            return Ok(self.key);
        }
        let at = m.carry([self.base.x * fw, self.base.y * fh]);
        let key = Key {
            frame: f,
            x: at[0] / fw,
            y: at[1] / fh,
            size_x: self.base.size_x * m.zoom,
            size_y: self.base.size_y * m.zoom,
            angle: self.base.angle + m.rot,
        };
        if self.hidden == 0 {
            let moved = [(key.x - self.key.x) * fw, (key.y - self.key.y) * fh];
            for i in 0..2 {
                self.speed[i] += (moved[i] - self.speed[i]) * SPEED_MIX;
            }
        }
        self.hidden = 0;
        self.key = key;
        (self.from_pts, self.now_pts) = pairs.into_iter().unzip();
        if f.abs_diff(self.anchor_frame) >= ANCHOR_SPAN || self.from_pts.len() < 6 {
            let fresh = pick(&frame, moved_region(&key, &self.start, self.region, fw, fh));
            if fresh.len() >= self.need.min(MIN_SEEN) {
                self.need = (fresh.len() / SEEN_SHARE).max(MIN_SEEN).min(fresh.len());
                self.anchor = frame;
                self.anchor_frame = f;
                self.base = key;
                self.now_pts = fresh.clone();
                self.from_pts = fresh;
            }
        }
        Ok(key)
    }
}

/// The starting region carried along to where the subject is now, at its current size.
fn moved_region(key: &Key, start: &Key, r: [f32; 4], fw: f32, fh: f32) -> [f32; 4] {
    let zoom = key.size_x / start.size_x.max(1e-4);
    let (cx, cy) = ((r[0] + r[2]) * 0.5 + (key.x - start.x) * fw, (r[1] + r[3]) * 0.5 + (key.y - start.y) * fh);
    let (hx, hy) = ((r[2] - r[0]) * 0.5 * zoom, (r[3] - r[1]) * 0.5 * zoom);
    [cx - hx, cy - hy, cx + hx, cy + hy]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::effects::noise01;

    /// Soft random blobs, so patches have something to lock on to.
    fn scene(w: usize, h: usize, dx: f32, dy: f32) -> Gray {
        let mut px = vec![0.0; w * h];
        for y in 0..h {
            for x in 0..w {
                let (fx, fy) = ((x as f32 - dx) / 9.0, (y as f32 - dy) / 9.0);
                let (ix, iy) = (fx.floor(), fy.floor());
                let (tx, ty) = (fx - ix, fy - iy);
                let n = |a: f32, b: f32| noise01(a as i32 as u32, b as i32 as u32, 3);
                let top = n(ix, iy) * (1.0 - tx) + n(ix + 1.0, iy) * tx;
                let low = n(ix, iy + 1.0) * (1.0 - tx) + n(ix + 1.0, iy + 1.0) * tx;
                px[x + y * w] = top * (1.0 - ty) + low * ty;
            }
        }
        Gray { w, h, px }
    }

    fn walker() -> Walker {
        let first = scene(160, 120, 0.0, 0.0);
        let mask = Mask { x: 0.5, y: 0.5, size_x: 0.3, size_y: 0.3, ..Mask::new(crate::masks::Shape::Rectangle) };
        Walker::new(first, &mask, 0).unwrap()
    }

    #[test]
    fn a_hidden_subject_coasts_and_is_found_again() {
        let mut w = walker();
        for f in 1..=4 {
            w.advance(scene(160, 120, f as f32 * 2.0, 0.0), f).unwrap();
        }
        // Six frames with nothing to see: the mask keeps going at the same speed.
        for f in 5..=10 {
            let k = w.advance(Gray { w: 160, h: 120, px: vec![0.0; 160 * 120] }, f).unwrap();
            assert!(k.x > w.start.x, "stopped at frame {f}");
        }
        assert_eq!(w.hidden, 6);
        let k = w.advance(scene(160, 120, 22.0, 0.0), 11).unwrap();
        assert_eq!(w.hidden, 0);
        assert!((k.x - (0.5 + 22.0 / 160.0)).abs() < 0.01, "{k:?}");
    }

    #[test]
    fn a_subject_gone_for_too_long_stops_the_track() {
        let mut w = walker();
        let blank = || Gray { w: 160, h: 120, px: vec![0.0; 160 * 120] };
        for f in 1..=MAX_COAST {
            w.advance(blank(), f).unwrap();
        }
        assert!(w.advance(blank(), MAX_COAST + 1).is_err());
    }

    #[test]
    fn the_walk_works_the_same_in_reverse() {
        let mut w = walker();
        for (i, f) in (0..6u32).rev().enumerate() {
            let k = w.advance(scene(160, 120, (i + 1) as f32 * -2.0, 0.0), f).unwrap();
            assert_eq!(k.frame, f);
        }
        assert!((w.key.x - (0.5 - 12.0 / 160.0)).abs() < 0.01, "{:?}", w.key);
    }

    #[test]
    fn smoothing_keeps_a_gentle_slope_in_place() {
        let p = Picture { w: 8, h: 8, px: (0..64).map(|i| [(i % 8) as f32 / 7.0; 3]).collect() };
        let g = Gray::of(&p);
        assert!((g.px[3 + 3 * 8] - 3.0 / 7.0).abs() < 1e-5);
    }

    #[test]
    fn flat_pictures_offer_nothing_to_track() {
        let f = Gray { w: 64, h: 64, px: vec![0.5; 64 * 64] };
        assert!(pick(&f, [0.0, 0.0, 64.0, 64.0]).is_empty());
    }

    #[test]
    fn a_known_pan_is_found() {
        let (a, b) = (scene(160, 120, 0.0, 0.0), scene(160, 120, 5.0, -3.0));
        let pts = pick(&a, [30.0, 30.0, 130.0, 90.0]);
        assert!(pts.len() >= 6);
        let (_, m) = step(&a, &b, &pts, &pts);
        assert!((m.dx - 5.0).abs() < 0.35 && (m.dy + 3.0).abs() < 0.35, "{m:?}");
        assert!((m.zoom - 1.0).abs() < 0.02 && m.rot.abs() < 0.8, "{m:?}");
    }

    #[test]
    fn fit_recovers_zoom_and_rotation() {
        let (zoom, rot) = (1.02f32, 2f32.to_radians());
        let (s, c) = rot.sin_cos();
        let pairs: Vec<_> = [[0.0, 0.0], [20.0, 0.0], [0.0, 20.0], [20.0, 20.0], [10.0, 5.0], [3.0, 17.0], [15.0, 9.0], [7.0, 12.0]]
            .iter()
            .map(|p: &[f32; 2]| (*p, [zoom * (c * p[0] - s * p[1]) + 4.0, zoom * (s * p[0] + c * p[1]) - 2.0]))
            .collect();
        let m = fit(&pairs).unwrap();
        assert!((m.zoom - zoom).abs() < 1e-3 && (m.rot - 2.0).abs() < 0.05, "{m:?}");
        // The fitted move lands every point where it really went.
        for (p, q) in &pairs {
            let at = m.carry(*p);
            assert!((at[0] - q[0]).abs() < 0.05 && (at[1] - q[1]).abs() < 0.05, "{at:?} {q:?}");
        }
    }

    #[test]
    fn noise_does_not_add_up_to_a_zoom() {
        let pairs: Vec<_> = (0..12)
            .map(|i| {
                let p = [(i * 7 % 40) as f32, (i * 11 % 30) as f32];
                let jitter = (noise01(i, 1, 2) - 0.5) * 0.04;
                (p, [p[0] + 2.0 + jitter, p[1] - 1.0 - jitter])
            })
            .collect();
        let m = fit(&pairs).unwrap();
        assert_eq!(m.zoom, 1.0);
        assert_eq!(m.rot, 0.0);
    }

    #[test]
    fn a_wild_estimate_is_held_to_a_believable_change() {
        let pairs: Vec<_> = [[0.0, 0.0], [20.0, 0.0], [0.0, 20.0], [20.0, 20.0], [10.0, 5.0], [3.0, 17.0], [15.0, 9.0]]
            .iter()
            .map(|p: &[f32; 2]| (*p, [p[0] * 0.5, p[1] * 0.5]))
            .collect();
        assert!(fit(&pairs).unwrap().zoom >= 1.0 - ZOOM_LIMIT);
    }

    #[test]
    fn a_point_that_disagrees_is_left_out() {
        let mut pairs: Vec<_> = (0..8)
            .map(|i| {
                let p = [i as f32 * 10.0, (i % 3) as f32 * 12.0];
                (p, [p[0] + 3.0, p[1] + 1.0])
            })
            .collect();
        pairs.push(([40.0, 40.0], [90.0, -60.0]));
        let m = fit(&pairs).unwrap();
        assert!((m.dx - 3.0).abs() < 0.2 && (m.dy - 1.0).abs() < 0.2, "{m:?}");
    }
}
