//! The look of the app: colours, type and the shared controls.

use crate::color;
use crate::curves::{Points, Table};
use crate::icons::{self, Icon};
use eframe::egui::epaint::{Mesh, Vertex, WHITE_UV};
use eframe::egui::{
    self, Align2, Area, Color32, Context, FontFamily, FontId, Id, Order, Pos2, Rect, Response, Sense, Shape, Stroke,
    StrokeKind, Ui, Vec2, pos2, vec2,
};

/// The one accent: the main action on a page, a value being set, and state. Never decoration.
pub const ACCENT: Color32 = Color32::from_rgb(0x52, 0x78, 0x8B);
/// The accent at a whisper, behind a chosen row or chip.
pub const ACCENT_SOFT: Color32 = Color32::from_rgb(0x25, 0x32, 0x39);
/// A chosen tab or segment: lifted, not lit.
pub const RAISED: Color32 = Color32::from_rgb(0x3C, 0x3C, 0x42);
/// Secondary text: values, names under tiles.
pub const TEXT_2: Color32 = Color32::from_rgb(0xD0, 0xD0, 0xD4);
/// Something went wrong.
pub const ERROR: Color32 = Color32::from_rgb(0xF2, 0x55, 0x7A);
/// The ring round whatever the keyboard is on.
pub const FOCUS: Color32 = Color32::from_rgb(0xA6, 0xC9, 0xD8);
pub const PANEL: Color32 = Color32::from_rgb(0x16, 0x16, 0x18);
pub const PANEL_EDGE: Color32 = Color32::from_rgb(0x2C, 0x2C, 0x2E);
pub const BUTTON: Color32 = Color32::from_rgb(0x2C, 0x2C, 0x2E);
pub const BUTTON_EDGE: Color32 = Color32::from_rgb(0x3E, 0x3E, 0x41);
pub const MENU: Color32 = Color32::from_rgb(0x24, 0x24, 0x26);
pub const MUTED: Color32 = Color32::from_rgb(0x8E, 0x8E, 0x93);
pub const TRACK: Color32 = Color32::from_rgb(0x3A, 0x3A, 0x3C);
pub const PANEL_RADIUS: u8 = 4;

pub const CHANNELS: [Color32; 4] = [
    Color32::WHITE,
    Color32::from_rgb(255, 86, 86),
    Color32::from_rgb(86, 222, 112),
    Color32::from_rgb(92, 150, 255),
];

/// Medium is the largest interface type size; canvas artwork keeps its own sizing.
pub const MEDIUM_FONT_SIZE: f32 = 14.0;

pub fn bold(size: f32) -> FontId {
    FontId::new(size.min(MEDIUM_FONT_SIZE), FontFamily::Name("bold".into()))
}
pub fn semi(size: f32) -> FontId {
    FontId::new(size.min(MEDIUM_FONT_SIZE), FontFamily::Name("semibold".into()))
}
pub fn regular(size: f32) -> FontId {
    FontId::new(size.min(MEDIUM_FONT_SIZE), FontFamily::Proportional)
}

pub fn text_width(ui: &Ui, s: &str, font: FontId) -> f32 {
    ui.painter().layout_no_wrap(s.to_string(), font, Color32::WHITE).size().x
}

pub fn card(ui: &Ui, r: Rect) {
    let p = ui.painter();
    p.rect_filled(r, PANEL_RADIUS, PANEL);
    p.rect_stroke(r, PANEL_RADIUS, Stroke::new(1.0, PANEL_EDGE), StrokeKind::Inside);
}

/// Two half-width cells, for controls that sit side by side.
pub fn split_row(ui: &mut Ui, h: f32, gap: f32) -> (Rect, Rect) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), h), Sense::hover());
    let w = (r.width() - gap) / 2.0;
    (Rect::from_min_size(r.min, vec2(w, h)), Rect::from_min_size(pos2(r.left() + w + gap, r.top()), vec2(w, h)))
}

/// A rounded tab: the blue fill marks the chosen one.
pub fn pill(ui: &mut Ui, r: Rect, label: &str, on: bool, size: f32) -> Response {
    let resp = ui.interact(r, ui.scope_id().with(("pill", label, r.min.x as i32, r.min.y as i32)), Sense::click());
    let p = ui.painter();
    let radius = 4;
    let fill = if on {
        RAISED
    } else if resp.hovered() {
        BUTTON_EDGE
    } else {
        Color32::TRANSPARENT
    };
    p.rect_filled(r, radius, fill);
    if on {
        p.rect_stroke(r, radius, Stroke::new(1.0, Color32::from_white_alpha(18)), StrokeKind::Inside);
    }
    let text = if on || resp.hovered() { Color32::WHITE } else { TEXT_2 };
    p.text(r.center(), Align2::CENTER_CENTER, label, semi(size), text);
    focus_ring(ui, &resp, r, radius);
    resp
}

/// Whether the keyboard, not the pointer, moved last: rings show only then.
pub fn keyboard_moved(ctx: &Context) -> bool {
    ctx.data(|d| d.get_temp::<bool>(Id::NULL.with("keyboard-moved"))).unwrap_or(false)
}

/// Called once a frame: a Tab press turns focus rings on, a pointer press turns them off.
pub fn note_input(ctx: &Context) {
    let (tab, pressed) = ctx.input(|i| (i.key_pressed(egui::Key::Tab), i.pointer.any_pressed()));
    if tab || pressed {
        ctx.data_mut(|d| d.insert_temp(Id::NULL.with("keyboard-moved"), tab));
    }
}

/// A ring round a control the keyboard is on, drawn at once and never animated.
pub fn focus_ring(ui: &Ui, resp: &Response, r: Rect, radius: u8) {
    if resp.has_focus() && keyboard_moved(ui.ctx()) {
        ui.painter().rect_stroke(r.expand(2.0), radius.saturating_add(2), Stroke::new(2.0, FOCUS), StrokeKind::Outside);
    }
}

