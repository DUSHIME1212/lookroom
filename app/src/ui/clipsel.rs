//! Several clips at once: picking them (Ctrl or Shift and a click), copying one clip's
//! adjustments onto others from the right-click menu, and changing the sound of every
//! picked clip together.

use super::App;
use super::kit::*;
use crate::pipeline::Grade;
use crate::timeline::Picked;
use eframe::egui::{self, Color32, Ui};

/// How strong Clean Up Sound is when switched on from the menu.
const DENOISE_DEFAULT: f32 = 0.5;

#[derive(Default)]
pub struct ClipSel {
    /// Clips picked together with the picked one, by id.
    pub group: Vec<u32>,
    /// Adjustments copied from a clip, ready to paste.
    copied: Option<Grade>,
    copied_from: String,
}

impl App {
    /// The picked clips: the group while the picked clip is in it, else the one picked.
    pub(super) fn clip_selection(&self) -> Vec<u32> {
        let Some(Picked::Clip(id)) = self.picked else { return Vec::new() };
        if self.edit.clips.group.contains(&id) {
            self.edit.clips.group.iter().copied().filter(|g| self.project.clips.iter().any(|c| c.id == *g)).collect()
        } else {
            vec![id]
        }
    }

    pub(super) fn clip_selected(&self, id: u32) -> bool {
        match self.picked {
            Some(Picked::Clip(p)) if p == id => true,
            Some(Picked::Clip(p)) => self.edit.clips.group.contains(&p) && self.edit.clips.group.contains(&id),
            _ => false,
        }
    }

    /// A click on a clip: alone, Ctrl to add or take away, Shift for the run since the last.
    pub(super) fn click_clip(&mut self, id: u32, ctrl: bool, shift: bool) {
        let mut all = self.clip_selection();
        if shift && let Some(Picked::Clip(from)) = self.picked {
            let index = |c: u32| self.project.clips.iter().position(|x| x.id == c);
            if let (Some(a), Some(b)) = (index(from), index(id)) {
                let (lo, hi) = (a.min(b), a.max(b));
                for c in &self.project.clips[lo..=hi] {
                    if !all.contains(&c.id) {
                        all.push(c.id);
                    }
                }
            }
        } else if ctrl {
            if let Some(i) = all.iter().position(|g| *g == id) {
                all.remove(i);
                self.edit.clips.group = all.clone();
                self.picked = all.last().map(|g| Picked::Clip(*g));
                return;
            }
            all.push(id);
        } else {
            all = vec![id];
        }
        self.edit.clips.group = all;
        self.picked = Some(Picked::Clip(id));
    }

    /// The clips a sound change applies to: the picked ones, or else the clip being edited.
    pub(super) fn sound_targets(&self) -> Vec<u32> {
        let picked = self.clip_selection();
        if picked.is_empty() { self.active.into_iter().collect() } else { picked }
    }

    /// Whether the clips a sound change would reach are all muted.
    pub(super) fn targets_muted(&self) -> bool {
        let t = self.sound_targets();
        !t.is_empty() && self.project.clips.iter().filter(|c| t.contains(&c.id)).all(|c| c.muted)
    }

    /// Mutes the picked clips, or brings their sound back if they're all muted already.
    pub(super) fn toggle_clip_mute(&mut self) {
        let targets = self.sound_targets();
        if targets.is_empty() {
            return self.say("Pick a clip first");
        }
        let unmute = self.targets_muted();
        for c in self.project.clips.iter_mut().filter(|c| targets.contains(&c.id)) {
            c.muted = !unmute;
        }
        let n = targets.len();
        let what = if n == 1 { "Clip".to_string() } else { format!("{n} clips") };
        self.say(if unmute { format!("{what} unmuted") } else { format!("{what} muted") });
    }

    /// Removes every picked clip when several are picked. False when there's only one,
    /// which the usual delete handles.
    pub(super) fn delete_clip_selection(&mut self) -> bool {
        let picked = self.clip_selection();
        if picked.len() < 2 {
            return false;
        }
        self.project.clips.retain(|c| !picked.contains(&c.id));
        self.picked = None;
        self.edit.clips.group.clear();
        self.needs_activate = true;
        true
    }

    pub(super) fn select_clips(&mut self, ids: Vec<u32>) {
        self.picked = ids.first().map(|id| Picked::Clip(*id));
        self.edit.clips.group = ids;
    }

