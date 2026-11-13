//! Moving things: animations, keyframes and fonts for text and stickers, how fast a clip
//! plays, and the plugin packs that add to all of it.

use super::App;
use super::kit::*;
use crate::icons::{self, Icon};
use crate::overlay;
use crate::plugins;
use crate::shared::Lru;
use crate::textanim::{self, Anim, Cell, Ease, Keyed, Motion, Order, Phase, Repeat, SPARE_GLYPHS, Sway, Unit};
use crate::timeline::{Picked, SPEEDS};
use eframe::egui::{self, Align2, Area, Color32, ColorImage, FontId, Order as Layer, Rect, Sense, Stroke, StrokeKind, TextureHandle, TextureOptions, Ui, Vec2, pos2, vec2};
use eframe::epaint::TextShape;

const TILE_H: f32 = 62.0;
const TILE_GAP: f32 = 6.0;
const TILE_COLUMNS: usize = 3;
const PREVIEW_WORD: &str = "Text";
const PREVIEW_SIZE: f32 = 19.0;
const PREVIEW_FPS: f32 = 30.0;
/// How long a finished preview rests before it plays again.
const PREVIEW_REST: f32 = 0.7;
/// How far through its animation a preview sits when nobody is pointing at it.
const PREVIEW_STILL: f32 = 0.4;
const FONT_ROW_H: f32 = 32.0;
const FONT_LIST_H: f32 = 250.0;
const FONT_SAMPLE_PX: f32 = 20.0;
/// Tallest a font's sample is shown, in points; its strip has room above and below.
const FONT_SAMPLE_H: f32 = 30.0;
const FONT_SAMPLES_KEPT: usize = 160;
/// Drawing a font's sample takes a few milliseconds, so only this many a frame.
const FONT_SAMPLES_A_FRAME: usize = 3;
const WEIGHTS: [(&str, u16); 5] = [("Light", 300), ("Regular", 400), ("Semibold", 600), ("Bold", 700), ("Black", 900)];
const DIAMOND: f32 = 6.0;
const SPEED_W: f32 = 300.0;

pub struct MotionState {
    /// In, Out or Loop.
    tab: usize,
    fonts: Option<Vec<String>>,
    font_open: bool,
    font_search: String,
    /// Each font's name drawn in itself, by its place in \`fonts\`.
    samples: Lru<usize, TextureHandle>,
    pub speed_open: bool,
}

impl Default for MotionState {
    fn default() -> Self {
        Self { tab: 0, fonts: None, font_open: false, font_search: String::new(), samples: Lru::new(FONT_SAMPLES_KEPT), speed_open: false }
    }
}

/// One choice in the animation grid.
#[derive(Clone)]
enum Choice {
    Built(Anim),
    Plugin(usize),
    Loop(Sway),
}

fn title(ui: &mut Ui, text: &str) {
    ui.add_space(6.0);
    ui.label(egui::RichText::new(text).font(bold(14.0)).color(Color32::WHITE));
    ui.add_space(2.0);
}

/// The preview word cut into letters, laid out the way the real text is.
fn preview_cells(ui: &Ui) -> Vec<(Cell, char)> {
    let font = FontId::proportional(PREVIEW_SIZE);
    let widths: Vec<f32> = PREVIEW_WORD.chars().map(|c| ui.painter().layout_no_wrap(c.to_string(), font.clone(), Color32::WHITE).size().x).collect();
    let total: f32 = widths.iter().sum();
    let mut x = -total / 2.0;
    PREVIEW_WORD
        .chars()
        .zip(widths)
        .enumerate()
        .map(|(i, (c, w))| {
            let cell = Cell { x: x + w / 2.0, y: 0.0, w, h: PREVIEW_SIZE * 1.2, groups: [0, 0, 0, i] };
            x += w;
            (cell, c)
        })
        .collect()
}

/// The run of an In or Out across the preview word, to its last letter.
fn run(p: &Phase) -> f32 {
    if p.kind == Anim::None { 0.0 } else { p.seconds + p.delay * (PREVIEW_WORD.len() as f32 - 1.0) }
}

/// A moment that shows what the animation does, for a tile nobody is pointing at.
fn preview_still(m: &Motion, seconds: f32) -> f32 {
    if m.repeat.kind != Sway::None {
        0.3
    } else if m.leave.kind != Anim::None {
        seconds - run(&m.leave) * (1.0 - PREVIEW_STILL)
    } else {
        run(&m.enter) * PREVIEW_STILL
    }
}

/// Seconds a preview takes before it rests: the run of the slowest letter.
fn preview_seconds(m: &Motion) -> f32 {
    if m.repeat.kind != Sway::None { 2.0 } else { (run(&m.enter) + run(&m.leave)).max(0.3) + 0.5 }
}

