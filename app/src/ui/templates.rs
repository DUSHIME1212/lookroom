//! Title layouts and backgrounds for motion-graphics videos: marker scribbles, callouts,
//! two-tone headlines, big gradient words, pill tags and more, each made of ordinary text
//! items that stay editable, placed together and selected as a group. Also the Style
//! section of the Text page: gradient, highlighted words and a shape behind the text.

use super::App;
use super::kit::*;
use crate::brand::hex;
use crate::textanim::{Anim, Backing, Motion, Phase, Unit};
use crate::timeline::{Body, Item, ROW_FX2};
use eframe::egui::{self, Align2, Color32, FontId, Rect, Sense, Ui, pos2, vec2};
use std::path::PathBuf;

/// One text of a template, placed around the template's centre.
struct Piece {
    text: &'static str,
    size: f32,
    colour: [f32; 3],
    weight: u16,
    /// Offset from the centre, as a share of the picture's width and height.
    dx: f32,
    dy: f32,
    gradient: Option<[f32; 3]>,
    highlight: [f32; 3],
    backing: Backing,
    backing_colour: [f32; 3],
    enter: Anim,
    unit: Unit,
    /// Seconds after the template starts that this piece comes in.
    after: f32,
}

const fn piece(text: &'static str, size: f32, colour: [f32; 3], dx: f32, dy: f32) -> Piece {
    Piece { text, size, colour, weight: 800, dx, dy, gradient: None, highlight: [0.18, 0.42, 1.0], backing: Backing::None, backing_colour: [0.0; 3], enter: Anim::SlideUp, unit: Unit::Whole, after: 0.0 }
}

const BLACK: [f32; 3] = [0.05, 0.05, 0.06];
const WHITE: [f32; 3] = [1.0; 3];
const LIME: [f32; 3] = [0.84, 0.98, 0.37];
const VIOLET: [f32; 3] = [0.56, 0.5, 0.98];
const BLUE: [f32; 3] = [0.18, 0.42, 1.0];
const GREY: [f32; 3] = [0.55, 0.59, 0.66];

pub struct Template {
    pub name: &'static str,
    pub about: &'static str,
    pieces: Vec<Piece>,
}

/// Every template, in the order the page shows them.
pub fn templates() -> Vec<Template> {
    vec![
        Template {
            name: "Marker Title",
            about: "A bold word on a brush stroke, with a small tag",
            pieces: vec![
                Piece { backing: Backing::Marker, backing_colour: LIME, enter: Anim::Pop, ..piece("Scribble", 120.0, BLACK, 0.0, 0.0) },
                Piece { weight: 600, backing: Backing::Marker, backing_colour: BLACK, after: 0.35, ..piece("marker titles", 44.0, WHITE, 0.09, 0.1) },
            ],
        },
        Template {
            name: "Callout",
            about: "A framed title over a small coloured label",
            pieces: vec![
                Piece { backing: Backing::Frame, backing_colour: VIOLET, enter: Anim::Fade, ..piece("CALL OUT TITLE", 84.0, WHITE, 0.0, -0.03) },
                Piece { weight: 600, backing: Backing::Box, backing_colour: VIOLET, after: 0.25, ..piece("MODERN CREATIVE", 38.0, WHITE, -0.04, 0.07) },
            ],
        },
        Template {
            name: "Two-Tone Headline",
            about: "Two lines, the second picked out in colour",
            pieces: vec![Piece { weight: 700, highlight: BLUE, unit: Unit::Word, ..piece("Money should\n*be easier*", 104.0, GREY, 0.0, 0.0) }],
        },
        Template {
            name: "Big Word",
            about: "One huge word with a gradient",
            pieces: vec![Piece { weight: 900, gradient: Some([0.45, 0.7, 1.0]), enter: Anim::Scale, ..piece("into", 300.0, BLUE, 0.0, 0.0) }],
        },
        Template {
            name: "Pill Tags",
            about: "Short words in pills, arriving one by one",
            pieces: vec![
                Piece { backing: Backing::Pill, backing_colour: [0.75, 0.84, 1.0], enter: Anim::Pop, ..piece("secure", 64.0, BLUE, -0.2, 0.0) },
                Piece { enter: Anim::Pop, after: 0.25, ..piece("full control", 64.0, BLACK, 0.08, -0.12) },
                Piece { backing: Backing::Pill, backing_colour: [0.75, 0.84, 1.0], enter: Anim::Pop, after: 0.5, ..piece("low friction", 64.0, BLUE, 0.16, 0.1) },
            ],
        },
        Template {
            name: "Lower Third",
            about: "A name and a role in boxes, low on the left",
            pieces: vec![
                Piece { backing: Backing::Box, backing_colour: BLACK, enter: Anim::SlideLeft, ..piece("Alex Morgan", 60.0, WHITE, -0.22, 0.28) },
                Piece { weight: 600, backing: Backing::Box, backing_colour: VIOLET, enter: Anim::SlideLeft, after: 0.2, ..piece("Product Designer", 34.0, WHITE, -0.25, 0.36) },
            ],
        },
        Template {
            name: "Stacked Statement",
            about: "Short lines rising one at a time, the last one lit",
            pieces: vec![Piece { weight: 500, highlight: LIME, unit: Unit::Line, enter: Anim::SlideUp, ..piece("Because\ngreat\ndesign\nisn't just\n*how it looks*", 78.0, [0.92, 0.97, 0.82], 0.0, 0.0) }],
        },
        Template {
            name: "Underlined Note",
            about: "A line with a hand-drawn underline",
            pieces: vec![Piece { weight: 600, highlight: [0.12, 0.45, 0.25], backing: Backing::Underline, backing_colour: LIME, enter: Anim::Fade, ..piece("Ready to *grow?*", 80.0, [0.1, 0.25, 0.15], 0.0, 0.0) }],
        },
    ]
}

