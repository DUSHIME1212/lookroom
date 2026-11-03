//! The preset strip and the LUT library, with its stack of selected tables.

use super::kit::*;
use super::{App, key_of, short};
use crate::icons::{self, Icon, Tool};
use crate::looks::{LUT_GROUPS, PRESET_GROUPS, RECIPES};
use crate::pipeline::Grade;
use crate::store::{self, UserPreset};
use eframe::egui::{
    self, Align2, Color32, Context, CornerRadius, Rect, Response, Sense, Stroke, StrokeKind, TextureHandle, Ui, UiBuilder,
    Vec2, pos2, vec2,
};
use std::collections::HashMap;

// Preset tile, from the preset screen's code.
const TILE_CORNER: u8 = 4;
const TILE_IMAGE_SHARE: f32 = 0.8;
const TILE_CAPTION_PAD: f32 = 4.0;
const TILE_DIM: f32 = 0.6;
const DIVIDER_ALPHA: f32 = 0.4;
const STRIP_BOTTOM: f32 = 20.0;
const TITLE_ROW: f32 = 17.0;
const TITLE_GAP: f32 = 8.0;
const TILE_GAP: f32 = 8.0;
// LUT thumbnails, measured from the reference screens.
const LUT_THUMB: Vec2 = vec2(68.0, 50.0);
const LUT_CAPTION: f32 = 16.0;
const LUT_PACKS: [&str; 6] = ["All", "Favorites", "User LUTs", "Print", "Film", "Creative"];

#[derive(Clone)]
struct TileItem {
    id: String,
    name: String,
    group: String,
    user: bool,
}

enum Act {
    ApplyPreset(String),
    Duplicate(String),
    StartRename(String),
    UpdateFromCurrent(String),
    Delete(String),
    ToggleLut(String),
    ToggleFavorite(String),
    RemoveLut(String),
}

impl App {
    fn grade_of(&self, id: &str) -> Option<Grade> {
        self.user
            .iter()
            .find(|u| u.id == id)
            .map(|u| u.grade.clone())
            .or_else(|| self.builtins.iter().find(|b| b.id == id).map(|b| b.grade.clone()))
    }

    fn name_of(&self, id: &str) -> Option<String> {
        self.user
            .iter()
            .find(|u| u.id == id)
            .map(|u| u.name.clone())
            .or_else(|| self.builtins.iter().find(|b| b.id == id).map(|b| b.name.clone()))
    }

    fn act(&mut self, ctx: &Context, a: Act) {
        match a {
            Act::ApplyPreset(id) => {
                if let Some(mut g) = self.grade_of(&id) {
                    // Framing, masks and the camera format belong to the picture, not the look.
                    g.frame_ratio = self.grade.frame_ratio;
                    g.crop = self.grade.crop.clone();
                    g.cst = self.grade.cst;
                    g.layers = std::mem::take(&mut self.grade.layers);
                    self.grade = g;
                    self.selected = Some(id);
                }
            }
            Act::Duplicate(id) => {
                if let (Some(g), Some(n)) = (self.grade_of(&id), self.name_of(&id)) {
                    let nid = store::new_id();
                    self.user.push(UserPreset { id: nid.clone(), name: format!("{n} copy"), grade: g.clone(), created_at: store::now_seconds() });
                    self.rebuild_thumb(ctx, &nid, &g);
                    self.persist();
                }
            }
            Act::StartRename(id) => {
                self.name_buf = self.name_of(&id).unwrap_or_default();
                self.sheet = Some(super::Sheet::Rename(id));
            }
            Act::UpdateFromCurrent(id) => {
                let g = self.look_only();
                if let Some(u) = self.user.iter_mut().find(|u| u.id == id) {
                    u.grade = g.clone();
                }
                self.rebuild_thumb(ctx, &id, &g);
                self.persist();
                self.say("Preset updated");
            }
            Act::Delete(id) => {
                self.user.retain(|u| u.id != id);
                self.thumbs.remove(&id);
                if self.selected.as_deref() == Some(&id) {
                    self.selected = None;
                }
                self.persist();
            }
            Act::ToggleLut(id) => self.toggle_lut(&id),
            Act::ToggleFavorite(id) => {
                if let Some(i) = self.settings.favorites.iter().position(|f| *f == id) {
                    self.settings.favorites.remove(i);
                } else {
                    self.settings.favorites.push(id);
                }
                store::save_settings(&self.settings);
            }
            Act::RemoveLut(id) => {
                if let Some(name) = id.strip_prefix("file:") {
                    let _ = std::fs::remove_file(store::luts_dir().join(format!("{name}.cube")));
                }
                self.bank.luts.remove(&id);
                self.imported.retain(|(i, _)| *i != id);
                self.grade.luts.retain(|l| l.id != id);
                for l in &mut self.grade.layers {
                    l.grade.luts.retain(|t| t.id != id);
                }
                self.thumbs.remove(&id);
            }
        }
    }

