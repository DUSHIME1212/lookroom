//! Layers: designs brought in from Figma, and the list of everything laid over the picture
//! at the playhead, to show, hide, order and animate. Also the video meme library, whose
//! clips sit over the picture as overlays.

use super::App;
use super::edit::Rail;
use super::kit::*;
use crate::clips::{self, Clip, DEFAULT_KEY, DEFAULT_TOLERANCE};
use crate::graphics;
use crate::icons::{self, Icon};
use crate::player::image_of;
use crate::textanim::{Anim, Motion, Phase, Unit};
use crate::timeline::{Body, Item, Picked, ROW_FX2};
use eframe::egui::{self, Align2, Color32, ColorImage, Context, Rect, Sense, TextureHandle, TextureOptions, Ui, Vec2, pos2, vec2};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

/// How long a design stays on screen when it arrives.
const DESIGN_SECONDS: f64 = 5.0;
const ROW_H: f32 = 34.0;
const THUMB: (usize, usize) = (192, 108);
const CLIP_COLUMNS: usize = 2;
const CLIP_LIST_H: f32 = 380.0;
/// The In animations offered for a whole design at once.
const DESIGN_INS: [Anim; 9] = [Anim::Fade, Anim::SlideUp, Anim::SlideLeft, Anim::Scale, Anim::Pop, Anim::Blur, Anim::Drop, Anim::Swing, Anim::Flip];

type Thumb = (PathBuf, Result<(ColorImage, Option<[f32; 3]>), String>);

pub struct DesignState {
    /// Seconds between one layer arriving and the next, for Animate Every Layer.
    stagger: f32,
    clips: Option<Vec<Clip>>,
    thumbs: HashMap<PathBuf, (TextureHandle, Option<[f32; 3]>)>,
    asked: HashSet<PathBuf>,
    failed: HashSet<PathBuf>,
    news_tx: Sender<Thumb>,
    news_rx: Receiver<Thumb>,
    worker: Option<Sender<PathBuf>>,
}

impl Default for DesignState {
    fn default() -> Self {
        let (news_tx, news_rx) = mpsc::channel();
        Self { stagger: 0.12, clips: None, thumbs: HashMap::new(), asked: HashSet::new(), failed: HashSet::new(), news_tx, news_rx, worker: None }
    }
}

enum LayerAct {
    Pick(u32),
    Hide(usize),
    Swap(usize, usize),
}

fn layer_icon(b: &Body) -> Icon {
    match b {
        Body::Text { .. } => Icon::Text,
        Body::Sticker { .. } => Icon::Sticker,
        Body::Graphic { .. } => Icon::Layers01,
        _ => Icon::Video01,
    }
}

impl App {
    /// Each layer of an SVG design becomes an item at the playhead, in its own place.
    pub(super) fn import_design(&mut self, text: &str) {
        let (file, d) = match graphics::import(text) {
            Ok(v) => v,
            Err(e) => return self.say(e),
        };
        let (pw, ph) = (self.project.size.0 as f32, self.project.size.1 as f32);
        let len = self.project.frames(DESIGN_SECONDS);
        for (i, layer) in d.layers.iter().enumerate() {
            let (x, y) = d.place(i, pw, ph);
            let body = Body::Graphic { file: file.clone(), layer: i, name: layer.name.clone(), size: 1.0, x, y, motion: Motion::default() };
            let (id, at) = (self.project.new_id(), self.head);
            self.project.items.push(Item { id, row: ROW_FX2, at, len, body });
            self.picked = Some(Picked::Item(id));
        }
        self.edit.rail = Rail::Sticker;
        self.show_layers_tab();
        self.say(format!("Added {} layers", d.layers.len()));
    }

    /// Gives every layer of a design the same In, each arriving a little after the one
    /// below it. Their ends stay where they were.
    fn animate_design(&mut self, file: &str, kind: Anim) {
        let gap = self.project.frames(self.edit.design.stagger as f64);
        let first = self.project.items.iter().filter(|t| matches!(&t.body, Body::Graphic { file: f, .. } if f == file)).map(|t| t.at).min();
        let Some(first) = first else { return };
        let mut n = 0;
        for t in self.project.items.iter_mut() {
            let Body::Graphic { file: f, motion, .. } = &mut t.body else { continue };
            if f != file {
                continue;
            }
            motion.enter = Phase { unit: Unit::Whole, ..Phase::preset(kind) };
            let end = t.end();
            t.at = (first + gap * n).min(end.saturating_sub(2));
            t.len = end - t.at;
            n += 1;
        }
    }

