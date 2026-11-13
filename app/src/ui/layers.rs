//! Masks and layers, motion tracking, and the crop panel.

use super::kit::*;
use super::{App, DEFAULT_BRUSH, short};
use crate::crop::{self, Crop, STRAIGHTEN_MAX};
use crate::icons::{self, Icon, Tool};
use crate::masks::{Combine, Key, Mask, Shape};
use crate::pipeline::Grade;
use crate::track;
use eframe::egui::{Align2, Color32, Context, Rect, Response, Sense, Shape as Line, Stroke, Ui, Vec2, pos2, vec2};
use std::sync::atomic::Ordering;

const LAYER_ROW_H: f32 = 40.0;
const GRAPH_H: f32 = 74.0;
const CROP_RATIOS: [(&str, f32); 10] = [
    ("Free", 0.0),
    ("Original", -1.0),
    ("1:1", 1.0),
    ("4:5", 0.8),
    ("5:4", 1.25),
    ("3:4", 0.75),
    ("4:3", 4.0 / 3.0),
    ("2:3", 2.0 / 3.0),
    ("3:2", 1.5),
    ("16:9", 16.0 / 9.0),
];
const PAN: Color32 = Color32::from_rgb(70, 210, 230);
const TILT: Color32 = Color32::from_rgb(255, 150, 60);
const ZOOM: Color32 = Color32::from_rgb(110, 225, 130);
const ROTATE: Color32 = Color32::from_rgb(170, 120, 255);

/// A change picked in the layer list, made once the list is drawn.
enum LayerAct {
    Select(Option<usize>),
    ToggleVisible(usize),
}

/// A change picked in the layer controls, made once they are drawn.
enum PanelAct {
    EditLayer,
    Duplicate,
    ResetEdits,
    Delete,
    ChooseReference,
    SelectMask(usize),
    RemoveMask,
}

/// One row of the layer list; `visible` is empty for the base image.
fn layer_row(ui: &mut Ui, name: &str, visible: Option<bool>, on: bool) -> Response {
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), LAYER_ROW_H), Sense::click());
    let p = ui.painter();
    let fill = if on { ACCENT_SOFT } else if resp.hovered() { BUTTON_EDGE } else { BUTTON };
    p.rect_filled(r, 4, fill);
    if let Some(v) = visible {
        let c = pos2(r.left() + LAYER_ROW_H / 2.0, r.center().y);
        let i = Rect::from_center_size(c, Vec2::splat(16.0));
        icons::paint(p, if v { Icon::View } else { Icon::ViewOff }, i, Color32::WHITE);
    }
    let at = pos2(r.left() + if visible.is_some() { LAYER_ROW_H + 2.0 } else { 12.0 }, r.center().y);
    p.text(at, Align2::LEFT_CENTER, short(name, 34), semi(12.5), Color32::WHITE);
    resp
}

