//! The Brand page: a company's kit (logo, colours, fonts, caption look, watermark, voice)
//! made once and put on any project with one press. New text and new captions follow the
//! project's kit from then on.

use super::App;
use super::captions::{STYLES, caption_motion, is_caption};
use super::kit::*;
use crate::brand::{self, Brand, CORNERS, MOST_COLOURS};
use crate::textanim::Motion;
use crate::timeline::{Body, Item, ROW_FX};
use eframe::egui::{self, Color32, Rect, Sense, Ui, pos2, vec2};

/// Items a kit placed, so applying it again replaces them instead of stacking.
pub const LOGO_GROUP: &str = "brand-logo";
const FONT_LIST_H: f32 = 220.0;
/// A row of corners with none of its own chosen.
const NONE_CHOSEN: usize = usize::MAX;
const WEIGHTS: [(&str, u16); 4] = [("Regular", 400), ("Semibold", 600), ("Bold", 700), ("Black", 900)];

pub struct BrandState {
    pub brands: Vec<Brand>,
    /// The kit open on the page, by id.
    open: Option<String>,
    /// Which font box is open, and what is typed in its search.
    font_open: Option<&'static str>,
    font_search: String,
    fonts: Option<Vec<String>>,
    problem: Option<String>,
}

impl Default for BrandState {
    fn default() -> Self {
        Self { brands: brand::list(), open: None, font_open: None, font_search: String::new(), fonts: None, problem: None }
    }
}

impl App {
    /// The kit this project uses, if any.
    pub(super) fn project_brand(&self) -> Option<&Brand> {
        let id = &self.project.brand;
        if id.is_empty() {
            return None;
        }
        self.edit.brand.brands.iter().find(|b| &b.id == id)
    }

    /// Puts a kit on the project: the logo in its corner, the caption look on every caption,
    /// and the heading font on every other text. Says what it did.
    pub(super) fn apply_brand(&mut self, id: &str) -> Result<String, String> {
        let b = self.edit.brand.brands.iter().find(|b| b.id == id).cloned().ok_or("That brand kit wasn't found.")?;
        self.project.brand = b.id.clone();
        let mut did = Vec::new();
        self.project.items.retain(|t| !t.body.placed().is_some_and(|p| p.2.group == LOGO_GROUP));
        if b.watermark.on && !b.logo.is_empty() {
            let (file, design) = crate::graphics::import(&b.logo)?;
            let (pw, ph) = (self.project.size.0 as f32, self.project.size.1 as f32);
            let [_, _, lw, lh] = design.layers.first().map(|l| l.bounds).ok_or("The logo has nothing to draw.")?;
            let fit = design.fit(pw, ph);
            // Sized by height, so wide and tall logos look equally weighty.
            let size = b.watermark.size * ph / (lh * fit).max(1e-3);
            let (x, y) = b.watermark.centre(lw * fit * size, lh * fit * size, pw, ph);
            let motion = Motion { opacity: b.watermark.opacity, group: LOGO_GROUP.into(), ..Motion::default() };
            let (id, len) = (self.project.new_id(), self.project.total().max(1));
            let body = Body::Graphic { file, layer: 0, name: format!("{} logo", b.name), size, x, y, motion };
            self.project.items.push(Item { id, row: ROW_FX, at: 0, len, body });
            did.push("the logo".to_string());
        }
        let (mut captions, mut titles) = (0, 0);
        for t in self.project.items.iter_mut() {
            let caption = is_caption(t);
            let Body::Text { size, colour, motion, .. } = &mut t.body else { continue };
            if caption {
                dress_caption(&b, size, colour, motion);
                captions += 1;
            } else if !b.heading_font.is_empty() {
                motion.font = b.heading_font.clone();
                titles += 1;
            }
        }
        if captions > 0 {
            did.push(format!("{captions} captions"));
        }
        if titles > 0 {
            did.push(format!("{titles} titles"));
        }
        Ok(if did.is_empty() { format!("{} is now this project's brand", b.name) } else { format!("Applied {}: {}", b.name, did.join(", ")) })
    }