/// Draws the preview word moving the way \`m\` says, inside a tile.
fn paint_preview(ui: &Ui, r: Rect, m: &Motion, live: bool) {
    let cells = preview_cells(ui);
    let only: Vec<Cell> = cells.iter().map(|c| c.0).collect();
    let seconds = preview_seconds(m);
    let lap = seconds + PREVIEW_REST;
    let t = if live { (ui.input(|i| i.time) as f32) % lap } else { preview_still(m, seconds) };
    let len = (seconds * PREVIEW_FPS) as u32 + 1;
    let frame = ((t * PREVIEW_FPS) as u32).min(len - 1);
    let placed = textanim::compose(&only, m, 0.5, 0.42, frame, len, PREVIEW_FPS, PREVIEW_SIZE, (r.width(), r.height()), SPARE_GLYPHS.len());
    let p = ui.painter().with_clip_rect(r.shrink(2.0));
    for q in placed {
        let (cell, letter) = cells[q.cell];
        let shown = q.spare.and_then(|s| SPARE_GLYPHS.chars().nth(s)).unwrap_or(letter);
        let scale = ((q.scale_x.abs() + q.scale_y.abs()) / 2.0).clamp(0.05, 6.0);
        let galley = p.layout_no_wrap(shown.to_string(), FontId::proportional(PREVIEW_SIZE * scale), Color32::WHITE);
        let centre = r.min + vec2(q.x, q.y + q.sunk * cell.h);
        let soft = 1.0 / (1.0 + q.blur * 0.2);
        let shape = TextShape::new(centre - galley.size() / 2.0, galley, Color32::WHITE)
            .with_angle_and_anchor(q.rotate.to_radians(), Align2::CENTER_CENTER)
            .with_opacity_factor((q.opacity * soft).clamp(0.0, 1.0));
        // Reveal hides whatever has sunk below the letter's own box.
        let room = Rect::from_center_size(r.min + vec2(q.x, q.y), vec2(cell.w * 2.0, cell.h));
        if q.sunk > 0.0 { p.with_clip_rect(room.intersect(r)).add(shape) } else { p.add(shape) };
    }
}

/// A grid of choices, each showing its own animation. Gives the one clicked.
fn tiles(ui: &mut Ui, list: &[(String, Motion, bool)]) -> Option<usize> {
    let w = ui.available_width();
    let cell = (w - TILE_GAP * (TILE_COLUMNS - 1) as f32) / TILE_COLUMNS as f32;
    let rows = list.len().div_ceil(TILE_COLUMNS);
    let (area, _) = ui.allocate_exact_size(vec2(w, rows as f32 * (TILE_H + TILE_GAP)), Sense::hover());
    let mut chosen = None;
    for (i, (name, m, on)) in list.iter().enumerate() {
        let r = Rect::from_min_size(
            pos2(area.left() + (cell + TILE_GAP) * (i % TILE_COLUMNS) as f32, area.top() + (TILE_H + TILE_GAP) * (i / TILE_COLUMNS) as f32),
            vec2(cell, TILE_H),
        );
        let resp = ui.interact(r, ui.scope_id().with(("tile", i, name.as_str())), Sense::click());
        let p = ui.painter();
        p.rect_filled(r, 4, if resp.hovered() { BUTTON_EDGE } else { BUTTON });
        if *on {
            p.rect_stroke(r, 4, Stroke::new(2.0, ACCENT), StrokeKind::Inside);
        }
        let stage = Rect::from_min_max(r.min, pos2(r.right(), r.bottom() - 18.0));
        paint_preview(ui, stage, m, resp.hovered());
        let label = ui.painter().with_clip_rect(r.shrink(4.0));
        label.text(pos2(r.center().x, r.bottom() - 11.0), Align2::CENTER_CENTER, name, semi(11.0), if *on { Color32::WHITE } else { MUTED });
        if resp.hovered() {
            ui.ctx().request_repaint();
        }
        if resp.clicked() {
            chosen = Some(i);
        }
    }
    ui.add_space(6.0);
    chosen
}

fn unit_row(ui: &mut Ui, unit: &mut Unit) {
    ui.label(egui::RichText::new("Animate by").font(semi(12.5)).color(Color32::WHITE));
    let mut at = Unit::ALL.iter().position(|u| u == unit).unwrap_or(0);
    if segmented(ui, &Unit::NAMES, &mut at) {
        *unit = Unit::ALL[at];
    }
}