/// Backgrounds a motion-graphics video sits on.
const BACKGROUNDS: [(&str, [f32; 3], [f32; 3]); 6] = [
    ("Deep Green", [0.02, 0.2, 0.15], [0.3, 0.75, 0.45]),
    ("Paper", [0.96, 0.96, 0.95], [0.9, 0.92, 0.95]),
    ("Night Blue", [0.03, 0.05, 0.16], [0.18, 0.42, 1.0]),
    ("Sunset", [0.98, 0.45, 0.25], [0.55, 0.2, 0.6]),
    ("Lime Pop", [0.84, 0.98, 0.37], [0.97, 1.0, 0.85]),
    ("Charcoal", [0.08, 0.08, 0.09], [0.22, 0.22, 0.25]),
];
const BACKGROUND_SECONDS: f64 = 10.0;

pub struct TemplatesState {
    background: usize,
    animated: bool,
    working: Option<std::sync::mpsc::Receiver<Result<PathBuf, String>>>,
}

impl Default for TemplatesState {
    fn default() -> Self {
        Self { background: 0, animated: true, working: None }
    }
}

impl App {
    /// Gradient, highlighted words and the shape behind the picked text.
    pub(super) fn style_section(&mut self, ui: &mut Ui) {
        let Some(t) = self.picked_item() else { return };
        let Some((_, _, m)) = t.body.placed_mut() else { return };
        ui.add_space(6.0);
        ui.label(egui::RichText::new("Style").font(bold(14.0)).color(Color32::WHITE));
        let mut on = m.gradient.is_some();
        if toggle(ui, "Gradient", &mut on) {
            m.gradient = on.then_some([0.45, 0.7, 1.0]);
        }
        if let Some(g) = &mut m.gradient {
            ui.horizontal(|ui| {
                colour_button(ui, g);
                ui.label(egui::RichText::new("Blends to this colour, top to bottom").font(regular(12.0)).color(TEXT_2));
            });
        }
        ui.horizontal(|ui| {
            colour_button(ui, &mut m.highlight);
            ui.label(egui::RichText::new("Highlight colour").font(regular(12.0)).color(TEXT_2));
        });
        note(ui, "Put *stars* around words to colour them and make them bold.");
        ui.label(egui::RichText::new("Behind the text").font(semi(12.5)).color(Color32::WHITE));
        let mut chips = Chips::new(ui, 26.0);
        for b in Backing::ALL {
            if chips.add(b.name(), m.backing == b) {
                m.backing = b;
            }
        }
        chips.end();
        if m.backing != Backing::None {
            ui.horizontal(|ui| {
                colour_button(ui, &mut m.backing_colour);
                ui.label(egui::RichText::new("Shape colour").font(regular(12.0)).color(TEXT_2));
            });
        }
    }

