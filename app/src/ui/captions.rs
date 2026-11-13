//! The Captions tab of the Text page: Auto Captions, the plugin that comes with the app.
//! It listens to the timeline, puts each line on the text track as a caption, and lists
//! them as a transcript to correct.

use super::App;
use super::kit::*;
use crate::captions::{self, Cue, Job, LANGUAGES, MODELS};
use crate::textanim::{Anim, Motion, Phase, Unit};
use crate::timeline::{Body, Item, ROW_FX2};
use eframe::egui::{self, Align2, Color32, Context, Rect, Sense, Ui, pos2, vec2};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, Receiver};

/// What every caption from this tab carries, so they can be found again as a set.
pub const GROUP: &str = "captions";
pub(super) const STYLES: [&str; 5] = ["Clean", "Bold", "Pop", "Rise", "Typewriter"];
const DEFAULT_LINE: f32 = 42.0;
/// Captions sit low and centred, where viewers look for them.
const CAPTION_Y: f32 = 0.86;
const TRANSCRIPT_H: f32 = 300.0;

enum Task {
    Download,
    Listen,
}

struct Work {
    task: Task,
    progress: Arc<AtomicU32>,
    stop: Arc<AtomicBool>,
    done: Receiver<Result<Vec<Cue>, String>>,
}

pub struct CaptionsState {
    pub tab: usize,
    model: usize,
    language: usize,
    whole: bool,
    style: usize,
    line: f32,
    replace: bool,
    translate: bool,
    work: Option<Work>,
    problem: Option<String>,
}

impl Default for CaptionsState {
    fn default() -> Self {
        Self { tab: 0, model: 1, language: 0, whole: true, style: 0, line: DEFAULT_LINE, replace: true, translate: false, work: None, problem: None }
    }
}

/// A caption's look in one of the styles. Each style animates by words at most, never so
/// much that reading falls behind.
pub(super) fn caption_motion(style: usize) -> Motion {
    let quick = |kind: Anim, unit: Unit, seconds: f32, delay: f32| Phase { unit, seconds, delay, ..Phase::preset(kind) };
    let (weight, outline, enter) = match style {
        1 => (800, 0.08, Phase::default()),
        2 => (700, 0.06, quick(Anim::Pop, Unit::Word, 0.22, 0.05)),
        3 => (700, 0.06, quick(Anim::WordRise, Unit::Word, 0.3, 0.06)),
        4 => (600, 0.06, quick(Anim::Typewriter, Unit::Letter, 0.02, 0.03)),
        _ => (600, 0.05, Phase::default()),
    };
    Motion { weight, outline, enter, group: GROUP.into(), ..Motion::default() }
}

fn caption_size(style: usize) -> f32 {
    if style == 1 { 78.0 } else { 64.0 }
}

pub(super) fn is_caption(t: &Item) -> bool {
    matches!(&t.body, Body::Text { motion, .. } if motion.group == GROUP)
}

fn bar(ui: &mut Ui, share: f32, label: &str) {
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(vec2(w, 28.0), Sense::hover());
    let p = ui.painter();
    let track = Rect::from_min_size(pos2(r.left(), r.top() + 18.0), vec2(w, 6.0));
    p.rect_filled(track, 3, TRACK);
    p.rect_filled(Rect::from_min_size(track.min, vec2(w * share.clamp(0.0, 1.0), 6.0)), 3, ACCENT);
    p.text(pos2(r.left(), r.top() + 6.0), Align2::LEFT_CENTER, label, semi(12.0), TEXT_2);
}

pub(super) fn clock(seconds: f64) -> String {
    let s = seconds.max(0.0);
    format!("{}:{:04.1}", (s / 60.0) as u32, s % 60.0)
}

impl App {
    /// Takes in a finished download or transcription, wherever the person is in the app.
    pub(super) fn poll_captions(&mut self, ctx: &Context) {
        let Some(w) = &self.edit.captions.work else { return };
        ctx.request_repaint_after(std::time::Duration::from_millis(120));
        let Ok(result) = w.done.try_recv() else { return };
        let listened = matches!(w.task, Task::Listen);
        self.edit.captions.work = None;
        match result {
            Ok(cues) if listened => self.place_captions(&cues),
            Ok(_) => self.say("The model is ready"),
            Err(e) if e == "Stopped." => {}
            Err(e) => self.edit.captions.problem = Some(e),
        }
    }

