//! Turns the project into pictures. One compositor serves playback, stills and export,
//! so what plays is what gets written. Playback adds what keeps it smooth: frames decoded
//! ahead, colour baked to a table for each clip, finished frames queued for the screen,
//! and a picture size chosen before it starts, from how long the grade actually takes.

use crate::export::VideoOptions;
use crate::film;
use crate::lut::{Lut3D, LutBank};
use crate::overlay::{self, Setting, Stamp, TextCells};
use crate::pipeline::{self, Picture};
use crate::scopes::{self, Kind as ScopeKind};
use crate::textanim;
use crate::timeline::{Body, Index, Item, Kind, Project, ROWS, mix_graph};
use crate::video::{self, Codec, Reader};
use eframe::egui::{Color32, ColorImage};
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::Arc;
use std::time::Instant;

/// Finished frames held ready for the screen.
const QUEUE: usize = 8;
/// Decoded frames held ready for grading, per clip, at most.
const DECODE_AHEAD: usize = 4;
/// Memory each clip's decoded frames may take while waiting. A 4K frame is about 100 MB
/// once decoded, so at 4K this holds one or two, and at 1080p all four.
const DECODE_AHEAD_BYTES: usize = 160 * 1024 * 1024;
/// Where in the timeline an export estimate samples, and how many frames at each.
const ESTIMATE_AT: [f64; 4] = [0.15, 0.4, 0.65, 0.9];
const ESTIMATE_FRAMES: u32 = 3;
/// Seconds a clip's decoder takes to start during an export, on a typical machine.
const CLIP_START_SECONDS: f64 = 0.15;
/// Drawn texts and shapes stay this long either side of where they show.
const KEEP_DRAWN_SECONDS: f64 = 2.0;
/// Below this many drawn items nothing is worth forgetting.
const KEEP_DRAWN_ALWAYS: usize = 24;
pub const PLAY_SIDES: [usize; 5] = [1280, 960, 720, 540, 400];
/// Share of a frame's time that grading may take.
const BUSY: f32 = 0.7;
const SCOPE_EVERY: u32 = 3;
/// Frames to skip at once when falling behind. Small, so motion stays continuous.
const MOST_SKIPPED: u32 = 4;
const PROBE: (usize, usize) = (640, 360);
const PROBE_RUNS: usize = 3;
/// Clips sampled when choosing a size; a project's heaviest looks are found among them.
const PROBE_CLIPS: usize = 12;
/// Reading forward through this much is quicker than restarting the decoder.
const SKIP_LIMIT: u32 = 48;
/// Frames ahead of the end of a clip to begin decoding the next one.
const PREPARE: u32 = 18;
const REFERENCE_H: f32 = 1080.0;
/// The widest blur text gets, in pixels. It stays quick to draw on every frame.
const MOST_BLUR: usize = 40;

/// A snapshot of everything needed to make pictures, safe to hand to another thread.
pub struct Scene {
    pub project: Project,
    pub bank: LutBank,
    pub stills: HashMap<u32, Arc<Picture>>,
}

/// For a ramped clip, which frame of its source (counted at the project's rate from the
/// clip's start) shows at `offset` frames in.
fn ramp_frame(c: &crate::timeline::Clip, offset: u32, fps: f64) -> u32 {
    ((c.source_at(offset as f64, fps) - c.start) * fps.max(1.0)).round().max(0.0) as u32
}

/// A picture in the project's shape at about its own size: filled and cropped, or fitted
/// with bars. A picture already that shape is left alone.
pub fn frame_to(p: Picture, size: (usize, usize), fill: bool) -> Picture {
    let (w, h) = fit_size(size, p.w.max(p.h));
    let same = ((w as f32 / h as f32) - (p.w as f32 / p.h.max(1) as f32)).abs() < 0.01;
    match (same, fill) {
        (true, _) => p,
        (false, true) => overlay::cover(p, w, h),
        (false, false) => overlay::letterbox(p, w, h),
    }
}

/// A size that fits inside a limit, even on both sides, for encoders.
pub fn fit_size(size: (usize, usize), side: usize) -> (usize, usize) {
    let long = size.0.max(size.1).max(1);
    let s = (side as f64 / long as f64).min(1.0);
    ((((size.0 as f64 * s).round() as usize) & !1).max(2), (((size.1 as f64 * s).round() as usize) & !1).max(2))
}

struct Source {
    clip: u32,
    frames: Receiver<Picture>,
    next: u32,
    last: Option<Arc<Picture>>,
    stop: Arc<AtomicBool>,
}

impl Drop for Source {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Text and stickers, drawn once and reused until they change.
#[derive(Default)]
pub struct Laid {
    stamps: HashMap<u32, (usize, String, u32, Arc<Stamp>)>,
    texts: HashMap<u32, (Setting, Arc<TextCells>)>,
    designs: HashMap<u32, (String, usize, u32, Arc<Stamp>)>,
    /// Each text's background shape, kept until its look or size changes.
    backings: HashMap<u32, (BackingKey, Arc<Stamp>)>,
}

type BackingKey = (textanim::Backing, [u32; 3], u32, u32);

impl Laid {
    /// A sticker's picture. Only its look decides this, so moving it costs nothing.
    fn stamp(&mut self, id: u32, icon: usize, emoji: &str, size: f32, h: usize) -> Option<Arc<Stamp>> {
        let px = (size * h as f32 / REFERENCE_H) as u32;
        if let Some((i, e, sized, s)) = self.stamps.get(&id)
            && (*i, e.as_str(), *sized) == (icon, emoji, px)
        {
            return Some(s.clone());
        }
        let made = if emoji.is_empty() { overlay::sticker_stamp(icon, px) } else { crate::assets::emoji_stamp(emoji, px) };
        let s = Arc::new(made?);
        self.stamps.insert(id, (icon, emoji.to_string(), px, s.clone()));
        Some(s)
    }