/// Sliders and buttons for one kind of mask.
fn mask_settings(ui: &mut Ui, m: &mut Mask, brush: &mut (f32, f32, f32), erase: &mut bool) {
    match m.shape {
        Shape::Linear => {
            slider(ui, "Position X", &mut m.x, 0.0, 1.0, 0.5, Fmt::Percent);
            slider(ui, "Position Y", &mut m.y, 0.0, 1.0, 0.5, Fmt::Percent);
            slider(ui, "Feather", &mut m.feather, 0.0, 1.0, 0.3, Fmt::Percent);
            slider(ui, "Angle", &mut m.angle, 0.0, 360.0, 90.0, Fmt::Degrees);
        }
        Shape::Radial | Shape::Rectangle => {
            slider(ui, "Position X", &mut m.x, 0.0, 1.0, 0.5, Fmt::Percent);
            slider(ui, "Position Y", &mut m.y, 0.0, 1.0, 0.5, Fmt::Percent);
            slider(ui, "Feather", &mut m.feather, 0.0, 1.0, 0.3, Fmt::Percent);
            slider(ui, "Width", &mut m.size_x, 0.01, 1.0, 0.25, Fmt::Percent);
            slider(ui, "Height", &mut m.size_y, 0.01, 1.0, 0.25, Fmt::Percent);
            slider(ui, "Angle", &mut m.angle, 0.0, 360.0, 0.0, Fmt::Degrees);
        }
        Shape::Draw => {
            slider(ui, "Brush Size", &mut brush.0, 0.005, 0.25, DEFAULT_BRUSH.0, Fmt::Percent);
            slider(ui, "Softness", &mut brush.1, 0.0, 1.0, DEFAULT_BRUSH.1, Fmt::Percent);
            slider(ui, "Opacity", &mut brush.2, 0.0, 1.0, DEFAULT_BRUSH.2, Fmt::Percent);
            let mut chips = Chips::new(ui, 28.0);
            if chips.add("Erase", *erase) {
                *erase = !*erase;
            }
            if chips.add("Clear Drawing", false) {
                m.strokes.clear();
            }
            chips.end();
            note(ui, "Drag on the photo to paint.");
        }
        Shape::Curve => {
            slider(ui, "Feather", &mut m.feather, 0.0, 1.0, 0.3, Fmt::Percent);
            let mut chips = Chips::new(ui, 28.0);
            if chips.add("Remove Last Point", false) {
                m.points.pop();
            }
            if chips.add("Clear Points", false) {
                m.points.clear();
            }
            chips.end();
            note(ui, "Click the photo to place points. Right-click a point to remove it.");
        }
        Shape::Qualifier => {
            let mut chips = Chips::new(ui, 28.0);
            if chips.add("Use Hue", m.hue_on) {
                m.hue_on = !m.hue_on;
            }
            if chips.add("Use Saturation", m.sat_on) {
                m.sat_on = !m.sat_on;
            }
            if chips.add("Use Luminance", m.lum_on) {
                m.lum_on = !m.lum_on;
            }
            chips.end();
            let band = hue_colours();
            slider_on(ui, "Hue Center", &mut m.hue_center, 0.0, 360.0, 120.0, Fmt::Degrees, Some(&band));
            slider(ui, "Hue Width", &mut m.hue_width, 0.0, 180.0, 60.0, Fmt::Degrees);
            slider(ui, "Hue Softness", &mut m.hue_soft, 0.0, 90.0, 30.0, Fmt::Degrees);
            slider(ui, "Saturation Low", &mut m.sat_low, 0.0, 1.0, 0.1, Fmt::Percent);
            slider(ui, "Saturation High", &mut m.sat_high, 0.0, 1.0, 1.0, Fmt::Percent);
            slider(ui, "Saturation Softness", &mut m.sat_soft, 0.0, 1.0, 0.1, Fmt::Percent);
            slider(ui, "Luminance Low", &mut m.lum_low, 0.0, 1.0, 0.0, Fmt::Percent);
            slider(ui, "Luminance High", &mut m.lum_high, 0.0, 1.0, 1.0, Fmt::Percent);
            slider(ui, "Luminance Softness", &mut m.lum_soft, 0.0, 1.0, 0.1, Fmt::Percent);
        }
    }
}

/// The tracked move as lines over time, with the play position marked.
fn graph(ui: &mut Ui, m: &Mask, frames: u32, frame: u32) {
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(vec2(w, GRAPH_H), Sense::hover());
    let p = ui.painter().with_clip_rect(r);
    p.rect_filled(r, 4, Color32::from_rgb(0x10, 0x10, 0x12));
    p.hline(r.x_range(), r.center().y, Stroke::new(1.0, Color32::from_white_alpha(26)));
    let x = |f: u32| r.left() + r.width() * f as f32 / frames.saturating_sub(1).max(1) as f32;
    if let Some(k0) = m.keys.first().copied() {
        // Each move on a scale that fills the box, so small ones can still be read.
        let mut series: [(Color32, Box<dyn Fn(&Key) -> f32>); 4] = [
            (PAN, Box::new(move |k: &Key| k.x - k0.x)),
            (TILT, Box::new(move |k: &Key| k.y - k0.y)),
            (ZOOM, Box::new(move |k: &Key| k.size_x / k0.size_x.max(1e-4) - 1.0)),
            (ROTATE, Box::new(move |k: &Key| (k.angle - k0.angle) / 90.0)),
        ];
        let reach = series
            .iter_mut()
            .flat_map(|(_, f)| m.keys.iter().map(|k| f(k).abs()).collect::<Vec<_>>())
            .fold(0.01f32, f32::max);
        for (col, f) in &series {
            let line: Vec<_> = m
                .keys
                .iter()
                .map(|k| pos2(x(k.frame), r.center().y - f(k) / reach * (r.height() * 0.45)))
                .collect();
            if line.len() >= 2 {
                p.add(Line::line(line, Stroke::new(1.3, *col)));
            }
        }
    }
    p.vline(x(frame), r.y_range(), Stroke::new(1.5, Color32::WHITE));
    ui.add_space(4.0);
}

