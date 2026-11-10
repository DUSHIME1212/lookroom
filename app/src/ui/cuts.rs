//! Cuts beside Transitions: the edits short videos are made of. Hard, J and L cuts and the
//! smash cut work on the cut nearest the playhead; jump cuts, punch-ins and cutaways
//! (B-roll over the main shot) work at the playhead. Also the speed-ramp curve.

use super::App;
use super::kit::*;
use crate::textanim::Motion;
use crate::timeline::{Body, Fx, Item, Kind, RAMPS, ROW_FX, ROW_FX2, RampPoint};
use eframe::egui::{self, Align2, Color32, Rect, Sense, Stroke, Ui, pos2, vec2};

/// The slowest and fastest a ramp shows, on a scale where each doubling is the same height.
const RAMP_LOW: f64 = 0.1;
const RAMP_HIGH: f64 = 8.0;
const RAMP_H: f32 = 120.0;
/// Frames of white flash and of shake on a smash cut.
const SMASH_FLASH: u32 = 4;
const SMASH_SHAKE: u32 = 9;
/// How far a jump cut's punch-in enlarges the picture.
const PUNCH: f32 = 1.15;

pub struct CutsState {
    pub tab: usize,
    pub(super) jump: f32,
    pub(super) punch: bool,
    broll: usize,
    broll_seconds: f32,
}

impl Default for CutsState {
    fn default() -> Self {
        Self { tab: 0, jump: 0.5, punch: true, broll: 0, broll_seconds: 2.0 }
    }
}

impl App {
    pub(super) fn cuts_page(&mut self, ui: &mut Ui) {
        let w = ui.available_width();
        note(ui, "Cuts change how one shot meets the next. The first ones work on the cut nearest the playhead; the rest work at the playhead.");
        let starts = self.project.starts();
        if let Some(i) = self.project.nearest_cut(self.head) {
            let at = self.project.seconds(starts[i]);
            ui.label(egui::RichText::new(format!("Cut at {}", super::captions::clock(at))).font(semi(13.0)).color(Color32::WHITE));
            let (r, _) = ui.allocate_exact_size(vec2(w, 32.0), Sense::hover());
            let half = Rect::from_min_size(r.min, vec2(w / 2.0 - 3.0, r.height()));
            if action(ui, half, "Hard Cut", false, 12.5).on_hover_text("Straight from one shot to the next: no transition, and the sound cuts with the picture").clicked() {
                self.project.clips[i].transition = None;
                self.project.clips[i].lead = 0.0;
                self.project.clips[i - 1].tail = 0.0;
                self.say("Hard cut");
            }
            if action(ui, Rect::from_min_size(pos2(half.right() + 6.0, r.top()), half.size()), "Smash Cut", false, 12.5).on_hover_text("A white flash and a jolt of shake right on the cut, for a sudden change").clicked() {
                self.smash_cut(i);
            }
            let mut lead = self.project.clips[i].lead as f32;
            if slider(ui, "J Cut: next sound starts early (s)", &mut lead, 0.0, 2.0, 0.0, Fmt::Plain).changed() {
                self.project.clips[i].lead = lead as f64;
            }
            let mut tail = self.project.clips[i - 1].tail as f32;
            if slider(ui, "L Cut: this sound runs on (s)", &mut tail, 0.0, 2.0, 0.0, Fmt::Plain).changed() {
                self.project.clips[i - 1].tail = tail as f64;
            }
            note(ui, "J and L cuts let the sound lead or trail the picture, so a conversation or a scene flows across the cut.");
        } else {
            note(ui, "Put two clips on the timeline to cut between them.");
        }

        ui.add_space(6.0);
        ui.label(egui::RichText::new("At the Playhead").font(semi(13.0)).color(Color32::WHITE));
        slider(ui, "Jump: skip (s)", &mut self.edit.cuts.jump, 0.1, 3.0, 0.5, Fmt::Plain);
        toggle(ui, "Punch in after the jump", &mut self.edit.cuts.punch);
        let (r, _) = ui.allocate_exact_size(vec2(w, 32.0), Sense::hover());
        if action(ui, r, "Jump Cut Here", true, 12.5).on_hover_text("Splits the shot here and drops the next moment of it, so time jumps forward").clicked() {
            self.jump_cut();
        }
        if let Some(s) = self.project.at(self.head) {
            let mut z = self.project.clips[s.clip].zoom;
            if slider(ui, "Punch In (this clip)", &mut z, 1.0, 1.8, 1.0, Fmt::Percent).changed() {
                self.project.clips[s.clip].zoom = z;
            }
        }

        ui.add_space(6.0);
        ui.label(egui::RichText::new("Cutaway (B-roll)").font(semi(13.0)).color(Color32::WHITE));
        let videos: Vec<(u32, String)> = self.project.media.iter().filter(|m| m.kind == Kind::Video).map(|m| (m.id, m.name.clone())).collect();
        if videos.is_empty() {
            note(ui, "Import a second video to cut away to.");
        } else {
            let names: Vec<&str> = videos.iter().map(|v| v.1.as_str()).collect();
            let mut at = self.edit.cuts.broll.min(videos.len() - 1);
            // Cutting away to the shot already showing is rarely meant; start on another.
            let here = self.project.at(self.head).map(|s| self.project.clips[s.clip].media);
            if Some(videos[at].0) == here && videos.len() > 1 {
                at = (at + 1) % videos.len();
            }
            let (r, _) = ui.allocate_exact_size(vec2(w, 52.0), Sense::hover());
            dropdown(ui, r, Some("Shot to cut away to"), &names, &mut at);
            self.edit.cuts.broll = at;
            slider(ui, "Seconds", &mut self.edit.cuts.broll_seconds, 0.5, 8.0, 2.0, Fmt::Plain);
            let (r, _) = ui.allocate_exact_size(vec2(w, 32.0), Sense::hover());
            if action(ui, r, "Add Cutaway Here", true, 12.5).on_hover_text("Lays the other shot over this one, full frame and silent, while this shot's sound carries on").clicked() {
                let secs = self.edit.cuts.broll_seconds as f64;
                match self.add_cutaway(videos[at].0, self.head, secs, 0.0) {
                    Ok(_) => self.say("Added a cutaway"),
                    Err(e) => self.say(e),
                }
            }
            note(ui, "For cross-cutting or a whole A-roll and B-roll edit, ask the Assistant: it can lay out the shots and make the cuts.");
        }
    }