    fn lay_sticker(&mut self, p: &mut Picture, t: &Item, frame: u32, fps: f32) {
        let Body::Sticker { icon, emoji, size, x, y, motion } = &t.body else { return };
        let Some(s) = self.stamp(t.id, *icon, emoji, *size, p.h) else { return };
        lay_one(p, &s, motion, (*x, *y), t, frame, fps);
    }

    /// A design's layer, drawn again only when its size on the picture changes.
    fn lay_graphic(&mut self, p: &mut Picture, t: &Item, frame: u32, fps: f32) {
        let Body::Graphic { file, layer, size, x, y, motion, .. } = &t.body else { return };
        let Some(d) = crate::graphics::load(file) else { return };
        let scale = d.fit(p.w as f32, p.h as f32) * size.max(0.01);
        let key = (scale * 1000.0) as u32;
        let s = match self.designs.get(&t.id) {
            Some((f, l, k, s)) if (f.as_str(), *l, *k) == (file.as_str(), *layer, key) => s.clone(),
            _ => {
                let Some(made) = d.draw(*layer, scale) else { return };
                let s = Arc::new(made);
                self.designs.insert(t.id, (file.clone(), *layer, key, s.clone()));
                s
            }
        };
        lay_one(p, &s, motion, (*x, *y), t, frame, fps);
    }

    /// A video over the picture, with its key colour taken out.
    fn lay_video(&mut self, p: &mut Picture, t: &Item, frame: u32, fps: f32, videos: &HashMap<u32, Arc<Picture>>) {
        let Body::Overlay { size, x, y, key, tolerance, motion, .. } = &t.body else { return };
        let Some(src) = videos.get(&t.id) else { return };
        let h = (p.h as f32 * size).round().max(2.0) as usize;
        let w = (h as f32 * src.w as f32 / src.h.max(1) as f32).round().max(2.0) as usize;
        let s = crate::clips::stamp(src, w, h, *key, *tolerance);
        lay_one(p, &s, motion, (*x, *y), t, frame, fps);
    }

    /// A text's pieces. Moving text only rearranges them, so they are redrawn only
    /// when its words or look change.
    fn cells(&mut self, id: u32, setting: Setting) -> Option<Arc<TextCells>> {
        if let Some((s, cells)) = self.texts.get(&id)
            && *s == setting
        {
            return Some(cells.clone());
        }
        let cells = Arc::new(overlay::text_cells(&setting)?);
        self.texts.insert(id, (setting, cells.clone()));
        Some(cells)
    }

    /// The shape behind a text, sized to its words and moving with it.
    #[allow(clippy::too_many_arguments)]
    fn lay_backing(&mut self, p: &mut Picture, t: &Item, cells: &TextCells, motion: &textanim::Motion, (x, y, em): (f32, f32, f32), frame: u32, fps: f32) {
        // The words' extent around the text's centre, in pixels.
        let (mut l, mut r, mut top, mut bottom) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
        for c in &cells.cells {
            l = l.min(c.x - c.w / 2.0);
            r = r.max(c.x + c.w / 2.0);
            top = top.min(c.y - c.h / 2.0);
            bottom = bottom.max(c.y + c.h / 2.0);
        }
        if l > r {
            return;
        }
        let (w, h) = (r - l, bottom - top);
        let key: BackingKey = (motion.backing, motion.backing_colour.map(f32::to_bits), w as u32, h as u32);
        let stamp = match self.backings.get(&t.id) {
            Some((k, s)) if *k == key => s.clone(),
            _ => {
                let Some(made) = overlay::backing(motion.backing, motion.backing_colour, w, h, em, t.id) else { return };
                let s = Arc::new(made);
                self.backings.insert(t.id, (key, s.clone()));
                s
            }
        };
        // An underline sits just under the words; the others are centred on them.
        let cy = if motion.backing == textanim::Backing::Underline { bottom + em * 0.05 } else { (top + bottom) / 2.0 };
        let at = (x + (l + r) / 2.0 / p.w as f32, y + cy / p.h as f32);
        let whole = textanim::Motion { enter: whole_phase(motion.enter), leave: whole_phase(motion.leave), ..motion.clone() };
        lay_one(p, &stamp, &whole, at, t, frame, fps);
    }