    pub(super) fn layers_panel(&mut self, ui: &mut Ui) {
        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(vec2(w, 34.0), Sense::hover());
        if action(ui, r, "Import a Figma or SVG File", true, 12.5).clicked()
            && let Some(path) = rfd::FileDialog::new().add_filter("SVG design", &["svg"]).pick_file()
        {
            match std::fs::read_to_string(&path) {
                Ok(text) => self.import_design(&text),
                Err(e) => self.say(format!("That file could not be read: {e}")),
            }
        }
        note(ui, "From Figma: pick a frame, choose Copy as SVG, and press Ctrl+V here. Each layer arrives on its own, ready to animate.");
        ui.label(egui::RichText::new("Layers at the Playhead").font(bold(14.0)).color(Color32::WHITE));
        ui.add_space(2.0);
        let head = self.head;
        let here: Vec<usize> = (0..self.project.items.len()).filter(|i| {
            let t = &self.project.items[*i];
            t.body.placed().is_some() && head >= t.at && head < t.end()
        }).collect();
        if here.is_empty() {
            note(ui, "Nothing is laid over the picture here. Move the playhead over text, a sticker, a video or a design.");
        }
        let picked = match self.picked {
            Some(Picked::Item(id)) => Some(id),
            _ => None,
        };
        let mut act = None;
        // Top of the list draws on top.
        for (n, &i) in here.iter().enumerate().rev() {
            let t = &self.project.items[i];
            let hidden = t.body.placed().is_some_and(|p| p.2.hidden);
            let (row, resp) = ui.allocate_exact_size(vec2(w, ROW_H), Sense::click());
            let on = picked == Some(t.id);
            let p = ui.painter();
            if on {
                chosen_row(p, row.shrink2(vec2(0.0, 1.0)), 8);
            } else {
                p.rect_filled(row.shrink2(vec2(0.0, 1.0)), 8, if resp.hovered() { BUTTON_EDGE } else { BUTTON });
            }
            let eye = Rect::from_center_size(pos2(row.left() + 18.0, row.center().y), Vec2::splat(28.0));
            if icon_button(ui, eye, if hidden { Icon::ViewOff } else { Icon::View }, "Show or hide", false).clicked() {
                act = Some(LayerAct::Hide(i));
            }
            icons::paint(ui.painter(), layer_icon(&t.body), Rect::from_center_size(pos2(row.left() + 46.0, row.center().y), Vec2::splat(15.0)), if hidden { MUTED } else { Color32::WHITE });
            let tint = if hidden { MUTED } else { Color32::WHITE };
            ui.painter().with_clip_rect(row.shrink2(vec2(60.0, 0.0))).text(pos2(row.left() + 62.0, row.center().y), Align2::LEFT_CENTER, t.label(), semi(12.5), tint);
            if n + 1 < here.len() && icon_button(ui, Rect::from_center_size(pos2(row.right() - 46.0, row.center().y), Vec2::splat(26.0)), Icon::ArrowUp01, "Bring forward", false).clicked() {
                act = Some(LayerAct::Swap(i, here[n + 1]));
            }
            if n > 0 && icon_button(ui, Rect::from_center_size(pos2(row.right() - 18.0, row.center().y), Vec2::splat(26.0)), Icon::ArrowDown01, "Send back", false).clicked() {
                act = Some(LayerAct::Swap(i, here[n - 1]));
            }
            if resp.clicked() && act.is_none() {
                act = Some(LayerAct::Pick(t.id));
            }
        }
        match act {
            Some(LayerAct::Pick(id)) => self.picked = Some(Picked::Item(id)),
            Some(LayerAct::Hide(i)) => {
                if let Some((_, _, m)) = self.project.items[i].body.placed_mut() {
                    m.hidden ^= true;
                }
            }
            Some(LayerAct::Swap(a, b)) => self.project.items.swap(a, b),
            None => {}
        }
        ui.add_space(6.0);
        let mut animate = None;
        if let Some(t) = self.picked_item()
            && let Body::Graphic { name, size, file, .. } = &mut t.body
        {
            ui.add(egui::TextEdit::singleline(name).hint_text("Layer name").desired_width(f32::INFINITY));
            slider(ui, "Size", size, 0.1, 3.0, 1.0, Fmt::Percent);
            let file = file.clone();
            ui.label(egui::RichText::new("Animate Every Layer").font(bold(14.0)).color(Color32::WHITE));
            note(ui, "The same In for the whole design, each layer arriving after the one below it.");
            slider(ui, "Stagger (s)", &mut self.edit.design.stagger, 0.0, 0.5, 0.12, Fmt::Plain);
            let mut chips = Chips::new(ui, 28.0);
            for a in DESIGN_INS {
                if chips.add(a.name(), false) {
                    animate = Some((file.clone(), a));
                }
            }
            chips.end();
        }
        if let Some((file, a)) = animate {
            self.animate_design(&file, a);
        }
        if self.picked_item().is_some_and(|t| t.body.placed().is_some()) {
            self.animation_section(ui);
            self.keyframe_section(ui);
        }
    }