    fn preset_items(&self) -> Vec<TileItem> {
        let mut v: Vec<TileItem> = self
            .user
            .iter()
            .map(|u| TileItem { id: u.id.clone(), name: u.name.clone(), group: "My Presets".into(), user: true })
            .collect();
        v.extend(self.builtins.iter().map(|b| TileItem {
            id: b.id.clone(),
            name: b.name.clone(),
            group: b.group.to_string(),
            user: false,
        }));
        v
    }

    fn lut_items(&self) -> Vec<TileItem> {
        let mut v: Vec<TileItem> = Vec::new();
        for f in &self.settings.favorites {
            let name = self.lut_name(f);
            v.push(TileItem { id: f.clone(), name, group: "Favorites".into(), user: false });
        }
        for (id, n) in &self.imported {
            v.push(TileItem { id: id.clone(), name: n.clone(), group: "User LUTs".into(), user: true });
        }
        for g in LUT_GROUPS {
            for r in RECIPES.iter().filter(|r| r.group == *g) {
                v.push(TileItem { id: r.id(), name: r.name.to_string(), group: g.to_string(), user: false });
            }
        }
        v
    }

    pub(super) fn presets_panel(&mut self, ui: &mut Ui, ctx: &Context) {
        let area = ui.max_rect();
        ui.painter().text(area.left_top() + vec2(0.0, 10.0), Align2::LEFT_CENTER, "Filter Presets", bold(15.0), Color32::WHITE);
        let w = text_width(ui, "Save As Preset", semi(12.5)) + 26.0;
        let b = Rect::from_min_size(pos2(area.right() - w, area.top() - 3.0), vec2(w, 26.0));
        if action(ui, b, "Save As Preset", false, 12.5).clicked() {
            self.name_buf = "My Preset".to_string();
            self.sheet = Some(super::Sheet::SavePreset);
        }
        let all = self.preset_items();
        let sel = match self.sub("presets") {
            0 => "",
            1 => "My Presets",
            n => PRESET_GROUPS.get(n - 2).copied().unwrap_or(crate::plugins::GROUP),
        };
        let items: Vec<TileItem> = all.into_iter().filter(|i| sel.is_empty() || i.group == sel).collect();
        let strip = Rect::from_min_max(pos2(area.left(), area.top() + TITLE_ROW + TITLE_GAP), pos2(area.right(), area.bottom() + 12.0));
        if items.is_empty() {
            ui.painter().text(
                strip.center() - vec2(0.0, STRIP_BOTTOM / 2.0),
                Align2::CENTER_CENTER,
                "Save an edit as a preset to find it here.",
                regular(14.0),
                MUTED,
            );
            return;
        }
        let selected: Vec<String> = self.selected.iter().cloned().collect();
        let mut acts = Vec::new();
        ui.scope_builder(UiBuilder::new().max_rect(strip), |ui| {
            tile_strip(ui, "presets-strip", &items, &self.thumbs, &selected, &mut acts, |r, it, acts| {
                r.context_menu(|ui| {
                    if ui.button("Duplicate").clicked() {
                        acts.push(Act::Duplicate(it.id.clone()));
                        ui.close();
                    }
                    if it.user {
                        if ui.button("Update Using Current").clicked() {
                            acts.push(Act::UpdateFromCurrent(it.id.clone()));
                            ui.close();
                        }
                        if ui.button("Rename Preset").clicked() {
                            acts.push(Act::StartRename(it.id.clone()));
                            ui.close();
                        }
                        if ui.button("Delete").clicked() {
                            acts.push(Act::Delete(it.id.clone()));
                            ui.close();
                        }
                    }
                });
            }, Act::ApplyPreset);
        });
        for a in acts {
            self.act(ctx, a);
        }
    }