    pub(super) fn templates_page(&mut self, ui: &mut Ui) {
        self.poll_background(ui.ctx());
        note(ui, "Title layouts made of ordinary text: change the words, colours and fonts after placing. Each lands at the playhead, its parts selected together.");
        let w = ui.available_width();
        let list = templates();
        let cell = (w - 6.0) / 2.0;
        let tall = cell * 0.56 + 34.0;
        let mut chosen = None;
        for row in list.chunks(2).enumerate() {
            let (r, _) = ui.allocate_exact_size(vec2(w, tall), Sense::hover());
            for (n, t) in row.1.iter().enumerate() {
                let b = Rect::from_min_size(pos2(r.left() + (cell + 6.0) * n as f32, r.top()), vec2(cell, tall));
                let pic = Rect::from_min_max(b.min, pos2(b.right(), b.bottom() - 34.0));
                let resp = ui.interact(b, ui.scope_id().with(("template", t.name)), Sense::click()).on_hover_text(t.about);
                paint_template(ui, pic, t, resp.hovered());
                ui.painter().text(pos2(b.left() + 2.0, b.bottom() - 22.0), Align2::LEFT_CENTER, t.name, semi(12.0), Color32::WHITE);
                ui.painter().with_clip_rect(b).text(pos2(b.left() + 2.0, b.bottom() - 7.0), Align2::LEFT_CENTER, t.about, regular(10.0), MUTED);
                if resp.clicked() {
                    chosen = Some(row.0 * 2 + n);
                }
            }
            ui.add_space(4.0);
        }
        if let Some(i) = chosen {
            let at = self.head;
            let n = self.place_template(&list[i], None, at, (0.5, 0.5));
            self.say(format!("Added {} ({} parts)", list[i].name, n));
        }

        ui.add_space(8.0);
        ui.label(egui::RichText::new("Backgrounds").font(bold(14.0)).color(Color32::WHITE));
        note(ui, "A colour field to put titles on, added as a clip at the playhead.");
        let names: Vec<&str> = BACKGROUNDS.iter().map(|b| b.0).collect();
        let mut at = self.edit.templates.background;
        segmented(ui, &names[..3], &mut at);
        let mut second = if at >= 3 { at - 3 } else { usize::MAX };
        if segmented(ui, &names[3..], &mut second) && second < 3 {
            at = second + 3;
        }
        self.edit.templates.background = at;
        toggle(ui, "Moving glow", &mut self.edit.templates.animated);
        let (r, _) = ui.allocate_exact_size(vec2(w, 34.0), Sense::hover());
        let busy = self.edit.templates.working.is_some();
        if action(ui, r, if busy { "Making…" } else { "Add Background" }, !busy, 12.5).clicked() && !busy {
            let (_, a, b) = BACKGROUNDS[at];
            let (size, animated) = (self.project.size, self.edit.templates.animated);
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(make_background(a, b, size, animated));
            });
            self.edit.templates.working = Some(rx);
        }
    }

    fn poll_background(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.edit.templates.working else { return };
        match rx.try_recv() {
            Ok(got) => {
                self.edit.templates.working = None;
                match got.and_then(|p| self.add_media(&p)) {
                    Ok(id) => {
                        let slot = self.project.slot_at(self.head);
                        self.place(id, Some(slot));
                    }
                    Err(e) => self.say(e),
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(150)),
            Err(_) => self.edit.templates.working = None,
        }
    }

    /// Places a template's texts from `at`, centred on `centre`. `words` replaces each
    /// piece's words in order. The pieces end up selected together; says how many.
    pub(super) fn place_template(&mut self, t: &Template, words: Option<&[String]>, at: u32, centre: (f32, f32)) -> usize {
        let len = self.project.frames(crate::timeline::ITEM_SECONDS);
        let font = self.project_brand().map(|b| b.heading_font.clone()).unwrap_or_default();
        let mut ids = Vec::new();
        for (n, p) in t.pieces.iter().enumerate() {
            let text = words.and_then(|w| w.get(n)).cloned().unwrap_or_else(|| p.text.to_string());
            let after = self.project.frames(p.after as f64);
            let enter = Phase { unit: p.unit, ..Phase::preset(p.enter) };
            let motion = Motion { font: font.clone(), weight: p.weight, gradient: p.gradient, highlight: p.highlight, backing: p.backing, backing_colour: p.backing_colour, enter, ..Motion::default() };
            let body = Body::Text { text, size: p.size, colour: p.colour, x: centre.0 + p.dx, y: centre.1 + p.dy, motion };
            let id = self.project.new_id();
            self.project.items.push(Item { id, row: ROW_FX2, at: at + after, len: len.saturating_sub(after).max(2), body });
            ids.push(id);
        }
        let n = ids.len();
        self.select_these(ids);
        self.edit.rail = super::edit::Rail::Text;
        self.edit.captions.tab = 0;
        n
    }
}