/// A chosen row in a list: a soft wash of the accent with a mark at its edge.
pub fn chosen_row(p: &egui::Painter, r: Rect, radius: u8) {
    let radius = radius.min(4);
    p.rect_filled(r, radius, ACCENT_SOFT);
    p.rect_stroke(r, radius, Stroke::new(1.0, ACCENT.gamma_multiply(0.55)), StrokeKind::Inside);
}

/// The play control: a white disc, the one control on the transport that is always found first.
pub fn play_button(ui: &mut Ui, c: Pos2, playing: bool) -> Response {
    let r = Rect::from_center_size(c, Vec2::splat(40.0));
    let resp = ui.interact(r, ui.scope_id().with("play"), Sense::click()).on_hover_text(if playing { "Pause (Space)" } else { "Play (Space)" });
    let p = ui.painter();
    let disc = if resp.is_pointer_button_down_on() { Color32::from_gray(200) } else if resp.hovered() { Color32::from_gray(235) } else { Color32::WHITE };
    p.rect_filled(Rect::from_center_size(c, Vec2::splat(36.0)), 4, disc);
    icons::paint(p, if playing { Icon::Pause } else { Icon::Play }, Rect::from_center_size(c + vec2(if playing { 0.0 } else { 1.0 }, 0.0), Vec2::splat(18.0)), PANEL);
    focus_ring(ui, &resp, Rect::from_center_size(c, Vec2::splat(36.0)), 4);
    resp
}

/// Helper text that says something failed.
pub fn error_note(ui: &mut Ui, text: &str) {
    ui.add(egui::Label::new(egui::RichText::new(text).font(regular(12.5)).color(ERROR)).wrap());
    ui.add_space(6.0);
}

/// A filled rounded button, for actions rather than tabs.
pub fn action(ui: &mut Ui, r: Rect, label: &str, on: bool, size: f32) -> Response {
    let resp = ui.interact(r, ui.scope_id().with(("action", label, r.min.x as i32, r.min.y as i32)), Sense::click());
    let p = ui.painter();
    let radius = 4;
    let fill = if on {
        ACCENT
    } else if resp.hovered() {
        BUTTON_EDGE
    } else {
        BUTTON
    };
    p.rect_filled(r, radius, fill);
    p.text(r.center(), Align2::CENTER_CENTER, label, semi(size), Color32::WHITE);
    focus_ring(ui, &resp, r, radius);
    resp
}

/// Buttons laid out in a line that wraps onto the next row when it runs out of room.
pub struct Chips<'a> {
    ui: &'a mut Ui,
    x: f32,
    y: f32,
    left: f32,
    right: f32,
    h: f32,
}

impl<'a> Chips<'a> {
    pub fn new(ui: &'a mut Ui, h: f32) -> Self {
        let r = ui.available_rect_before_wrap();
        Self { x: r.left(), y: r.top(), left: r.left(), right: r.right(), h, ui }
    }

    pub fn add(&mut self, label: &str, on: bool) -> bool {
        let w = text_width(self.ui, label, semi(12.5)) + 22.0;
        if self.x + w > self.right && self.x > self.left {
            self.x = self.left;
            self.y += self.h + 6.0;
        }
        let r = Rect::from_min_size(pos2(self.x, self.y), vec2(w, self.h));
        self.x += w + 6.0;
        let resp = self.ui.interact(r, self.ui.scope_id().with(("chip", label, r.min.x as i32, r.min.y as i32)), Sense::click());
        let radius = 4;
        let p = self.ui.painter();
        if on {
            chosen_row(p, r, radius);
        } else {
            p.rect_filled(r, radius, if resp.hovered() { BUTTON_EDGE } else { BUTTON });
        }
        p.text(r.center(), Align2::CENTER_CENTER, label, semi(12.5), if on { Color32::WHITE } else { TEXT_2 });
        focus_ring(self.ui, &resp, r, radius);
        resp.clicked()
    }

    pub fn end(self) {
        let used = Rect::from_min_max(pos2(self.left, self.y), pos2(self.right, self.y + self.h));
        self.ui.advance_cursor_after_rect(used);
        self.ui.add_space(8.0);
    }
}

pub fn icon_button(ui: &mut Ui, r: Rect, icon: Icon, tip: &str, on: bool) -> Response {
    let resp = ui.interact(r, ui.scope_id().with(("icon", tip, r.min.x as i32, r.min.y as i32)), Sense::click()).on_hover_text(tip);
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(r, 4, Color32::from_white_alpha(22));
    }
    let side = (r.height() * 0.62).round();
    let tint = if on { ACCENT } else { Color32::WHITE };
    icons::paint(p, icon, Rect::from_center_size(r.center(), Vec2::splat(side)), tint);
    focus_ring(ui, &resp, r, (r.height() / 2.0) as u8);
    resp
}

/// The dark round buttons at the ends of the top row.
pub fn round_button(ui: &mut Ui, c: Pos2, side: f32, icon: Icon, tip: &str) -> Response {
    let r = Rect::from_center_size(c, Vec2::splat(side));
    let resp = ui.interact(r, ui.scope_id().with(("round", tip)), Sense::click()).on_hover_text(tip);
    let p = ui.painter();
    p.rect_filled(r, 4, if resp.hovered() { BUTTON_EDGE } else { BUTTON });
    icons::paint(p, icon, Rect::from_center_size(c, Vec2::splat((side * 0.52).round())), Color32::WHITE);
    focus_ring(ui, &resp, r, 4);
    resp
}

#[derive(Clone, Copy)]
pub enum Fmt {
    Percent,
    Plain,
    Int,
    Kelvin,
    Degrees,
    Stops,
}