    /// New captions take the project's kit, if it has one.
    pub(super) fn brand_new_captions(&mut self, ids: &[u32]) {
        let Some(b) = self.project_brand().cloned() else { return };
        for t in self.project.items.iter_mut().filter(|t| ids.contains(&t.id)) {
            if let Body::Text { size, colour, motion, .. } = &mut t.body {
                dress_caption(&b, size, colour, motion);
            }
        }
    }

    /// New text takes the kit's heading font and title colour.
    pub(super) fn brand_new_text(&self, colour: &mut [f32; 3], motion: &mut Motion) {
        if let Some(b) = self.project_brand() {
            motion.font = b.heading_font.clone();
            *colour = b.title_colour;
        }
    }

    pub(super) fn brand_page(&mut self, ui: &mut Ui) {
        ui.label(egui::RichText::new("Brand Kit").font(bold(15.0)).color(Color32::WHITE));
        note(ui, "A company's logo, colours, fonts and caption look, made once and put on any video in one step. Kits can be shared with a team as a file.");
        let w = ui.available_width();
        let names: Vec<(String, String)> = self.edit.brand.brands.iter().map(|b| (b.id.clone(), b.name.clone())).collect();
        if self.edit.brand.open.as_ref().is_none_or(|o| !names.iter().any(|(id, _)| id == o)) {
            self.edit.brand.open = names.iter().find(|(id, _)| *id == self.project.brand).or(names.first()).map(|n| n.0.clone());
        }
        let mut chips = Chips::new(ui, 30.0);
        for (id, name) in &names {
            let on = self.edit.brand.open.as_deref() == Some(id);
            let label = if *id == self.project.brand { format!("{name} (in use)") } else { name.clone() };
            if chips.add(&label, on) {
                self.edit.brand.open = Some(id.clone());
            }
        }
        if chips.add("New Brand", false) {
            let b = Brand { id: crate::store::new_id(), ..Brand::default() };
            if let Err(e) = brand::save(&b) {
                self.edit.brand.problem = Some(e);
            }
            self.edit.brand.open = Some(b.id.clone());
            self.edit.brand.brands.push(b);
        }
        if chips.add("Import Kit", false)
            && let Some(path) = rfd::FileDialog::new().add_filter("Brand kit", &[brand::EXTENSION, "json"]).pick_file()
        {
            match brand::import(&path) {
                Ok(b) => {
                    self.say(format!("Added {}", b.name));
                    self.edit.brand.open = Some(b.id.clone());
                    self.edit.brand.brands.push(b);
                }
                Err(e) => self.edit.brand.problem = Some(e),
            }
        }
        chips.end();
        if let Some(p) = self.edit.brand.problem.clone() {
            error_note(ui, &p);
        }
        let Some(i) = self.edit.brand.open.as_ref().and_then(|o| self.edit.brand.brands.iter().position(|b| &b.id == o)) else {
            note(ui, "No kits yet. Press New Brand to make the first one.");
            return;
        };
        let before = self.edit.brand.brands[i].clone();
        let mut b = before.clone();
        self.brand_form(ui, &mut b);
        if b != before {
            if let Err(e) = brand::save(&b) {
                self.edit.brand.problem = Some(e);
            }
            self.edit.brand.brands[i] = b.clone();
        }
        ui.add_space(8.0);
        let (r, _) = ui.allocate_exact_size(vec2(w, 38.0), Sense::hover());
        let in_use = self.project.brand == b.id;
        let label = if in_use { "Apply Again to This Project" } else { "Use on This Project" };
        if action(ui, r, label, true, 13.5).on_hover_text("Places the logo, restyles captions and titles, and makes new text follow this kit").clicked() {
            match self.apply_brand(&b.id) {
                Ok(m) => self.say(m),
                Err(e) => self.edit.brand.problem = Some(e),
            }
        }
        let (r, _) = ui.allocate_exact_size(vec2(w, 32.0), Sense::hover());
        // Stop Using only shows for the kit in use, so the row has two or three buttons.
        let n = if in_use { 3.0 } else { 2.0 };
        let cell = (w - 6.0 * (n - 1.0)) / n;
        let at = |i: f32| Rect::from_min_size(pos2(r.left() + (cell + 6.0) * i, r.top()), vec2(cell, r.height()));
        if action(ui, at(0.0), "Share Kit", false, 12.5).on_hover_text("Save this kit as one file for the rest of the team").clicked()
            && let Some(path) = rfd::FileDialog::new().add_filter("Brand kit", &[brand::EXTENSION]).set_file_name(format!("{}.{}", b.name, brand::EXTENSION)).save_file()
        {
            match brand::export(&b, &path) {
                Ok(()) => self.say(format!("Saved {}", path.display())),
                Err(e) => self.edit.brand.problem = Some(e),
            }
        }
        if in_use && action(ui, at(1.0), "Stop Using", false, 12.5).on_hover_text("New text goes back to the app's own look; nothing already made changes").clicked() {
            self.project.brand.clear();
        }
        if action(ui, at(n - 1.0), "Delete Kit", false, 12.5).clicked() {
            brand::delete(&b.id);
            self.edit.brand.brands.retain(|x| x.id != b.id);
            if self.project.brand == b.id {
                self.project.brand.clear();
            }
            self.edit.brand.open = None;
        }
    }

