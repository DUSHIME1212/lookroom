//! The libraries in the window: emoji stickers, sound effects and meme templates, and
//! the tiles that preview each effect and transition on the picture being edited.

use super::App;
use super::kit::*;
use crate::assets::{self, EMOJI_GROUPS, SOUND_PACKS};
use crate::audio;
use crate::memes::{self, Template};
use crate::overlay;
use crate::pipeline::Picture;
use crate::player::image_of;
use crate::shared::Lru;
use crate::textanim::Motion;
use crate::timeline::{Body, Fx, Item, Part, Picked, ROW_FX, ROW_FX2, ROW_MUSIC, Wipe};
use eframe::egui::{self, Align2, Color32, ColorImage, Rect, Sense, Stroke, StrokeKind, TextureHandle, TextureOptions, Ui, Vec2, pos2, vec2};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Instant;

const EMOJI_COLUMNS: usize = 6;
const EMOJI_PX: u32 = 64;
const EMOJI_LIST_H: f32 = 330.0;
const EMOJI_KEPT: usize = 400;
/// Drawing an emoji takes about a millisecond, so only this many new ones a frame.
const EMOJI_A_FRAME: usize = 12;
const SOUND_ROW_H: f32 = 38.0;
const SOUND_LIST_H: f32 = 360.0;
const MEME_COLUMNS: usize = 3;
const MEME_THUMB: u32 = 200;
const MEME_LIST_H: f32 = 420.0;
const MEME_CAPTION_SIZE: f32 = 120.0;
/// The size the effect and transition tiles are drawn at.
const FX_W: usize = 192;
const FX_H: usize = 108;
const FX_COLUMNS: usize = 3;
/// A hovered tile plays this long, then holds the end before it starts again.
const FX_RUN: f32 = 1.4;
const FX_HOLD: f32 = 0.5;
/// Frames shown before and after a transition when previewing it on the picture.
const PREVIEW_MARGIN: u32 = 12;

enum MemeNews {
    List(Result<Vec<Template>, String>),
    Picture(String, Result<ColorImage, String>),
}

pub struct ExtrasState {
    transition_search: String,
    transition_category: usize,
    sticker_tab: usize,
    emoji_group: usize,
    emoji_search: String,
    emoji_tex: Lru<usize, TextureHandle>,
    pub sound_tab: usize,
    sound_pack: usize,
    sound_search: String,
    sound_feed: Option<audio::Feed>,
    playing: Option<(usize, Instant, f64)>,
    memes: Option<Result<Vec<Template>, String>>,
    meme_loading: bool,
    news_tx: Sender<MemeNews>,
    news_rx: Receiver<MemeNews>,
    fetcher: Option<Sender<Template>>,
    meme_thumbs: HashMap<String, TextureHandle>,
    meme_asked: HashSet<String>,
    meme_failed: HashMap<String, String>,
    /// The two pictures every effect and transition tile shows, and what they came from.
    samples: Option<(u64, Arc<Picture>, Arc<Picture>)>,
    still: HashMap<String, TextureHandle>,
    live: Option<(String, TextureHandle)>,
    hover_started: Option<(String, f64)>,
}

impl Default for ExtrasState {
    fn default() -> Self {
        let (news_tx, news_rx) = mpsc::channel();
        Self {
            transition_search: String::new(),
            transition_category: 0,
            sticker_tab: 0,
            emoji_group: 0,
            emoji_search: String::new(),
            emoji_tex: Lru::new(EMOJI_KEPT),
            sound_tab: 0,
            sound_pack: 0,
            sound_search: String::new(),
            sound_feed: None,
            playing: None,
            memes: None,
            meme_loading: false,
            news_tx,
            news_rx,
            fetcher: None,
            meme_thumbs: HashMap::new(),
            meme_asked: HashSet::new(),
            meme_failed: HashMap::new(),
            samples: None,
            still: HashMap::new(),
            live: None,
            hover_started: None,
        }
    }
}

fn image_of_stamp(s: &overlay::Stamp) -> ColorImage {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    ColorImage::new([s.w, s.h], s.px.iter().map(|c| Color32::from_rgba_premultiplied(b(c[0]), b(c[1]), b(c[2]), b(c[3]))).collect())
}