    fn lay_text(&mut self, p: &mut Picture, t: &Item, frame: u32, fps: f32) {
        let Body::Text { text, size, colour, x, y, motion } = &t.body else { return };
        let em = size * p.h as f32 / REFERENCE_H;
        let setting = Setting {
            text: text.clone(),
            size: em,
            colour: *colour,
            font: motion.font.clone(),
            weight: motion.weight,
            italic: motion.italic,
            spacing: motion.spacing,
            outline: motion.outline,
            finest: motion.finest(),
            scrambles: motion.scrambles(),
            gradient: motion.gradient,
            highlight: motion.highlight,
        };
        let Some(cells) = self.cells(t.id, setting) else { return };
        if motion.backing != textanim::Backing::None {
            self.lay_backing(p, t, &cells, motion, (*x, *y, em), frame, fps);
        }
        let placed = textanim::compose(&cells.cells, motion, *x, *y, frame - t.at, t.len, fps, em, (p.w as f32, p.h as f32), cells.spares.len());
        for q in &placed {
            let stamp = q.spare.and_then(|s| cells.spares.get(s)).unwrap_or(&cells.stamps[q.cell]);
            if q.settled {
                overlay::place(p, stamp, q.x / p.w as f32, q.y / p.h as f32);
            } else if q.blur >= 0.5 {
                overlay::draw(p, &overlay::soften(stamp, (q.blur.round() as usize).min(MOST_BLUR)), q);
            } else {
                overlay::draw(p, stamp, q);
            }
        }
    }

    /// Effects first, then everything laid on top. Later in the list draws over earlier,
    /// which is how the layers list orders them. `videos` holds each overlay's frame.
    pub fn apply(&mut self, p: &mut Picture, project: &Project, index: &Index, frame: u32, videos: &HashMap<u32, Arc<Picture>>) {
        let mut here = index.items_in(&project.items, frame, frame + 1);
        here.sort_unstable();
        let on = || here.iter().map(|i| &project.items[*i]).filter(|t| !project.tracks[t.row.min(ROWS - 1)].hidden);
        for t in on() {
            if let Body::Effect { kind, amount } = &t.body {
                let progress = (frame - t.at) as f32 / t.len.max(1) as f32;
                overlay::effect(p, *kind, *amount, progress, frame);
            }
        }
        for t in on() {
            match &t.body {
                Body::Text { .. } => self.lay_text(p, t, frame, project.fps as f32),
                Body::Sticker { .. } => self.lay_sticker(p, t, frame, project.fps as f32),
                Body::Graphic { .. } => self.lay_graphic(p, t, frame, project.fps as f32),
                Body::Overlay { .. } => self.lay_video(p, t, frame, project.fps as f32, videos),
                _ => {}
            }
        }
        self.forget_far(project, index, frame);
    }

    /// Drops the drawn texts, stickers, shapes and design layers of items more than a few
    /// seconds from `frame`. Each is several megabytes at 4K, so a long project's hundreds
    /// of captions would otherwise pile up until memory ran out. Coming back just draws
    /// them again.
    fn forget_far(&mut self, project: &Project, index: &Index, frame: u32) {
        let reach = (project.fps.max(1.0) * KEEP_DRAWN_SECONDS) as u32;
        let cached = self.texts.len() + self.stamps.len() + self.designs.len() + self.backings.len();
        if cached <= KEEP_DRAWN_ALWAYS {
            return;
        }
        let near: std::collections::HashSet<u32> = index.items_in(&project.items, frame.saturating_sub(reach), frame + reach).into_iter().map(|i| project.items[i].id).collect();
        self.texts.retain(|id, _| near.contains(id));
        self.stamps.retain(|id, _| near.contains(id));
        self.designs.retain(|id, _| near.contains(id));
        self.backings.retain(|id, _| near.contains(id));
    }
}

/// One picture moved by an item's motion: a sticker, a design layer or a video.
/// The same animation taking the whole shape at once, for a text's background.
fn whole_phase(ph: textanim::Phase) -> textanim::Phase {
    textanim::Phase { unit: textanim::Unit::Whole, delay: 0.0, ..ph }
}

fn lay_one(p: &mut Picture, s: &Stamp, motion: &textanim::Motion, at: (f32, f32), t: &Item, frame: u32, fps: f32) {
    let cell = textanim::Cell { x: 0.0, y: 0.0, w: s.w as f32, h: s.h as f32, groups: [0; 4] };
    for q in textanim::compose(&[cell], motion, at.0, at.1, frame - t.at, t.len, fps, s.h as f32, (p.w as f32, p.h as f32), 0) {
        if q.settled {
            overlay::place(p, s, q.x / p.w as f32, q.y / p.h as f32);
        } else if q.blur >= 0.5 {
            overlay::draw(p, &overlay::soften(s, (q.blur.round() as usize).min(MOST_BLUR)), &q);
        } else {
            overlay::draw(p, s, &q);
        }
    }
}

/// A grade with nothing changed, made once to compare clips against.
fn untouched() -> &'static pipeline::Grade {
    static G: std::sync::OnceLock<pipeline::Grade> = std::sync::OnceLock::new();
    G.get_or_init(pipeline::Grade::default)
}

pub struct Composer {
    scene: Arc<Scene>,
    pub w: usize,
    pub h: usize,
    /// Playback's quicker path; export and stills take the full one.
    play: bool,
    deep: bool,
    sources: Vec<Source>,
    stills: HashMap<u32, Arc<Picture>>,
    tables: HashMap<u32, Arc<Lut3D>>,
    /// Export's colour tables, finer than playback's and in the steps' own order.
    staged: HashMap<u32, Arc<pipeline::Staged>>,
    laid: Laid,
    index: Index,
}

impl Composer {
    pub fn new(scene: Arc<Scene>, w: usize, h: usize, play: bool, deep: bool) -> Self {
        let index = Index::of(&scene.project);
        Self { scene, w, h, play, deep, sources: Vec::new(), stills: HashMap::new(), tables: HashMap::new(), staged: HashMap::new(), laid: Laid::default(), index }
    }