    /// The span Auto Captions listens to: the whole timeline, or the clip at the playhead.
    fn caption_span(&self) -> (u32, u32) {
        let total = self.project.total();
        if self.edit.captions.whole {
            return (0, total);
        }
        let index = self.timeline_index();
        match self.project.nearest(self.head) {
            Some(i) => (index.starts[i], index.starts[i] + self.project.clips[i].len),
            None => (0, total),
        }
    }

    fn start_listening(&mut self) {
        let m = &MODELS[self.edit.captions.model];
        let (from, to) = self.caption_span();
        if to <= from {
            self.edit.captions.problem = Some("There is nothing on the timeline to listen to yet.".into());
            return;
        }
        let parts = self.project.parts();
        if parts.is_empty() {
            self.edit.captions.problem = Some("Nothing on the timeline has sound.".into());
            return;
        }
        let c = &self.edit.captions;
        let job = Job {
            parts,
            from: self.project.seconds(from),
            seconds: self.project.seconds(to - from),
            model: captions::model_path(m),
            language: LANGUAGES[c.language].1.to_string(),
            max_len: c.line.round() as u32,
            translate: c.translate,
        };
        let (progress, stop) = (Arc::new(AtomicU32::new(0)), Arc::new(AtomicBool::new(false)));
        let (tx, done) = mpsc::channel();
        let (p, s) = (progress.clone(), stop.clone());
        std::thread::spawn(move || {
            let _ = tx.send(captions::transcribe(&job, p, s));
        });
        self.edit.captions.problem = None;
        self.edit.captions.work = Some(Work { task: Task::Listen, progress, stop, done });
    }

    /// Auto Captions started for the assistant, with the first model already downloaded.
    pub(super) fn listen_for(&mut self, whole: bool) -> Result<String, String> {
        if !captions::engine_ready() {
            return Err("this copy of ffmpeg can't make captions".into());
        }
        if self.edit.captions.work.is_some() {
            return Err("Auto Captions is already working".into());
        }
        if !captions::have(&MODELS[self.edit.captions.model]) {
            self.edit.captions.model = MODELS.iter().position(captions::have).ok_or("no speech model is downloaded yet; the person can download one on Text, Captions")?;
        }
        self.edit.captions.whole = whole;
        self.start_listening();
        match self.edit.captions.problem.clone() {
            Some(p) => Err(p),
            None => Ok("Listening for captions; they appear when it finishes".into()),
        }
    }

    fn start_download(&mut self) {
        let m = self.edit.captions.model;
        let (progress, stop) = (Arc::new(AtomicU32::new(0)), Arc::new(AtomicBool::new(false)));
        let (tx, done) = mpsc::channel();
        let (p, s) = (progress.clone(), stop.clone());
        std::thread::spawn(move || {
            let _ = tx.send(captions::download(&MODELS[m], p, s).map(|_| Vec::new()));
        });
        self.edit.captions.problem = None;
        self.edit.captions.work = Some(Work { task: Task::Download, progress, stop, done });
    }

    /// Each heard line becomes a text item on the text track. A caption ends where the
    /// next begins, so two are never on screen together.
    fn place_captions(&mut self, cues: &[Cue]) {
        if cues.is_empty() {
            self.edit.captions.problem = Some("No speech was heard. Is the sound muted, or is it music only?".into());
            return;
        }
        let (from, to) = self.caption_span();
        if self.edit.captions.replace {
            self.project.items.retain(|t| !(is_caption(t) && t.at < to && t.end() > from));
        }
        let style = self.edit.captions.style;
        let mut made = Vec::new();
        for (n, c) in cues.iter().enumerate() {
            let at = self.project.frames(c.start);
            let mut end = self.project.frames(c.end).max(at + 2);
            if let Some(next) = cues.get(n + 1) {
                end = end.min(self.project.frames(next.start).max(at + 2));
            }
            let id = self.project.new_id();
            let body = Body::Text { text: c.text.clone(), size: caption_size(style), colour: [1.0; 3], x: 0.5, y: CAPTION_Y, motion: caption_motion(style) };
            self.project.items.push(Item { id, row: ROW_FX2, at, len: end - at, body });
            made.push(id);
        }
        // A project with a brand kit gets captions in the kit's look.
        self.brand_new_captions(&made);
        // Selected together, so the Text tab changes them all straight away.
        self.select_captions();
        self.say(format!("Added {} captions", cues.len()));
    }

