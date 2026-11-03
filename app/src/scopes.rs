//! Scopes: histogram, waveform, RGB parade and vectorscope, each drawn to a small image.

use crate::pipeline::Picture;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Histogram,
    Waveform,
    Parade,
    Vector,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::Histogram, Kind::Waveform, Kind::Parade, Kind::Vector];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Histogram => "Histogram",
            Kind::Waveform => "Waveform",
            Kind::Parade => "RGB Parade",
            Kind::Vector => "Vectorscope",
        }
    }

    pub fn next(self) -> Kind {
        let i = Self::ALL.iter().position(|k| *k == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }

    fn size(self) -> (usize, usize) {
        match self {
            Kind::Vector => (VECTOR_SIDE, VECTOR_SIDE),
            _ => (TRACE_W, TRACE_H),
        }
    }
}

const TRACE_W: usize = 256;
const TRACE_H: usize = 128;
const VECTOR_SIDE: usize = 180;
/// Every pixel would be wasted on a picture this small.
const STEP: usize = 3;
/// Share of the disc that a fully saturated colour reaches.
pub const VECTOR_SCALE: f32 = 0.82;

pub struct Scope {
    pub w: usize,
    pub h: usize,
    pub rgba: Vec<u8>,
}

/// Soft response, so lightly hit cells still show and dense ones do not clip.
#[inline]
fn glow(n: u32, peak: f32) -> f32 {
    ((n as f32).ln_1p() / peak.ln_1p().max(1.0)).sqrt()
}

/// One column to a pixel, split into `panes` side by side.
fn traces(p: &Picture, panes: usize) -> Scope {
    let (w, h) = (TRACE_W, TRACE_H);
    let mut bins = vec![[0u32; 3]; w * h];
    let inner = w / panes;
    for y in (0..p.h).step_by(STEP) {
        for x in (0..p.w).step_by(STEP) {
            let c = p.px[x + y * p.w];
            let col = x * inner / p.w;
            for ch in 0..3 {
                let row = ((1.0 - c[ch].clamp(0.0, 1.0)) * (h - 1) as f32) as usize;
                let at = if panes == 3 { col + ch * inner } else { col };
                bins[at + row * w][ch] += 1;
            }
        }
    }
    let peak = bins.iter().flatten().copied().max().unwrap_or(1).max(1) as f32;
    let mut rgba = vec![0u8; w * h * 4];
    for (o, b) in rgba.chunks_exact_mut(4).zip(bins.iter()) {
        let v = b.map(|n| (glow(n, peak) * 255.0) as u8);
        o[..3].copy_from_slice(&v);
        o[3] = v[0].max(v[1]).max(v[2]);
    }
    Scope { w, h, rgba }
}

fn histogram(p: &Picture) -> Scope {
    let (w, h) = (TRACE_W, TRACE_H);
    let mut bins = [[0u32; TRACE_W]; 3];
    for c in p.px.iter().step_by(STEP) {
        for ch in 0..3 {
            bins[ch][(c[ch].clamp(0.0, 1.0) * (w - 1) as f32) as usize] += 1;
        }
    }
    let peak = bins.iter().flatten().copied().max().unwrap_or(1).max(1) as f32;
    let mut rgba = vec![0u8; w * h * 4];
    for x in 0..w {
        for ch in 0..3 {
            // Square root keeps a small peak visible beside a tall one.
            let tall = ((bins[ch][x] as f32 / peak).sqrt() * (h - 1) as f32) as usize;
            for y in h - tall.min(h)..h {
                let o = &mut rgba[(x + y * w) * 4..(x + y * w) * 4 + 4];
                o[ch] = 215;
                o[3] = 215;
            }
        }
    }
    Scope { w, h, rgba }
}

/// Colour difference plotted round a circle; each dot keeps its own colour.
fn vector(p: &Picture) -> Scope {
    let n = VECTOR_SIDE;
    let mut count = vec![0u32; n * n];
    let mut sum = vec![[0.0f32; 3]; n * n];
    let half = n as f32 * 0.5;
    for y in (0..p.h).step_by(STEP) {
        for x in (0..p.w).step_by(STEP) {
            let c = p.px[x + y * p.w].map(|v| v.clamp(0.0, 1.0));
            let [u, v] = chroma(c);
            let (px, py) = (half + u * half * VECTOR_SCALE, half - v * half * VECTOR_SCALE);
            if px >= 0.0 && py >= 0.0 && (px as usize) < n && (py as usize) < n {
                let at = px as usize + py as usize * n;
                count[at] += 1;
                for i in 0..3 {
                    sum[at][i] += c[i];
                }
            }
        }
    }
    let peak = count.iter().copied().max().unwrap_or(1).max(1) as f32;
    let mut rgba = vec![0u8; n * n * 4];
    for (i, o) in rgba.chunks_exact_mut(4).enumerate() {
        if count[i] == 0 {
            continue;
        }
        let k = glow(count[i], peak);
        let c = sum[i].map(|v| v / count[i] as f32);
        let top = c[0].max(c[1]).max(c[2]).max(0.2);
        for ch in 0..3 {
            o[ch] = ((c[ch] / top * 0.75 + 0.25) * 255.0) as u8;
        }
        o[3] = (k * 255.0) as u8;
    }
    Scope { w: n, h: n, rgba }
}