impl App {
    pub(super) fn masks_body(&mut self, ui: &mut Ui, _ctx: &Context) {
        // On video a chosen mask can be followed; that gets a view of its own.
        let chosen = self.active_layer.filter(|i| *i < self.grade.layers.len()).and_then(|i| {
            let mi = self.active_mask.min(self.grade.layers[i].masks.len().saturating_sub(1));
            self.grade.layers[i].masks.get(mi).filter(|m| m.shape != Shape::Qualifier).map(|_| (i, mi))
        });
        if let Some((i, mi)) = chosen.filter(|_| self.clip.is_some()) {
            let mut view = self.pick.mask_view;
            segmented(ui, &["Layer", "Tracking"], &mut view);
            self.pick.mask_view = view;
            if view == 1 {
                self.tracking_panel(ui, i, mi);
                return;
            }
        }
        self.layer_list(ui);
        ui.add_space(4.0);
        match self.active_layer.filter(|i| *i < self.grade.layers.len()) {
            Some(i) => self.layer_controls(ui, i),
            None => note(ui, "Pick a mask from the row below to add a layer, then choose Edit Layer to grade just that area."),
        }
    }

    fn layer_list(&mut self, ui: &mut Ui) {
        let mut act = None;
        let chosen = self.active_layer;
        let names: Vec<(String, bool)> = self.grade.layers.iter().map(|l| (l.name.clone(), l.visible)).collect();
        ui.spacing_mut().item_spacing.y = 3.0;
        if layer_row(ui, "Base Image", None, chosen.is_none()).clicked() {
            act = Some(LayerAct::Select(None));
        }
        for (i, (name, visible)) in names.iter().enumerate() {
            let row = layer_row(ui, name, Some(*visible), chosen == Some(i));
            if row.clicked() {
                act = Some(LayerAct::Select(Some(i)));
            }
            let eye = Rect::from_min_size(row.rect.min, vec2(LAYER_ROW_H, LAYER_ROW_H));
            if ui.interact(eye, ui.scope_id().with(("eye", i)), Sense::click()).clicked() {
                act = Some(LayerAct::ToggleVisible(i));
            }
        }
        match act {
            Some(LayerAct::Select(i)) => {
                self.active_layer = i;
                self.active_mask = 0;
                self.add_to_layer = false;
            }
            Some(LayerAct::ToggleVisible(i)) => self.grade.layers[i].visible ^= true,
            None => {}
        }
    }