pub fn fmt_value(v: f32, f: Fmt) -> String {
    match f {
        Fmt::Percent => format!("{:.0}%", v * 100.0),
        Fmt::Plain => format!("{v:.2}"),
        Fmt::Int => format!("{v:.0}"),
        Fmt::Kelvin => format!("{:.0}K", v + 6500.0),
        Fmt::Degrees => format!("{v:.1}°"),
        Fmt::Stops => format!("{v:+.2}"),
    }
}

/// Label and editable value above a dedicated track. Shift-drag adjusts precisely;
/// keyboard arrows step, Home/End reach bounds, and reset restores the neutral value.
pub fn slider(ui: &mut Ui, label: &str, v: &mut f32, min: f32, max: f32, neutral: f32, f: Fmt) -> Response {
    slider_on(ui, label, v, min, max, neutral, f, None)
}

pub fn slider_on(
    ui: &mut Ui,
    label: &str,
    v: &mut f32,
    min: f32,
    max: f32,
    neutral: f32,
    f: Fmt,
    band: Option<&[Color32]>,
) -> Response {
    let old = *v;
    let (r, allocated) = ui.allocate_exact_size(vec2(ui.available_width(), 58.0), Sense::hover());
    let span = (max - min).max(f32::EPSILON);
    let neutral = neutral.clamp(min, max);
    let reset_rect = Rect::from_min_size(pos2(r.right() - 24.0, r.top()), vec2(24.0, 24.0));
    let value_rect = Rect::from_min_size(pos2((reset_rect.left() - 86.0).max(r.left()), r.top()), vec2(80.0, 24.0));
    let label_rect = Rect::from_min_max(r.min, pos2(value_rect.left() - 6.0, r.top() + 24.0));
    ui.painter().with_clip_rect(label_rect).text(pos2(r.left(), r.top() + 12.0), Align2::LEFT_CENTER, label, regular(13.0), TEXT_2);

    // Edit in the units shown, so typing 50 into a percentage means 50%, not 5000%.
    let (scale, offset, suffix, decimals) = match f {
        Fmt::Percent => (100.0, 0.0, "%", 1),
        Fmt::Kelvin => (1.0, 6500.0, " K", 0),
        Fmt::Degrees => (1.0, 0.0, "°", 1),
        Fmt::Int => (1.0, 0.0, "", 0),
        Fmt::Stops => (1.0, 0.0, "", 2),
        Fmt::Plain => (1.0, 0.0, "", 2),
    };
    let mut displayed = *v * scale + offset;
    let step = match f { Fmt::Int | Fmt::Kelvin => 1.0, _ => span / 200.0 };
    ui.painter().rect_filled(value_rect, 6, BUTTON);
    let value_response = ui.put(value_rect, egui::DragValue::new(&mut displayed)
        .range((min * scale + offset)..=(max * scale + offset))
        .speed(step * scale).max_decimals(decimals).suffix(suffix))
        .on_hover_text("Click to type a value · drag to adjust · Shift for precision");
    ui.painter().rect_stroke(value_rect, 6, Stroke::new(1.0, if value_response.has_focus() { FOCUS } else if value_response.hovered() { MUTED } else { BUTTON_EDGE }), StrokeKind::Inside);
    if value_response.changed() { *v = ((displayed - offset) / scale).clamp(min, max); }
    let reset = ui.put(reset_rect, egui::Button::new("↺").frame(false))
        .on_hover_text(format!("Reset to {}", fmt_value(neutral, f)));
    if reset.clicked() { *v = neutral; }

    let hit = Rect::from_min_max(pos2(r.left(), r.top() + 26.0), pos2(r.right(), r.bottom()));
    let mut resp = ui.interact(hit, allocated.id.with("track"), Sense::click_and_drag())
        .on_hover_text("Drag to adjust · Shift for precision · arrows to step · double-click to reset");
    let (tl, tr) = (hit.left() + 9.0, hit.right() - 9.0);
    let width = (tr - tl).max(1.0);
    if resp.clicked() {
        resp.request_focus();
        if let Some(p) = resp.interact_pointer_pos() { *v = min + ((p.x - tl) / width).clamp(0.0, 1.0) * span; }
    }
    let drag_id = allocated.id.with("drag-value");
    if resp.drag_started() { ui.data_mut(|d| d.insert_temp(drag_id, *v)); }
    if resp.dragged() {
        resp.request_focus();
        let delta = ui.input(|i| i.pointer.delta().x * if i.modifiers.shift { 0.1 } else { 1.0 });
        let raw = ui.data(|d| d.get_temp::<f32>(drag_id)).unwrap_or(*v);
        let raw = (raw + delta / width * span).clamp(min, max);
        ui.data_mut(|d| d.insert_temp(drag_id, raw));
        *v = raw;
    }
    if resp.double_clicked() { *v = neutral; }
    if resp.has_focus() {
        ui.input(|i| {
            let fine = if i.modifiers.shift && !matches!(f, Fmt::Int | Fmt::Kelvin) { 0.1 } else { 1.0 };
            if i.key_pressed(egui::Key::ArrowRight) || i.key_pressed(egui::Key::ArrowUp) { *v += step * fine; }
            if i.key_pressed(egui::Key::ArrowLeft) || i.key_pressed(egui::Key::ArrowDown) { *v -= step * fine; }
            if i.key_pressed(egui::Key::Home) { *v = min; }
            if i.key_pressed(egui::Key::End) { *v = max; }
        });
    }
    if *v != old {
        *v = (*v).clamp(min, max);
        if matches!(f, Fmt::Int | Fmt::Kelvin) { *v = v.round().clamp(min, max); }
    }
    let y = hit.center().y;
    let x = tl + width * ((*v - min) / span).clamp(0.0, 1.0);
    let nx = tl + width * ((neutral - min) / span).clamp(0.0, 1.0);
    let bar = Rect::from_min_max(pos2(tl, y - 2.5), pos2(tr, y + 2.5));
    let p = ui.painter();
    p.rect_filled(bar, 3, TRACK);
    if let Some(colours) = band { icons::colour_band(p, bar, colours); }
    else { p.rect_filled(Rect::from_min_max(pos2(x.min(nx), y - 2.5), pos2(x.max(nx), y + 2.5)), 3, ACCENT); }
    p.line_segment([pos2(nx, y - 5.0), pos2(nx, y + 5.0)], Stroke::new(1.0, MUTED));
    let handle = Rect::from_center_size(pos2(x, y), vec2(10.0, 20.0));
    if resp.hovered() || resp.dragged() { p.rect_filled(handle.expand(4.0), 4, Color32::from_white_alpha(18)); }
    p.rect_filled(handle, 2, Color32::WHITE);
    p.rect_stroke(handle, 2, Stroke::new(1.0, if resp.dragged() { ACCENT } else { BUTTON_EDGE }), StrokeKind::Inside);
    if resp.has_focus() { p.rect_stroke(handle.expand(3.0), 4, Stroke::new(1.5, FOCUS), StrokeKind::Inside); }
    if resp.dragged() { value_bubble(ui, pos2(x, y - 16.0), &fmt_value(*v, f)); }
    if *v != old { resp.mark_changed(); }
    resp.widget_info(|| egui::WidgetInfo::slider(ui.is_enabled(), *v as f64, label));
    resp | value_response | reset
}