/// Settings of an In or Out once one is chosen.
fn phase_settings(ui: &mut Ui, ph: &mut Phase) {
    unit_row(ui, &mut ph.unit);
    ui.label(egui::RichText::new("Order").font(semi(12.5)).color(Color32::WHITE));
    let mut at = Order::ALL.iter().position(|o| *o == ph.order).unwrap_or(0);
    if segmented(ui, &Order::NAMES, &mut at) {
        ph.order = Order::ALL[at];
    }
    slider(ui, "Duration (s)", &mut ph.seconds, 0.05, 3.0, 0.6, Fmt::Plain);
    slider(ui, "Stagger (s)", &mut ph.delay, 0.0, 0.5, 0.0, Fmt::Plain);
    let names: Vec<&str> = Ease::ALL.iter().map(|e| e.name()).collect();
    let mut at = Ease::ALL.iter().position(|e| *e == ph.ease).unwrap_or(0);
    let w = ui.available_width();
    let (r, _) = ui.allocate_exact_size(vec2(w, 52.0), Sense::hover());
    if dropdown(ui, r, Some("Curve"), &names, &mut at) {
        ph.ease = Ease::ALL[at];
    }
    ui.add_space(6.0);
}

impl App {
    fn picked_id(&self) -> Option<u32> {
        match self.picked {
            Some(Picked::Item(id)) => Some(id),
            _ => None,
        }
    }

    /// The picked text or sticker's motion, with the frame the playhead is at inside it.
    fn picked_motion(&mut self) -> Option<(u32, &mut f32, &mut f32, &mut Motion)> {
        let head = self.head;
        let t = self.picked_item()?;
        let local = head.saturating_sub(t.at).min(t.len.saturating_sub(1));
        let (x, y, m) = t.body.placed_mut()?;
        Some((local, x, y, m))
    }

    /// The picked item's In or Out, or nothing.
    fn picked_phase(&mut self, enter: bool) -> Phase {
        self.picked_motion().map_or_else(Phase::default, |(_, _, _, m)| if enter { m.enter } else { m.leave })
    }

    /// In, Out and Loop, each a grid of animations that show themselves.
    pub(super) fn animation_section(&mut self, ui: &mut Ui) {
        title(ui, "Animation");
        let mut tab = self.edit.motion.tab;
        segmented(ui, &["In", "Out", "Loop"], &mut tab);
        self.edit.motion.tab = tab;
        let plugins: Vec<(String, Phase)> = self.plugins.animations.iter().map(|a| (a.name.clone(), a.phase)).collect();
        let phase = self.picked_phase(tab == 0);
        let named = self.plugins.animation_of(&phase).map(|a| format!("{}, from the {} pack.", a.name, a.pack));
        let Some((_, _, _, m)) = self.picked_motion() else { return };
        let mut choices: Vec<Choice> = Vec::new();
        let mut list: Vec<(String, Motion, bool)> = Vec::new();
        if tab == 2 {
            for s in Sway::ALL {
                list.push((s.name().to_string(), Motion { repeat: Repeat::preset(s), ..Motion::default() }, m.repeat.kind == s));
                choices.push(Choice::Loop(s));
            }
        } else {
            let now = if tab == 0 { m.enter } else { m.leave };
            let shown = |ph: Phase| if tab == 0 { Motion { enter: ph, ..Motion::default() } } else { Motion { leave: ph, ..Motion::default() } };
            for a in Anim::ALL {
                list.push((a.name().to_string(), shown(Phase::preset(a)), now.kind == a));
                choices.push(Choice::Built(a));
            }
            for (i, (name, ph)) in plugins.iter().enumerate() {
                list.push((name.clone(), shown(*ph), now.kind == Anim::Custom && now.from == ph.from));
                choices.push(Choice::Plugin(i));
            }
        }
        if let Some(i) = tiles(ui, &list) {
            match choices[i] {
                Choice::Loop(s) => m.repeat = Repeat::preset(s),
                Choice::Built(a) if tab == 0 => m.enter = Phase::preset(a),
                Choice::Built(a) => m.leave = Phase::preset(a),
                Choice::Plugin(n) if tab == 0 => m.enter = plugins[n].1,
                Choice::Plugin(n) => m.leave = plugins[n].1,
            }
        }
        match tab {
            2 if m.repeat.kind != Sway::None => {
                unit_row(ui, &mut m.repeat.unit);
                slider(ui, "Speed (rounds a second)", &mut m.repeat.speed, 0.1, 4.0, 1.0, Fmt::Plain);
                slider(ui, "Amount", &mut m.repeat.amount, 0.0, 1.0, 0.5, Fmt::Percent);
            }
            0 | 1 => {
                let ph = if tab == 0 { &mut m.enter } else { &mut m.leave };
                if ph.kind != Anim::None {
                    if ph.kind == Anim::Custom {
                        note(ui, named.as_deref().unwrap_or("From a plugin that isn't loaded. It still plays."));
                    }
                    phase_settings(ui, ph);
                }
            }
            _ => {}
        }
    }