    /// The chosen tables in the order they run, then the library, then its pack menu.
    pub(super) fn luts_body(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        if !self.target().luts.is_empty() {
            ui.label(egui::RichText::new("Selected").font(semi(13.0)).color(Color32::WHITE));
            ui.add_space(2.0);
            let list: Vec<(String, String)> = self.target().luts.iter().map(|l| (l.id.clone(), l.name.clone())).collect();
            self.pick.lut = self.pick.lut.min(list.len() - 1);
            let mut pick = None;
            let mut swap = None;
            let mut drop_one = None;
            egui::ScrollArea::horizontal()
                .id_salt("lut-stack")
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        for (i, (id, name)) in list.iter().enumerate() {
                            let r = lut_thumb(ui, self.thumbs.get(id), name, i == self.pick.lut, true);
                            if r.clicked() {
                                pick = Some(i);
                            }
                            r.context_menu(|ui| {
                                if i > 0 && ui.button("Run Earlier").clicked() {
                                    swap = Some((i, i - 1));
                                    ui.close();
                                }
                                if i + 1 < list.len() && ui.button("Run Later").clicked() {
                                    swap = Some((i, i + 1));
                                    ui.close();
                                }
                                if ui.button("Remove").clicked() {
                                    drop_one = Some(i);
                                    ui.close();
                                }
                            });
                        }
                    });
                });
            if let Some(i) = pick {
                self.pick.lut = i;
            }
            if let Some((a, b)) = swap {
                self.target().luts.swap(a, b);
                self.pick.lut = b;
            }
            if let Some(i) = drop_one {
                self.target().luts.remove(i);
                self.pick.lut = 0;
            }
            ui.add_space(2.0);
            let at = self.pick.lut;
            if let Some(l) = self.target().luts.get_mut(at) {
                slider(ui, "Intensity", &mut l.intensity, 0.0, 1.0, 1.0, Fmt::Percent);
            }
            ui.add_space(4.0);
        }

        let key = key_of(Tool::Luts);
        let pack = self.sub(key).min(LUT_PACKS.len() - 1);
        let name = LUT_PACKS[pack];
        let all = self.lut_items();
        let items: Vec<TileItem> = all
            .into_iter()
            .filter(|i| if pack == 0 { i.group != "Favorites" } else { i.group == name })
            .collect();
        let active: Vec<String> = self.target().luts.iter().map(|l| l.id.clone()).collect();
        let favs = self.settings.favorites.clone();
        let mut acts = Vec::new();
        if items.is_empty() {
            let msg = if pack == 1 { "Right-click a LUT to add it to your favorites." } else { "Imported LUTs will appear here." };
            note(ui, msg);
        } else {
            let hovered = ui.rect_contains_pointer(Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), LUT_THUMB.y + LUT_CAPTION)));
            egui::ScrollArea::horizontal()
                .id_salt("lut-library")
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    if hovered {
                        let dy = ui.input(|i| i.smooth_scroll_delta.y);
                        if dy != 0.0 {
                            ui.scroll_with_delta(vec2(dy, 0.0));
                        }
                    }
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let mut group = None::<&str>;
                        for it in &items {
                            if group.is_some_and(|g| g != it.group) {
                                let (r, _) = ui.allocate_exact_size(vec2(1.0, LUT_THUMB.y), Sense::hover());
                                ui.painter().rect_filled(r, 0.0, Color32::WHITE.gamma_multiply(DIVIDER_ALPHA));
                            }
                            group = Some(&it.group);
                            let on = active.iter().any(|a| *a == it.id);
                            let r = lut_thumb(ui, self.thumbs.get(&it.id), &it.name, on, false);
                            if r.clicked() {
                                acts.push(Act::ToggleLut(it.id.clone()));
                            }
                            r.context_menu(|ui| {
                                let label = if favs.contains(&it.id) { "Unfavorite" } else { "Favorite" };
                                if ui.button(label).clicked() {
                                    acts.push(Act::ToggleFavorite(it.id.clone()));
                                    ui.close();
                                }
                                if it.user && ui.button("Delete").clicked() {
                                    acts.push(Act::RemoveLut(it.id.clone()));
                                    ui.close();
                                }
                            });
                        }
                    });
                });
        }
        for a in acts {
            self.act(&ctx, a);
        }
        ui.add_space(6.0);

        let w = ui.available_width();
        let (row, _) = ui.allocate_exact_size(vec2(w, 32.0), Sense::hover());
        let mut at = pack;
        let menu = Rect::from_min_size(row.min, vec2(132.0, 32.0));
        if dropdown(ui, menu, None, &LUT_PACKS, &mut at) {
            self.subs.insert(key, at);
        }
        let cell = 32.0;
        let slot = |i: f32| Rect::from_min_size(pos2(row.right() - cell * (i + 1.0), row.top()), Vec2::splat(cell));
        if icon_button(ui, slot(0.0), Icon::Layers01, "Stack LUTs: tap adds rather than replaces", self.stack).clicked() {
            self.stack = !self.stack;
        }
        if icon_button(ui, slot(1.0), Icon::FileExport, "Save this look as a .cube LUT (64 points)", false).clicked() {
            self.export_lut(64);
        }
        if icon_button(ui, slot(2.0), Icon::Download01, "Save this look as a .cube LUT (33 points)", false).clicked() {
            self.export_lut(33);
        }
        if icon_button(ui, slot(3.0), Icon::Folder01, "Import .cube or HaldCLUT", false).clicked() {
            self.import_luts(&ctx);
        }
    }
}