    fn layer_controls(&mut self, ui: &mut Ui, i: usize) {
        let mut act = None;
        ui.separator();
        ui.label("Selected adjustment layer");
        ui.add(eframe::egui::TextEdit::singleline(&mut self.grade.layers[i].name).hint_text("Layer name").desired_width(f32::INFINITY));
        let has_masks = !self.grade.layers[i].masks.is_empty();
        let mut chips = Chips::new(ui, 28.0);
        if chips.add("Edit Layer", false) {
            act = Some(PanelAct::EditLayer);
        }
        if has_masks && chips.add("Show Mask", self.show_mask) {
            self.show_mask = !self.show_mask;
        }
        if chips.add("Add Mask", self.add_to_layer) {
            self.add_to_layer = !self.add_to_layer;
        }
        if chips.add("Reset Edits", false) {
            act = Some(PanelAct::ResetEdits);
        }
        if chips.add("Duplicate", false) {
            act = Some(PanelAct::Duplicate);
        }
        if chips.add("Delete", false) {
            act = Some(PanelAct::Delete);
        }
        chips.end();

        slider(ui, "Layer Strength", &mut self.grade.layers[i].opacity, 0.0, 1.0, 1.0, Fmt::Percent);
        let mi = self.active_mask.min(self.grade.layers[i].masks.len().saturating_sub(1));
        self.active_mask = mi;
        let count = self.grade.layers[i].masks.len();

        if has_masks {
            let mut chips = Chips::new(ui, 28.0);
            if count > 1 {
                for n in 0..count {
                    if chips.add(&format!("Mask {}", n + 1), n == mi) {
                        act = Some(PanelAct::SelectMask(n));
                    }
                }
            }
            let m = &mut self.grade.layers[i].masks[mi];
            if chips.add("Invert", m.invert) {
                m.invert = !m.invert;
            }
            if mi > 0 {
                for (label, c) in [("Add", Combine::Add), ("Subtract", Combine::Subtract), ("Intersect", Combine::Intersect)] {
                    if chips.add(label, m.combine == c) {
                        m.combine = c;
                    }
                }
            }
            if count > 1 && chips.add("Remove Mask", false) {
                act = Some(PanelAct::RemoveMask);
            }
            chips.end();

            if self.clip.is_none() && self.grade.layers[i].masks[mi].shape != Shape::Qualifier {
                note(ui, "Open a video and this mask can follow its subject.");
            }
            let (mut brush, mut erase) = (self.brush, self.erase);
            let m = &mut self.grade.layers[i].masks[mi];
            mask_settings(ui, m, &mut brush, &mut erase);
            self.brush = brush;
            self.erase = erase;
        } else if let Some(s) = &self.grade.layers[i].grade.shot {
            let text = format!("Matched to {}. Layer Strength sets how far it goes.", s.reference);
            note(ui, &text);
            let mut chips = Chips::new(ui, 28.0);
            if chips.add("Choose Reference", false) {
                act = Some(PanelAct::ChooseReference);
            }
            chips.end();
        } else {
            note(ui, "This layer covers the whole picture. Choose Edit Layer to grade it.");
        }

        match act {
            Some(PanelAct::EditLayer) => self.tool = Tool::Filters,
            Some(PanelAct::Duplicate) => {
                let mut copy = self.grade.layers[i].clone();
                copy.name = format!("{} copy", copy.name);
                self.grade.layers.insert(i + 1, copy);
                self.active_layer = Some(i + 1);
            }
            Some(PanelAct::ResetEdits) => self.grade.layers[i].grade = Grade::default(),
            Some(PanelAct::Delete) => {
                self.grade.layers.remove(i);
                self.active_layer = None;
                self.active_mask = 0;
            }
            Some(PanelAct::ChooseReference) => {
                self.choose_reference(i);
            }
            Some(PanelAct::SelectMask(n)) => self.active_mask = n,
            Some(PanelAct::RemoveMask) => {
                self.grade.layers[i].masks.remove(mi);
                self.active_mask = 0;
            }
            None => {}
        }
    }

