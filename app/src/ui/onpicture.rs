//! Moving and resizing the picked layer right on the picture: a video over the picture,
//! a text, a sticker or a design layer. Drag it to move it; drag its corner to resize it.
//! With keyframes, the change sets a keyframe at the playhead, as the sliders do.

use super::App;
use super::kit::*;
use crate::timeline::{Body, Picked};
use eframe::egui::{self, Color32, CursorIcon, Rect, Sense, Stroke, StrokeKind, Ui, pos2, vec2};

/// Text and sticker sizes are points on a picture this tall.
const REFERENCE_H: f32 = 1080.0;
/// About how wide a letter is, in text heights, for the outline round a text.
const LETTER_W: f32 = 0.56;
const HANDLE: f32 = 12.0;
/// The smallest and largest a drag can make a layer, as a multiple of its size.
const SMALLEST: f32 = 0.05;
const LARGEST: f32 = 6.0;

impl App {
    /// The right-click menu of a layer on the timeline: its place in the stack, and for a
    /// video, its framing and sound.
    pub(super) fn item_menu(&mut self, ui: &mut Ui, id: u32) {
        if !self.is_selected(id) {
            self.select_only(id);
        }
        let Some(i) = self.project.items.iter().position(|t| t.id == id) else { return };
        let last = self.project.items.len() - 1;
        // Later in the list draws on top.
        if ui.add_enabled(i < last, egui::Button::new("Bring to Front")).clicked() {
            let t = self.project.items.remove(i);
            self.project.items.push(t);
            ui.close();
        }
        if ui.add_enabled(i < last, egui::Button::new("Bring Forward")).clicked() {
            self.project.items.swap(i, i + 1);
            ui.close();
        }
        if ui.add_enabled(i > 0, egui::Button::new("Send Backward")).clicked() {
            self.project.items.swap(i, i - 1);
            ui.close();
        }
        if ui.add_enabled(i > 0, egui::Button::new("Send to Back")).clicked() {
            let t = self.project.items.remove(i);
            self.project.items.insert(0, t);
            ui.close();
        }
        let video = match &self.project.items[i].body {
            Body::Overlay { media, volume, .. } => Some((*media, *volume)),
            _ => None,
        };
        if let Some((media, volume)) = video {
            ui.separator();
            let (pw, ph) = (self.project.size.0 as f32, self.project.size.1 as f32);
            let aspect = self.project.media_of(media).map_or(16.0 / 9.0, |m| m.size.0.max(1) as f32 / m.size.1.max(1) as f32);
            let mut frame = None;
            if ui.button("Full Frame").on_hover_text("Fills the picture, its edges cropped if its shape differs").clicked() {
                frame = Some((((pw / ph) / aspect).max(1.0), 0.5, 0.5));
            }
            if ui.button("Picture in Picture").on_hover_text("A small window low on the right").clicked() {
                frame = Some((0.38, 0.74, 0.72));
            }
            if let Some((s, nx, ny)) = frame
                && let Body::Overlay { size, x, y, .. } = &mut self.project.items[i].body
            {
                (*size, *x, *y) = (s, nx, ny);
                ui.close();
            }
            let muted = volume <= 0.001;
            if ui.button(if muted { "Unmute Its Sound" } else { "Mute Its Sound" }).clicked() {
                if let Body::Overlay { volume, .. } = &mut self.project.items[i].body {
                    *volume = if muted { 1.0 } else { 0.0 };
                }
                ui.close();
            }
        }
        ui.separator();
        if ui.button("Duplicate").clicked() {
            self.picked = self.project.duplicate(Picked::Item(id));
            ui.close();
        }
        if ui.button("Delete").clicked() {
            if !self.delete_selection() {
                self.project.items.retain(|t| t.id != id);
                self.picked = None;
            }
            ui.close();
        }
    }