    /// A white flash and a jolt of shake on the cut into clip `i`, with any transition gone.
    pub(super) fn smash_cut(&mut self, i: usize) {
        let at = self.project.starts()[i];
        self.project.clips[i].transition = None;
        for (kind, len, amount) in [(Fx::Flash, SMASH_FLASH, 1.0), (Fx::Shake, SMASH_SHAKE, 0.8)] {
            let id = self.project.new_id();
            self.project.items.push(Item { id, row: ROW_FX, at, len, body: Body::Effect { kind, amount } });
        }
        self.say("Smash cut");
    }

    /// Splits at the playhead and drops the next stretch of the same shot.
    pub(super) fn jump_cut(&mut self) {
        let skip = self.project.frames(self.edit.cuts.jump as f64).max(1);
        self.sync_active();
        let Some(i) = self.project.split_clip(self.head) else {
            return self.say("Move the playhead inside a clip, away from its ends");
        };
        let id = self.project.clips[i].id;
        self.project.trim_clip_start(id, skip as i64);
        if self.edit.cuts.punch {
            // Alternate, so a run of jump cuts goes in and out rather than ever closer.
            let before = self.project.clips[i - 1].zoom;
            self.project.clips[i].zoom = if before > 1.001 { 1.0 } else { PUNCH };
        }
        self.needs_activate = true;
        self.say("Jump cut");
    }

    /// Lays a video over the picture at `at`, full frame and silent, for `seconds`. The
    /// main shot's sound carries on underneath. Gives the new item's id.
    pub(super) fn add_cutaway(&mut self, media: u32, at: u32, seconds: f64, from: f64) -> Result<u32, String> {
        let m = self.project.media_of(media).cloned().ok_or("no such video")?;
        if m.kind != Kind::Video {
            return Err("only videos can be cutaways".into());
        }
        let len = self.project.frames(seconds.min((m.seconds - from).max(0.1))).max(2);
        let (pw, ph) = (self.project.size.0 as f32, self.project.size.1 as f32);
        let aspect = m.size.0.max(1) as f32 / m.size.1.max(1) as f32;
        // Tall enough that its width covers the picture too.
        let size = ((pw / ph) / aspect).max(1.0);
        let body = Body::Overlay { media, start: from.max(0.0), size, x: 0.5, y: 0.5, key: None, tolerance: 0.2, volume: 0.0, motion: Motion::default() };
        let id = self.project.new_id();
        self.project.items.push(Item { id, row: ROW_FX2, at, len, body });
        Ok(id)
    }