    fn start_source(&mut self, index: usize, offset: u32) {
        let p = &self.scene.project;
        let c = &p.clips[index];
        let Some(info) = p.media_of(c.media).and_then(|m| m.info.clone()) else { return };
        if c.ramp.len() >= 2 {
            // A ramp is read at the source's own pace; frames are picked along the curve.
            let k = ramp_frame(c, offset, p.fps);
            self.start_reader(c.id, info, c.start + k as f64 / p.fps.max(1.0), 1.0, k);
            return;
        }
        let (id, seconds, speed) = (c.id, c.source_at(offset as f64, p.fps), c.speed);
        self.start_reader(id, info, seconds, speed, offset);
    }

    /// Starts decoding a video from `seconds` into it, as frame `offset` of whatever shows it.
    fn start_reader(&mut self, id: u32, info: crate::video::Info, seconds: f64, speed: f64, offset: u32) {
        let fps = self.scene.project.fps;
        let (w, h) = {
            let (fw, fh) = (self.w as f64 / info.w.max(1) as f64, self.h as f64 / info.h.max(1) as f64);
            let s = fw.min(fh).min(1.0);
            (((info.w as f64 * s).round() as usize & !1).max(2), ((info.h as f64 * s).round() as usize & !1).max(2))
        };
        let (play, deep) = (self.play, self.deep);
        let frame_bytes = w * h * std::mem::size_of::<[f32; 3]>();
        let depth = (DECODE_AHEAD_BYTES / frame_bytes.max(1)).clamp(1, DECODE_AHEAD);
        let (tx, frames): (SyncSender<Picture>, Receiver<Picture>) = mpsc::sync_channel(depth);
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        std::thread::spawn(move || {
            let Ok(mut reader) = Reader::conformed(&info, seconds, w, h, fps, speed, play, deep) else { return };
            while !flag.load(Ordering::Relaxed) {
                let Some(p) = reader.next() else { break };
                if tx.send(p).is_err() {
                    break;
                }
            }
        });
        self.sources.retain(|s| s.clip != id);
        self.sources.push(Source { clip: id, frames, next: offset, last: None, stop });
    }

    fn video_frame(&mut self, index: usize, offset: u32) -> Option<Arc<Picture>> {
        let c = &self.scene.project.clips[index];
        let id = c.id;
        // Slow stretches repeat a frame and fast ones skip ahead; the reader allows both.
        let at = if c.ramp.len() >= 2 { ramp_frame(c, offset, self.scene.project.fps) } else { offset };
        self.read_frame(id, at, |me| me.start_source(index, offset))
    }

    /// Each overlay's frame at `f`, decoded beside the main clips.
    fn overlay_frames(&mut self, f: u32) -> HashMap<u32, Arc<Picture>> {
        let scene = self.scene.clone();
        let p = &scene.project;
        let mut out = HashMap::new();
        for i in self.index.items_in(&p.items, f, f + 1) {
            let t = &p.items[i];
            let Body::Overlay { media, start, .. } = &t.body else { continue };
            let Some(info) = p.media_of(*media).and_then(|m| m.info.clone()) else { continue };
            let (id, offset) = (t.id, f - t.at);
            let seconds = start + offset as f64 / p.fps.max(1.0);
            if let Some(pic) = self.read_frame(id, offset, |me| me.start_reader(id, info, seconds, 1.0, offset)) {
                out.insert(id, pic);
            }
        }
        out
    }

    /// The frame at `offset` from the source called `id`, starting it with `start` if it
    /// isn't open or is too far away to read on to.
    fn read_frame(&mut self, id: u32, offset: u32, start: impl FnOnce(&mut Self)) -> Option<Arc<Picture>> {
        let usable = self.sources.iter().any(|s| s.clip == id && offset >= s.next && offset - s.next <= SKIP_LIMIT);
        let same = self.sources.iter().any(|s| s.clip == id && s.next == offset + 1 && s.last.is_some());
        if !usable && !same {
            start(self);
        }
        let s = self.sources.iter_mut().find(|s| s.clip == id)?;
        if s.next == offset + 1 {
            return s.last.clone();
        }
        while s.next <= offset {
            match s.frames.recv() {
                Ok(p) => {
                    s.next += 1;
                    if s.next == offset + 1 {
                        s.last = Some(Arc::new(p));
                    }
                }
                // A source a frame or two shorter than promised holds its last picture.
                Err(_) => break,
            }
        }
        s.last.clone()
    }

    fn still(&mut self, media: u32) -> Option<Arc<Picture>> {
        if let Some(p) = self.stills.get(&media) {
            return Some(p.clone());
        }
        let full = self.scene.stills.get(&media)?;
        let fitted = Arc::new(full.fit(self.w.max(self.h)));
        self.stills.insert(media, fitted.clone());
        Some(fitted)
    }

    fn table(&mut self, index: usize) -> Arc<Lut3D> {
        let c = &self.scene.project.clips[index];
        if let Some(t) = self.tables.get(&c.id) {
            return t.clone();
        }
        let t = Arc::new(pipeline::bake_lut(&c.grade, &self.scene.bank, film::PLAY_TABLE));
        self.tables.insert(c.id, t.clone());
        t
    }

