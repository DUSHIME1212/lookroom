//! What is drawn over the photo while cropping or shaping a mask.

use super::kit::*;
use super::{App, Drag};
use crate::crop::{self, Sides};
use crate::masks::{Brush, Key, Mask, Shape, hsl};
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};

const HANDLE_REACH: f32 = 16.0;
const CROP_HANDLE: f32 = 8.0;
const KNOB_GAP: f32 = 0.08;
const LINEAR_KNOB: f32 = 0.18;

impl App {
    /// The crop box: dimmed surroundings, a thirds grid and eight handles.
    pub(super) fn crop_overlay(&mut self, ui: &mut Ui, rect: Rect) {
        let [x, y, w, h] = self.grade.crop.rect();
        let bx = Rect::from_min_size(
            rect.min + vec2(x * rect.width(), y * rect.height()),
            vec2(w * rect.width(), h * rect.height()),
        );
        let p = ui.painter().with_clip_rect(rect);
        let dim = Color32::from_black_alpha(150);
        p.rect_filled(Rect::from_min_max(rect.min, pos2(rect.right(), bx.top())), 0.0, dim);
        p.rect_filled(Rect::from_min_max(pos2(rect.left(), bx.bottom()), rect.max), 0.0, dim);
        p.rect_filled(Rect::from_min_max(pos2(rect.left(), bx.top()), pos2(bx.left(), bx.bottom())), 0.0, dim);
        p.rect_filled(Rect::from_min_max(pos2(bx.right(), bx.top()), pos2(rect.right(), bx.bottom())), 0.0, dim);
        let line = Stroke::new(1.0, Color32::from_white_alpha(120));
        for t in [1.0 / 3.0, 2.0 / 3.0] {
            p.vline(bx.left() + bx.width() * t, bx.y_range(), line);
            p.hline(bx.x_range(), bx.top() + bx.height() * t, line);
        }
        p.rect_stroke(bx, 0, Stroke::new(1.5, Color32::WHITE), StrokeKind::Middle);
        let handles = crop_handles(bx);
        for (at, _) in handles {
            p.rect_filled(Rect::from_center_size(at, Vec2::splat(CROP_HANDLE)), 1, Color32::WHITE);
        }

        let resp = ui.interact(rect, ui.scope_id().with("crop-box"), Sense::click_and_drag());
        // A drag is only noticed once the pointer has moved, so what it grabbed
        // is judged from where the button went down.
        if resp.drag_started()
            && let Some(at) = ui.input(|i| i.pointer.press_origin())
        {
            let near = handles
                .iter()
                .map(|(h, s)| (h.distance(at), *s))
                .filter(|(d, _)| *d < HANDLE_REACH)
                .min_by(|a, b| a.0.total_cmp(&b.0));
            self.drag = near.map(|(_, s)| Drag::CropSides(s)).or(bx.contains(at).then_some(Drag::CropMove));
        }
        if resp.dragged() {
            let d = resp.drag_delta();
            let (dx, dy) = (d.x / rect.width(), d.y / rect.height());
            let (fw, fh) = self.turned_size();
            let c = &mut self.grade.crop;
            match self.drag {
                Some(Drag::CropMove) => {
                    c.x = (c.x + dx).clamp(0.0, 1.0 - c.w);
                    c.y = (c.y + dy).clamp(0.0, 1.0 - c.h);
                }
                Some(Drag::CropSides(s)) => {
                    let r = crop::resize_box(c.rect(), s, dx, dy, c.ratio, fw, fh);
                    c.set_rect(r);
                }
                _ => {}
            }
        }
        if resp.drag_stopped() {
            self.drag = None;
        }
    }