    fn brand_form(&mut self, ui: &mut Ui, b: &mut Brand) {
        let w = ui.available_width();
        heading(ui, "Name");
        ui.add(egui::TextEdit::singleline(&mut b.name).desired_width(f32::INFINITY));
        heading(ui, "Logo");
        let (r, _) = ui.allocate_exact_size(vec2(w, 32.0), Sense::hover());
        let half = Rect::from_min_size(r.min, vec2(w / 2.0 - 3.0, r.height()));
        let label = if b.logo.is_empty() { "Choose a Logo" } else { "Change Logo" };
        if action(ui, half, label, false, 12.5).on_hover_text("PNG, JPEG, WebP or SVG. A see-through background suits a watermark best.").clicked()
            && let Some(path) = rfd::FileDialog::new().add_filter("Logo", &["png", "jpg", "jpeg", "webp", "svg"]).pick_file()
        {
            match brand::logo_from(&path) {
                Ok(svg) => {
                    b.logo = svg;
                    self.edit.brand.problem = None;
                }
                Err(e) => self.edit.brand.problem = Some(e),
            }
        }
        if !b.logo.is_empty() && action(ui, Rect::from_min_size(pos2(half.right() + 6.0, r.top()), half.size()), "Remove Logo", false, 12.5).clicked() {
            b.logo.clear();
        }
        note(ui, if b.logo.is_empty() { "No logo yet." } else { "Logo saved in the kit." });

        heading(ui, "Colours");
        ui.horizontal_wrapped(|ui| {
            let mut gone = None;
            for (i, c) in b.colours.iter_mut().enumerate() {
                let resp = colour_button(ui, c).on_hover_text(format!("{} (right-click to remove)", brand::hex(*c)));
                if resp.secondary_clicked() && i > 0 {
                    gone = Some(i);
                }
            }
            if let Some(i) = gone {
                b.colours.remove(i);
            }
            if b.colours.len() < MOST_COLOURS && ui.button("Add").clicked() {
                b.colours.push([0.5; 3]);
            }
        });
        note(ui, "The first is the main colour. They appear beside the text colours.");
        ui.horizontal(|ui| {
            colour_button(ui, &mut b.title_colour);
            ui.label(egui::RichText::new("Title colour").font(regular(12.5)).color(TEXT_2));
        });

        heading(ui, "Fonts");
        self.font_field(ui, "heading", "Titles", &mut b.heading_font);
        self.font_field(ui, "body", "Body text", &mut b.body_font);

        heading(ui, "Captions");
        self.font_field(ui, "captions", "Caption font", &mut b.captions.font);
        let mut chips = Chips::new(ui, 28.0);
        for (i, s) in STYLES.iter().enumerate() {
            if chips.add(s, b.captions.style == i) {
                b.captions.style = i;
            }
        }
        chips.end();
        let mut chips = Chips::new(ui, 28.0);
        for (name, wt) in WEIGHTS {
            if chips.add(name, b.captions.weight == wt) {
                b.captions.weight = wt;
            }
        }
        chips.end();
        slider(ui, "Size", &mut b.captions.size, 24.0, 160.0, 64.0, Fmt::Int);
        slider(ui, "Outline", &mut b.captions.outline, 0.0, 0.15, 0.06, Fmt::Percent);
        ui.horizontal(|ui| {
            colour_button(ui, &mut b.captions.colour);
            ui.label(egui::RichText::new("Caption colour").font(regular(12.5)).color(TEXT_2));
        });

        heading(ui, "Watermark");
        toggle(ui, "Show the logo on every video", &mut b.watermark.on);
        if b.watermark.on {
            // Two rows of two corners, read as one choice.
            for first in [0, 2] {
                let c = &mut b.watermark.corner;
                let mut at = if (first..first + 2).contains(c) { *c - first } else { NONE_CHOSEN };
                if segmented(ui, &CORNERS[first..first + 2], &mut at) && at < 2 {
                    *c = first + at;
                }
            }
            slider(ui, "Size", &mut b.watermark.size, 0.04, 0.3, 0.1, Fmt::Percent);
            slider(ui, "Opacity", &mut b.watermark.opacity, 0.1, 1.0, 0.85, Fmt::Percent);
        }

        heading(ui, "Brand Voice");
        note(ui, "How the brand speaks: tone, words to use and words to avoid. The assistant writes titles and captions this way.");
        ui.add(egui::TextEdit::multiline(&mut b.voice).desired_rows(3).desired_width(f32::INFINITY).hint_text("Friendly and direct. Say \"customers\", never \"users\"."));
    }