    /// Mask tracking: transport, keyframes and the graph of the move.
    fn tracking_panel(&mut self, ui: &mut Ui, i: usize, mi: usize) {
        let Some(c) = &self.clip else { return };
        let (frame, frames, fps, first, last) = (c.frame, c.info.frames, c.info.fps, c.first, c.last);
        let w = ui.available_width();

        let (title, _) = ui.allocate_exact_size(vec2(w, 26.0), Sense::hover());
        let p = ui.painter();
        p.text(pos2(title.left(), title.center().y), Align2::LEFT_CENTER, "Mask tracking", semi(13.0), Color32::WHITE);
        let inout = Rect::from_min_size(pos2(title.right() - 74.0, title.top()), vec2(74.0, 26.0));
        let half = |n: f32| Rect::from_min_size(pos2(inout.left() + 37.0 * n, inout.top()), vec2(36.0, 26.0));
        if action(ui, half(0.0), "In", first == frame && first > 0, 12.0).on_hover_text("Start the clip here").clicked()
        {
            self.set_in(frame.min(last));
        }
        if action(ui, half(1.0), "Out", last == frame && last + 1 < frames, 12.0).on_hover_text("End the clip here").clicked()
        {
            self.set_out(frame.max(first));
        }
        ui.add_space(2.0);

        let (row, _) = ui.allocate_exact_size(vec2(w, 32.0), Sense::hover());
        let cell = 32.0;
        let slot = |n: f32| Rect::from_min_size(pos2(row.left() + cell * n, row.top()), Vec2::splat(cell));
        let end = |n: f32| Rect::from_min_size(pos2(row.right() - cell * (n + 1.0), row.top()), Vec2::splat(cell));
        let busy = self.tracking.is_some();
        if icon_button(ui, slot(0.0), Icon::Previous, "Back one frame", false).clicked() {
            self.seek(frame.saturating_sub(1));
        }
        let whole = self.pick.track_scope == 0;
        if icon_button(ui, slot(1.0), Icon::Backward01, "Track backward", false).clicked() && !busy {
            let to = if whole { first } else { frame.saturating_sub(1).max(first) };
            self.start_tracking(i, mi, frame, to);
        }
        let run_icon = if busy { Icon::Pause } else { Icon::Play };
        if icon_button(ui, slot(2.0), run_icon, "Track forward", busy).clicked() {
            if let Some((_, _, run)) = &self.tracking {
                run.cancel.store(true, Ordering::Relaxed);
            } else {
                let to = if whole { last } else { (frame + 1).min(last) };
                self.start_tracking(i, mi, frame, to);
            }
        }
        if icon_button(ui, slot(3.0), Icon::Next, "Forward one frame", false).clicked() {
            self.seek(frame + 1);
        }
        let s = frame as f64 / fps.max(1.0);
        let code = format!("{:02}:{:02}:{:02}", (s / 60.0) as u32, s as u32 % 60, (frame as f64 % fps.max(1.0)) as u32);
        ui.painter().text(pos2(slot(4.0).left() + 4.0, row.center().y), Align2::LEFT_CENTER, code, semi(12.5), Color32::WHITE);

        let m = &self.grade.layers[i].masks[mi];
        let has_key = m.keys.iter().any(|k| k.frame == frame);
        let before = m.keys.iter().rev().find(|k| k.frame < frame).map(|k| k.frame);
        let after = m.keys.iter().find(|k| k.frame > frame).map(|k| k.frame);
        if icon_button(ui, end(0.0), Icon::Cancel01, "Remove all keys", false).clicked() {
            self.grade.layers[i].masks[mi].keys.clear();
        }
        if icon_button(ui, end(1.0), Icon::KeyframeRight, "Next key", false).clicked()
            && let Some(f) = after
        {
            self.seek(f);
        }
        let key_icon = if has_key { Icon::KeyframeRemove } else { Icon::KeyframeAdd };
        if icon_button(ui, end(2.0), key_icon, "Add or remove a key here", has_key).clicked() {
            let m = &mut self.grade.layers[i].masks[mi];
            if has_key {
                m.keys.retain(|k| k.frame != frame);
            } else {
                let k = Key::of(m, frame);
                m.set_key(k);
            }
        }
        if icon_button(ui, end(3.0), Icon::KeyframeLeft, "Previous key", false).clicked()
            && let Some(f) = before
        {
            self.seek(f);
        }
        ui.add_space(4.0);

        graph(ui, &self.grade.layers[i].masks[mi], frames, frame);

        let (row, _) = ui.allocate_exact_size(vec2(w, 30.0), Sense::hover());
        let p = ui.painter();
        let mut x = row.left();
        for (name, col) in [("Pan", PAN), ("Tilt", TILT), ("Zoom", ZOOM), ("Rotate", ROTATE)] {
            p.circle_filled(pos2(x + 4.0, row.center().y), 3.5, col);
            let g = p.layout_no_wrap(name.to_string(), regular(11.5), Color32::WHITE);
            let tw = g.size().x;
            p.galley(pos2(x + 12.0, row.center().y - g.size().y / 2.0), g, Color32::WHITE);
            x += tw + 22.0;
        }
        let seg = Rect::from_min_size(pos2(row.right() - 104.0, row.top()), vec2(104.0, 30.0));
        ui.painter().rect_filled(seg, 4, BUTTON);
        let part = |n: f32| Rect::from_min_size(pos2(seg.left() + 52.0 * n, seg.top()), vec2(52.0, 30.0)).shrink(2.0);
        if pill(ui, part(0.0), "Clip", self.pick.track_scope == 0, 12.0).clicked() {
            self.pick.track_scope = 0;
        }
        if pill(ui, part(1.0), "Frame", self.pick.track_scope == 1, 12.0).clicked() {
            self.pick.track_scope = 1;
        }
        ui.add_space(6.0);
    }