    /// Handles for the chosen mask, drawn over the photo.
    pub(super) fn mask_overlay(&mut self, ui: &mut Ui, rect: Rect) {
        let Some(i) = self.active_layer.filter(|i| *i < self.grade.layers.len()) else { return };
        let mi = self.active_mask;
        if self.grade.layers[i].masks.get(mi).is_none() {
            return;
        }
        let geo = self.grade.crop.geometry(self.preview.w, self.preview.h, true);
        let aspect = self.preview.w as f32 / self.preview.h as f32;
        let to_src = |p: Pos2| {
            let (u, v) = geo.to_source((p.x - rect.left()) / rect.width(), (p.y - rect.top()) / rect.height());
            [u, v]
        };
        let to_scr = |q: [f32; 2]| {
            let (u, v) = geo.from_source(q[0], q[1]);
            pos2(rect.left() + u * rect.width(), rect.top() + v * rect.height())
        };
        let resp = ui.interact(rect, ui.scope_id().with("mask-handles"), Sense::click_and_drag());
        let at = resp.interact_pointer_pos().or(resp.hover_pos());
        let p = ui.painter().with_clip_rect(rect);
        let (brush, erase) = (self.brush, self.erase);
        let mut drag = self.drag;
        let mut picked = None;
        let frame = self.frame();
        let m = &mut self.grade.layers[i].masks[mi];
        // A mask with keys sits wherever they put it at this frame.
        if !m.keys.is_empty() {
            let k = m.key_at(frame);
            (m.x, m.y, m.size_x, m.size_y, m.angle) = (k.x, k.y, k.size_x, k.size_y, k.angle);
        }
        let (sin, cos) = m.angle.to_radians().sin_cos();
        // A point along the mask's own turned axes, in photo positions.
        let along = |m: &Mask, ax: f32, ay: f32| [m.x + (ax * cos - ay * sin) / aspect, m.y + ax * sin + ay * cos];
        let ink = |p: &egui::Painter, pts: Vec<Pos2>, closed: bool| {
            let mut pts = pts;
            if closed && let Some(first) = pts.first().copied() {
                pts.push(first);
            }
            p.add(egui::Shape::line(pts.clone(), Stroke::new(3.0, Color32::from_black_alpha(110))));
            p.add(egui::Shape::line(pts, Stroke::new(1.5, Color32::WHITE)));
        };

        let mut handles: Vec<(Drag, [f32; 2])> = Vec::new();
        match m.shape {
            Shape::Linear => {
                let reach = 2.0;
                ink(&p, vec![to_scr(along(m, 0.0, -reach)), to_scr(along(m, 0.0, reach))], false);
                let half = m.feather * 0.5;
                for s in [-half, half] {
                    let thin = Stroke::new(1.0, Color32::from_white_alpha(130));
                    p.line_segment([to_scr(along(m, s, -reach)), to_scr(along(m, s, reach))], thin);
                }
                handles.push((Drag::Centre, [m.x, m.y]));
                handles.push((Drag::Turn, along(m, LINEAR_KNOB, 0.0)));
            }
            Shape::Radial => {
                let pts = (0..64)
                    .map(|n| {
                        let t = n as f32 / 64.0 * std::f32::consts::TAU;
                        to_scr(along(m, t.cos() * m.size_x, t.sin() * m.size_y))
                    })
                    .collect();
                ink(&p, pts, true);
                handles.push((Drag::Centre, [m.x, m.y]));
                handles.push((Drag::Width, along(m, m.size_x, 0.0)));
                handles.push((Drag::Height, along(m, 0.0, m.size_y)));
                handles.push((Drag::Turn, along(m, 0.0, -m.size_y - KNOB_GAP)));
            }
            Shape::Rectangle => {
                let (a, b) = (m.size_x, m.size_y);
                let pts = [(-a, -b), (a, -b), (a, b), (-a, b)].map(|(x, y)| to_scr(along(m, x, y)));
                ink(&p, pts.to_vec(), true);
                handles.push((Drag::Centre, [m.x, m.y]));
                handles.push((Drag::Corner, along(m, a, b)));
                handles.push((Drag::Turn, along(m, 0.0, -b - KNOB_GAP)));
            }
            Shape::Draw => {
                if let Some(at) = at {
                    let long = geo.out_w.max(geo.out_h) as f32 / rect.width().max(rect.height());
                    let radius = brush.0 * self.preview.w.max(self.preview.h) as f32 / long;
                    p.circle_stroke(at, radius, Stroke::new(1.5, Color32::WHITE));
                }
            }
            Shape::Curve => {
                if m.points.len() >= 3 {
                    ink(&p, m.outline().into_iter().map(to_scr).collect(), true);
                } else if m.points.len() == 2 {
                    ink(&p, m.points.iter().copied().map(to_scr).collect(), false);
                }
                for (n, q) in m.points.iter().enumerate() {
                    handles.push((Drag::Point(n), *q));
                }
            }
            Shape::Qualifier => {}
        }
        for (d, q) in &handles {
            let c = to_scr(*q);
            let fill = if matches!(d, Drag::Turn) { ACCENT } else { Color32::WHITE };
            p.circle(c, 6.0, fill, Stroke::new(1.5, Color32::from_black_alpha(140)));
        }
        let nearest_to = |a: Pos2| {
            handles
                .iter()
                .map(|(d, q)| (*d, to_scr(*q).distance(a)))
                .filter(|(_, r)| *r < HANDLE_REACH)
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(d, _)| d)
        };
        let nearest = at.and_then(nearest_to);

