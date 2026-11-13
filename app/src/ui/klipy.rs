//! KLIPY on the Clips tab: trending video memes and a search, each shown by a still from
//! its preview. A click fetches the clip and lays it over the picture at the playhead,
//! green screen taken out when it has one, just like a clip from the person's own library.

use super::App;
use super::kit::*;
use crate::klipy::{self, Clip, Page};
use eframe::egui::{self, Align2, Color32, ColorImage, Context, Key, Rect, Sense, TextureHandle, TextureOptions, Ui, pos2, vec2};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};

const COLUMNS: usize = 2;
const THUMB: (u32, u32) = (192, 108);
const LIST_H: f32 = 420.0;

type Added = Result<(PathBuf, String, Option<[f32; 3]>), String>;

pub struct KlipyState {
    /// KLIPY or the person's own clips, on the Clips tab.
    pub source: usize,
    query: String,
    /// What the shown results are for: empty is trending.
    shown_for: Option<String>,
    clips: Vec<Clip>,
    page: u32,
    has_next: bool,
    loading: Option<Receiver<Result<Page, String>>>,
    thumbs: HashMap<String, TextureHandle>,
    asked: HashSet<String>,
    failed: HashSet<String>,
    thumb_tx: Sender<(String, Option<ColorImage>)>,
    thumb_rx: Receiver<(String, Option<ColorImage>)>,
    worker: Option<Sender<Clip>>,
    /// The clip being fetched to go on the timeline.
    adding: Option<(String, Receiver<Added>)>,
    key_input: String,
    problem: Option<String>,
}

impl Default for KlipyState {
    fn default() -> Self {
        let (thumb_tx, thumb_rx) = mpsc::channel();
        Self {
            source: 0,
            query: String::new(),
            shown_for: None,
            clips: Vec::new(),
            page: 0,
            has_next: false,
            loading: None,
            thumbs: HashMap::new(),
            asked: HashSet::new(),
            failed: HashSet::new(),
            thumb_tx,
            thumb_rx,
            worker: None,
            adding: None,
            key_input: String::new(),
            problem: None,
        }
    }
}