    fn start_tracking(&mut self, i: usize, mi: usize, from: u32, to: u32) {
        let Some(c) = &self.clip else { return };
        if to == from {
            self.say("There are no more frames to track in that direction.");
            return;
        }
        let info = c.info.clone();
        let m = &self.grade.layers[i].masks[mi];
        let k = m.key_at(from);
        let mask = Mask { x: k.x, y: k.y, size_x: k.size_x, size_y: k.size_y, angle: k.angle, ..m.clone() };
        self.tracking = Some((i, mi, track::start(info, mask, from, to)));
    }

    pub(super) fn crop_body(&mut self, ui: &mut Ui) {
        let (fw, fh) = self.turned_size();
        let original = fw / fh;
        let current = self.grade.crop.ratio;
        let mut chips = Chips::new(ui, 30.0);
        for (label, ratio) in CROP_RATIOS {
            let value = if ratio < 0.0 { original } else { ratio };
            let on = if ratio == 0.0 { current == 0.0 } else { (current - value).abs() < 1e-3 };
            if chips.add(label, on && (ratio >= 0.0 || CROP_RATIOS[2..].iter().all(|r| (r.1 - original).abs() > 1e-3))) {
                self.grade.crop.apply_ratio(value, fw, fh);
            }
        }
        chips.end();
        slider(ui, "Straighten", &mut self.grade.crop.angle, -STRAIGHTEN_MAX, STRAIGHTEN_MAX, 0.0, Fmt::Degrees);
        ui.add_space(4.0);
        let w = ui.available_width();
        let (row, _) = ui.allocate_exact_size(vec2(w, 36.0), Sense::hover());
        let cell = w / 6.0;
        let slot = |n: f32| Rect::from_center_size(pos2(row.left() + cell * (n + 0.5), row.center().y), Vec2::splat(36.0));
        if icon_button(ui, slot(0.0), Icon::RotateLeft01, "Rotate left", false).clicked() {
            self.grade.crop.turn_left();
        }
        if icon_button(ui, slot(1.0), Icon::RotateRight01, "Rotate right", false).clicked() {
            self.grade.crop.turn_right();
        }
        let (flip_h, flip_v) = (self.grade.crop.flip_h, self.grade.crop.flip_v);
        if icon_button(ui, slot(2.0), Icon::FlipHorizontal, "Flip horizontal", flip_h).clicked() {
            self.grade.crop.flip_horizontal();
        }
        if icon_button(ui, slot(3.0), Icon::FlipVertical, "Flip vertical", flip_v).clicked() {
            self.grade.crop.flip_vertical();
        }
        if icon_button(ui, slot(4.0), Icon::MagicWand01, "Auto crop", false).clicked() {
            let ratio = self.grade.crop.ratio;
            let r = crop::auto_crop(&self.uncut(), ratio);
            self.grade.crop.set_rect(r);
            self.say("Auto Crop found a framing");
        }
        if icon_button(ui, slot(5.0), Icon::RefreshCcw, "Reset crop", false).clicked() {
            self.grade.crop = Crop::default();
        }
    }
}