/// The value over a slider's knob, on top of everything so no panel edge cuts it off.
pub fn value_bubble(ui: &Ui, tip: Pos2, text: &str) {
    let layer = egui::LayerId::new(Order::Tooltip, ui.scope_id().with("value-bubble"));
    let p = ui.ctx().layer_painter(layer);
    let galley = p.layout_no_wrap(text.to_string(), semi(11.5), Color32::WHITE);
    let size = galley.size() + vec2(14.0, 6.0);
    let body = Rect::from_center_size(tip - vec2(0.0, size.y / 2.0 + 5.0), size);
    p.rect_filled(body, 5, ACCENT);
    p.add(Shape::convex_polygon(vec![pos2(tip.x - 5.0, body.bottom() - 0.5), pos2(tip.x + 5.0, body.bottom() - 0.5), pos2(tip.x, tip.y)], ACCENT, Stroke::NONE));
    p.galley(body.center() - galley.size() / 2.0, galley, Color32::WHITE);
}

/// A row of short choices sharing one capsule.
pub fn segmented(ui: &mut Ui, options: &[&str], at: &mut usize) -> bool {
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(vec2(w, 32.0), Sense::hover());
    ui.painter().rect_filled(r, 4, BUTTON);
    let cell = w / options.len().max(1) as f32;
    let mut changed = false;
    for (i, o) in options.iter().enumerate() {
        let c = Rect::from_min_size(pos2(r.left() + cell * i as f32, r.top()), vec2(cell, 32.0)).shrink(2.0);
        if pill(ui, c, o, *at == i, 12.0).clicked() && *at != i {
            *at = i;
            changed = true;
        }
    }
    ui.add_space(8.0);
    changed
}