    fn staged(&mut self, index: usize) -> Arc<pipeline::Staged> {
        let c = &self.scene.project.clips[index];
        if let Some(t) = self.staged.get(&c.id) {
            return t.clone();
        }
        let t = Arc::new(pipeline::bake_staged(&c.grade, &self.scene.bank));
        self.staged.insert(c.id, t.clone());
        t
    }

    fn graded(&mut self, index: usize, offset: u32) -> Picture {
        let scene = self.scene.clone();
        let p = &scene.project;
        let c = &p.clips[index];
        let m = p.media_of(c.media);
        let raw = match m.map(|m| m.kind) {
            Some(Kind::Video) => self.video_frame(index, offset),
            _ => self.still(c.media),
        };
        let Some(raw) = raw else {
            return Picture { w: self.w, h: self.h, px: vec![[0.0; 3]; self.w * self.h] };
        };
        // Masks and grain are keyed to the clip's own source frames.
        let source_fps = m.and_then(|m| m.info.as_ref()).map_or(p.fps, |i| i.fps);
        let frame = (c.source_at(offset as f64, p.fps) * source_fps).round() as u32;
        // An untouched clip comes out as it went in, so it skips the grade altogether.
        let mut out = if *c.grade == *untouched() {
            (*raw).clone()
        } else if self.play {
            let table = self.table(index);
            pipeline::render_play(&raw, &c.grade, &table, &scene.bank, frame)
        } else if self.deep {
            let staged = self.staged(index);
            pipeline::render_staged(&raw, &c.grade, &staged, &scene.bank, frame)
        } else {
            pipeline::render_at(&raw, &c.grade, &scene.bank, true, true, frame)
        };
        pipeline::apply_frame(&mut out, c.grade.frame_ratio);
        let framed = if p.fill { overlay::cover(out, self.w, self.h) } else { overlay::letterbox(out, self.w, self.h) };
        // Punch in: the framed picture enlarged from its centre, edges cropped.
        if c.zoom > 1.001 { overlay::zoom(framed, c.zoom) } else { framed }
    }

    pub fn frame(&mut self, f: u32) -> Picture {
        let scene = self.scene.clone();
        let p = &scene.project;
        let Some(shot) = self.index.shot(p, f) else {
            let mut black = Picture { w: self.w, h: self.h, px: vec![[0.0; 3]; self.w * self.h] };
            let videos = self.overlay_frames(f);
            self.laid.apply(&mut black, p, &self.index, f, &videos);
            return black;
        };
        let hidden = p.tracks[crate::timeline::ROW_VIDEO].hidden;
        let mut pic = if hidden {
            Picture { w: self.w, h: self.h, px: vec![[0.0; 3]; self.w * self.h] }
        } else {
            let main = self.graded(shot.clip, shot.offset);
            match (shot.blend, p.clips[shot.clip].transition) {
                (Some((other, offset, t)), Some(tr)) => {
                    let leaving = self.graded(other, offset.min(p.clips[other].len.saturating_sub(1)));
                    overlay::blend(&leaving, &main, tr.kind, t)
                }
                _ => main,
            }
        };
        // Clips no longer in view stop decoding; the next one starts early so cuts are clean.
        let mut keep: Vec<u32> = [Some(shot.clip), shot.blend.map(|b| b.0)].into_iter().flatten().map(|i| p.clips[i].id).collect();
        keep.extend(self.index.items_in(&p.items, f, f + 1).into_iter().filter(|i| matches!(p.items[*i].body, Body::Overlay { .. })).map(|i| p.items[i].id));
        let next = p.clips.get(shot.clip + 1).map(|n| n.id);
        self.sources.retain(|s| keep.contains(&s.clip) || Some(s.clip) == next);
        // Colour tables only for the clips on screen and the next; a long project's
        // would otherwise add up to gigabytes over an export.
        self.tables.retain(|id, _| keep.contains(id) || Some(*id) == next);
        self.staged.retain(|id, _| keep.contains(id) || Some(*id) == next);
        let left = p.clips[shot.clip].len.saturating_sub(shot.offset);
        // Playing and exporting run on through cuts; a single still never does.
        if (self.play || self.deep) && left < PREPARE && shot.clip + 1 < p.clips.len() {
            let n = shot.clip + 1;
            let is_video = p.media_of(p.clips[n].media).is_some_and(|m| m.kind == Kind::Video);
            if is_video && !self.sources.iter().any(|s| s.clip == p.clips[n].id) {
                self.start_source(n, 0);
            }
        }
        let videos = self.overlay_frames(f);
        self.laid.apply(&mut pic, p, &self.index, f, &videos);
        pic
    }
}

pub struct Shown {
    pub frame: u32,
    pub image: ColorImage,
    pub scope: Option<ColorImage>,
}

/// Produces frames ahead of the screen on its own thread.
pub struct Player {
    pub frames: Receiver<Shown>,
    stop: Arc<AtomicBool>,
    /// How many frames are waiting, so playback starts with some in hand.
    pub made: Arc<AtomicU32>,
    /// Where the clock has reached, so a thread falling behind can jump ahead.
    pub clock: Arc<AtomicU32>,
    pub side: Arc<AtomicUsize>,
}

/// Built in parallel, and directly: the picture is opaque, so there's nothing to premultiply.
pub fn image_of(p: &Picture) -> ColorImage {
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    let pixels: Vec<Color32> = p.px.par_iter().map(|c| Color32::from_rgb(byte(c[0]), byte(c[1]), byte(c[2]))).collect();
    ColorImage::new([p.w, p.h], pixels)
}