    /// The picked layer's outline and handles on the picture shown in `area`.
    pub(super) fn layer_handles(&mut self, ui: &mut Ui, area: Rect) {
        if self.run.is_some() {
            return;
        }
        let Some(Picked::Item(id)) = self.picked else { return };
        let Some(view) = &self.view_tex else { return };
        let (iw, ih) = (view.size()[0] as f32, view.size()[1] as f32);
        let (pw, ph) = (self.project.size.0 as f32, self.project.size.1 as f32);
        // Only when the picture shown is the video's own shape, so positions line up.
        if ((iw / ih) - (pw / ph)).abs() > 0.01 {
            return;
        }
        let s = (area.width() / iw).min(area.height() / ih);
        let pic = Rect::from_center_size(area.center(), vec2(iw * s, ih * s));
        let head = self.head;
        let Some(t) = self.project.items.iter().find(|t| t.id == id && head >= t.at && head < t.end()).cloned() else { return };
        let Some((x, y, m)) = t.body.placed() else { return };
        let local = head - t.at;
        let now = m.at(local, x, y);
        // The layer's size on the picture, in screen points.
        let (w, h) = match &t.body {
            Body::Overlay { media, size, .. } => {
                let aspect = self.project.media_of(*media).map_or(16.0 / 9.0, |m| m.size.0.max(1) as f32 / m.size.1.max(1) as f32);
                (size * pic.height() * aspect, size * pic.height())
            }
            Body::Sticker { size, .. } => (size / REFERENCE_H * pic.height(), size / REFERENCE_H * pic.height()),
            Body::Graphic { file, layer, size, .. } => match crate::graphics::load(file) {
                Some(d) => {
                    let [_, _, lw, lh] = d.layers.get(*layer).map_or([0.0, 0.0, 1.0, 1.0], |l| l.bounds);
                    let fit = d.fit(pw, ph) * size;
                    (lw * fit / pw * pic.width(), lh * fit / ph * pic.height())
                }
                None => return,
            },
            Body::Text { text, size, .. } => {
                let plain = crate::overlay::plain(text);
                let em = size / REFERENCE_H * pic.height();
                let longest = plain.lines().map(|l| l.chars().count()).max().unwrap_or(1) as f32;
                (longest * em * LETTER_W, plain.lines().count().max(1) as f32 * em * 1.2)
            }
            _ => return,
        };
        let (w, h) = (w * now.scale, h * now.scale);
        let centre = pos2(pic.left() + now.x * pic.width(), pic.top() + now.y * pic.height());
        let outline = Rect::from_center_size(centre, vec2(w.max(8.0), h.max(8.0)));
        let corner = Rect::from_center_size(outline.right_bottom(), vec2(HANDLE, HANDLE));
        let body = ui.interact(outline.intersect(pic), ui.scope_id().with(("layer-move", id)), Sense::drag());
        let grip = ui.interact(corner, ui.scope_id().with(("layer-size", id)), Sense::drag());
        let p = ui.painter().with_clip_rect(area);
        p.rect_stroke(outline, 2, Stroke::new(1.5, ACCENT), StrokeKind::Outside);
        p.rect_filled(corner, 2, Color32::WHITE);
        p.rect_stroke(corner, 2, Stroke::new(1.5, ACCENT), StrokeKind::Inside);
        if grip.hovered() || grip.dragged() {
            ui.ctx().set_cursor_icon(CursorIcon::ResizeNwSe);
        } else if body.hovered() || body.dragged() {
            ui.ctx().set_cursor_icon(if body.dragged() { CursorIcon::Grabbing } else { CursorIcon::Grab });
        }
        let moved = body.drag_delta();
        let grown = if grip.dragged() {
            // How much farther the corner now sits from the centre.
            let before = (corner.center() - centre).length().max(1.0);
            let after = (corner.center() + grip.drag_delta() - centre).length();
            Some((after / before).clamp(0.5, 2.0))
        } else {
            None
        };
        if moved == egui::Vec2::ZERO && grown.is_none() {
            return;
        }
        let Some(item) = self.project.items.iter_mut().find(|t| t.id == id) else { return };
        let keyed = item.body.placed().is_some_and(|(_, _, m)| !m.keys.is_empty());
        // Unkeyed text and stickers grow by their own size, which keeps letters sharp.
        if let (Some(k), false) = (grown, keyed) {
            match &mut item.body {
                Body::Overlay { size, .. } | Body::Graphic { size, .. } => *size = (*size * k).clamp(SMALLEST, LARGEST),
                Body::Sticker { size, .. } | Body::Text { size, .. } => *size = (*size * k).clamp(8.0, 2000.0),
                _ => {}
            }
        }
        let Some((x, y, m)) = item.body.placed_mut() else { return };
        let (dx, dy) = (moved.x / pic.width(), moved.y / pic.height());
        if keyed {
            let mut v = m.at(local, *x, *y);
            v.x += dx;
            v.y += dy;
            if let Some(k) = grown {
                v.scale = (v.scale * k).clamp(SMALLEST, LARGEST);
            }
            m.set_key(local, v);
        } else {
            *x += dx;
            *y += dy;
        }
    }
}