    /// The speed ramp of clip `id`: presets, then the curve, whose points can be dragged,
    /// added with a double-click and removed with a right-click.
    pub(super) fn ramp_editor(&mut self, ui: &mut Ui, id: u32) {
        let Some(c) = self.project.clips.iter().find(|c| c.id == id).cloned() else { return };
        ui.add_space(6.0);
        ui.label(egui::RichText::new("Speed Ramp").font(bold(13.0)).color(Color32::WHITE));
        let mut chips = Chips::new(ui, 26.0);
        if chips.add("None", c.ramp.len() < 2) && c.ramp.len() >= 2 {
            self.project.set_ramp(id, Vec::new());
            self.needs_activate = true;
        }
        for (name, pts) in RAMPS {
            if chips.add(name, false) {
                self.project.set_ramp(id, pts.iter().map(|(at, speed)| RampPoint { at: *at, speed: *speed }).collect());
                self.needs_activate = true;
            }
        }
        chips.end();
        if c.ramp.len() < 2 {
            note(ui, "Pick a ramp to speed up and slow down within the clip.");
            return;
        }
        let w = ui.available_width();
        let (r, bg) = ui.allocate_exact_size(vec2(w, RAMP_H), Sense::click());
        let to_y = |s: f64| r.bottom() - ((s.ln() - RAMP_LOW.ln()) / (RAMP_HIGH.ln() - RAMP_LOW.ln())) as f32 * r.height();
        let from_y = |y: f32| (RAMP_LOW.ln() + ((r.bottom() - y) / r.height()) as f64 * (RAMP_HIGH.ln() - RAMP_LOW.ln())).exp();
        let p = ui.painter();
        p.rect_filled(r, 8, PANEL);
        for (s, label) in [(1.0, "1x"), (4.0, "4x"), (0.25, "0.25x")] {
            let y = to_y(s);
            p.line_segment([pos2(r.left(), y), pos2(r.right(), y)], Stroke::new(1.0, BUTTON_EDGE));
            p.text(pos2(r.left() + 4.0, y - 7.0), Align2::LEFT_CENTER, label, regular(10.0), MUTED);
        }
        let line: Vec<egui::Pos2> = (0..=60).map(|n| n as f64 / 60.0).map(|t| pos2(r.left() + t as f32 * r.width(), to_y(c.speed_at(t)))).collect();
        p.add(egui::Shape::line(line, Stroke::new(2.0, ACCENT)));
        let mut pts = c.ramp.clone();
        let mut changed = false;
        let mut gone = None;
        for (n, pt) in c.ramp.iter().enumerate() {
            let at = pos2(r.left() + pt.at as f32 * r.width(), to_y(pt.speed));
            let hit = ui.interact(Rect::from_center_size(at, vec2(16.0, 16.0)), ui.scope_id().with(("ramp-point", n)), Sense::click_and_drag());
            ui.painter().circle_filled(at, if hit.hovered() || hit.dragged() { 6.0 } else { 4.5 }, Color32::WHITE);
            if hit.dragged()
                && let Some(pos) = hit.interact_pointer_pos()
            {
                // The ends stay at the clip's ends; only their speed moves.
                let ends = n == 0 || n + 1 == c.ramp.len();
                if !ends {
                    pts[n].at = ((pos.x - r.left()) / r.width()).clamp(0.01, 0.99) as f64;
                }
                pts[n].speed = from_y(pos.y.clamp(r.top(), r.bottom())).clamp(RAMP_LOW, RAMP_HIGH);
                changed = true;
            }
            if hit.dragged() || hit.hovered() {
                ui.painter().text(at + vec2(0.0, -14.0), Align2::CENTER_CENTER, super::motion::speed_label((pts[n].speed * 20.0).round() / 20.0), semi(11.0), Color32::WHITE);
            }
            if hit.secondary_clicked() && c.ramp.len() > 2 && n != 0 && n + 1 != c.ramp.len() {
                gone = Some(n);
            }
        }
        if bg.double_clicked()
            && let Some(pos) = bg.interact_pointer_pos()
        {
            let t = ((pos.x - r.left()) / r.width()).clamp(0.01, 0.99) as f64;
            pts.push(RampPoint { at: t, speed: c.speed_at(t) });
            changed = true;
        }
        if let Some(n) = gone {
            pts.remove(n);
            changed = true;
        }
        if changed {
            self.project.shape_ramp(id, pts);
            self.needs_activate = true;
        }
        note(ui, "Drag a point to change its speed, double-click to add one, right-click to remove one. A ramped clip plays without its own sound; put music under it.");
    }
}