    /// Every caption selected together, in time order, the one at the playhead shown.
    pub(super) fn select_captions(&mut self) {
        let mut all: Vec<(u32, u32, u32)> = self.project.items.iter().filter(|t| is_caption(t)).map(|t| (t.at, t.end(), t.id)).collect();
        all.sort_unstable();
        let head = self.head;
        let shown = all.iter().position(|(a, b, _)| head >= *a && head < *b).unwrap_or(0);
        if !all.is_empty() {
            // The one shown goes first, so the panel opens on it.
            all.swap(0, shown);
        }
        self.select_these(all.into_iter().map(|c| c.2).collect());
    }

    fn restyle_captions(&mut self) {
        let style = self.edit.captions.style;
        for t in self.project.items.iter_mut().filter(|t| is_caption(t)) {
            if let Body::Text { size, motion, .. } = &mut t.body {
                // The words' font and keyframes stay; the style sets the rest.
                let (font, keys) = (motion.font.clone(), std::mem::take(&mut motion.keys));
                *motion = Motion { font, keys, ..caption_motion(style) };
                *size = caption_size(style);
            }
        }
    }

    pub(super) fn captions_page(&mut self, ui: &mut Ui) {
        ui.label(egui::RichText::new("Auto Captions").font(bold(15.0)).color(Color32::WHITE));
        note(ui, "Listens to the timeline's sound and writes what is said onto it as captions. Whisper runs on this computer; nothing is uploaded.");
        if !captions::engine_ready() {
            error_note(ui, "This copy of ffmpeg was built without Whisper. Install a recent full build (ffmpeg 8 or later) to make captions.");
            return;
        }
        let busy = self.edit.captions.work.is_some();
        ui.label(egui::RichText::new("Model").font(semi(12.5)).color(Color32::WHITE));
        let names: Vec<&str> = MODELS.iter().map(|m| m.name).collect();
        let mut model = self.edit.captions.model;
        if segmented(ui, &names, &mut model) && !busy {
            self.edit.captions.model = model;
        }
        let m = &MODELS[self.edit.captions.model];
        note(ui, &format!("{}. {} MB, downloaded once.", m.note, m.megabytes));
        let w = ui.available_width();
        if let Some(work) = &self.edit.captions.work {
            let (label, share) = match work.task {
                Task::Download => {
                    let kb = work.progress.load(Ordering::Relaxed) as f32;
                    (format!("Downloading {}: {:.0} of {} MB", m.name, kb / 1024.0, m.megabytes), kb / 1024.0 / m.megabytes as f32)
                }
                Task::Listen => {
                    let p = work.progress.load(Ordering::Relaxed) as f32 / 1000.0;
                    (format!("Listening: {:.0}%", p * 100.0), p)
                }
            };
            bar(ui, share, &label);
            let (r, _) = ui.allocate_exact_size(vec2(w, 32.0), Sense::hover());
            if action(ui, r, "Cancel", false, 12.5).clicked() {
                work.stop.store(true, Ordering::Relaxed);
            }
        } else if !captions::have(m) {
            let (r, _) = ui.allocate_exact_size(vec2(w, 36.0), Sense::hover());
            if action(ui, r, &format!("Download {} ({} MB)", m.name, m.megabytes), true, 13.0).clicked() {
                self.start_download();
            }
        }
        if let Some(p) = &self.edit.captions.problem {
            error_note(ui, &p.clone());
        }
        ui.add_space(4.0);
        let langs: Vec<&str> = LANGUAGES.iter().map(|l| l.0).collect();
        let (r, _) = ui.allocate_exact_size(vec2(w, 52.0), Sense::hover());
        dropdown(ui, r, Some("Spoken language"), &langs, &mut self.edit.captions.language);
        ui.add_space(4.0);
        toggle(ui, "Translate into English", &mut self.edit.captions.translate);
        ui.label(egui::RichText::new("Listen to").font(semi(12.5)).color(Color32::WHITE));
        let mut scope = if self.edit.captions.whole { 0 } else { 1 };
        segmented(ui, &["Whole timeline", "Clip at the playhead"], &mut scope);
        self.edit.captions.whole = scope == 0;
        ui.label(egui::RichText::new("Style").font(semi(12.5)).color(Color32::WHITE));
        let mut chips = Chips::new(ui, 28.0);
        for (i, s) in STYLES.iter().enumerate() {
            if chips.add(s, self.edit.captions.style == i) {
                self.edit.captions.style = i;
            }
        }
        chips.end();
        slider(ui, "Longest line (characters)", &mut self.edit.captions.line, 16.0, 80.0, DEFAULT_LINE, Fmt::Int);
        toggle(ui, "Replace earlier captions", &mut self.edit.captions.replace);
        ui.add_space(4.0);
        let ready = captions::have(m) && !busy;
        let (r, _) = ui.allocate_exact_size(vec2(w, 38.0), Sense::hover());
        if ready {
            if action(ui, r, "Generate Captions", true, 13.5).clicked() {
                self.start_listening();
            }
        } else {
            ui.painter().rect_filled(r, 19, BUTTON);
            let why = if busy { "Working…" } else { "Download a model first" };
            ui.painter().text(r.center(), Align2::CENTER_CENTER, why, semi(13.5), MUTED);
        }
        self.transcript(ui);
    }