    /// Place, size, turn and fade. With keyframes, a change sets a keyframe at the playhead.
    pub(super) fn keyframe_section(&mut self, ui: &mut Ui) {
        title(ui, "Position and Keyframes");
        let head = self.head;
        let Some(t) = self.picked_item() else { return };
        let (at, len) = (t.at, t.len);
        let inside = head >= at && head < at + len;
        let Some((local, x, y, m)) = self.picked_motion() else { return };
        let keyed = !m.keys.is_empty();
        let here = m.keys.iter().position(|k| k.frame == local);
        let w = ui.available_width();
        let (row, _) = ui.allocate_exact_size(vec2(w, 34.0), Sense::hover());
        let b = |n: f32| Rect::from_min_size(pos2(row.left() + n * 38.0, row.top() + 1.0), Vec2::splat(32.0));
        let mut go = None;
        if icon_button(ui, b(0.0), Icon::KeyframeLeft, "Previous keyframe ([)", false).clicked() {
            go = m.previous_key(local);
        }
        let (add_icon, add_tip) = if here.is_some() { (Icon::KeyframeRemove, "Remove this keyframe (Ctrl Shift K)") } else { (Icon::KeyframeAdd, "Add a keyframe here (Ctrl K)") };
        if icon_button(ui, b(1.0), add_icon, add_tip, here.is_some()).clicked() && inside {
            match here {
                Some(_) => m.remove_key(local),
                None => {
                    let v = m.at(local, *x, *y);
                    m.set_key(local, v);
                }
            }
        }
        if icon_button(ui, b(2.0), Icon::KeyframeRight, "Next keyframe (])", false).clicked() {
            go = m.next_key(local);
        }
        let count = match m.keys.len() {
            0 => "No keyframes".to_string(),
            1 => "1 keyframe".to_string(),
            n => format!("{n} keyframes"),
        };
        ui.painter().text(pos2(b(3.0).left() + 4.0, row.center().y), Align2::LEFT_CENTER, count, regular(12.5), MUTED);
        if keyed && ui.put(Rect::from_min_size(pos2(row.right() - 64.0, row.top() + 3.0), vec2(64.0, 28.0)), egui::Button::new("Clear")).on_hover_text("Remove every keyframe").clicked() {
            let v = m.at(local, *x, *y);
            m.keys.clear();
            (*x, *y, m.scale, m.rotate, m.opacity) = (v.x, v.y, v.scale, v.rotate, v.opacity);
        }
        if !inside {
            note(ui, "Move the playhead over this item to set keyframes.");
        } else if !keyed {
            note(ui, "Add a keyframe, move the playhead, then change anything below. A keyframe goes there by itself.");
        }
        let before: Keyed = m.at(local, *x, *y);
        let mut v = before;
        slider(ui, "Across", &mut v.x, -0.25, 1.25, 0.5, Fmt::Percent);
        slider(ui, "Down", &mut v.y, -0.25, 1.25, 0.5, Fmt::Percent);
        slider(ui, "Scale", &mut v.scale, 0.1, 4.0, 1.0, Fmt::Percent);
        slider(ui, "Rotation", &mut v.rotate, -180.0, 180.0, 0.0, Fmt::Degrees);
        slider(ui, "Opacity", &mut v.opacity, 0.0, 1.0, 1.0, Fmt::Percent);
        if v != before {
            if keyed && inside {
                m.set_key(local, v);
            } else if !keyed {
                (*x, *y, m.scale, m.rotate, m.opacity) = (v.x, v.y, v.scale, v.rotate, v.opacity);
            }
        }
        // The curve out of the keyframe at or before the playhead.
        if let Some(i) = m.keys.iter().rposition(|k| k.frame <= local).filter(|i| i + 1 < m.keys.len()) {
            let names: Vec<&str> = Ease::ALL.iter().map(|e| e.name()).collect();
            let mut e = Ease::ALL.iter().position(|e| *e == m.keys[i].ease).unwrap_or(0);
            let (r, _) = ui.allocate_exact_size(vec2(w, 52.0), Sense::hover());
            if dropdown(ui, r, Some("Curve to the next keyframe"), &names, &mut e) {
                m.keys[i].ease = Ease::ALL[e];
            }
        }
        if let Some(f) = go {
            self.seek_head(at + f);
        }
    }