/// The largest size this machine can grade in time. A real grade on a small picture is
/// timed once before playing, because restarting a decoder mid-play costs most of a
/// second on a 4K clip, which is far worse than playing slightly smaller.
fn pick_side(scene: &Scene) -> usize {
    let (w, h) = PROBE;
    let px = (0..w * h).map(|i| {
        let (x, y) = ((i % w) as f32 / w as f32, (i / w) as f32 / h as f32);
        [x, y, 1.0 - x * y]
    });
    let test = Picture { w, h, px: px.collect() };
    let p = &scene.project;
    let budget = 1000.0 / p.fps.max(1.0) as f32 * BUSY;
    let mut worst = 0.0f32;
    for c in p.clips.iter().take(PROBE_CLIPS) {
        let table = pipeline::bake_lut(&c.grade, &scene.bank, film::PLAY_TABLE);
        // Grain, masks and the rest are all in here, because it's the real render.
        let fastest = (0..PROBE_RUNS)
            .map(|_| {
                let began = Instant::now();
                let out = pipeline::render_play(&test, &c.grade, &table, &scene.bank, 1);
                let _ = out.to_rgba8();
                began.elapsed().as_secs_f32() * 1000.0
            })
            .fold(f32::MAX, f32::min);
        worst = worst.max(fastest);
    }
    let per_pixel = worst / (w * h) as f32;
    let fits = |side: usize| {
        let (a, b) = fit_size(p.size, side);
        (a * b) as f32 * per_pixel < budget
    };
    PLAY_SIDES.iter().copied().find(|s| fits(*s)).unwrap_or(PLAY_SIDES[PLAY_SIDES.len() - 1])
}