impl App {
    pub(super) fn klipy_library(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        self.take_klipy_news(&ctx);
        let Some(key) = klipy::key(&self.settings.klipy_key) else {
            return self.klipy_key_form(ui);
        };
        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(vec2(w, 32.0), Sense::hover());
        let field = Rect::from_min_max(r.min, pos2(r.right() - 88.0, r.bottom()));
        let edit = ui.put(field, egui::TextEdit::singleline(&mut self.edit.klipy.query).hint_text("Search KLIPY").align(Align2::LEFT_CENTER));
        let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
        let go = Rect::from_min_max(pos2(field.right() + 6.0, r.top()), r.max);
        if action(ui, go, "Search", true, 12.5).clicked() || enter {
            self.klipy_load(&key, 1);
        }
        ui.painter().text(pos2(r.left(), r.bottom() + 9.0), Align2::LEFT_CENTER, "Powered by KLIPY", semi(10.5), MUTED);
        ui.add_space(16.0);
        if self.edit.klipy.shown_for.is_none() && self.edit.klipy.loading.is_none() {
            self.klipy_load(&key, 1);
        }
        if let Some(p) = self.edit.klipy.problem.clone() {
            error_note(ui, &p);
        }
        let heading = match self.edit.klipy.shown_for.as_deref() {
            Some("") | None => "Trending".to_string(),
            Some(q) => format!("Results for \"{q}\""),
        };
        ui.label(egui::RichText::new(heading).font(semi(12.5)).color(Color32::WHITE));
        self.klipy_grid(ui);
        let busy = self.edit.klipy.loading.is_some();
        if busy {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(egui::RichText::new("Loading…").font(regular(12.0)).color(MUTED));
            });
        } else if self.edit.klipy.has_next {
            let (r, _) = ui.allocate_exact_size(vec2(w, 32.0), Sense::hover());
            if action(ui, r, "Load More", false, 12.5).clicked() {
                let next = self.edit.klipy.page + 1;
                self.klipy_load(&key, next);
            }
        }
    }

    fn klipy_key_form(&mut self, ui: &mut Ui) {
        note(ui, "Trending and searchable video memes come from KLIPY. Paste a KLIPY API key from partner.klipy.com to turn them on. It is kept on this computer.");
        ui.add(egui::TextEdit::singleline(&mut self.edit.klipy.key_input).password(true).hint_text("KLIPY API key").desired_width(f32::INFINITY));
        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(vec2(w, 34.0), Sense::hover());
        let ready = !self.edit.klipy.key_input.trim().is_empty();
        if action(ui, r, "Save Key", ready, 12.5).clicked() && ready {
            self.settings.klipy_key = std::mem::take(&mut self.edit.klipy.key_input).trim().to_string();
            crate::store::save_settings(&self.settings);
        }
    }

    /// A page of trending clips, or of a search when something is typed. Page 1 starts over.
    fn klipy_load(&mut self, key: &str, page: u32) {
        let query = self.edit.klipy.query.trim().to_string();
        if page == 1 {
            self.edit.klipy.clips.clear();
            self.edit.klipy.shown_for = Some(query.clone());
        }
        self.edit.klipy.problem = None;
        self.edit.klipy.page = page;
        let customer = self.klipy_customer();
        let key = key.to_string();
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let got = if query.is_empty() { klipy::trending(&key, &customer, page) } else { klipy::search(&key, &customer, &query, page) };
            let _ = tx.send(got);
        });
        self.edit.klipy.loading = Some(rx);
    }

    /// One anonymous id for this computer, so KLIPY can keep a person's recent clips.
    fn klipy_customer(&mut self) -> String {
        if self.settings.klipy_customer.is_empty() {
            self.settings.klipy_customer = format!("lookroom-{}", crate::store::new_id());
            crate::store::save_settings(&self.settings);
        }
        self.settings.klipy_customer.clone()
    }

    fn klipy_grid(&mut self, ui: &mut Ui) {
        let w = ui.available_width();
        let cell = w / COLUMNS as f32;
        let tall = cell * THUMB.1 as f32 / THUMB.0 as f32 + 26.0;
        let clips = self.edit.klipy.clips.clone();
        let adding = self.edit.klipy.adding.as_ref().map(|a| a.0.clone());
        let mut chosen = None;
        let mut wanted = Vec::new();
        egui::ScrollArea::vertical().id_salt("klipy").max_height(LIST_H).auto_shrink([false, true]).show_rows(ui, tall, clips.len().div_ceil(COLUMNS), |ui, range| {
            for row in range {
                let (r, _) = ui.allocate_exact_size(vec2(w, tall), Sense::hover());
                for col in 0..COLUMNS {
                    let Some(c) = clips.get(row * COLUMNS + col) else { break };
                    let b = Rect::from_min_size(pos2(r.left() + cell * col as f32, r.top()), vec2(cell, tall)).shrink(3.0);
                    let pic = Rect::from_min_max(b.min, pos2(b.right(), b.bottom() - 22.0));
                    let resp = ui.interact(b, ui.scope_id().with(("klipy", &c.id)), Sense::click()).on_hover_text(&c.title);
                    let p = ui.painter();
                    p.rect_filled(pic, 4, if resp.hovered() { BUTTON_EDGE } else { BUTTON });
                    match self.edit.klipy.thumbs.get(&c.id) {
                        Some(t) => {
                            p.image(t.id(), pic, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                        }
                        None => {
                            let failed = self.edit.klipy.failed.contains(&c.id);
                            p.text(pic.center(), Align2::CENTER_CENTER, if failed { "No preview" } else { "Loading…" }, regular(11.0), MUTED);
                            if !failed {
                                wanted.push(c.clone());
                            }
                        }
                    }
                    if adding.as_deref() == Some(c.id.as_str()) {
                        p.rect_filled(pic, 4, Color32::from_black_alpha(150));
                        p.text(pic.center(), Align2::CENTER_CENTER, "Adding…", semi(12.0), Color32::WHITE);
                    }
                    p.with_clip_rect(b).text(pos2(b.left() + 2.0, b.bottom() - 10.0), Align2::LEFT_CENTER, &c.title, semi(11.5), Color32::WHITE);
                    if resp.clicked() {
                        chosen = Some(c.clone());
                    }
                }
            }
        });
        if !wanted.is_empty() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(120));
        }
        for c in wanted {
            self.want_klipy_thumb(c);
        }
        if let Some(c) = chosen
            && self.edit.klipy.adding.is_none()
        {
            self.klipy_add(c);
        }
    }

    /// One worker reads previews in the order they come into view.
    fn want_klipy_thumb(&mut self, c: Clip) {
        if !self.edit.klipy.asked.insert(c.id.clone()) {
            return;
        }
        let tx = self.edit.klipy.thumb_tx.clone();
        let worker = self.edit.klipy.worker.get_or_insert_with(|| {
            let (jobs, rx) = mpsc::channel::<Clip>();
            std::thread::spawn(move || {
                for c in rx {
                    let img = klipy::preview(&c).ok().and_then(|p| still_of(&p));
                    if tx.send((c.id, img)).is_err() {
                        return;
                    }
                }
            });
            jobs
        });
        let _ = worker.send(c);
    }

    /// Fetches the clip, looks for a green screen on its first frame, then lays it over.
    fn klipy_add(&mut self, c: Clip) {
        let (tx, rx) = mpsc::channel();
        let id = c.id.clone();
        std::thread::spawn(move || {
            let got = klipy::download(&c).map(|path| {
                let key = crate::video::still(&path).ok().and_then(|pic| crate::clips::green_screen(&pic));
                (path, c.title.clone(), key)
            });
            let _ = tx.send(got);
        });
        self.edit.klipy.adding = Some((id, rx));
    }

    fn take_klipy_news(&mut self, ctx: &Context) {
        while let Ok((id, img)) = self.edit.klipy.thumb_rx.try_recv() {
            match img {
                Some(img) => {
                    let tex = ctx.load_texture(format!("klipy-{id}"), img, TextureOptions::LINEAR);
                    self.edit.klipy.thumbs.insert(id, tex);
                }
                None => {
                    self.edit.klipy.failed.insert(id);
                }
            }
        }
        if let Some(rx) = &self.edit.klipy.loading {
            match rx.try_recv() {
                Ok(Ok(page)) => {
                    self.edit.klipy.loading = None;
                    self.edit.klipy.has_next = page.has_next;
                    if page.clips.is_empty() && self.edit.klipy.clips.is_empty() {
                        self.edit.klipy.problem = Some("Nothing found. Try other words.".into());
                    }
                    self.edit.klipy.clips.extend(page.clips);
                }
                Ok(Err(e)) => {
                    self.edit.klipy.loading = None;
                    self.edit.klipy.problem = Some(e);
                }
                Err(mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(100)),
                Err(mpsc::TryRecvError::Disconnected) => self.edit.klipy.loading = None,
            }
        }
        if let Some((_, rx)) = &self.edit.klipy.adding {
            match rx.try_recv() {
                Ok(got) => {
                    self.edit.klipy.adding = None;
                    match got {
                        Ok((path, name, key)) => self.add_overlay(&crate::clips::Clip { path, name, group: "KLIPY".into() }, key),
                        Err(e) => self.edit.klipy.problem = Some(e),
                    }
                }
                Err(mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(100)),
                Err(mpsc::TryRecvError::Disconnected) => self.edit.klipy.adding = None,
            }
        }
    }
}

/// The first frame of a preview, cropped to fill a tile.
fn still_of(path: &std::path::Path) -> Option<ColorImage> {
    let img = image::open(path).ok()?.resize_to_fill(THUMB.0, THUMB.1, image::imageops::FilterType::Triangle).to_rgba8();
    Some(ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw()))
}