/// A picture with a small caption under it. A selected one wears a tick.
fn lut_thumb(ui: &mut Ui, tex: Option<&TextureHandle>, name: &str, on: bool, stacked: bool) -> Response {
    let (r, resp) = ui.allocate_exact_size(vec2(LUT_THUMB.x, LUT_THUMB.y + LUT_CAPTION), Sense::click());
    if !ui.is_rect_visible(r) {
        return resp;
    }
    let top = Rect::from_min_size(r.min, LUT_THUMB);
    match tex {
        Some(t) => {
            egui::Image::new((t.id(), top.size())).corner_radius(TILE_CORNER).paint_at(ui, top);
        }
        None => {
            ui.painter().rect_filled(top, TILE_CORNER, BUTTON);
        }
    }
    let p = ui.painter();
    if on {
        p.rect_stroke(top, TILE_CORNER, Stroke::new(2.0, if stacked { Color32::WHITE } else { ACCENT }), StrokeKind::Inside);
        let c = top.right_top() + vec2(-10.0, 10.0);
        p.circle_filled(c, 7.0, Color32::WHITE);
        icons::paint(p, Icon::Tick02, Rect::from_center_size(c, Vec2::splat(11.0)), Color32::BLACK);
    } else if resp.hovered() {
        p.rect_stroke(top, TILE_CORNER, Stroke::new(1.0, Color32::from_white_alpha(170)), StrokeKind::Outside);
    }
    let bar = Rect::from_min_max(pos2(r.left(), top.bottom()), r.max);
    p.with_clip_rect(bar).text(bar.center(), Align2::CENTER_CENTER, short(name, 13), regular(10.0), Color32::from_rgb(0xC8, 0xC8, 0xCC));
    resp
}