impl Player {
    pub fn start(scene: Arc<Scene>, from: u32, scope: Option<ScopeKind>) -> Self {
        let (tx, frames) = mpsc::sync_channel(QUEUE);
        let stop = Arc::new(AtomicBool::new(false));
        let made = Arc::new(AtomicU32::new(0));
        let clock = Arc::new(AtomicU32::new(from));
        let side = Arc::new(AtomicUsize::new(PLAY_SIDES[0]));
        let (flag, count, time, size) = (stop.clone(), made.clone(), clock.clone(), side.clone());
        std::thread::spawn(move || {
            let total = scene.project.total();
            let mut chosen = pick_side(&scene);
            size.store(chosen, Ordering::Relaxed);
            let (w, h) = fit_size(scene.project.size, chosen);
            let mut c = Composer::new(scene.clone(), w, h, true, false);
            let (mut f, mut n) = (from, 0u32);
            let mut work = 0.0;
            let mut samples = 0u32;
            while f < total && !flag.load(Ordering::Relaxed) {
                let began = Instant::now();
                let pic = c.frame(f);
                let scope = scope.filter(|_| n % SCOPE_EVERY == 0).map(|k| {
                    let s = scopes::draw(k, &pic);
                    ColorImage::from_rgba_unmultiplied([s.w, s.h], &s.rgba)
                });
                let image = image_of(&pic);
                work += began.elapsed().as_secs_f64();
                samples += 1;
                n += 1;
                if tx.send(Shown { frame: f, image, scope }).is_err() {
                    return;
                }
                count.fetch_add(1, Ordering::Relaxed);
                // Behind the clock: drop a few frames rather than freeze on one.
                let behind = time.load(Ordering::Relaxed).saturating_sub(f);
                f += 1 + behind.min(MOST_SKIPPED);
                // The startup benchmark only measures grading. Use sustained actual
                // decode/composite/scope work to recover when that estimate was optimistic.
                if samples >= 30 {
                    let budget = 1.0 / scene.project.fps.max(1.0);
                    if behind > 2 && work / samples as f64 > budget
                        && let Some(next) = PLAY_SIDES.iter().copied().find(|s| *s < chosen)
                    {
                        chosen = next;
                        size.store(chosen, Ordering::Relaxed);
                        let (w, h) = fit_size(scene.project.size, chosen);
                        c = Composer::new(scene.clone(), w, h, true, false);
                    }
                    work = 0.0;
                    samples = 0;
                }
            }
        });
        Self { frames, stop, made, clock, side }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Writes the whole project to a file: every clip, its grade, the mix and the overlays.
pub fn export(scene: Arc<Scene>, out: &Path, o: &VideoOptions, done: &AtomicU32, cancel: &AtomicBool) -> Result<(), String> {
    let first = write_project(scene.clone(), out, o, done, cancel);
    // A graphics chip can turn down a size, or its driver fail partway; software always works.
    if first.is_err() && o.fast && !cancel.load(Ordering::Relaxed) && video::chip_for(o.codec, o.ten_bit).is_some() {
        video::forget_chip(o.codec, o.ten_bit);
        done.store(0, Ordering::Relaxed);
        return write_project(scene, out, o, done, cancel);
    }
    first
}

fn write_project(scene: Arc<Scene>, out: &Path, o: &VideoOptions, done: &AtomicU32, cancel: &AtomicBool) -> Result<(), String> {
    let p = &scene.project;
    let total = p.total();
    if total == 0 {
        return Err("The timeline is empty.".into());
    }
    let gif = o.codec == Codec::Gif;
    let side = if gif { o.side.min(video::GIF_MAX_SIDE) } else { o.side.min(video::MAX_SIDE) };
    let (w, h) = fit_size(p.size, side);
    let seconds = p.seconds(total);
    let deep = o.codec.deep(o.ten_bit);
    // A GIF keeps few frames a second, so only those are made.
    let rate = if gif { video::GIF_FPS.min(p.fps) } else { p.fps };
    let frames: Vec<u32> = if gif { (0..(seconds * rate).ceil() as u32).map(|n| ((n as f64 * p.fps / rate).round() as u32).min(total - 1)).collect() } else { (0..total).collect() };
    let mut a: Vec<String> = ["-y", "-v", "error", "-f", "rawvideo", "-pix_fmt", video::raw_format(deep)].map(String::from).to_vec();
    a.extend(["-s".into(), format!("{w}x{h}"), "-r".into(), format!("{rate:.6}"), "-i".into(), "pipe:0".into()]);
    if gif {
        a.extend(["-map".into(), "0:v:0".into()]);
        a.extend(video::picture_args(o.codec, false, o.fast));
    } else {
        let (inputs, filter) = mix_graph(&p.parts(), 0.0, seconds, 48000, 1);
        a.extend(inputs);
        a.extend(crate::timeline::graph_args(&filter));
        a.extend(["-map".into(), "0:v:0".into(), "-map".into(), "[mix]".into()]);
        a.extend(video::picture_args(o.codec, o.ten_bit, o.fast));
        a.extend(["-c:a", "aac", "-b:a", "256k"].map(String::from));
    }
    a.extend(["-t".into(), format!("{seconds:.6}"), out.to_string_lossy().to_string()]);
    let mut child = video::tool("ffmpeg").args(a).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn().map_err(|e| video::cannot_start(&e))?;
    let errors = video::drain_errors(child.stderr.take());
    let mut c = Composer::new(scene.clone(), w, h, false, true);
    let mut result = Ok(());
    if let Some(pipe) = child.stdin.take() {
        // This thread grades; the feeder turns finished frames into bytes and writes them.
        let feeder = video::Feeder::start(pipe, w, h, deep);
        for f in frames {
            if cancel.load(Ordering::Relaxed) {
                result = Err("Cancelled".to_string());
                break;
            }
            if !feeder.send(c.frame(f)) {
                break;
            }
            done.store(f + 1, Ordering::Relaxed);
        }
        let fed = feeder.finish();
        if result.is_ok() {
            result = fed;
        }
    }
    let err = errors.join().unwrap_or_default();
    let status = child.wait().map_err(|e| e.to_string())?;
    video::finished(out, result, status.success(), &err, cancel)
}

/// About how long an export will take, in seconds: a few real frames from across the
/// timeline are made at the export's size and timed, and the whole worked out from them.
pub fn estimate(scene: Arc<Scene>, o: &VideoOptions) -> Option<f64> {
    let p = &scene.project;
    let total = p.total();
    if total == 0 {
        return None;
    }
    let gif = o.codec == Codec::Gif;
    let side = if gif { o.side.min(video::GIF_MAX_SIDE) } else { o.side.min(video::MAX_SIDE) };
    let (w, h) = fit_size(p.size, side);
    let frames = if gif { p.seconds(total) * video::GIF_FPS.min(p.fps) } else { total as f64 };
    let mut times = Vec::new();
    for k in ESTIMATE_AT {
        let at = ((total as f64 * k) as u32).min(total - 1);
        let mut c = Composer::new(scene.clone(), w, h, false, true);
        // The first frame starts the decoders, which a running export does once per clip.
        std::hint::black_box(c.frame(at));
        let end = (at + 1 + ESTIMATE_FRAMES).min(total);
        let began = Instant::now();
        for f in at + 1..end {
            std::hint::black_box(c.frame(f));
        }
        if end > at + 1 {
            times.push(began.elapsed().as_secs_f64() / (end - at - 1) as f64);
        }
    }
    times.sort_by(f64::total_cmp);
    let per = *times.get(times.len() / 2)?;
    // Writing the file runs alongside the grading, costing a little on a graphics chip
    // and more on the processor; each clip's decoder also takes a moment to start.
    let writing = if !gif && o.fast && video::chip_for(o.codec, o.ten_bit).is_some() { 1.1 } else { 1.35 };
    Some(per * frames * writing + p.clips.len() as f64 * CLIP_START_SECONDS)
}

/// Exports on its own thread. `done` counts frames; `finished` holds the result.
pub fn start_export(scene: Arc<Scene>, out: PathBuf, o: VideoOptions) -> video::Job {
    let done = Arc::new(AtomicU32::new(0));
    let cancel = Arc::new(AtomicBool::new(false));
    let (tx, finished) = mpsc::channel();
    let total = scene.project.total();
    let (d, c) = (done.clone(), cancel.clone());
    std::thread::spawn(move || {
        let _ = tx.send(export(scene, &out, &o, &d, &c));
    });
    video::Job { done, cancel, total, finished, started: std::time::Instant::now() }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::timeline::{Fx, Item, Media, Wipe};

    /// Two solid-colour stills in a row, a second each at ten frames a second.
    fn scene(edit: impl FnOnce(&mut Project)) -> Arc<Scene> {
        let mut p = Project { fps: 10.0, size: (32, 18), ..Project::default() };
        let mut stills = HashMap::new();
        for c in [[0.8, 0.0, 0.0], [0.0, 0.0, 0.8]] {
            let id = p.new_id();
            p.media.push(Media { id, path: None, name: String::new(), kind: Kind::Photo, seconds: 0.0, info: None, has_audio: false, size: (32, 18) });
            stills.insert(id, Arc::new(Picture { w: 32, h: 18, px: vec![c; 32 * 18] }));
            let i = p.insert_clip(id, 9).unwrap();
            p.clips[i].len = 10;
        }
        edit(&mut p);
        Arc::new(Scene { project: p, bank: LutBank::default(), stills })
    }

    fn colour(s: Arc<Scene>, f: u32, play: bool) -> [f32; 3] {
        Composer::new(s, 32, 18, play, false).frame(f).px[100]
    }

    #[test]
    fn each_clip_shows_in_its_own_time() {
        let s = scene(|_| {});
        assert!(colour(s.clone(), 2, false)[0] > 0.7);
        assert!(colour(s, 15, false)[2] > 0.7);
    }

    #[test]
    fn each_clip_keeps_its_own_grade() {
        let s = scene(|p| p.clips[1].grade.brightness = -0.5);
        assert!(colour(s.clone(), 2, false)[0] > 0.7);
        assert!(colour(s, 15, false)[2] < 0.5);
    }

    #[test]
    fn a_dissolve_really_mixes_the_two_clips() {
        let s = scene(|p| {
            p.set_transition(1, Wipe::Dissolve, 4);
        });
        let c = colour(s, 8, false);
        assert!(c[0] > 0.2 && c[2] > 0.2, "{c:?}");
    }

    #[test]
    fn every_transition_survives_project_save_and_matches_playback_to_export() {
        for kind in Wipe::ALL {
            let s = scene(|p| {
                assert!(p.set_transition(1, kind, 4));
                let saved = serde_json::to_string(p).unwrap();
                *p = serde_json::from_str(&saved).unwrap();
                assert_eq!(p.clips[1].transition.unwrap().kind, kind);
            });
            for f in [6, 7, 8, 9, 10] {
                let export = Composer::new(s.clone(), 32, 18, false, false).frame(f);
                let playback = Composer::new(s.clone(), 32, 18, true, false).frame(f);
                assert!(export.px.iter().zip(&playback.px).all(|(a, b)| (0..3).all(|ch| (a[ch] - b[ch]).abs() < 0.03)), "{kind:?}, frame {f}");
            }
        }
    }

    #[test]
    fn effects_only_touch_their_stretch_of_the_timeline() {
        let s = scene(|p| {
            let id = p.new_id();
            p.items.push(Item { id, row: crate::timeline::ROW_FX, at: 0, len: 5, body: Body::Effect { kind: Fx::BlackWhite, amount: 1.0 } });
        });
        let inside = colour(s.clone(), 2, false);
        assert!((inside[0] - inside[1]).abs() < 1e-4);
        assert!(colour(s, 7, false)[1] < 0.05);
    }

    #[test]
    fn drawn_texts_far_from_the_playhead_are_let_go() {
        // Forty captions a second apart: only those near the frame drawn stay drawn.
        let s = scene(|p| {
            p.fps = 10.0;
            for n in 0..40 {
                let id = p.new_id();
                let body = Body::Text { text: format!("Caption {n}"), size: 40.0, colour: [1.0; 3], x: 0.5, y: 0.8, motion: Default::default() };
                p.items.push(Item { id, row: crate::timeline::ROW_FX2, at: n * 10, len: 10, body });
            }
        });
        let index = Index::of(&s.project);
        let mut laid = Laid::default();
        let mut pic = Picture { w: 320, h: 180, px: vec![[0.0; 3]; 320 * 180] };
        for f in (0..400).step_by(5) {
            laid.apply(&mut pic, &s.project, &index, f, &HashMap::new());
        }
        assert!(laid.texts.len() <= KEEP_DRAWN_ALWAYS + 8, "{} kept", laid.texts.len());
    }

    #[test]
    fn a_hidden_track_shows_nothing() {
        let s = scene(|p| p.tracks[crate::timeline::ROW_VIDEO].hidden = true);
        assert_eq!(colour(s, 2, false), [0.0; 3]);
    }

    #[test]
    fn playback_and_export_agree_on_colour() {
        let s = scene(|p| {
            p.clips[0].grade.exposure = 0.5;
            p.clips[0].grade.saturation = 0.8;
        });
        let (a, b) = (colour(s.clone(), 2, false), colour(s, 2, true));
        assert!((0..3).all(|i| (a[i] - b[i]).abs() < 0.03), "{a:?} {b:?}");
    }

    #[test]
    fn a_heavier_grade_never_gets_a_bigger_picture() {
        let light = scene(|_| {});
        let heavy = scene(|p| {
            for c in p.clips.iter_mut() {
                c.grade.halation_intensity = 1.0;
                c.grade.diffusion.strength = 1.0;
                c.grade.clarity = 1.0;
            }
        });
        assert!(pick_side(&heavy) <= pick_side(&light));
    }

    #[test]
    fn fitted_sizes_are_even_and_never_grow() {
        assert_eq!(fit_size((3840, 2160), 960), (960, 540));
        assert_eq!(fit_size((1081, 721), 3840), (1080, 720));
    }
}