    /// The font list, each name drawn in its own font, then weight, slant and spacing.
    pub(super) fn font_section(&mut self, ui: &mut Ui) {
        title(ui, "Font");
        let fonts = self.edit.motion.fonts.get_or_insert_with(overlay::font_families).clone();
        let Some((_, _, _, m)) = self.picked_motion() else { return };
        let current = if m.font.is_empty() { overlay::default_font() } else { m.font.clone() };
        let w = ui.available_width();
        let (r, resp) = ui.allocate_exact_size(vec2(w, 38.0), Sense::click());
        let open = self.edit.motion.font_open;
        let p = ui.painter();
        p.rect_filled(r, 4, if resp.hovered() || open { BUTTON_EDGE } else { BUTTON });
        icons::paint(p, Icon::TextFont, Rect::from_center_size(pos2(r.left() + 18.0, r.center().y), Vec2::splat(17.0)), MUTED);
        let found = fonts.iter().position(|f| *f == current);
        if !self.font_sample(ui, found, pos2(r.left() + 36.0, r.center().y), r.width() - 70.0, true) {
            ui.painter().text(pos2(r.left() + 36.0, r.center().y), Align2::LEFT_CENTER, &current, semi(13.0), Color32::WHITE);
        }
        icons::paint(ui.painter(), if open { Icon::ArrowUp01 } else { Icon::ArrowDown01 }, Rect::from_center_size(pos2(r.right() - 16.0, r.center().y), Vec2::splat(14.0)), MUTED);
        if resp.clicked() {
            self.edit.motion.font_open = !open;
        }
        let mut chosen: Option<String> = None;
        if self.edit.motion.font_open {
            ui.add(egui::TextEdit::singleline(&mut self.edit.motion.font_search).hint_text("Search fonts").desired_width(f32::INFINITY));
            let needle = self.edit.motion.font_search.to_lowercase();
            let shown: Vec<usize> = (0..fonts.len()).filter(|i| needle.is_empty() || fonts[*i].to_lowercase().contains(&needle)).collect();
            let mut drawn = 0;
            egui::ScrollArea::vertical().id_salt("fonts").max_height(FONT_LIST_H).auto_shrink([false, true]).show_rows(ui, FONT_ROW_H, shown.len(), |ui, range| {
                for n in range {
                    let i = shown[n];
                    let (row, hit) = ui.allocate_exact_size(vec2(ui.available_width(), FONT_ROW_H), Sense::click());
                    if hit.hovered() || fonts[i] == current {
                        if fonts[i] == current {
                            chosen_row(ui.painter(), row, 8);
                        } else {
                            ui.painter().rect_filled(row, 4, BUTTON_EDGE);
                        }
                    }
                    let can_draw = drawn < FONT_SAMPLES_A_FRAME || self.edit.motion.samples.contains(&i);
                    if !(can_draw && self.font_sample(ui, Some(i), pos2(row.left() + 10.0, row.center().y), row.width() - 20.0, false)) {
                        ui.painter().text(pos2(row.left() + 10.0, row.center().y), Align2::LEFT_CENTER, &fonts[i], regular(13.0), Color32::WHITE);
                        ui.ctx().request_repaint();
                    }
                    drawn += 1;
                    if hit.clicked() {
                        chosen = Some(fonts[i].clone());
                    }
                }
            });
            if shown.is_empty() {
                note(ui, "No font has that in its name.");
            }
        }
        let Some((_, _, _, m)) = self.picked_motion() else { return };
        if let Some(f) = chosen {
            m.font = if f == overlay::default_font() { String::new() } else { f };
            self.edit.motion.font_open = false;
        }
        let Some((_, _, _, m)) = self.picked_motion() else { return };
        ui.add_space(4.0);
        let mut chips = Chips::new(ui, 30.0);
        for (name, weight) in WEIGHTS {
            if chips.add(name, m.weight == weight) {
                m.weight = weight;
            }
        }
        if chips.add("Italic", m.italic) {
            m.italic = !m.italic;
        }
        chips.end();
        slider(ui, "Letter Spacing", &mut m.spacing, -0.1, 0.6, 0.0, Fmt::Percent);
        slider(ui, "Outline", &mut m.outline, 0.0, 0.15, 0.0, Fmt::Percent);
        let (r, _) = ui.allocate_exact_size(vec2(w, 32.0), Sense::hover());
        if action(ui, r, "Add a Font File", false, 12.5).on_hover_text("TrueType or OpenType. It is kept with the app.").clicked() {
            self.add_font_file();
        }
    }

