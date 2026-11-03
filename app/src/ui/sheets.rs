//! Sheets that open over the editor, and the jobs that run behind them.

use super::kit::*;
use super::keys::{GROUPS, KEYS};
use super::{App, short};
use crate::export::{self, Batch, Format, MAX_BATCH};
use crate::icons::{self, Icon};
use crate::store::{self, UserPreset};
use crate::track;
use crate::player;
use crate::video::{self, Codec};
use eframe::egui::{self, Align2, Area, Color32, Context, Id, Order, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, Receiver};

const SHEET_W: f32 = 380.0;
const KEYS_SHEET_W: f32 = 760.0;
const KEYS_SHEET_H: f32 = 620.0;
const RESOLUTIONS: [(&str, usize); 4] = [("4K", 3840), ("1440p", 2560), ("1080p", 1920), ("720p", 1280)];

#[derive(Clone, PartialEq, Debug)]
pub enum Sheet {
    Home,
    Export,
    Batch,
    History,
    Keys,
    SavePreset,
    Rename(String),
    About(String, String),
}

impl Sheet {
    pub fn named(s: &str) -> Option<Sheet> {
        match s {
            "home" => Some(Sheet::Home),
            "export" => Some(Sheet::Export),
            "batch" => Some(Sheet::Batch),
            "history" => Some(Sheet::History),
            "keys" => Some(Sheet::Keys),
            _ => None,
        }
    }
}

/// Long work happening in the background, shown as a bar over the photo.
pub enum Task {
    Photo(Receiver<Result<(), String>>),
    Clip(video::Job),
    Batch(Batch),
}

/// The shortcut table, grouped into two columns.
fn keys_sheet(ui: &mut Ui) {
    let column = (ui.available_width() - 24.0) / 2.0;
    let group = |ui: &mut Ui, name: &str| {
        ui.add_space(6.0);
        ui.label(egui::RichText::new(name).font(bold(13.5)).color(Color32::WHITE));
        for b in KEYS.iter().filter(|b| b.group == name) {
            let (r, _) = ui.allocate_exact_size(vec2(column, 23.0), Sense::hover());
            let w = text_width(ui, b.keys, semi(11.5)) + 14.0;
            let chip = Rect::from_min_size(pos2(r.left(), r.top() + 2.0), vec2(w, 19.0));
            let p = ui.painter();
            p.rect_filled(chip, 5, BUTTON);
            p.text(chip.center(), Align2::CENTER_CENTER, b.keys, semi(11.5), Color32::WHITE);
            p.text(pos2(r.left() + 100.0, r.center().y), Align2::LEFT_CENTER, b.label, regular(12.5), TEXT_2);
        }
    };
    egui::ScrollArea::vertical().max_height(KEYS_SHEET_H).show(ui, |ui| {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(column);
                ui.spacing_mut().item_spacing.y = 1.0;
                group(ui, GROUPS[0]);
                group(ui, GROUPS[2]);
                group(ui, GROUPS[5]);
            });
            ui.add_space(20.0);
            ui.vertical(|ui| {
                ui.set_width(column);
                ui.spacing_mut().item_spacing.y = 1.0;
                group(ui, GROUPS[1]);
                group(ui, GROUPS[3]);
                group(ui, GROUPS[4]);
            });
        });
    });
}

impl App {
    pub(super) fn tick_tasks(&mut self, ctx: &Context) {
        if let Some((i, mi, run)) = &self.tracking {
            let (i, mi) = (*i, *mi);
            let mut ended = None;
            while let Ok(u) = run.updates.try_recv() {
                match u {
                    track::Update::Key(k) => {
                        if let Some(m) = self.grade.layers.get_mut(i).and_then(|l| l.masks.get_mut(mi)) {
                            m.set_key(k);
                        }
                    }
                    track::Update::Finished(r) => ended = Some(r),
                }
            }
            if let Some(r) = ended {
                self.tracking = None;
                match r {
                    Ok(()) => self.say("Tracking finished"),
                    Err(e) if e == "Cancelled" => {}
                    Err(e) => self.say(e),
                }
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(60));
        }
        let finished = match &self.task {
            Some(Task::Photo(rx)) => rx.try_recv().ok().map(|r| r.map(|_| "Export Successful!".to_string())),
            Some(Task::Clip(job)) => job.finished.try_recv().ok().map(|r| r.map(|_| "Export Successful!".to_string())),
            Some(Task::Batch(b)) => b.finished.try_recv().ok().map(|problems| match problems.first() {
                None => Ok(format!("{} files exported", b.total)),
                Some(p) => Err(format!("{} of {} failed. {p}", problems.len(), b.total)),
            }),
            None => None,
        };
        if let Some(r) = finished {
            self.task = None;
            match r {
                Ok(t) => self.say(t),
                Err(e) if e == "Cancelled" => self.say("Export cancelled"),
                Err(e) => self.say(format!("Export failed: {e}")),
            }
        } else if self.task.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }

    pub(super) fn task_view(&mut self, ui: &mut Ui, _ctx: &Context, photo: Rect) {
        let tracking = self.tracking.as_ref().map(|(_, _, run)| ("Tracking", run.done.load(Ordering::Relaxed) as f32 / run.total.max(1) as f32));
        let (label, done) = match (&self.task, tracking) {
            (Some(Task::Photo(_)), _) => ("Exporting".to_string(), None),
            (Some(Task::Clip(j)), _) => {
                let d = j.done.load(Ordering::Relaxed);
                (format!("Exporting frame {d} of {}", j.total), Some(d as f32 / j.total.max(1) as f32))
            }
            (Some(Task::Batch(b)), _) => {
                let d = b.done.load(Ordering::Relaxed);
                (format!("Batch: {d} of {} done", b.total), Some(d as f32 / b.total.max(1) as f32))
            }
            (None, Some((name, f))) => (name.to_string(), Some(f)),
            (None, None) => return,
        };
        let w = 240.0f32.min(photo.width() - 20.0);
        let r = Rect::from_center_size(pos2(photo.center().x, photo.bottom() - 34.0), vec2(w, 48.0));
        let p = ui.painter();
        p.rect_filled(r, 14, MENU);
        p.rect_stroke(r, 14, Stroke::new(1.0, BUTTON_EDGE), StrokeKind::Inside);
        p.text(r.left_top() + vec2(14.0, 16.0), Align2::LEFT_CENTER, label, semi(12.5), Color32::WHITE);
        let bar = Rect::from_min_max(pos2(r.left() + 14.0, r.bottom() - 14.0), pos2(r.right() - 40.0, r.bottom() - 10.0));
        p.rect_filled(bar, 2, TRACK);
        if let Some(f) = done {
            p.rect_filled(Rect::from_min_max(bar.min, pos2(bar.left() + bar.width() * f.clamp(0.0, 1.0), bar.bottom())), 2, ACCENT);
        }
        let stop = Rect::from_center_size(pos2(r.right() - 22.0, r.center().y), Vec2::splat(28.0));
        if icon_button(ui, stop, Icon::Cancel01, "Cancel", false).clicked() {
            match &self.task {
                Some(Task::Clip(j)) => j.cancel.store(true, Ordering::Relaxed),
                Some(Task::Batch(b)) => b.cancel.store(true, Ordering::Relaxed),
                _ => {}
            }
            if let Some((_, _, run)) = &self.tracking {
                run.cancel.store(true, Ordering::Relaxed);
            }
        }
    }