/// A classic meme caption: Impact, white, with a black edge.
pub fn meme_caption(text: &str, y: f32) -> Body {
    let motion = Motion { font: "Impact".into(), weight: 400, outline: 0.07, ..Motion::default() };
    Body::Text { text: text.into(), size: MEME_CAPTION_SIZE, colour: [1.0; 3], x: 0.5, y, motion }
}

fn heading(ui: &mut Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new(text).font(bold(14.0)).color(Color32::WHITE));
    ui.add_space(2.0);
}

impl App {
    /// Places an item at the playhead and picks it.
    fn put(&mut self, row: usize, len: u32, body: Body) {
        let (id, at) = (self.project.new_id(), self.head);
        self.project.items.push(Item { id, row, at, len: len.max(2), body });
        self.picked = Some(Picked::Item(id));
    }

    pub(super) fn show_clips_tab(&mut self) {
        self.edit.extras.sticker_tab = 3;
    }

    pub(super) fn show_layers_tab(&mut self) {
        self.edit.extras.sticker_tab = 4;
    }

    pub(super) fn sticker_tabs(&mut self, ui: &mut Ui) -> usize {
        let mut tab = self.edit.extras.sticker_tab;
        segmented(ui, &["Emoji", "Icons", "Memes", "Clips", "Layers"], &mut tab);
        self.edit.extras.sticker_tab = tab;
        tab
    }