/// One preset tile: picture on the top 80%, caption bar on the rest.
fn tile(ui: &mut Ui, side: f32, tex: Option<&TextureHandle>, name: &str, selected: bool) -> Response {
    let (r, resp) = ui.allocate_exact_size(Vec2::splat(side), Sense::click());
    if !ui.is_rect_visible(r) {
        return resp;
    }
    let dim = if resp.is_pointer_button_down_on() { TILE_DIM } else { 1.0 };
    let tint = Color32::WHITE.gamma_multiply(dim);
    let image_h = r.height() * TILE_IMAGE_SHARE;
    let top = Rect::from_min_size(r.min, vec2(r.width(), image_h));
    let bar = Rect::from_min_max(pos2(r.left(), top.bottom()), r.max);
    let p = ui.painter();
    let c = TILE_CORNER;
    if let Some(t) = tex {
        egui::Image::new((t.id(), top.size()))
            .corner_radius(CornerRadius { nw: c, ne: c, sw: 0, se: 0 })
            .tint(tint)
            .paint_at(ui, top);
    } else {
        p.rect_filled(top, CornerRadius { nw: c, ne: c, sw: 0, se: 0 }, BUTTON);
    }
    let fill = if selected { ACCENT } else { Color32::WHITE };
    p.rect_filled(bar, CornerRadius { nw: 0, ne: 0, sw: c, se: c }, fill.gamma_multiply(dim));
    // The caption shrinks to half its size at most before it is cut off.
    let room = bar.width() - TILE_CAPTION_PAD * 2.0;
    let mut size = 12.0;
    let mut g = p.layout_no_wrap(name.to_string(), semi(size), Color32::BLACK);
    while g.size().x > room && size > 6.0 {
        size -= 0.5;
        g = p.layout_no_wrap(name.to_string(), semi(size), Color32::BLACK);
    }
    let pos = bar.center() - g.size() / 2.0;
    p.with_clip_rect(bar).galley(pos, g, Color32::BLACK);
    if resp.hovered() {
        p.rect_stroke(r, c, Stroke::new(1.0, Color32::from_white_alpha(170)), StrokeKind::Outside);
    }
    resp
}

/// The sideways-scrolling row with thin lines between groups.
fn tile_strip(
    ui: &mut Ui,
    id: &str,
    items: &[TileItem],
    thumbs: &HashMap<String, TextureHandle>,
    selected: &[String],
    acts: &mut Vec<Act>,
    menu: impl Fn(&Response, &TileItem, &mut Vec<Act>),
    on_click: impl Fn(String) -> Act,
) {
    ui.spacing_mut().item_spacing.x = TILE_GAP;
    let area = ui.max_rect();
    // Tiles are square, so the strip's height sets their size.
    let side = area.height() - STRIP_BOTTOM;
    let hovered = ui.rect_contains_pointer(area);
    egui::ScrollArea::horizontal()
        .id_salt(id)
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .show(ui, |ui| {
            if hovered {
                let dy = ui.input(|i| i.smooth_scroll_delta.y);
                if dy != 0.0 {
                    ui.scroll_with_delta(vec2(dy, 0.0));
                }
            }
            ui.horizontal(|ui| {
                let mut group = None::<&str>;
                for it in items {
                    if group.is_some_and(|g| g != it.group) {
                        let (r, _) = ui.allocate_exact_size(vec2(1.0, side), Sense::hover());
                        let line = Rect::from_center_size(r.center(), vec2(1.0, side - 32.0));
                        ui.painter().rect_filled(line, 0.0, Color32::WHITE.gamma_multiply(DIVIDER_ALPHA));
                    }
                    group = Some(&it.group);
                    let on = selected.iter().any(|s| *s == it.id);
                    let r = tile(ui, side, thumbs.get(&it.id), &it.name, on);
                    if r.clicked() {
                        acts.push(on_click(it.id.clone()));
                    }
                    menu(&r, it, acts);
                }
            });
            ui.add_space(STRIP_BOTTOM);
        });
}