    /// The right-click menu of a clip. Right-clicking outside the selection picks just it.
    pub(super) fn clip_menu(&mut self, ui: &mut Ui, id: u32) {
        if !self.clip_selected(id) {
            self.click_clip(id, false, false);
        }
        let picked = self.clip_selection();
        let n = picked.len();
        let many = if n == 1 { String::new() } else { format!(" ({n} clips)") };
        if ui.button("Copy Adjustments").clicked() {
            self.sync_active();
            if let Some(c) = self.project.clips.iter().find(|c| c.id == id) {
                self.edit.clips.copied = Some((*c.grade).clone());
                self.edit.clips.copied_from = self.project.media_of(c.media).map(|m| m.name.clone()).unwrap_or_default();
            }
            self.say("Adjustments copied");
            ui.close();
        }
        let can = self.edit.clips.copied.is_some();
        if ui.add_enabled(can, egui::Button::new(format!("Paste Adjustments{many}"))).on_hover_text(format!("From {}", self.edit.clips.copied_from)).clicked() {
            self.paste_adjustments(&picked);
            ui.close();
        }
        if ui.add_enabled(can, egui::Button::new("Paste Adjustments to Every Clip")).clicked() {
            let all: Vec<u32> = self.project.clips.iter().map(|c| c.id).collect();
            self.paste_adjustments(&all);
            ui.close();
        }
        if ui.button(format!("Reset Adjustments{many}")).clicked() {
            for c in &picked {
                self.change_grade(*c, |g| *g = Grade { crop: g.crop.clone(), frame_ratio: g.frame_ratio, ..Grade::default() });
            }
            self.say("Adjustments reset");
            ui.close();
        }
        ui.separator();
        if ui.button("Select Every Clip").clicked() {
            let all = self.project.clips.iter().map(|c| c.id).collect();
            self.select_clips(all);
            ui.close();
        }
        if ui.button("Select Clips From This File").clicked() {
            let media = self.project.clips.iter().find(|c| c.id == id).map(|c| c.media);
            let same = self.project.clips.iter().filter(|c| Some(c.media) == media).map(|c| c.id).collect();
            self.select_clips(same);
            ui.close();
        }
        ui.separator();
        let muted = self.project.clips.iter().filter(|c| picked.contains(&c.id)).all(|c| c.muted);
        if ui.button(if muted { format!("Unmute Sound{many}") } else { format!("Mute Sound{many}") }).clicked() {
            for c in self.project.clips.iter_mut().filter(|c| picked.contains(&c.id)) {
                c.muted = !muted;
            }
            ui.close();
        }
        let clean = self.project.clips.iter().filter(|c| picked.contains(&c.id)).all(|c| c.denoise > 0.01);
        if ui.button(if clean { format!("Stop Cleaning Up Sound{many}") } else { format!("Clean Up Sound{many}") }).on_hover_text("Takes down steady hiss, hum and rumble").clicked() {
            for c in self.project.clips.iter_mut().filter(|c| picked.contains(&c.id)) {
                c.denoise = if clean { 0.0 } else { DENOISE_DEFAULT };
            }
            ui.close();
        }
        ui.separator();
        if ui.button(format!("Delete{many}")).clicked() {
            self.project.clips.retain(|c| !picked.contains(&c.id));
            self.picked = None;
            self.edit.clips.group.clear();
            self.needs_activate = true;
            ui.close();
        }
    }

    /// The copied adjustments onto each clip. Each keeps its own framing: crop and frame
    /// shape belong to its picture, not to the look.
    fn paste_adjustments(&mut self, ids: &[u32]) {
        let Some(copied) = self.edit.clips.copied.clone() else { return };
        self.sync_active();
        for id in ids {
            self.change_grade(*id, |g| {
                *g = Grade { crop: g.crop.clone(), frame_ratio: g.frame_ratio, ..copied.clone() };
            });
        }
        self.needs_activate = true;
        self.say(format!("Pasted adjustments onto {} clip{}", ids.len(), if ids.len() == 1 { "" } else { "s" }));
    }

    /// Sound of the picked clips, changed together: volume, mute and noise clean-up.
    pub(super) fn clip_sound_section(&mut self, ui: &mut Ui) {
        let picked = self.clip_selection();
        if picked.is_empty() {
            return;
        }
        let n = picked.len();
        ui.label(egui::RichText::new(if n == 1 { "Clip Sound".to_string() } else { format!("Sound of {n} Clips") }).font(bold(14.0)).color(Color32::WHITE));
        if n > 1 {
            note(ui, "Changes here go to every picked clip. Ctrl or Shift and a click on the timeline picks more.");
        }
        let first = self.project.clips.iter().find(|c| picked.contains(&c.id)).cloned();
        let Some(first) = first else { return };
        let mut volume = first.volume;
        if slider(ui, "Volume", &mut volume, 0.0, 2.0, 1.0, Fmt::Percent).changed() {
            for c in self.project.clips.iter_mut().filter(|c| picked.contains(&c.id)) {
                c.volume = volume;
            }
        }
        let mut muted = first.muted;
        if toggle(ui, "Mute", &mut muted) {
            for c in self.project.clips.iter_mut().filter(|c| picked.contains(&c.id)) {
                c.muted = muted;
            }
        }
        let mut clean = first.denoise;
        if slider(ui, "Clean Up Noise", &mut clean, 0.0, 1.0, 0.0, Fmt::Percent).on_hover_text("Takes down steady hiss, hum and rumble; higher is stronger").changed() {
            for c in self.project.clips.iter_mut().filter(|c| picked.contains(&c.id)) {
                c.denoise = clean;
            }
        }
        note(ui, "Clean Up Noise suits wind, fans, hum and room hiss. Very strong settings can make voices sound thin.");
        ui.add_space(6.0);
    }
}
