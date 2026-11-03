//! The video meme library: clips the person adds, kept in %APPDATA%\Lookroom\Video Memes,
//! grouped by folder. Most meme clips are shot on a green screen, so the first frame is
//! checked for one, and the overlay keys that colour out.

use crate::overlay::Stamp;
use crate::pipeline::Picture;
use rayon::prelude::*;
use crate::store;
use crate::video;
use std::path::{Path, PathBuf};

/// Share of the border that must be the same green for a clip to count as green screen.
const GREEN_BORDER: f32 = 0.6;
/// The key colour used when none was found.
pub const DEFAULT_KEY: [f32; 3] = [0.1, 0.8, 0.2];
/// How far, in colour, a pixel can be from the key and still vanish.
pub const DEFAULT_TOLERANCE: f32 = 0.18;

#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    pub path: PathBuf,
    pub name: String,
    /// The folder it sits in, or empty for the top.
    pub group: String,
}

pub fn dir() -> PathBuf {
    store::data_dir().join("Video Memes")
}

fn clips_in(folder: &Path, group: &str, out: &mut Vec<Clip>) {
    let Ok(list) = std::fs::read_dir(folder) else { return };
    let mut entries: Vec<PathBuf> = list.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() && group.is_empty() {
            let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            clips_in(&p, &name, out);
        } else if video::is_video(&p) {
            let name = p.file_stem().map(|n| n.to_string_lossy().replace(['_', '-'], " ")).unwrap_or_default();
            out.push(Clip { path: p, name, group: group.to_string() });
        }
    }
}

/// Every clip in the library, top folder first, then each folder in name order.
pub fn list() -> Vec<Clip> {
    let mut out = Vec::new();
    clips_in(&dir(), "", &mut out);
    out
}

/// Copies clips into the library. Gives how many went in.
pub fn add(files: &[PathBuf]) -> Result<usize, String> {
    std::fs::create_dir_all(dir()).map_err(|e| e.to_string())?;
    let mut n = 0;
    for f in files.iter().filter(|f| video::is_video(f)) {
        let name = f.file_name().ok_or("That is not a file.")?;
        std::fs::copy(f, dir().join(name)).map_err(|e| e.to_string())?;
        n += 1;
    }
    Ok(n)
}

fn greenish(c: [f32; 3]) -> bool {
    c[1] > 0.3 && c[1] > c[0] * 1.4 && c[1] > c[2] * 1.4
}

/// The screen colour, when the frame's border is mostly one green.
pub fn green_screen(p: &Picture) -> Option<[f32; 3]> {
    if p.w < 4 || p.h < 4 {
        return None;
    }
    let border: Vec<[f32; 3]> = (0..p.w).flat_map(|x| [p.px[x], p.px[x + (p.h - 1) * p.w]]).chain((0..p.h).flat_map(|y| [p.px[y * p.w], p.px[p.w - 1 + y * p.w]])).collect();
    let green: Vec<[f32; 3]> = border.iter().copied().filter(|c| greenish(*c)).collect();
    if (green.len() as f32) < border.len() as f32 * GREEN_BORDER {
        return None;
    }
    let n = green.len() as f32;
    Some(green.iter().fold([0.0; 3], |s, c| [s[0] + c[0] / n, s[1] + c[1] / n, s[2] + c[2] / n]))
}

/// Colour without brightness, so a shadow on the screen keys out like the rest of it.
/// The small floor keeps near-black from swinging wildly.
fn chroma(c: [f32; 3]) -> (f32, f32) {
    let y = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
    let k = 1.0 / (y + 0.1);
    ((c[2] - y) * k, (c[0] - y) * k)
}

/// How solid a pixel stays with \`key\` taken out: 0 gone, 1 untouched. The edge is soft.
pub fn keep(c: [f32; 3], key: [f32; 3], tolerance: f32) -> f32 {
    let ((a, b), (ka, kb)) = (chroma(c), chroma(key));
    let d = ((a - ka).powi(2) + (b - kb).powi(2)).sqrt();
    ((d - tolerance) / (tolerance * 0.6).max(1e-3)).clamp(0.0, 1.0)
}