    /// Every caption in time order, its words editable in place; a time jumps there.
    fn transcript(&mut self, ui: &mut Ui) {
        let mut list: Vec<(usize, u32)> = self.project.items.iter().enumerate().filter(|(_, t)| is_caption(t)).map(|(i, t)| (i, t.at)).collect();
        if list.is_empty() {
            return;
        }
        list.sort_by_key(|x| x.1);
        ui.add_space(8.0);
        ui.label(egui::RichText::new(format!("Transcript ({})", list.len())).font(bold(14.0)).color(Color32::WHITE));
        note(ui, "Fix a word here and the caption changes. A time moves the playhead there.");
        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(vec2(w, 36.0), Sense::hover());
        if action(ui, r, "Edit All Captions", true, 13.0).on_hover_text("Select every caption and open Text: font, size, colour, place and animation change on all of them").clicked() {
            self.select_captions();
            self.edit.captions.tab = 0;
        }
        let (r, _) = ui.allocate_exact_size(vec2(w, 32.0), Sense::hover());
        let half = Rect::from_min_size(r.min, vec2(w / 2.0 - 3.0, r.height()));
        if action(ui, half, "Restyle All", false, 12.5).on_hover_text("Give every caption the style chosen above").clicked() {
            self.restyle_captions();
        }
        if action(ui, Rect::from_min_size(pos2(half.right() + 6.0, r.top()), half.size()), "Remove All", false, 12.5).clicked() {
            self.project.items.retain(|t| !is_caption(t));
            return;
        }
        ui.add_space(4.0);
        let fps = self.project.fps;
        let mut go = None;
        egui::ScrollArea::vertical().id_salt("transcript").max_height(TRANSCRIPT_H).auto_shrink([false, true]).show(ui, |ui| {
            for (i, at) in list {
                ui.horizontal(|ui| {
                    let when = clock(at as f64 / fps.max(1.0));
                    if ui.add(egui::Button::new(egui::RichText::new(when).font(regular(11.5)).color(ACCENT)).frame(false)).clicked() {
                        go = Some(at);
                    }
                    if let Body::Text { text, .. } = &mut self.project.items[i].body {
                        ui.add(egui::TextEdit::singleline(text).desired_width(f32::INFINITY));
                    }
                });
            }
        });
        if let Some(f) = go {
            self.seek_head(f);
        }
    }
}

/// The Auto Captions line in Settings' plugin list.
pub fn plugin_row(ui: &mut Ui) {
    let ready: Vec<&str> = MODELS.iter().filter(|m| captions::have(m)).map(|m| m.name).collect();
    let state = if !captions::engine_ready() {
        "needs ffmpeg with Whisper".to_string()
    } else if ready.is_empty() {
        "no model downloaded yet".to_string()
    } else {
        format!("models: {}", ready.join(", "))
    };
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(vec2(w, 54.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(r, 4, BUTTON);
    crate::icons::paint(p, crate::icons::Icon::ClosedCaption, Rect::from_center_size(pos2(r.left() + 20.0, r.center().y), egui::Vec2::splat(18.0)), ACCENT);
    p.text(pos2(r.left() + 40.0, r.top() + 17.0), Align2::LEFT_CENTER, "Auto Captions, comes with the app", semi(13.0), Color32::WHITE);
    p.text(pos2(r.left() + 40.0, r.top() + 37.0), Align2::LEFT_CENTER, format!("Whisper speech to text, on this computer; {state}"), regular(11.5), MUTED);
    ui.add_space(4.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_caption_style_marks_its_captions() {
        for s in 0..STYLES.len() {
            let m = caption_motion(s);
            assert_eq!(m.group, GROUP);
            assert!(m.outline > 0.0, "captions need an edge to read over any picture");
        }
    }

    #[test]
    fn clock_reads_minutes_and_tenths() {
        assert_eq!(clock(62.25), "1:02.2");
        assert_eq!(clock(5.0), "0:05.0");
    }
}