    fn take_clip_news(&mut self, ctx: &Context) {
        while let Ok((path, got)) = self.edit.design.news_rx.try_recv() {
            match got {
                Ok((img, key)) => {
                    let tex = ctx.load_texture(format!("clip-{}", path.display()), img, TextureOptions::LINEAR);
                    self.edit.design.thumbs.insert(path, (tex, key));
                }
                Err(_) => {
                    self.edit.design.failed.insert(path);
                }
            }
        }
    }

    /// One worker reads first frames in the order clips come into view.
    fn want_clip(&mut self, path: &PathBuf) {
        if !self.edit.design.asked.insert(path.clone()) {
            return;
        }
        let worker = self.edit.design.worker.get_or_insert_with(|| {
            let (tx, rx) = mpsc::channel::<PathBuf>();
            let news = self.edit.design.news_tx.clone();
            std::thread::spawn(move || {
                for p in rx {
                    let got = crate::video::still(&p).map(|pic| (image_of(&pic.cover(THUMB.0, THUMB.1)), clips::green_screen(&pic)));
                    if news.send((p, got)).is_err() {
                        return;
                    }
                }
            });
            tx
        });
        let _ = worker.send(path.clone());
    }

    /// The video meme library: the person's own clips, laid over the picture when picked.
    pub(super) fn clip_library(&mut self, ui: &mut Ui) {
        self.take_clip_news(&ui.ctx().clone());
        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(vec2(w, 34.0), Sense::hover());
        let third = (w - 12.0) / 3.0;
        let at = |n: f32| Rect::from_min_size(pos2(r.left() + (third + 6.0) * n, r.top()), vec2(third, r.height()));
        if action(ui, at(0.0), "Add Clips", true, 12.5).clicked()
            && let Some(files) = rfd::FileDialog::new().add_filter("Video", &crate::video::EXTENSIONS).pick_files()
        {
            match clips::add(&files) {
                Ok(n) => self.say(format!("Added {n} clips to the library")),
                Err(e) => self.say(format!("Those clips could not be added: {e}")),
            }
            self.edit.design.clips = None;
        }
        if action(ui, at(1.0), "Open Folder", false, 12.5).clicked() {
            crate::platform::reveal(&clips::dir());
        }
        if action(ui, at(2.0), "Refresh", false, 12.5).clicked() {
            self.edit.design.clips = None;
        }
        note(ui, "Your video memes. Add clips you have the right to use. A green screen is found and taken out, so the meme sits over your video. Folders become groups.");
        let list = self.edit.design.clips.get_or_insert_with(clips::list).clone();
        if list.is_empty() {
            note(ui, "The library is empty. Add clips, or drop them in the folder.");
        } else {
            let cell = w / CLIP_COLUMNS as f32;
            let tall = cell * THUMB.1 as f32 / THUMB.0 as f32 + 34.0;
            let mut chosen = None;
            let mut wanted = Vec::new();
            egui::ScrollArea::vertical().id_salt("clips").max_height(CLIP_LIST_H).auto_shrink([false, true]).show_rows(ui, tall, list.len().div_ceil(CLIP_COLUMNS), |ui, range| {
                for r in range {
                    let (row, _) = ui.allocate_exact_size(vec2(w, tall), Sense::hover());
                    for c in 0..CLIP_COLUMNS {
                        let Some(clip) = list.get(r * CLIP_COLUMNS + c) else { break };
                        let b = Rect::from_min_size(pos2(row.left() + cell * c as f32, row.top()), vec2(cell, tall)).shrink(3.0);
                        let pic = Rect::from_min_max(b.min, pos2(b.right(), b.bottom() - 30.0));
                        let resp = ui.interact(b, ui.scope_id().with(("clip", clip.path.to_string_lossy().as_ref())), Sense::click());
                        let p = ui.painter();
                        p.rect_filled(pic, 8, if resp.hovered() { BUTTON_EDGE } else { BUTTON });
                        match self.edit.design.thumbs.get(&clip.path) {
                            Some((tex, key)) => {
                                p.image(tex.id(), pic, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                                if key.is_some() {
                                    let tag = Rect::from_min_size(pic.min + vec2(6.0, 6.0), vec2(84.0, 18.0));
                                    p.rect_filled(tag, 9, Color32::from_black_alpha(170));
                                    p.text(tag.center(), Align2::CENTER_CENTER, "Green screen", semi(10.5), Color32::from_rgb(0x7C, 0xE0, 0x8A));
                                }
                                if resp.clicked() {
                                    chosen = Some((clip.clone(), *key));
                                }
                            }
                            None => {
                                let failed = self.edit.design.failed.contains(&clip.path);
                                let why = if failed { "Couldn't read this clip" } else { "Loading…" };
                                p.text(pic.center(), Align2::CENTER_CENTER, why, regular(11.0), if failed { ERROR } else { MUTED });
                                wanted.push(clip.path.clone());
                            }
                        }
                        let label = ui.painter().with_clip_rect(b);
                        label.text(pos2(b.left() + 2.0, b.bottom() - 21.0), Align2::LEFT_CENTER, &clip.name, semi(11.5), Color32::WHITE);
                        if !clip.group.is_empty() {
                            label.text(pos2(b.left() + 2.0, b.bottom() - 7.0), Align2::LEFT_CENTER, &clip.group, regular(10.5), MUTED);
                        }
                    }
                }
            });
            let waiting = !wanted.is_empty();
            for p in wanted {
                self.want_clip(&p);
            }
            if waiting {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
            }
            if let Some((clip, key)) = chosen {
                self.add_overlay(&clip, key);
            }
        }
        ui.add_space(6.0);
        if let Some(t) = self.picked_item()
            && let Body::Overlay { size, key, tolerance, volume, .. } = &mut t.body
        {
            ui.label(egui::RichText::new("Video Over the Picture").font(bold(14.0)).color(Color32::WHITE));
            slider(ui, "Size", size, 0.1, 1.5, 1.0, Fmt::Percent);
            let mut on = key.is_some();
            if toggle(ui, "Remove Green Screen", &mut on) {
                *key = on.then_some(DEFAULT_KEY);
            }
            if key.is_some() {
                slider(ui, "Key Tolerance", tolerance, 0.02, 0.5, DEFAULT_TOLERANCE, Fmt::Plain);
            }
            slider(ui, "Volume", volume, 0.0, 1.0, 1.0, Fmt::Percent);
            self.animation_section(ui);
            self.keyframe_section(ui);
        }
    }

    /// A library clip over the picture at the playhead. Green-screen clips fill the frame;
    /// others sit in a corner, picture in picture.
    fn add_overlay(&mut self, clip: &Clip, key: Option<[f32; 3]>) {
        let known = self.project.media.iter().find(|m| m.path.as_deref() == Some(clip.path.as_path())).map(|m| m.id);
        let media = match known {
            Some(id) => id,
            None => match self.add_media(&clip.path) {
                Ok(id) => id,
                Err(e) => return self.say(format!("That clip could not be opened: {e}")),
            },
        };
        let seconds = self.project.media_of(media).map_or(3.0, |m| m.seconds);
        let len = self.project.frames(seconds).max(2);
        let keyed = key.is_some();
        let (size, x, y) = if keyed { (1.0, 0.5, 0.5) } else { (0.42, 0.74, 0.72) };
        let body = Body::Overlay { media, start: 0.0, size, x, y, key, tolerance: DEFAULT_TOLERANCE, volume: 1.0, motion: Motion::default() };
        let (id, at) = (self.project.new_id(), self.head);
        self.project.items.push(Item { id, row: ROW_FX2, at, len, body });
        self.picked = Some(Picked::Item(id));
        self.say(format!("Added {}", clip.name));
    }
}