/// A quick picture of a template: its shapes and words in their colours.
fn paint_template(ui: &Ui, r: Rect, t: &Template, hot: bool) {
    let p = ui.painter().with_clip_rect(r);
    let dark = t.pieces.iter().all(|x| x.colour[0] + x.colour[1] + x.colour[2] > 1.5 || x.backing != Backing::None);
    p.rect_filled(r, 8, if dark { Color32::from_rgb(0x1E, 0x22, 0x2A) } else { Color32::from_rgb(0xEE, 0xEF, 0xF2) });
    if hot {
        p.rect_stroke(r, 8, egui::Stroke::new(1.5, ACCENT), egui::StrokeKind::Inside);
    }
    let col = |c: [f32; 3]| Color32::from_rgb((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8);
    let k = r.height() / 1080.0 * 1.6;
    for piece in &t.pieces {
        let c = pos2(r.center().x + piece.dx * r.width(), r.center().y + piece.dy * r.height() * 1.4);
        let text = piece.text.replace('*', "");
        let font = FontId::proportional((piece.size * k).clamp(7.0, 34.0));
        let galley = ui.painter().layout_no_wrap(text.clone(), font.clone(), col(piece.colour));
        let size = galley.size();
        let back = Rect::from_center_size(c, size + vec2(10.0, 4.0));
        match piece.backing {
            Backing::Box | Backing::Marker => {
                p.rect_filled(back, 2, col(piece.backing_colour));
            }
            Backing::Pill => {
                p.rect_filled(back, back.height() / 2.0, col(piece.backing_colour));
            }
            Backing::Frame => {
                p.rect_stroke(back, 0, egui::Stroke::new(1.0, col(piece.backing_colour)), egui::StrokeKind::Inside);
            }
            Backing::Underline => {
                p.line_segment([pos2(back.left(), back.bottom()), pos2(back.right(), back.bottom())], egui::Stroke::new(2.0, col(piece.backing_colour)));
            }
            Backing::None => {}
        }
        // A sketch, not a render: a piece with highlighted words shows in their colour.
        let colour = if piece.text.contains('*') { col(piece.highlight) } else { col(piece.colour) };
        p.text(c, Align2::CENTER_CENTER, text, font, colour);
    }
}

/// A background as a still picture, or a 10-second moving glow made by ffmpeg.
fn make_background(a: [f32; 3], b: [f32; 3], size: (usize, usize), animated: bool) -> Result<PathBuf, String> {
    let dir = crate::store::data_dir().join("Backgrounds");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let (w, h) = (size.0.max(2) & !1, size.1.max(2) & !1);
    let tag = format!("{}-{}-{w}x{h}", hex(a).trim_start_matches('#'), hex(b).trim_start_matches('#'));
    if animated {
        let out = dir.join(format!("glow-{tag}.mp4"));
        if out.is_file() {
            return Ok(out);
        }
        let colour = |c: [f32; 3]| format!("0x{}", hex(c).trim_start_matches('#'));
        let source = format!("gradients=s={w}x{h}:c0={}:c1={}:c2={}:n=3:type=radial:speed=0.006:d={BACKGROUND_SECONDS}:r=30:seed=7", colour(a), colour(b), colour(a));
        let status = crate::video::tool("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", &source, "-vf", "gblur=sigma=40", "-c:v", "libx264", "-preset", "veryfast", "-crf", "18", "-pix_fmt", "yuv420p"])
            .arg(&out)
            .status()
            .map_err(|e| crate::video::cannot_start(&e))?;
        return if status.success() { Ok(out) } else { Err("The moving background couldn't be made.".into()) };
    }
    let out = dir.join(format!("still-{tag}.png"));
    if !out.is_file() {
        let img = image::RgbImage::from_fn(w as u32, h as u32, |x, y| {
            // A soft diagonal blend, lighter towards the top right.
            let t = ((x as f32 / w as f32) * 0.6 + (1.0 - y as f32 / h as f32) * 0.4).clamp(0.0, 1.0);
            let m = |i: usize| ((a[i] + (b[i] - a[i]) * t).clamp(0.0, 1.0) * 255.0) as u8;
            image::Rgb([m(0), m(1), m(2)])
        });
        img.save(&out).map_err(|e| e.to_string())?;
    }
    Ok(out)
}