    fn add_font_file(&mut self) {
        let Some(path) = rfd::FileDialog::new().add_filter("Fonts", &["ttf", "otf", "ttc"]).pick_file() else { return };
        match overlay::add_font(&path) {
            Ok(name) => {
                self.edit.motion.fonts = None;
                self.edit.motion.samples = Lru::new(FONT_SAMPLES_KEPT);
                if let Some((_, _, _, m)) = self.picked_motion() {
                    m.font = name.clone();
                }
                self.say(format!("Added {name}"));
            }
            Err(e) => self.say(e),
        }
    }

    /// Draws a font's name in that font, left-aligned at \`at\`. False when it can't yet.
    fn font_sample(&mut self, ui: &Ui, index: Option<usize>, at: egui::Pos2, room: f32, bright: bool) -> bool {
        let Some(i) = index else { return false };
        let tex = match self.edit.motion.samples.get(&i) {
            Some(t) => t,
            None => {
                let Some(name) = self.edit.motion.fonts.as_ref().and_then(|f| f.get(i)).cloned() else { return false };
                let Some(img) = font_image(&name) else { return false };
                let t = ui.ctx().load_texture(format!("font-{i}"), img, TextureOptions::LINEAR);
                self.edit.motion.samples.put(i, t.clone());
                t
            }
        };
        let size = tex.size_vec2() / ui.ctx().pixels_per_point();
        let fit = (room / size.x).min(FONT_SAMPLE_H / size.y).min(1.0);
        let r = Rect::from_min_size(pos2(at.x, at.y - size.y * fit / 2.0), size * fit);
        let tint = if bright { Color32::WHITE } else { Color32::from_gray(225) };
        ui.painter().image(tex.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), tint);
        true
    }

    /// Changes the clip's speed through the one-click list, for the keyboard.
    pub(super) fn nudge_speed(&mut self, faster: bool) {
        let Some(id) = self.speed_target() else { return };
        let Some(now) = self.project.clips.iter().find(|c| c.id == id).map(|c| c.speed) else { return };
        let next = if faster { SPEEDS.iter().copied().find(|s| *s > now + 1e-6) } else { SPEEDS.iter().rev().copied().find(|s| *s < now - 1e-6) };
        if let Some(s) = next {
            self.project.set_speed(id, s);
            self.needs_activate = true;
            self.say(format!("Speed {}", speed_label(s)));
        }
    }

    fn speed_target(&self) -> Option<u32> {
        match self.picked {
            Some(Picked::Clip(id)) => Some(id),
            _ => self.active,
        }
    }

    /// The speed panel, opened from the timeline's toolbar, standing above it.
    pub(super) fn speed_popup(&mut self, ui: &Ui, anchor: Rect) {
        if !self.edit.motion.speed_open {
            return;
        }
        let Some(id) = self.speed_target() else {
            self.edit.motion.speed_open = false;
            return;
        };
        let Some(now) = self.project.clips.iter().find(|c| c.id == id).map(|c| c.speed) else { return };
        // With a ramp, no single speed is the clip's.
        let ramped = self.project.clips.iter().any(|c| c.id == id && c.ramp.len() >= 2);
        let screen = ui.ctx().content_rect();
        let x = (anchor.right() - SPEED_W).clamp(screen.left() + 8.0, screen.right() - SPEED_W - 8.0);
        let mut set = None;
        let mut close = false;
        let shown = Area::new(ui.scope_id().with("speed-popup")).order(Layer::Foreground).pivot(Align2::LEFT_BOTTOM).fixed_pos(pos2(x, anchor.top() - 8.0)).show(ui.ctx(), |ui| {
            egui::Frame::new().fill(MENU).stroke(Stroke::new(1.0, BUTTON_EDGE)).corner_radius(4).inner_margin(12.0).show(ui, |ui| {
                ui.set_width(SPEED_W - 24.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Clip Speed").font(bold(14.0)).color(Color32::WHITE));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new(speed_label(now)).font(semi(13.0)).color(ACCENT));
                    });
                });
                ui.add_space(6.0);
                let mut chips = Chips::new(ui, 28.0);
                for s in SPEEDS {
                    if chips.add(&speed_label(s), !ramped && (s - now).abs() < 1e-6) {
                        set = Some(s);
                    }
                }
                chips.end();
                let mut v = now as f32;
                if slider(ui, "Custom", &mut v, 0.1, 8.0, 1.0, Fmt::Plain).changed() {
                    set = Some((v as f64 * 20.0).round() / 20.0);
                }
                note(ui, "Faster plays more of the clip in less time, and shortens it. Sound keeps its pitch.");
                self.ramp_editor(ui, id);
                let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
                if action(ui, r, "Done", true, 12.5).clicked() {
                    close = true;
                }
            });
        });
        if let Some(s) = set {
            // A steady speed replaces any ramp.
            if self.project.clips.iter().any(|c| c.id == id && c.ramp.len() >= 2) {
                self.project.set_ramp(id, Vec::new());
            }
            self.project.set_speed(id, s);
            self.needs_activate = true;
        }
        let outside = ui.input(|i| i.pointer.any_pressed()) && !shown.response.contains_pointer() && !anchor.contains(ui.input(|i| i.pointer.interact_pos()).unwrap_or_default());
        if close || outside || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.edit.motion.speed_open = false;
        }
    }

    /// Sets or removes a keyframe at the playhead on the picked text or sticker.
    pub(super) fn key_here(&mut self, add: bool) {
        let head = self.head;
        let Some(t) = self.picked_item() else {
            self.say("Choose a text or sticker on the timeline first.");
            return;
        };
        if head < t.at || head >= t.end() {
            self.say("Move the playhead over the item first.");
            return;
        }
        let Some((local, x, y, m)) = self.picked_motion() else {
            self.say("Only text and stickers have keyframes.");
            return;
        };
        if add {
            let v = m.at(local, *x, *y);
            m.set_key(local, v);
        } else {
            m.remove_key(local);
        }
    }

    pub(super) fn jump_key(&mut self, forward: bool) {
        let Some(t) = self.picked_item() else { return };
        let at = t.at;
        let Some((local, _, _, m)) = self.picked_motion() else { return };
        let f = if forward { m.next_key(local) } else { m.previous_key(local) };
        if let Some(f) = f {
            self.seek_head(at + f);
        }
    }

    /// Keyframes as diamonds along the picked item on the timeline. A click goes to one; a
    /// drag moves it in time.
    pub(super) fn key_diamonds(&mut self, ui: &Ui, area: Rect, rect: Rect, item: u32, to_x: &dyn Fn(u32) -> f32, to_frame: &dyn Fn(f32) -> u32) {
        if self.picked_id() != Some(item) {
            return;
        }
        let paint = ui.painter().with_clip_rect(area);
        let Some(t) = self.project.items.iter().find(|t| t.id == item) else { return };
        let (at, len) = (t.at, t.len);
        let Some((_, _, m)) = t.body.placed() else { return };
        let frames: Vec<u32> = m.keys.iter().map(|k| k.frame).collect();
        let y = rect.bottom() - DIAMOND - 2.0;
        let mut go = None;
        let mut moved = None;
        for (i, f) in frames.iter().enumerate() {
            let c = pos2(to_x(at + f), y);
            if !area.x_range().contains(c.x) {
                continue;
            }
            let hit = ui.interact(Rect::from_center_size(c, Vec2::splat(DIAMOND * 2.6)), ui.scope_id().with(("key", item, i)), Sense::click_and_drag());
            let on = self.head == at + f;
            let fill = if on || hit.hovered() || hit.dragged() { Color32::WHITE } else { Color32::from_rgb(0xFF, 0xD2, 0x3C) };
            let pts = vec![pos2(c.x, c.y - DIAMOND), pos2(c.x + DIAMOND, c.y), pos2(c.x, c.y + DIAMOND), pos2(c.x - DIAMOND, c.y)];
            paint.add(egui::Shape::convex_polygon(pts, fill, Stroke::new(1.0, Color32::BLACK)));
            if hit.clicked() {
                go = Some(at + f);
            }
            if hit.dragged()
                && let Some(p) = hit.interact_pointer_pos()
            {
                moved = Some((i, to_frame(p.x).saturating_sub(at)));
            }
        }
        if let Some((i, f)) = moved
            && let Some(t) = self.project.items.iter_mut().find(|t| t.id == item)
            && let Some((_, _, m)) = t.body.placed_mut()
        {
            let landed = m.retime_key(i, f, len);
            self.head = at + landed;
            self.needs_activate = true;
        }
        if let Some(f) = go {
            self.seek_head(f);
        }
    }

    pub(super) fn reload_plugins(&mut self, ctx: &egui::Context) {
        self.plugins = plugins::load();
        self.builtins.retain(|b| b.group != plugins::GROUP);
        self.builtins.extend(self.plugins.looks.iter().cloned());
        for l in self.plugins.looks.clone() {
            self.rebuild_thumb(ctx, &l.id, &l.grade);
        }
    }

    /// What's loaded and from where, with ways to add more.
    pub(super) fn plugins_section(&mut self, ui: &mut Ui) {
        title(ui, "Plugins");
        super::captions::plugin_row(ui);
        note(ui, "A plugin pack is a JSON file that adds text animations and looks. Packs hold no code, so they can't run anything.");
        let packs = self.plugins.packs.clone();
        for p in &packs {
            let w = ui.available_width();
            let (r, _) = ui.allocate_exact_size(vec2(w, 54.0), Sense::hover());
            let paint = ui.painter();
            paint.rect_filled(r, 4, BUTTON);
            icons::paint(paint, Icon::Puzzle, Rect::from_center_size(pos2(r.left() + 20.0, r.center().y), Vec2::splat(18.0)), ACCENT);
            let by = if p.author.is_empty() { String::new() } else { format!(" by {}", p.author) };
            let version = if p.version.is_empty() { String::new() } else { format!(" {}", p.version) };
            paint.text(pos2(r.left() + 40.0, r.top() + 17.0), Align2::LEFT_CENTER, format!("{}{version}{by}", p.name), semi(13.0), Color32::WHITE);
            let source = p.file.as_ref().map_or_else(|| "Comes with the app".to_string(), |f| f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
            let what = format!("{} animations, {} looks · {source}", p.animations, p.looks);
            paint.text(pos2(r.left() + 40.0, r.top() + 37.0), Align2::LEFT_CENTER, what, regular(11.5), MUTED);
            if !p.description.is_empty() {
                let tip = ui.interact(r, ui.scope_id().with(("pack", p.name.as_str())), Sense::hover());
                tip.on_hover_text(&p.description);
            }
            ui.add_space(4.0);
        }
        for problem in self.plugins.problems.clone() {
            error_note(ui, &format!("Not loaded: {problem}"));
        }
        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(vec2(w, 34.0), Sense::hover());
        let half = Rect::from_min_size(r.min, vec2(w / 2.0 - 3.0, r.height()));
        if action(ui, half, "Install a Pack", false, 12.5).clicked()
            && let Some(path) = rfd::FileDialog::new().add_filter("Plugin pack", &["json"]).pick_file()
        {
            match plugins::install(&path) {
                Ok(name) => {
                    self.reload_plugins(ui.ctx());
                    self.say(format!("Installed {name}"));
                }
                Err(e) => self.say(format!("That pack could not be installed: {e}")),
            }
        }
        let other = Rect::from_min_size(pos2(half.right() + 6.0, r.top()), half.size());
        if action(ui, other, "Reload", false, 12.5).clicked() {
            self.reload_plugins(ui.ctx());
            self.say(format!("{} packs loaded", self.plugins.packs.len()));
        }
        ui.add_space(4.0);
        let (r, _) = ui.allocate_exact_size(vec2(w, 34.0), Sense::hover());
        if action(ui, r, "Open the Plugins Folder", false, 12.5).clicked() {
            crate::platform::reveal(&crate::store::plugins_dir());
        }
    }
}