/// Takes the screen's colour back out of edges it has spilled onto.
pub fn unspill(c: [f32; 3], key: [f32; 3]) -> [f32; 3] {
    let channel = (0..3).max_by(|a, b| key[*a].total_cmp(&key[*b])).unwrap_or(1);
    let others = (0..3).filter(|i| *i != channel).map(|i| c[i]).fold(0.0f32, f32::max);
    let mut out = c;
    out[channel] = c[channel].min(others + 0.04);
    out
}

/// The video frame drawn at `w` × `h`, with the key colour turned see-through and its
/// spill taken back out of the edges.
pub fn stamp(src: &Picture, w: usize, h: usize, key: Option<[f32; 3]>, tolerance: f32) -> Stamp {
    let (sx, sy) = (src.w as f32 / w as f32, src.h as f32 / h as f32);
    let at = |x: usize, y: usize| src.px[x.min(src.w - 1) + y.min(src.h - 1) * src.w];
    let px = (0..w * h)
        .into_par_iter()
        .map(|i| {
            let (fx, fy) = (((i % w) as f32 + 0.5) * sx - 0.5, ((i / w) as f32 + 0.5) * sy - 0.5);
            let (x0, y0) = (fx.max(0.0) as usize, fy.max(0.0) as usize);
            let (tx, ty) = ((fx - x0 as f32).clamp(0.0, 1.0), (fy - y0 as f32).clamp(0.0, 1.0));
            let (a, b, c, d) = (at(x0, y0), at(x0 + 1, y0), at(x0, y0 + 1), at(x0 + 1, y0 + 1));
            let colour: [f32; 3] = std::array::from_fn(|n| (a[n] * (1.0 - tx) + b[n] * tx) * (1.0 - ty) + (c[n] * (1.0 - tx) + d[n] * tx) * ty);
            let (colour, alpha) = match key {
                Some(k) => (unspill(colour, k), keep(colour, k, tolerance)),
                None => (colour, 1.0),
            };
            [colour[0] * alpha, colour[1] * alpha, colour[2] * alpha, alpha]
        })
        .collect();
    Stamp { w, h, px }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen_with_box() -> Picture {
        let (w, h) = (40, 30);
        let px = (0..w * h).map(|i| if (12..28).contains(&(i % w)) && (8..22).contains(&(i / w)) { [0.8, 0.4, 0.3] } else { [0.12, 0.75, 0.2] }).collect();
        Picture { w, h, px }
    }

    #[test]
    fn a_green_border_is_found_and_its_colour_given() {
        let k = green_screen(&screen_with_box()).unwrap();
        assert!((k[1] - 0.75).abs() < 1e-4);
        let plain = Picture { w: 10, h: 10, px: vec![[0.5; 3]; 100] };
        assert!(green_screen(&plain).is_none());
    }

    #[test]
    fn the_key_goes_and_the_subject_stays() {
        let key = [0.12, 0.75, 0.2];
        assert_eq!(keep([0.12, 0.75, 0.2], key, DEFAULT_TOLERANCE), 0.0);
        // Darker green still keys out: brightness doesn't matter.
        assert!(keep([0.06, 0.4, 0.1], key, DEFAULT_TOLERANCE) < 0.2);
        assert_eq!(keep([0.8, 0.4, 0.3], key, DEFAULT_TOLERANCE), 1.0);
        assert_eq!(keep([0.5, 0.5, 0.5], key, DEFAULT_TOLERANCE), 1.0);
    }

    #[test]
    fn a_keyed_frame_is_clear_where_the_screen_was() {
        let s = stamp(&screen_with_box(), 40, 30, Some([0.12, 0.75, 0.2]), DEFAULT_TOLERANCE);
        assert!(s.px[0][3] < 0.01);
        assert!(s.px[20 + 15 * 40][3] > 0.99);
        let plain = stamp(&screen_with_box(), 20, 15, None, DEFAULT_TOLERANCE);
        assert!(plain.px.iter().all(|c| c[3] == 1.0) && plain.w == 20);
    }

    #[test]
    fn spill_is_pulled_back() {
        let c = unspill([0.5, 0.8, 0.45], [0.1, 0.8, 0.2]);
        assert!(c[1] <= 0.55 && c[0] == 0.5);
    }
}