        if resp.drag_started() {
            // Judged from where the button went down: a quick drag has already moved on.
            let from = ui.input(|i| i.pointer.press_origin()).or(at);
            drag = from.and_then(nearest_to);
            if m.shape == Shape::Draw
                && let Some(a) = from
            {
                drag = Some(Drag::Paint);
                m.strokes.push(Brush { points: vec![to_src(a)], radius: brush.0, softness: brush.1, opacity: brush.2, erase });
            }
        }
        if resp.dragged()
            && let (Some(a), Some(d)) = (at, drag)
        {
            let q = to_src(a);
            let (dx, dy) = ((q[0] - m.x) * aspect, q[1] - m.y);
            match d {
                Drag::Centre => {
                    m.x = q[0].clamp(0.0, 1.0);
                    m.y = q[1].clamp(0.0, 1.0);
                }
                Drag::Turn => {
                    let toward = dy.atan2(dx).to_degrees();
                    let turned = if m.shape == Shape::Linear { toward } else { toward + 90.0 };
                    m.angle = turned.rem_euclid(360.0);
                }
                Drag::Width => m.size_x = (dx * cos + dy * sin).abs().max(0.01),
                Drag::Height => m.size_y = (dy * cos - dx * sin).abs().max(0.01),
                Drag::Corner => {
                    m.size_x = (dx * cos + dy * sin).abs().max(0.01);
                    m.size_y = (dy * cos - dx * sin).abs().max(0.01);
                }
                Drag::Point(n) => {
                    if let Some(point) = m.points.get_mut(n) {
                        *point = q;
                    }
                }
                Drag::Paint => {
                    if let Some(b) = m.strokes.last_mut() {
                        b.points.push(q);
                    }
                }
                _ => {}
            }
        }
        if resp.dragged() && !m.keys.is_empty() && matches!(drag, Some(Drag::Centre | Drag::Turn | Drag::Width | Drag::Height | Drag::Corner)) {
            m.set_key(Key::of(m, frame));
        }
        if resp.drag_stopped() {
            drag = None;
        }
        if resp.clicked()
            && let Some(a) = at
        {
            let q = to_src(a);
            match m.shape {
                Shape::Curve if nearest.is_none() => m.points.push(q),
                Shape::Draw => {
                    m.strokes.push(Brush { points: vec![q], radius: brush.0, softness: brush.1, opacity: brush.2, erase });
                }
                Shape::Qualifier => {
                    let px = &self.preview;
                    let (x, y) = ((q[0] * px.w as f32) as usize, (q[1] * px.h as f32) as usize);
                    let colour = px.px[x.min(px.w - 1) + y.min(px.h - 1) * px.w];
                    let (hue, _, _) = hsl(colour);
                    m.hue_on = true;
                    m.hue_center = hue;
                    picked = Some(hue);
                }
                _ => {}
            }
        }
        if resp.secondary_clicked()
            && let Some(Drag::Point(n)) = nearest
        {
            m.points.remove(n);
        }
        self.drag = drag;
        if let Some(h) = picked {
            self.say(format!("Picked hue {h:.0}"));
        }
    }
}

/// The eight grab points of a crop box, with the sides each one moves.
fn crop_handles(b: Rect) -> [(Pos2, Sides); 8] {
    let sides = |left, top, right, bottom| Sides { left, top, right, bottom };
    let (cx, cy) = (b.center().x, b.center().y);
    [
        (b.left_top(), sides(true, true, false, false)),
        (b.right_top(), sides(false, true, true, false)),
        (b.left_bottom(), sides(true, false, false, true)),
        (b.right_bottom(), sides(false, false, true, true)),
        (pos2(cx, b.top()), sides(false, true, false, false)),
        (pos2(cx, b.bottom()), sides(false, false, false, true)),
        (pos2(b.left(), cy), sides(true, false, false, false)),
        (pos2(b.right(), cy), sides(false, false, true, false)),
    ]
}