/// "2×", "0.5×", "1.25×".
pub fn speed_label(s: f64) -> String {
    let t = format!("{s:.2}");
    format!("{}×", t.trim_end_matches('0').trim_end_matches('.'))
}

/// A font's own name, drawn in it, ready to show.
fn font_image(name: &str) -> Option<ColorImage> {
    let setting = overlay::Setting {
        text: name.to_string(),
        size: FONT_SAMPLE_PX * 2.0,
        colour: [1.0; 3],
        font: name.to_string(),
        weight: 400,
        italic: false,
        spacing: 0.0,
        outline: 0.0,
        finest: Unit::Line,
        scrambles: false,
        gradient: None,
        highlight: [1.0; 3],
    };
    let cells = overlay::text_cells(&setting)?;
    let s = cells.stamps.first()?;
    let px = s.px.iter().map(|c| {
        let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        Color32::from_rgba_premultiplied(b(c[0]), b(c[1]), b(c[2]), b(c[3]))
    });
    Some(ColorImage::new([s.w, s.h], px.collect()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn speeds_read_simply() {
        assert_eq!(speed_label(2.0), "2×");
        assert_eq!(speed_label(0.5), "0.5×");
        assert_eq!(speed_label(1.25), "1.25×");
        assert_eq!(speed_label(1.0), "1×");
    }

    #[test]
    fn a_preview_has_time_to_finish_before_it_rests() {
        let m = Motion { enter: Phase::preset(Anim::Typewriter), ..Motion::default() };
        assert!(preview_seconds(&m) > 0.3);
        let looped = Motion { repeat: Repeat::preset(Sway::Wave), ..Motion::default() };
        assert_eq!(preview_seconds(&looped), 2.0);
    }

    #[test]
    fn a_font_draws_its_own_name() {
        let img = font_image(&overlay::default_font()).expect("a font to draw with");
        assert!(img.pixels.iter().any(|p| p.a() > 128));
    }
}