    /// A font box with a search over every font on the computer. Empty is the app's own.
    fn font_field(&mut self, ui: &mut Ui, key: &'static str, label: &str, font: &mut String) {
        let fonts = self.edit.brand.fonts.get_or_insert_with(crate::overlay::font_families).clone();
        let w = ui.available_width();
        let (r, resp) = ui.allocate_exact_size(vec2(w, 34.0), Sense::click());
        let open = self.edit.brand.font_open == Some(key);
        let p = ui.painter();
        p.rect_filled(r, 10, if resp.hovered() || open { BUTTON_EDGE } else { BUTTON });
        p.text(pos2(r.left() + 12.0, r.center().y), egui::Align2::LEFT_CENTER, label, regular(12.0), MUTED);
        let shown = if font.is_empty() { "App default" } else { font.as_str() };
        p.with_clip_rect(r.shrink2(vec2(10.0, 0.0))).text(pos2(r.right() - 12.0, r.center().y), egui::Align2::RIGHT_CENTER, shown, semi(12.5), Color32::WHITE);
        if resp.clicked() {
            self.edit.brand.font_open = if open { None } else { Some(key) };
            self.edit.brand.font_search.clear();
        }
        if !open {
            return;
        }
        ui.add(egui::TextEdit::singleline(&mut self.edit.brand.font_search).hint_text("Search fonts").desired_width(f32::INFINITY));
        let needle = self.edit.brand.font_search.to_lowercase();
        let mut chosen = None;
        egui::ScrollArea::vertical().id_salt(("brand-fonts", key)).max_height(FONT_LIST_H).auto_shrink([false, true]).show(ui, |ui| {
            if needle.is_empty() && ui.selectable_label(font.is_empty(), "App default").clicked() {
                chosen = Some(String::new());
            }
            for f in fonts.iter().filter(|f| needle.is_empty() || f.to_lowercase().contains(&needle)) {
                if ui.selectable_label(f == font, f).clicked() {
                    chosen = Some(f.clone());
                }
            }
        });
        if let Some(f) = chosen {
            *font = f;
            self.edit.brand.font_open = None;
        }
    }
}

fn heading(ui: &mut Ui, text: &str) {
    ui.add_space(6.0);
    ui.label(egui::RichText::new(text).font(semi(13.0)).color(Color32::WHITE));
}

/// A caption in the kit's look: font, size, colour, weight, outline and entrance.
fn dress_caption(b: &Brand, size: &mut f32, colour: &mut [f32; 3], motion: &mut Motion) {
    let c = &b.captions;
    let style = caption_motion(c.style);
    motion.font = c.font.clone();
    motion.weight = c.weight;
    motion.outline = c.outline;
    motion.enter = style.enter;
    *size = c.size;
    *colour = c.colour;
}