pub fn toggle(ui: &mut Ui, label: &str, on: &mut bool) -> bool {
    let w = ui.available_width();
    let (r, resp) = ui.allocate_exact_size(vec2(w, 32.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
    }
    let p = ui.painter();
    p.text(pos2(r.left(), r.center().y), Align2::LEFT_CENTER, label, semi(13.0), Color32::WHITE);
    let s = Rect::from_min_size(pos2(r.right() - 42.0, r.center().y - 12.0), vec2(42.0, 24.0));
    p.rect_filled(s, 4, if *on { ACCENT } else { TRACK });
    let x = if *on { s.right() - 12.0 } else { s.left() + 12.0 };
    p.rect_filled(Rect::from_center_size(pos2(x, s.center().y), vec2(16.0, 18.0)), 2, Color32::WHITE);
    focus_ring(ui, &resp, s, 4);
    resp.clicked()
}

/// A colour button that changes `c` only when the person picks a colour. egui's own writes
/// back a round trip through its colour space every frame, which would look like an edit.
pub fn colour_button(ui: &mut Ui, c: &mut [f32; 3]) -> Response {
    let mut v = *c;
    let resp = egui::widgets::color_picker::color_edit_button_rgb(ui, &mut v);
    if resp.changed() {
        *c = v;
    }
    resp
}

pub fn note(ui: &mut Ui, text: &str) {
    ui.add(egui::Label::new(egui::RichText::new(text).font(regular(12.5)).color(MUTED)).wrap());
    ui.add_space(6.0);
}

/// Tappable colour dots, for choosing a band or a corner of the colour cube.
pub fn swatches(ui: &mut Ui, colours: &[Color32], at: &mut usize) {
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(vec2(w, 34.0), Sense::hover());
    let cell = w / colours.len().max(1) as f32;
    for (i, c) in colours.iter().enumerate() {
        let at_x = r.left() + cell * (i as f32 + 0.5);
        let centre = pos2(at_x, r.center().y);
        let hit = Rect::from_center_size(centre, Vec2::splat(30.0));
        let resp = ui.interact(hit, ui.scope_id().with(("swatch", i)), Sense::click());
        if resp.clicked() {
            *at = i;
        }
        let p = ui.painter();
        p.circle_filled(centre, 10.0, *c);
        if *at == i {
            p.circle_stroke(centre, 14.0, Stroke::new(2.0, Color32::WHITE));
        }
    }
    ui.add_space(4.0);
}

/// A labelled menu button; the list opens over everything, with a tick by the chosen row.
pub fn dropdown(ui: &mut Ui, r: Rect, label: Option<&str>, options: &[&str], at: &mut usize) -> bool {
    let id = ui.scope_id().with(("menu", r.min.x as i32, r.min.y as i32));
    let box_r = match label {
        Some(l) => {
            ui.painter().text(r.left_top() + vec2(0.0, 7.0), Align2::LEFT_CENTER, l, semi(12.5), Color32::WHITE);
            Rect::from_min_max(pos2(r.left(), r.top() + 18.0), r.max)
        }
        None => r,
    };
    let resp = ui.interact(box_r, id, Sense::click());
    let p = ui.painter();
    p.rect_filled(box_r, 4, if resp.hovered() { BUTTON_EDGE } else { BUTTON });
    let name = options.get(*at).copied().unwrap_or("");
    let clip = p.with_clip_rect(box_r.shrink2(vec2(10.0, 0.0)));
    clip.text(pos2(box_r.left() + 12.0, box_r.center().y), Align2::LEFT_CENTER, name, semi(12.5), Color32::WHITE);
    let tick = Rect::from_center_size(pos2(box_r.right() - 16.0, box_r.center().y), Vec2::splat(14.0));
    icons::paint(p, Icon::ArrowDown01, tick, MUTED);
    let open_id = id.with("open");
    let mut open = ui.ctx().data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
    if resp.clicked() {
        open = !open;
    }
    let mut changed = false;
    if open {
        let row = 34.0;
        let h = row * options.len() as f32 + 8.0;
        let w = box_r.width().max(170.0);
        let screen = ui.ctx().content_rect();
        let y = if box_r.top() - h - 6.0 > screen.top() { box_r.top() - h - 6.0 } else { (box_r.bottom() + 6.0).min(screen.bottom() - h) };
        let x = box_r.left().min(screen.right() - w - 8.0).max(screen.left() + 8.0);
        let shown = Area::new(id.with("list")).order(Order::Foreground).fixed_pos(pos2(x, y)).show(ui.ctx(), |ui| {
            let (bg, _) = ui.allocate_exact_size(vec2(w, h), Sense::click());
            let p = ui.painter();
            p.rect_filled(bg, 4, MENU);
            p.rect_stroke(bg, 4, Stroke::new(1.0, BUTTON_EDGE), StrokeKind::Inside);
            for (i, o) in options.iter().enumerate() {
                let rr = Rect::from_min_size(pos2(bg.left() + 4.0, bg.top() + 4.0 + row * i as f32), vec2(w - 8.0, row));
                let hit = ui.interact(rr, id.with(("row", i)), Sense::click());
                if hit.hovered() {
                    ui.painter().rect_filled(rr, 4, BUTTON_EDGE);
                }
                let p = ui.painter();
                if *at == i {
                    icons::paint(p, Icon::Tick02, Rect::from_center_size(pos2(rr.left() + 15.0, rr.center().y), Vec2::splat(15.0)), Color32::WHITE);
                }
                p.text(pos2(rr.left() + 30.0, rr.center().y), Align2::LEFT_CENTER, *o, semi(13.0), Color32::WHITE);
                if hit.clicked() {
                    changed = *at != i;
                    *at = i;
                    open = false;
                }
            }
        });
        if !resp.clicked() && ui.input(|i| i.pointer.any_pressed()) && !shown.response.contains_pointer() {
            open = false;
        }
    }
    ui.ctx().data_mut(|d| d.insert_temp(open_id, open));
    changed
}

/// Sliding tick marks for the master of a wheel, like the jog strip on a panel.
pub fn jog(ui: &mut Ui, v: &mut f32, speed: f32) -> Response {
    const SPACING: f32 = 5.0;
    let w = ui.available_width();
    let old = *v;
    let (r, mut resp) = ui.allocate_exact_size(vec2(w, 30.0), Sense::click_and_drag());
    resp = resp.on_hover_text("Drag to adjust master · Shift for precision · arrows to step · double-click to reset");
    if resp.clicked() || resp.dragged() { resp.request_focus(); }
    if resp.dragged() {
        *v += resp.drag_delta().x * speed * ui.input(|i| if i.modifiers.shift { 0.1 } else { 1.0 });
    }
    if resp.has_focus() {
        ui.input(|i| {
            let step = speed * if i.modifiers.shift { 0.1 } else { 1.0 };
            if i.key_pressed(egui::Key::ArrowRight) { *v += step; }
            if i.key_pressed(egui::Key::ArrowLeft) { *v -= step; }
        });
    }
    if resp.double_clicked() {
        *v = 0.0;
    }
    let p = ui.painter().with_clip_rect(r);
    p.rect_filled(r, 6, BUTTON);
    let phase = (*v / speed.max(f32::EPSILON)).rem_euclid(SPACING);
    let mut x = r.left() + phase;
    while x < r.right() {
        let edge = ((x - r.left()).min(r.right() - x) / 40.0).min(1.0);
        let c = Color32::from_white_alpha((edge * 130.0) as u8);
        p.vline(x, r.top() + 5.0..=r.bottom() - 5.0, Stroke::new(1.0, c));
        x += SPACING;
    }
    p.vline(r.center().x, r.top() + 3.0..=r.bottom() - 3.0, Stroke::new(2.0, ACCENT));
    focus_ring(ui, &resp, r, 6);
    if *v != old { resp.mark_changed(); }
    resp
}

const WHEEL_AXES: [f32; 3] = [103.0, 241.0, 347.0];

/// Where a red, green and blue push sits on a trackball, from -1 to 1 across it.
pub fn ball_of(rgb: [f32; 3]) -> Vec2 {
    // The axes are not evenly spaced, so this solves for the position properly:
    // the least-squares inverse of `rgb_of`.
    let (mut xx, mut xy, mut yy, mut bx, mut by) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for i in 0..3 {
        let a = WHEEL_AXES[i].to_radians();
        let (x, y) = (a.cos(), -a.sin());
        xx += x * x;
        xy += x * y;
        yy += y * y;
        bx += x * rgb[i];
        by += y * rgb[i];
    }
    let det = xx * yy - xy * xy;
    vec2((yy * bx - xy * by) / det, (xx * by - xy * bx) / det)
}

pub fn rgb_of(ball: Vec2) -> [f32; 3] {
    std::array::from_fn(|i| {
        let a = WHEEL_AXES[i].to_radians();
        ball.x * a.cos() - ball.y * a.sin()
    })
}

/// The hue at an angle on the wheel, measured clockwise from the right.
pub fn hue_at(angle: f32) -> f32 {
    (360.0 - WHEEL_AXES[0] - angle).rem_euclid(360.0)
}

/// The angle on the wheel for a hue; the reverse of `hue_at`.
pub fn angle_of(hue: f32) -> f32 {
    (360.0 - WHEEL_AXES[0] - hue).rem_euclid(360.0)
}

/// The rainbow rim, the dark bowl and its cross hairs.
pub fn wheel_face(p: &egui::Painter, c: Pos2, radius: f32) {
    const STEPS: usize = 96;
    let mut m = Mesh::default();
    let inner = radius - 5.0;
    for i in 0..=STEPS {
        let a = i as f32 / STEPS as f32 * std::f32::consts::TAU;
        let (s, co) = a.sin_cos();
        // The rim follows the vectorscope: red toward the upper left.
        let [r, g, b] = color::from_hsv([hue_at(a.to_degrees()), 0.85, 1.0]);
        let col = Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8);
        m.vertices.push(Vertex { pos: c + vec2(co, s) * radius, uv: WHITE_UV, color: col });
        m.vertices.push(Vertex { pos: c + vec2(co, s) * inner, uv: WHITE_UV, color: col });
        if i > 0 {
            let b = (i as u32 - 1) * 2;
            m.add_triangle(b, b + 1, b + 2);
            m.add_triangle(b + 1, b + 2, b + 3);
        }
    }
    p.add(Shape::mesh(m));
    p.circle_filled(c, inner, Color32::from_rgb(0x12, 0x12, 0x14));
    let hair = Stroke::new(1.0, Color32::from_white_alpha(22));
    p.line_segment([c - vec2(inner, 0.0), c + vec2(inner, 0.0)], hair);
    p.line_segment([c - vec2(0.0, inner), c + vec2(0.0, inner)], hair);
    for k in [0.33, 0.66] {
        p.circle_stroke(c, inner * k, hair);
    }
}