    pub(super) fn sheets(&mut self, ctx: &Context) {
        let Some(sheet) = self.sheet.clone() else { return };
        dim_screen(ctx, "sheet-dim");
        let screen = ctx.content_rect();
        let wanted = if sheet == Sheet::Keys { KEYS_SHEET_W } else { SHEET_W };
        let w = wanted.min(screen.width() - 24.0);
        let mut close = false;
        let id = Id::unique("sheet");
        let last = ctx.data(|d| d.get_temp::<f32>(id)).unwrap_or(300.0);
        let pos = pos2(screen.center().x - w / 2.0, (screen.center().y - last / 2.0).max(screen.top() + 12.0));
        let shown = Area::new(id).order(Order::Foreground).fixed_pos(pos).show(ctx, |ui| {
            egui::Frame::new().fill(PANEL).stroke(Stroke::new(1.0, BUTTON_EDGE)).corner_radius(PANEL_RADIUS).inner_margin(16).show(ui, |ui| {
                ui.set_width(w - 32.0);
                ui.spacing_mut().item_spacing.y = 6.0;
                let title = match &sheet {
                    Sheet::Home => "Lookroom",
                    Sheet::Export => "Export",
                    Sheet::Batch => "Batch Edit",
                    Sheet::History => "Undo History",
                    Sheet::Keys => "Keyboard Shortcuts",
                    Sheet::SavePreset => "Save As Preset",
                    Sheet::Rename(_) => "Rename Preset",
                    Sheet::About(_, t) => t.as_str(),
                };
                let (head, _) = ui.allocate_exact_size(vec2(w - 32.0, 30.0), Sense::hover());
                ui.painter().text(pos2(head.left(), head.center().y), Align2::LEFT_CENTER, title, bold(17.0), Color32::WHITE);
                let x = Rect::from_center_size(pos2(head.right() - 14.0, head.center().y), Vec2::splat(28.0));
                if icon_button(ui, x, Icon::Cancel01, "Close", false).clicked() {
                    close = true;
                }
                ui.add_space(4.0);
                match &sheet {
                    Sheet::Home => self.home_sheet(ui, ctx, &mut close),
                    Sheet::Export => self.export_sheet(ui, &mut close),
                    Sheet::Batch => self.batch_sheet(ui, &mut close),
                    Sheet::History => self.history_sheet(ui),
                    Sheet::Keys => keys_sheet(ui),
                    Sheet::SavePreset | Sheet::Rename(_) => self.name_sheet(ui, ctx, &sheet, &mut close),
                    Sheet::About(text, _) => note(ui, text),
                }
            });
        });
        ctx.data_mut(|d| d.insert_temp(id, shown.response.rect.height()));
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            close = true;
        }
        if close {
            self.sheet = None;
        }
    }

    fn list_row(&self, ui: &mut Ui, icon: Icon, label: &str) -> bool {
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 42.0), Sense::click());
        let p = ui.painter();
        p.rect_filled(r, 12, if resp.hovered() { BUTTON_EDGE } else { BUTTON });
        icons::paint(p, icon, Rect::from_center_size(pos2(r.left() + 22.0, r.center().y), Vec2::splat(20.0)), Color32::WHITE);
        p.text(pos2(r.left() + 42.0, r.center().y), Align2::LEFT_CENTER, label, semi(13.5), Color32::WHITE);
        resp.clicked()
    }

    fn home_sheet(&mut self, ui: &mut Ui, _ctx: &Context, close: &mut bool) {
        let name = short(&self.photo_name, 44);
        note(ui, &name);
        if !video::available() {
            note(ui, &format!("Video needs ffmpeg. {}", crate::platform::ffmpeg_hint()));
        }
        if self.list_row(ui, Icon::Folder01, "Open Photo or Video") {
            *close = true;
            self.pick_media();
        }
        if self.list_row(ui, Icon::FileExport, "Save Project") {
            *close = true;
            self.save_project();
        }
        if self.list_row(ui, Icon::FolderOpen, "Open Project") {
            *close = true;
            self.open_project();
        }
        if self.list_row(ui, Icon::GridView, "Batch Edit") {
            self.sheet = Some(Sheet::Batch);
        }
        if self.list_row(ui, Icon::RefreshCcw, "Reset All Edits") {
            let (frame_ratio, crop) = (self.grade.frame_ratio, self.grade.crop.clone());
            self.grade = crate::pipeline::Grade { frame_ratio, crop, ..Default::default() };
            self.selected = None;
            self.active_layer = None;
            *close = true;
        }
    }

    fn photo_choices(&mut self, ui: &mut Ui) {
        let names: Vec<&str> = Format::ALL.iter().map(|f| f.name()).collect();
        let mut at = Format::ALL.iter().position(|f| *f == self.photo_opts.format).unwrap_or(1);
        segmented(ui, &names, &mut at);
        let o = &mut self.photo_opts;
        o.format = Format::ALL[at];
        if let Some(label) = o.format.deep_label() {
            let mut d = o.deep as usize;
            segmented(ui, &["8-bit", label], &mut d);
            o.deep = d == 1;
        }
        if o.format.has_quality() {
            let mut q = o.quality as f32 / 100.0;
            slider(ui, "Quality", &mut q, 0.3, 1.0, 0.92, Fmt::Percent);
            o.quality = (q * 100.0).round() as u8;
        }
        toggle(ui, "Keep Metadata", &mut o.metadata);
        if o.format == Format::Heic && !video::available() {
            note(ui, &format!("HEIC needs ffmpeg. {}", crate::platform::ffmpeg_hint()));
        }
    }

    fn video_choices(&mut self, ui: &mut Ui) {
        let names: Vec<&str> = Codec::ALL.iter().map(|c| c.name()).collect();
        let mut at = Codec::ALL.iter().position(|c| *c == self.clip_opts.codec).unwrap_or(0);
        segmented(ui, &names, &mut at);
        let o = &mut self.clip_opts;
        o.codec = Codec::ALL[at];
        let mut size = RESOLUTIONS.iter().position(|r| r.1 == o.side).unwrap_or(0);
        let labels: Vec<&str> = RESOLUTIONS.iter().map(|r| r.0).collect();
        segmented(ui, &labels, &mut size);
        o.side = RESOLUTIONS[size].1;
        if o.codec == Codec::ProRes {
            o.mov = true;
            note(ui, "ProRes 422 HQ, 10-bit, in a MOV file.");
        } else {
            let mut c = o.mov as usize;
            segmented(ui, &["MP4", "MOV"], &mut c);
            o.mov = c == 1;
            toggle(ui, "10-bit", &mut o.ten_bit);
        }
    }

    fn export_sheet(&mut self, ui: &mut Ui, close: &mut bool) {
        // A lone photo stays a photo; anything with time in it is a video.
        let video = self.clip.is_some() || self.project.clips.len() > 1 || !self.project.items.is_empty();
        if video {
            self.video_choices(ui);
            let p = &self.project;
            let text = format!(
                "{} clips, {:.1} seconds at {:.2} frames a second, {} × {}. Sound is mixed in.",
                p.clips.len(),
                p.seconds(p.total()),
                p.fps,
                p.size.0,
                p.size.1,
            );
            note(ui, &text);
        } else {
            self.photo_choices(ui);
            let text = format!("{} × {}", self.full.w, self.full.h);
            note(ui, &text);
        }
        ui.add_space(2.0);
        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(vec2(w, 40.0), Sense::hover());
        if action(ui, r, "Export", true, 14.0).clicked() {
            *close = true;
            if video {
                self.start_clip_export();
            } else {
                self.start_photo_export();
            }
        }
    }

    fn start_photo_export(&mut self) {
        let f = self.photo_opts.format;
        let base = self.photo_name.rsplit_once('.').map(|x| x.0).unwrap_or(&self.photo_name).to_string();
        let Some(path) = rfd::FileDialog::new()
            .add_filter(f.name(), &[f.extension()])
            .set_file_name(format!("{base}-graded.{}", f.extension()))
            .save_file()
        else {
            return;
        };
        let (source, grade, bank, opts, exif) = (self.full.clone(), self.grade.clone(), self.bank.clone(), self.photo_opts, self.exif.clone());
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(export::export_photo(&path, &source, &grade, &bank, &opts, exif.as_deref()));
        });
        self.task = Some(Task::Photo(rx));
    }

    fn start_clip_export(&mut self) {
        let o = self.clip_opts;
        let extension = if o.mov || o.codec == Codec::ProRes { "mov" } else { "mp4" };
        let Some(out) = rfd::FileDialog::new()
            .add_filter(extension.to_uppercase(), &[extension])
            .set_file_name(format!("{}.{extension}", self.project.name))
            .save_file()
        else {
            return;
        };
        let scene = self.scene();
        self.task = Some(Task::Clip(player::start_export(scene, out, o)));
    }

    fn batch_sheet(&mut self, ui: &mut Ui, close: &mut bool) {
        note(ui, &format!("Applies the current look to up to {MAX_BATCH} photos or videos. Crops and masks stay out of it."));
        let names: Vec<String> = self.batch_files.iter().map(|f| short(&f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(), 42)).collect();
        let mut gone = None;
        for (i, n) in names.iter().enumerate() {
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
            ui.painter().rect_filled(r, 8, BUTTON);
            ui.painter().text(pos2(r.left() + 12.0, r.center().y), Align2::LEFT_CENTER, n, semi(12.5), Color32::WHITE);
            let x = Rect::from_center_size(pos2(r.right() - 16.0, r.center().y), Vec2::splat(26.0));
            if icon_button(ui, x, Icon::Cancel01, "Remove", false).clicked() {
                gone = Some(i);
            }
        }
        if let Some(i) = gone {
            self.batch_files.remove(i);
        }
        if self.batch_files.len() < MAX_BATCH && self.list_row(ui, Icon::Add01, "Add Files") {
            let mut all: Vec<&str> = export::PHOTO_EXTENSIONS.to_vec();
            all.extend(video::EXTENSIONS);
            if let Some(files) = rfd::FileDialog::new().add_filter("Photos and videos", &all).pick_files() {
                self.batch_files.extend(files);
                if self.batch_files.len() > MAX_BATCH {
                    self.batch_files.truncate(MAX_BATCH);
                    self.say(format!("Batch holds {MAX_BATCH} files at most"));
                }
            }
        }
        ui.label(egui::RichText::new("Photos").font(semi(13.0)).color(MUTED));
        self.photo_choices(ui);
        if self.batch_files.iter().any(|f| video::is_video(f)) {
            ui.label(egui::RichText::new("Videos").font(semi(13.0)).color(MUTED));
            self.video_choices(ui);
        }
        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(vec2(w, 40.0), Sense::hover());
        let ready = !self.batch_files.is_empty();
        if action(ui, r, "Choose Folder and Export", ready, 14.0).clicked()
            && ready
            && let Some(dir) = rfd::FileDialog::new().pick_folder()
        {
            *close = true;
            let files: Vec<PathBuf> = std::mem::take(&mut self.batch_files);
            let b = export::start_batch(files, dir, self.look_only(), self.bank.clone(), self.photo_opts, self.clip_opts);
            self.task = Some(Task::Batch(b));
        }
    }

    fn history_sheet(&mut self, ui: &mut Ui) {
        let labels: Vec<String> = self.history.iter().map(|h| h.0.clone()).collect();
        let at = self.history_at;
        let mut go = None;
        egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
            for (i, l) in labels.iter().enumerate().rev() {
                let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width() - 6.0, 34.0), Sense::click());
                let future = i > at;
                let p = ui.painter();
                if i == at {
                    chosen_row(p, r, 8);
                } else {
                    p.rect_filled(r, 8, if resp.hovered() { BUTTON_EDGE } else { BUTTON });
                }
                let col = if future { MUTED } else { Color32::WHITE };
                p.text(pos2(r.left() + 12.0, r.center().y), Align2::LEFT_CENTER, format!("{}. {l}", i + 1), semi(13.0), col);
                if resp.clicked() {
                    go = Some(i);
                }
            }
        });
        if let Some(i) = go {
            self.go_to_history(i);
        }
    }

    fn name_sheet(&mut self, ui: &mut Ui, ctx: &Context, sheet: &Sheet, close: &mut bool) {
        let edit = ui.add(egui::TextEdit::singleline(&mut self.name_buf).desired_width(f32::INFINITY));
        edit.request_focus();
        ui.add_space(6.0);
        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(vec2(w, 38.0), Sense::hover());
        let ok = action(ui, r, "Save", true, 14.0).clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter));
        if ok && !self.name_buf.trim().is_empty() {
            let name = self.name_buf.trim().to_string();
            match sheet {
                Sheet::SavePreset => {
                    let id = store::new_id();
                    let g = self.look_only();
                    self.user.push(UserPreset { id: id.clone(), name, grade: g.clone(), created_at: store::now_seconds() });
                    self.rebuild_thumb(ctx, &id, &g);
                    self.selected = Some(id);
                    self.persist();
                    self.say("Preset saved");
                }
                Sheet::Rename(id) => {
                    if let Some(u) = self.user.iter_mut().find(|u| u.id == *id) {
                        u.name = name;
                    }
                    self.persist();
                }
                _ => {}
            }
            *close = true;
        }
    }
}