pub const SPREAD_BINS: usize = 128;

/// How much of the picture sits at each hue, light and saturation, from 0 to 1.
pub type Spread = [[f32; SPREAD_BINS]; 3];

/// What the curve graph draws behind its line. Hue counts by how colourful a pixel
/// is, so grey areas with no real hue don't pile up at red.
pub fn spread(p: &Picture) -> Spread {
    const ROUNDS: usize = 3;
    let mut bins = [[0.0f32; SPREAD_BINS]; 3];
    let top = (SPREAD_BINS - 1) as f32;
    for c in p.px.iter().step_by(STEP * STEP) {
        let c = c.map(|v| v.clamp(0.0, 1.0));
        let [h, s, _] = crate::color::to_hsv(c);
        bins[0][((h / 360.0 * top) as usize).min(SPREAD_BINS - 1)] += s;
        bins[1][((0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]) * top) as usize] += 1.0;
        bins[2][(s * top) as usize] += 1.0;
    }
    for (i, b) in bins.iter_mut().enumerate() {
        for _ in 0..ROUNDS {
            let was = *b;
            for n in 0..SPREAD_BINS {
                // Hue is a circle; light and saturation stop at their ends.
                let (l, r) = if i == 0 {
                    ((n + SPREAD_BINS - 1) % SPREAD_BINS, (n + 1) % SPREAD_BINS)
                } else {
                    (n.saturating_sub(1), (n + 1).min(SPREAD_BINS - 1))
                };
                b[n] = (was[l] + was[n] * 2.0 + was[r]) / 4.0;
            }
        }
        let peak = b.iter().copied().fold(0.0, f32::max).max(1e-6);
        b.iter_mut().for_each(|v| *v /= peak);
    }
    bins
}

/// Where a colour falls on the disc, with full saturation near the edge.
#[inline]
pub fn chroma(c: [f32; 3]) -> [f32; 2] {
    let y = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
    [(c[2] - y) / 0.9278, (c[0] - y) / 0.7874]
}

pub fn draw(kind: Kind, p: &Picture) -> Scope {
    debug_assert!(kind.size().0 > 0);
    match kind {
        Kind::Histogram => histogram(p),
        Kind::Waveform => traces(p, 1),
        Kind::Parade => traces(p, 3),
        Kind::Vector => vector(p),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(c: [f32; 3]) -> Picture {
        Picture { w: 30, h: 30, px: vec![c; 900] }
    }

    #[test]
    fn grey_sits_at_the_centre_of_the_vectorscope() {
        let s = draw(Kind::Vector, &flat([0.5; 3]));
        let at = |x: usize, y: usize| s.rgba[(x + y * s.w) * 4 + 3];
        assert!(at(s.w / 2, s.h / 2) > 200);
        assert_eq!(at(4, 4), 0);
    }

    #[test]
    fn a_red_picture_leaves_the_centre() {
        let s = draw(Kind::Vector, &flat([1.0, 0.0, 0.0]));
        assert_eq!(s.rgba[(s.w / 2 + s.h / 2 * s.w) * 4 + 3], 0);
        assert!(s.rgba.chunks(4).any(|o| o[3] > 200));
    }

    #[test]
    fn the_parade_shows_each_channel_in_its_own_pane() {
        let wide = Picture { w: 600, h: 30, px: vec![[1.0, 0.0, 0.0]; 600 * 30] };
        let s = draw(Kind::Parade, &wide);
        let row_of = |x: usize| (0..s.h).find(|y| s.rgba[(x + y * s.w) * 4 + 3] > 0);
        assert_eq!(row_of(s.w / 6), Some(0));
        assert_eq!(row_of(s.w / 2), Some(s.h - 1));
    }

    #[test]
    fn the_spread_finds_a_pictures_hue() {
        let s = spread(&flat([0.1, 0.8, 0.1]));
        let peak = (0..SPREAD_BINS).max_by(|a, b| s[0][*a].total_cmp(&s[0][*b])).unwrap();
        assert!((peak as f32 / SPREAD_BINS as f32 - 1.0 / 3.0).abs() < 0.03, "{peak}");
    }

    #[test]
    fn grey_has_no_hue_to_show() {
        let s = spread(&flat([0.5; 3]));
        assert!(s[0].iter().all(|v| *v == 0.0));
        assert!(s[1].iter().any(|v| *v > 0.9));
    }

    #[test]
    fn the_histogram_marks_the_right_tone() {
        let s = draw(Kind::Histogram, &flat([0.5; 3]));
        let mid = s.w / 2;
        let hit = (mid - 2..mid + 2).any(|x| s.rgba[(x + (s.h - 1) * s.w) * 4 + 3] > 0);
        assert!(hit);
        assert_eq!(s.rgba[(2 + (s.h - 1) * s.w) * 4 + 3], 0);
    }
}