/// A trackball. `ball` runs from -1 to 1 across the disc.
pub fn trackball(ui: &mut Ui, c: Pos2, radius: f32, ball: &mut Vec2, id: Id) -> Response {
    let r = Rect::from_center_size(c, Vec2::splat(radius * 2.0));
    let mut resp = ui.interact(r, id, Sense::click_and_drag());
    wheel_face(ui.painter(), c, radius);
    let reach = radius - 11.0;
    if let Some(p) = resp.interact_pointer_pos()
        && (resp.dragged() || resp.drag_started())
    {
        let mut d = (p - c) / reach;
        if d.length() > 1.0 {
            d = d.normalized();
        }
        *ball = d;
        resp.mark_changed();
    }
    if resp.double_clicked() {
        *ball = Vec2::ZERO;
        resp.mark_changed();
    }
    let at = c + *ball * reach;
    let p = ui.painter();
    p.circle_filled(at, 5.5, Color32::WHITE);
    p.circle_stroke(at, 7.5, Stroke::new(1.0, Color32::from_black_alpha(120)));
    resp
}

pub fn hue_colours() -> Vec<Color32> {
    (0..=12)
        .map(|i| {
            let [r, g, b] = color::from_hsv([i as f32 * 30.0, 0.9, 1.0]);
            Color32::from_rgb((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
        })
        .collect()
}

/// What sits behind a curve: what its horizontal axis means.
#[derive(Clone, Copy, PartialEq)]
pub enum Backdrop {
    Plain,
    Hue,
    Light,
    Colour,
}

pub struct CurveLook<'a> {
    pub flat: bool,
    pub wraps: bool,
    pub backdrop: Backdrop,
    /// The picture's own distribution along the axis.
    pub spread: Option<&'a [f32]>,
    pub ink: Color32,
}

fn paint_backdrop(p: &egui::Painter, r: Rect, b: Backdrop) {
    const SHADE: [(f32, u8); 3] = [(0.0, 175), (0.5, 105), (1.0, 175)];
    match b {
        Backdrop::Plain => {
            p.rect_filled(r, 4, Color32::from_rgb(0x10, 0x10, 0x12));
            return;
        }
        Backdrop::Hue => icons::colour_band(p, r, &hue_colours()),
        Backdrop::Light => icons::colour_band(p, r, &[Color32::BLACK, Color32::WHITE]),
        Backdrop::Colour => icons::colour_band(p, r, &[Color32::from_gray(120), Color32::from_rgb(30, 140, 255)]),
    }
    let mut m = Mesh::default();
    for (i, (t, alpha)) in SHADE.iter().enumerate() {
        let y = r.top() + r.height() * t;
        let c = Color32::from_black_alpha(*alpha);
        m.vertices.push(Vertex { pos: pos2(r.left(), y), uv: WHITE_UV, color: c });
        m.vertices.push(Vertex { pos: pos2(r.right(), y), uv: WHITE_UV, color: c });
        if i > 0 {
            let b = (i as u32 - 1) * 2;
            m.add_triangle(b, b + 1, b + 2);
            m.add_triangle(b + 1, b + 2, b + 3);
        }
    }
    p.add(Shape::mesh(m));
}

/// Sorts the points, keeping `chosen` on the same point.
fn settle(pts: &mut Points, chosen: &mut Option<usize>) {
    let held = chosen.and_then(|i| pts.get(i).copied());
    pts.sort_by(|a, b| a[0].total_cmp(&b[0]));
    *chosen = held.and_then(|q| pts.iter().position(|o| *o == q));
}

/// The curve editor. Press on empty space to add a point, drag to move one,
/// double click to remove it. Flat curves rest at the middle line.
pub fn curve_box(ui: &mut Ui, h: f32, pts: &mut Points, look: &CurveLook, chosen: &mut Option<usize>) -> bool {
    const REACH: f32 = 15.0;
    const GRID: i32 = 6;
    let flat = look.flat;
    let w = ui.available_width();
    let (r, resp) = ui.allocate_exact_size(vec2(w, h), Sense::click_and_drag());
    let to_screen = |q: [f32; 2]| pos2(r.left() + q[0] * r.width(), r.bottom() - q[1] * r.height());
    let to_curve = |p: Pos2| [((p.x - r.left()) / r.width()).clamp(0.0, 1.0), ((r.bottom() - p.y) / r.height()).clamp(0.0, 1.0)];
    let held_id = resp.id.with("held");
    let mut held: Option<usize> = ui.ctx().data(|d| d.get_temp(held_id)).unwrap_or(None);
    let mut changed = false;
    let nearest = |pts: &Points, at: Pos2| {
        pts.iter()
            .enumerate()
            .map(|(i, q)| (i, to_screen(*q).distance(at)))
            .filter(|(_, d)| *d < REACH)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    };
    if resp.drag_started()
        && let Some(at) = ui.input(|i| i.pointer.press_origin())
    {
        held = nearest(pts, at);
        if held.is_none() && r.contains(at) {
            pts.push(to_curve(at));
            held = Some(pts.len() - 1);
            changed = true;
        }
        *chosen = held;
    }
    if resp.dragged()
        && let (Some(i), Some(at)) = (held, resp.interact_pointer_pos())
        && i < pts.len()
    {
        let mut q = to_curve(at);
        // The ends of a tone curve stay at the ends.
        if !flat && (i == 0 || pts[i][0] <= 0.0) {
            q[0] = 0.0;
        } else if !flat && pts[i][0] >= 1.0 {
            q[0] = 1.0;
        }
        pts[i] = q;
        changed = true;
    }
    if resp.drag_stopped() {
        held = None;
        settle(pts, chosen);
    }
    if resp.clicked()
        && let Some(at) = resp.interact_pointer_pos()
    {
        match nearest(pts, at) {
            Some(i) => *chosen = Some(i),
            None if r.contains(at) => {
                pts.push(to_curve(at));
                *chosen = Some(pts.len() - 1);
                settle(pts, chosen);
                changed = true;
            }
            None => {}
        }
    }
    if resp.double_clicked()
        && let Some(at) = resp.interact_pointer_pos()
        && let Some(i) = nearest(pts, at)
        && (flat || (pts[i][0] > 0.0 && pts[i][0] < 1.0))
    {
        pts.remove(i);
        *chosen = None;
        changed = true;
    }
    ui.ctx().data_mut(|d| d.insert_temp(held_id, held));

    let p = ui.painter().with_clip_rect(r);
    paint_backdrop(&p, r, look.backdrop);
    let grid = Stroke::new(1.0, Color32::from_white_alpha(20));
    for k in 1..GRID {
        p.vline(r.left() + r.width() * k as f32 / GRID as f32, r.y_range(), grid);
    }
    for k in 1..4 {
        p.hline(r.x_range(), r.top() + r.height() * k as f32 / 4.0, grid);
    }
    if let Some(s) = look.spread {
        let last = (s.len() - 1).max(1) as f32;
        let hill: Vec<Pos2> = s.iter().enumerate().map(|(i, v)| pos2(r.left() + r.width() * i as f32 / last, r.bottom() - v * r.height() * 0.92)).collect();
        p.add(Shape::line(hill, Stroke::new(1.3, Color32::from_white_alpha(120))));
    }
    if flat {
        for q in pts.iter() {
            let x = to_screen(*q).x;
            p.vline(x, r.y_range(), Stroke::new(1.0, Color32::from_white_alpha(70)));
        }
        p.hline(r.x_range(), r.center().y, Stroke::new(1.0, Color32::from_white_alpha(44)));
    } else {
        p.line_segment([r.left_bottom(), r.right_top()], Stroke::new(1.0, Color32::from_white_alpha(30)));
    }
    let table = if flat { Table::flat(pts, look.wraps) } else { Table::tone(pts) };
    let line: Vec<Pos2> = (0..=200)
        .map(|i| {
            let x = i as f32 / 200.0;
            to_screen([x, table.get(x)])
        })
        .collect();
    p.add(Shape::line(line.clone(), Stroke::new(3.0, Color32::from_black_alpha(90))));
    p.add(Shape::line(line, Stroke::new(1.8, look.ink)));
    for (i, q) in pts.iter().enumerate() {
        let c = to_screen(*q);
        if *chosen == Some(i) {
            p.circle_filled(c, 6.5, Color32::from_rgb(0x20, 0x20, 0x24));
            p.circle_stroke(c, 6.5, Stroke::new(1.8, look.ink));
        } else {
            p.circle_filled(c, 5.0, look.ink);
            p.circle_stroke(c, 6.0, Stroke::new(1.0, Color32::from_black_alpha(140)));
        }
    }
    ui.add_space(8.0);
    changed
}

/// A small icon button that opens a short list; gives back the row picked.
pub fn menu_button(ui: &mut Ui, r: Rect, icon: Icon, tip: &str, on: bool, options: &[&str]) -> Option<usize> {
    let id = ui.scope_id().with(("menu-button", tip));
    let resp = icon_button(ui, r, icon, tip, on);
    let open_id = id.with("open");
    let mut open = ui.ctx().data(|d| d.get_temp::<bool>(open_id)).unwrap_or(false);
    if resp.clicked() {
        open = !open;
    }
    let mut picked = None;
    if open {
        let row = 34.0;
        let (w, h) = (190.0, row * options.len() as f32 + 8.0);
        let screen = ui.ctx().content_rect();
        let at = pos2((r.right() - w).max(screen.left() + 8.0), (r.bottom() + 4.0).min(screen.bottom() - h - 8.0));
        let shown = Area::new(id.with("list")).order(Order::Foreground).fixed_pos(at).show(ui.ctx(), |ui| {
            let (bg, _) = ui.allocate_exact_size(vec2(w, h), Sense::click());
            let p = ui.painter();
            p.rect_filled(bg, 4, MENU);
            p.rect_stroke(bg, 4, Stroke::new(1.0, BUTTON_EDGE), StrokeKind::Inside);
            for (i, o) in options.iter().enumerate() {
                let rr = Rect::from_min_size(pos2(bg.left() + 4.0, bg.top() + 4.0 + row * i as f32), vec2(w - 8.0, row));
                let hit = ui.interact(rr, id.with(("row", i)), Sense::click());
                if hit.hovered() {
                    ui.painter().rect_filled(rr, 4, BUTTON_EDGE);
                }
                ui.painter().text(pos2(rr.left() + 12.0, rr.center().y), Align2::LEFT_CENTER, *o, semi(13.0), Color32::WHITE);
                if hit.clicked() {
                    picked = Some(i);
                    open = false;
                }
            }
        });
        if !resp.clicked() && ui.input(|i| i.pointer.any_pressed()) && !shown.response.contains_pointer() {
            open = false;
        }
    }
    ui.ctx().data_mut(|d| d.insert_temp(open_id, open));
    picked
}

/// A dot shaded diagonally from one colour to another, for the Tetra corners.
pub fn gradient_dot(p: &egui::Painter, c: Pos2, radius: f32, from: Color32, to: Color32) {
    const STEPS: u32 = 40;
    let shade = |t: f32| {
        let t = t.clamp(0.0, 1.0);
        let mix = |a: u8, b: u8| (a as f32 + (b as f32 - a as f32) * t) as u8;
        Color32::from_rgb(mix(from.r(), to.r()), mix(from.g(), to.g()), mix(from.b(), to.b()))
    };
    let mut m = Mesh::default();
    m.vertices.push(Vertex { pos: c, uv: WHITE_UV, color: shade(0.5) });
    for i in 0..=STEPS {
        let a = i as f32 / STEPS as f32 * std::f32::consts::TAU;
        let (s, co) = a.sin_cos();
        m.vertices.push(Vertex { pos: c + vec2(co, s) * radius, uv: WHITE_UV, color: shade(0.5 + (co + s) * 0.36) });
        if i > 0 {
            m.add_triangle(0, i, i + 1);
        }
    }
    p.add(Shape::mesh(m));
}

/// Darkens everything behind a sheet and swallows clicks meant for it.
pub fn dim_screen(ctx: &Context, layer: &str) {
    let screen = ctx.content_rect();
    Area::new(Id::unique(layer)).order(Order::Middle).fixed_pos(screen.min).show(ctx, |ui| {
        let (r, _) = ui.allocate_exact_size(screen.size(), Sense::click());
        ui.painter().rect_filled(r, 0, Color32::from_black_alpha(170));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_trackball_position_survives_the_trip_through_colour() {
        let v = vec2(0.3, -0.4);
        let back = ball_of(rgb_of(v));
        assert!((back - v).length() < 1e-4, "{back:?}");
    }

    #[test]
    fn the_rim_is_red_where_the_ball_raises_red() {
        let a = 360.0 - WHEEL_AXES[0];
        assert!(hue_at(a).abs() < 1e-3);
        assert!((angle_of(240.0) - hue_at(240.0)).abs() < 1e-3);
    }

    #[test]
    fn the_red_side_of_the_wheel_raises_red() {
        let a = WHEEL_AXES[0].to_radians();
        let rgb = rgb_of(vec2(a.cos(), -a.sin()));
        assert!(rgb[0] > 0.9 && rgb[1] < 0.0 && rgb[2] < 0.0, "{rgb:?}");
    }
}

#[cfg(test)]
mod slider_tests {
    use super::*;

    fn draw(ctx: &Context, value: &mut f32, input: egui::RawInput, format: Fmt) -> Response {
        let mut response = None;
        let _ = ctx.run_ui(input, |ui| {
            ui.set_width(300.0);
            response = Some(slider(ui, "Amount", value, 0.0, 1.0, 0.5, format));
        });
        response.unwrap()
    }

    #[test]
    fn displaying_a_rounded_value_does_not_change_the_project() {
        let ctx = Context::default();
        let mut value = 0.375;
        let response = draw(&ctx, &mut value, Default::default(), Fmt::Int);
        assert_eq!(value, 0.375);
        assert!(!response.changed());
    }

    #[test]
    fn focused_slider_supports_steps_and_endpoints() {
        let ctx = Context::default();
        let mut value = 0.5;
        let response = draw(&ctx, &mut value, Default::default(), Fmt::Plain);
        ctx.memory_mut(|m| m.request_focus(response.id));
        for (key, expected) in [(egui::Key::ArrowRight, 0.505), (egui::Key::Home, 0.0), (egui::Key::End, 1.0)] {
            let input = egui::RawInput { events: vec![egui::Event::Key {
                key, physical_key: None, pressed: true, repeat: false, modifiers: Default::default(),
            }], ..Default::default() };
            assert!(draw(&ctx, &mut value, input, Fmt::Plain).changed());
            assert!((value - expected).abs() < 1e-5);
        }
    }
}