    /// Fluent Emoji, by group or by name.
    pub(super) fn emoji_library(&mut self, ui: &mut Ui) {
        ui.add(egui::TextEdit::singleline(&mut self.edit.extras.emoji_search).hint_text("Search emoji").desired_width(f32::INFINITY));
        ui.add_space(4.0);
        let mut chips = Chips::new(ui, 26.0);
        let mut group = self.edit.extras.emoji_group;
        if chips.add("All", group == 0) {
            group = 0;
        }
        for (i, g) in EMOJI_GROUPS.iter().enumerate() {
            let short = g.split(" & ").next().unwrap_or(g);
            if chips.add(short, group == i + 1) {
                group = i + 1;
            }
        }
        chips.end();
        self.edit.extras.emoji_group = group;
        let needle = self.edit.extras.emoji_search.trim().to_lowercase().replace(' ', "-");
        let all = assets::emoji();
        let shown: Vec<usize> = (0..all.len()).filter(|i| (group == 0 || all[*i].group == EMOJI_GROUPS[group - 1]) && (needle.is_empty() || all[*i].name.contains(&needle))).collect();
        if shown.is_empty() {
            note(ui, "No emoji has that in its name.");
            return;
        }
        let cell = ui.available_width() / EMOJI_COLUMNS as f32;
        let rows = shown.len().div_ceil(EMOJI_COLUMNS);
        let mut drawn = 0;
        let mut chosen = None;
        egui::ScrollArea::vertical().id_salt("emoji").max_height(EMOJI_LIST_H).auto_shrink([false, true]).show_rows(ui, cell, rows, |ui, range| {
            for r in range {
                let (row, _) = ui.allocate_exact_size(vec2(cell * EMOJI_COLUMNS as f32, cell), Sense::hover());
                for c in 0..EMOJI_COLUMNS {
                    let Some(&i) = shown.get(r * EMOJI_COLUMNS + c) else { break };
                    let b = Rect::from_min_size(pos2(row.left() + cell * c as f32, row.top()), Vec2::splat(cell)).shrink(3.0);
                    let resp = ui.interact(b, ui.scope_id().with(("emoji", i)), Sense::click()).on_hover_text(all[i].label());
                    if resp.hovered() {
                        ui.painter().rect_filled(b, 8, BUTTON_EDGE);
                    }
                    let tex = match self.edit.extras.emoji_tex.get(&i) {
                        Some(t) => Some(t),
                        None if drawn < EMOJI_A_FRAME => {
                            drawn += 1;
                            assets::emoji_stamp(all[i].name, EMOJI_PX).map(|s| {
                                let t = ui.ctx().load_texture(format!("emoji-{i}"), image_of_stamp(&s), TextureOptions::LINEAR);
                                self.edit.extras.emoji_tex.put(i, t.clone());
                                t
                            })
                        }
                        None => {
                            ui.ctx().request_repaint();
                            None
                        }
                    };
                    if let Some(t) = tex {
                        let side = b.width() * 0.78;
                        ui.painter().image(t.id(), Rect::from_center_size(b.center(), Vec2::splat(side)), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                    }
                    if resp.clicked() {
                        chosen = Some(i);
                    }
                }
            }
        });
        note(ui, &format!("{} emoji. Fluent Emoji by Microsoft, MIT licence.", shown.len()));
        if let Some(i) = chosen {
            let len = self.project.frames(crate::timeline::ITEM_SECONDS);
            let body = Body::Sticker { icon: 0, emoji: all[i].name.to_string(), size: 220.0, x: 0.5, y: 0.5, motion: Motion::default() };
            self.put(ROW_FX2, len, body);
        }
    }

    /// Sound effects to hear and drop on the timeline at the playhead.
    pub(super) fn sound_library(&mut self, ui: &mut Ui) {
        ui.add(egui::TextEdit::singleline(&mut self.edit.extras.sound_search).hint_text("Search sounds").desired_width(f32::INFINITY));
        ui.add_space(4.0);
        let mut pack = self.edit.extras.sound_pack;
        let mut chips = Chips::new(ui, 26.0);
        if chips.add("All", pack == 0) {
            pack = 0;
        }
        for (i, p) in SOUND_PACKS.iter().enumerate() {
            if chips.add(p, pack == i + 1) {
                pack = i + 1;
            }
        }
        chips.end();
        self.edit.extras.sound_pack = pack;
        if self.edit.extras.playing.is_some_and(|(_, began, secs)| began.elapsed().as_secs_f64() > secs) {
            self.edit.extras.playing = None;
            self.edit.extras.sound_feed = None;
        }
        let needle = self.edit.extras.sound_search.trim().to_lowercase();
        let all = assets::sounds();
        let shown: Vec<usize> = (0..all.len()).filter(|i| (pack == 0 || all[*i].pack == SOUND_PACKS[pack - 1]) && (needle.is_empty() || all[*i].name.to_lowercase().contains(&needle))).collect();
        let playing = self.edit.extras.playing.map(|p| p.0);
        let (mut hear, mut add) = (None, None);
        egui::ScrollArea::vertical().id_salt("sounds").max_height(SOUND_LIST_H).auto_shrink([false, true]).show_rows(ui, SOUND_ROW_H, shown.len(), |ui, range| {
            for n in range {
                let i = shown[n];
                let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), SOUND_ROW_H), Sense::hover());
                let r = row.shrink2(vec2(0.0, 2.0));
                ui.painter().rect_filled(r, 8, BUTTON);
                let on = playing == Some(i);
                let play = Rect::from_center_size(pos2(r.left() + 18.0, r.center().y), Vec2::splat(28.0));
                let icon = if on { crate::icons::Icon::Pause } else { crate::icons::Icon::Play };
                if icon_button(ui, play, icon, "Listen", on).clicked() {
                    hear = Some(i);
                }
                let p = ui.painter().with_clip_rect(r.shrink2(vec2(40.0, 0.0)));
                p.text(pos2(r.left() + 38.0, r.center().y - 7.0), Align2::LEFT_CENTER, all[i].name, semi(12.5), Color32::WHITE);
                p.text(pos2(r.left() + 38.0, r.center().y + 8.0), Align2::LEFT_CENTER, all[i].pack, regular(10.5), MUTED);
                let plus = Rect::from_center_size(pos2(r.right() - 18.0, r.center().y), Vec2::splat(28.0));
                if icon_button(ui, plus, crate::icons::Icon::Add01, "Add at the playhead", false).clicked() {
                    add = Some(i);
                }
            }
        });
        if playing.is_some() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        }
        note(ui, &format!("{} sounds. For Editing is made by the app; Interface, Digital and Foley are Kenney's packs, CC0.", shown.len()));
        if let Some(i) = hear {
            self.hear(i);
        }
        if let Some(i) = add {
            self.add_sound(i);
        }
    }

    fn hear(&mut self, i: usize) {
        if self.edit.extras.playing.is_some_and(|p| p.0 == i) {
            self.edit.extras.playing = None;
            self.edit.extras.sound_feed = None;
            return;
        }
        if self.run.is_some() {
            self.stop_play();
        }
        let s = assets::sounds()[i];
        let (path, seconds) = match assets::sound_path(&s).and_then(|p| Ok((p.clone(), crate::video::sound_seconds(&p)?))) {
            Ok(v) => v,
            Err(e) => return self.say(format!("That sound could not be read: {e}")),
        };
        let Some(out) = self.audio_out.as_ref() else { return self.say("No sound device was found.") };
        let part = Part { path, at: 0.0, start: 0.0, len: seconds, volume: 1.0, speed: 1.0, denoise: 0.0 };
        let feed = audio::Feed::start(out, vec![part], 0.0, seconds);
        out.start_together();
        self.edit.extras.sound_feed = Some(feed);
        self.edit.extras.playing = Some((i, Instant::now(), seconds));
    }

    fn add_sound(&mut self, i: usize) {
        let s = assets::sounds()[i];
        let path = match assets::sound_path(&s) {
            Ok(p) => p,
            Err(e) => return self.say(format!("That sound could not be written: {e}")),
        };
        let known = self.project.media.iter().find(|m| m.path.as_deref() == Some(path.as_path())).map(|m| (m.id, m.seconds));
        let (media, seconds) = match known {
            Some(v) => v,
            None => match self.add_media(&path) {
                Ok(id) => (id, self.project.media_of(id).map_or(1.0, |m| m.seconds)),
                Err(e) => return self.say(format!("That sound could not be added: {e}")),
            },
        };
        let len = self.project.frames(seconds);
        self.put(ROW_MUSIC, len, Body::Audio { media, start: 0.0, volume: 1.0, name: s.name.to_string(), speed: 1.0, denoise: 0.0 });
        self.say(format!("Added {}", s.name));
    }

    fn take_meme_news(&mut self, ctx: &egui::Context) {
        while let Ok(n) = self.edit.extras.news_rx.try_recv() {
            match n {
                MemeNews::List(l) => {
                    self.edit.extras.meme_loading = false;
                    self.edit.extras.memes = Some(l);
                }
                MemeNews::Picture(id, Ok(img)) => {
                    let t = ctx.load_texture(format!("meme-{id}"), img, TextureOptions::LINEAR);
                    self.edit.extras.meme_thumbs.insert(id, t);
                }
                MemeNews::Picture(id, Err(e)) => {
                    self.edit.extras.meme_failed.insert(id, e);
                }
            }
        }
    }

    fn fetch_memes(&mut self) {
        self.edit.extras.meme_loading = true;
        let tx = self.edit.extras.news_tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(MemeNews::List(memes::list()));
        });
    }

    /// One worker fetches pictures in the order they come into view.
    fn want_meme(&mut self, t: &Template) {
        if !self.edit.extras.meme_asked.insert(t.id.clone()) {
            return;
        }
        let fetcher = self.edit.extras.fetcher.get_or_insert_with(|| {
            let (tx, rx) = mpsc::channel::<Template>();
            let news = self.edit.extras.news_tx.clone();
            std::thread::spawn(move || {
                for t in rx {
                    let made = memes::image(&t).and_then(|p| image::open(&p).map_err(|e| e.to_string())).map(|img| {
                        let small = img.thumbnail(MEME_THUMB, MEME_THUMB).to_rgba8();
                        ColorImage::from_rgba_unmultiplied([small.width() as usize, small.height() as usize], small.as_raw())
                    });
                    if news.send(MemeNews::Picture(t.id.clone(), made)).is_err() {
                        return;
                    }
                }
            });
            tx
        });
        let _ = fetcher.send(t.clone());
    }

    /// Imgflip's most used templates, fetched only when asked for.
    pub(super) fn meme_library(&mut self, ui: &mut Ui) {
        self.take_meme_news(&ui.ctx().clone());
        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(vec2(w, 34.0), Sense::hover());
        if action(ui, r, "Add a Meme Caption", true, 12.5).on_hover_text("Top and bottom text in the classic style").clicked() {
            let len = self.project.frames(crate::timeline::ITEM_SECONDS);
            self.put(ROW_FX2, len, meme_caption("TOP TEXT", 0.1));
            self.put(ROW_FX, len, meme_caption("BOTTOM TEXT", 0.88));
        }
        ui.add_space(6.0);
        heading(ui, "Meme Templates");
        note(ui, "The 100 most used templates on Imgflip. Each picture downloads when shown and belongs to its owner.");
        if self.edit.extras.meme_loading {
            note(ui, "Fetching the list…");
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
            return;
        }
        let list = match &self.edit.extras.memes {
            None => {
                let (r, _) = ui.allocate_exact_size(vec2(w, 34.0), Sense::hover());
                if action(ui, r, "Show Templates", false, 12.5).clicked() {
                    self.fetch_memes();
                }
                return;
            }
            Some(Err(e)) => {
                error_note(ui, &format!("The list could not be fetched: {e}"));
                let (r, _) = ui.allocate_exact_size(vec2(w, 34.0), Sense::hover());
                if action(ui, r, "Try Again", false, 12.5).clicked() {
                    self.fetch_memes();
                }
                return;
            }
            Some(Ok(l)) => l.clone(),
        };
        let cell = w / MEME_COLUMNS as f32;
        let tall = cell + 20.0;
        let mut chosen = None;
        let mut wanted = Vec::new();
        egui::ScrollArea::vertical().id_salt("memes").max_height(MEME_LIST_H).auto_shrink([false, true]).show_rows(ui, tall, list.len().div_ceil(MEME_COLUMNS), |ui, range| {
            for r in range {
                let (row, _) = ui.allocate_exact_size(vec2(w, tall), Sense::hover());
                for c in 0..MEME_COLUMNS {
                    let Some(t) = list.get(r * MEME_COLUMNS + c) else { break };
                    let b = Rect::from_min_size(pos2(row.left() + cell * c as f32, row.top()), vec2(cell, tall)).shrink(3.0);
                    let pic = Rect::from_min_max(b.min, pos2(b.right(), b.bottom() - 18.0));
                    let resp = ui.interact(b, ui.scope_id().with(("meme", t.id.as_str())), Sense::click()).on_hover_text(&t.name);
                    let p = ui.painter();
                    p.rect_filled(pic, 8, if resp.hovered() { BUTTON_EDGE } else { BUTTON });
                    match self.edit.extras.meme_thumbs.get(&t.id) {
                        Some(tex) => {
                            let s = tex.size_vec2();
                            let fit = (pic.width() / s.x).min(pic.height() / s.y);
                            p.image(tex.id(), Rect::from_center_size(pic.center(), s * fit), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                            if resp.clicked() {
                                chosen = Some(t.clone());
                            }
                        }
                        None => {
                            let failed = self.edit.extras.meme_failed.contains_key(&t.id);
                            let why = if failed { "Couldn't load" } else { "Loading…" };
                            p.text(pic.center(), Align2::CENTER_CENTER, why, regular(11.0), if failed { ERROR } else { MUTED });
                            wanted.push(t.clone());
                        }
                    }
                    ui.painter().with_clip_rect(b).text(pos2(b.center().x, b.bottom() - 8.0), Align2::CENTER_CENTER, &t.name, regular(10.5), Color32::from_gray(210));
                }
            }
        });
        for t in wanted {
            self.want_meme(&t);
        }
        if !self.edit.extras.meme_asked.iter().all(|id| self.edit.extras.meme_thumbs.contains_key(id) || self.edit.extras.meme_failed.contains_key(id)) {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        }
        if let Some(t) = chosen {
            self.add_meme(&t);
        }
    }

    /// The template as a clip at the end of the timeline, with a caption top and bottom.
    fn add_meme(&mut self, t: &Template) {
        let Some(path): Option<PathBuf> = memes::image_path(t).filter(|p| p.exists()) else { return };
        let before = self.project.clips.len();
        self.import(&path, true);
        if self.project.clips.len() == before {
            return;
        }
        let i = self.project.clips.len() - 1;
        let (at, len) = (self.project.starts()[i], self.project.clips[i].len);
        self.head = at;
        self.put(ROW_FX2, len, meme_caption("TOP TEXT", 0.1));
        self.put(ROW_FX, len, meme_caption("BOTTOM TEXT", 0.88));
        self.say(format!("Added {}. Pick a caption to change its words.", t.name));
    }

    /// The pair of pictures the tiles show: the frame being edited, and the sample scene.
    fn fx_samples(&mut self) -> (Arc<Picture>, Arc<Picture>) {
        let id = self.picture_id;
        match &self.edit.extras.samples {
            Some((seen, a, b)) if *seen == id => (a.clone(), b.clone()),
            _ => {
                let a = Arc::new(self.preview.cover(FX_W, FX_H));
                let b = Arc::new(crate::scene::sample_picture(FX_W, FX_H));
                self.edit.extras.samples = Some((id, a.clone(), b.clone()));
                self.edit.extras.still.clear();
                (a, b)
            }
        }
    }

    /// A grid of tiles, each drawn by \`draw\` on the two sample pictures. A tile shows its
    /// middle at rest, and plays from start to end while the pointer is over it.
    fn fx_tiles(&mut self, ui: &mut Ui, library: &str, list: &[(String, bool)], draw: &dyn Fn(usize, &Picture, &Picture, f32) -> Picture) -> Option<usize> {
        let (a, b) = self.fx_samples();
        let w = ui.available_width();
        let gap = 6.0;
        let cell = (w - gap * (FX_COLUMNS - 1) as f32) / FX_COLUMNS as f32;
        let pic_h = cell * FX_H as f32 / FX_W as f32;
        let tall = pic_h + 20.0;
        let rows = list.len().div_ceil(FX_COLUMNS);
        let (area, _) = ui.allocate_exact_size(vec2(w, rows as f32 * (tall + gap)), Sense::hover());
        let mut chosen = None;
        let mut hovered = None;
        for (i, (name, on)) in list.iter().enumerate() {
            let r = Rect::from_min_size(pos2(area.left() + (cell + gap) * (i % FX_COLUMNS) as f32, area.top() + (tall + gap) * (i / FX_COLUMNS) as f32), vec2(cell, tall));
            if !ui.is_rect_visible(r) {
                continue;
            }
            let pic = Rect::from_min_size(r.min, vec2(cell, pic_h));
            let resp = ui.interact(r, ui.scope_id().with(("fx", name.as_str())), Sense::click()).on_hover_text(name);
            let cache_key = format!("{library}:{name}");
            let tex = if resp.hovered() {
                hovered = Some(i);
                let time = ui.input(|i| i.time);
                let started = self.edit.extras.hover_started.get_or_insert_with(|| (cache_key.clone(), time));
                if started.0 != cache_key {
                    *started = (cache_key.clone(), time);
                }
                let lap = FX_RUN + FX_HOLD;
                let t = (((time - started.1) as f32 % lap) / FX_RUN).min(1.0);
                let img = image_of(&draw(i, &a, &b, t));
                match &mut self.edit.extras.live {
                    Some((key, tex)) if *key == cache_key => {
                        tex.set(img, TextureOptions::LINEAR);
                        tex.clone()
                    }
                    slot => {
                        let tex = ui.ctx().load_texture("fx-live", img, TextureOptions::LINEAR);
                        *slot = Some((cache_key.clone(), tex.clone()));
                        tex
                    }
                }
            } else {
                match self.edit.extras.still.get(&cache_key) {
                    Some(t) => t.clone(),
                    None => {
                        let tex = ui.ctx().load_texture(format!("fx-{name}"), image_of(&draw(i, &a, &b, 0.5)), TextureOptions::LINEAR);
                        self.edit.extras.still.insert(cache_key, tex.clone());
                        tex
                    }
                }
            };
            let p = ui.painter();
            p.image(tex.id(), pic, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            let ring = if *on { Stroke::new(2.0, ACCENT) } else if resp.hovered() { Stroke::new(1.5, Color32::WHITE) } else { Stroke::new(1.0, BUTTON_EDGE) };
            p.rect_stroke(pic, 6, ring, StrokeKind::Inside);
            p.with_clip_rect(r).text(pos2(r.center().x, r.bottom() - 9.0), Align2::CENTER_CENTER, name, semi(11.0), if *on { Color32::WHITE } else { MUTED });
            if resp.clicked() {
                chosen = Some(i);
            }
        }
        if hovered.is_some() {
            ui.ctx().request_repaint();
        } else {
            self.edit.extras.hover_started = None;
        }
        ui.add_space(4.0);
        chosen
    }

    pub(super) fn transition_tiles(&mut self, ui: &mut Ui, now: Option<Wipe>) -> Option<Wipe> {
        use crate::transitions::CATEGORIES;
        ui.add(egui::TextEdit::singleline(&mut self.edit.extras.transition_search)
            .hint_text("Search transitions…").desired_width(f32::INFINITY));
        let mut chips = Chips::new(ui, 26.0);
        for (i, name) in CATEGORIES.iter().enumerate() {
            if chips.add(name, self.edit.extras.transition_category == i) {
                self.edit.extras.transition_category = i;
            }
        }
        chips.end();
        let query = self.edit.extras.transition_search.trim().to_lowercase();
        let category = CATEGORIES[self.edit.extras.transition_category];
        let kinds: Vec<Wipe> = Wipe::ALL.iter().copied().filter(|k| {
            (category == "All" || k.category() == category)
                && format!("{} {}", k.name(), k.category()).to_lowercase().contains(&query)
        }).collect();
        note(ui, &format!("{} of {} transitions · hover to preview", kinds.len(), Wipe::ALL.len()));
        if kinds.is_empty() {
            note(ui, "No matches. Try another name or choose All.");
            return None;
        }
        let list: Vec<(String, bool)> = kinds.iter().map(|k| (k.name().to_string(), Some(*k) == now)).collect();
        let grid_height = (ui.clip_rect().bottom() - ui.cursor().top() - 8.0).clamp(80.0, 360.0);
        egui::ScrollArea::vertical().id_salt("transition-catalogue").max_height(grid_height)
            .auto_shrink([false, true]).show(ui, |ui| {
                self.fx_tiles(ui, "transition", &list, &|i, a, b, t| overlay::blend(a, b, kinds[i], t)).map(|i| kinds[i])
            }).inner
    }

    pub(super) fn effect_tiles(&mut self, ui: &mut Ui, now: Option<Fx>) -> Option<Fx> {
        let list: Vec<(String, bool)> = Fx::ALL.iter().map(|k| (k.name().to_string(), Some(*k) == now)).collect();
        let draw = |i: usize, a: &Picture, _: &Picture, t: f32| {
            let mut p = a.clone();
            overlay::effect(&mut p, Fx::ALL[i], 1.0, t, (t * 48.0) as u32);
            p
        };
        self.fx_tiles(ui, "effect", &list, &draw).map(|i| Fx::ALL[i])
    }

    /// Where everything that came from elsewhere came from, with its licence.
    pub(super) fn credits_section(&mut self, ui: &mut Ui) {
        heading(ui, "Credits and Licences");
        const CREDITS: [(&str, &str); 5] = [
            ("Icons: Hugeicons, MIT", include_str!("../../assets/hugeicons/LICENSE.md")),
            ("Stickers: Fluent Emoji by Microsoft, MIT", include_str!("../../assets/stickers/LICENSE.md")),
            ("Interface sounds: Kenney, CC0", include_str!("../../assets/sounds/interface/LICENSE.txt")),
            ("Digital sounds: Kenney, CC0", include_str!("../../assets/sounds/digital/LICENSE.txt")),
            ("Foley sounds: Kenney, CC0", include_str!("../../assets/sounds/foley/LICENSE.txt")),
        ];
        for (title, text) in CREDITS {
            egui::CollapsingHeader::new(egui::RichText::new(title).font(semi(12.5)).color(Color32::WHITE)).id_salt(title).show(ui, |ui| {
                ui.label(egui::RichText::new(text.trim()).font(regular(11.0)).color(MUTED));
            });
        }
        note(ui, "Meme templates are fetched from Imgflip when you browse them. Their pictures belong to their owners.");
        note(ui, "Auto Captions uses OpenAI's Whisper models (MIT) as converted by whisper.cpp (MIT), run by ffmpeg's whisper filter. The model you choose is downloaded once from Hugging Face.");
    }

    /// Plays a stretch of the timeline and stops at its end.
    fn play_between(&mut self, from: u32, to: u32) {
        if self.run.is_some() {
            self.stop_play();
        }
        self.head = from;
        self.start_play("a preview");
        self.play_until = Some(to);
    }

    /// Plays the picked effect, or the transition nearest the playhead, on the picture.
    pub(super) fn preview_fx(&mut self) {
        if let Some(Picked::Item(id)) = self.picked
            && let Some(t) = self.project.items.iter().find(|t| t.id == id)
        {
            let range = (t.at, t.end());
            return self.play_between(range.0, range.1);
        }
        let Some(cut) = self.project.nearest_cut(self.head) else {
            return self.say("Add a transition or an effect first.");
        };
        let Some(len) = self.project.clips[cut].transition.map(|t| t.len) else {
            return self.say("That cut has no transition yet.");
        };
        let at = self.project.starts()[cut];
        self.play_between(at.saturating_sub(PREVIEW_MARGIN), at + len + PREVIEW_MARGIN);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_meme_caption_is_bold_white_and_edged() {
        let Body::Text { colour, motion, .. } = meme_caption("HI", 0.1) else { panic!() };
        assert_eq!(colour, [1.0; 3]);
        assert_eq!(motion.font, "Impact");
        assert!(motion.outline > 0.0);
    }
}
