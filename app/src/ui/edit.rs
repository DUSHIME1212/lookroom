//! The edit workspace on a wide window: a rail of tools, the media bin, the preview
//! with its sound meter, and the timeline. The Adjustment page holds every grading tool.

use super::kit::*;
use super::{App, Sheet, THUMB, short};
use crate::icons::{self, Icon, Tool};
use crate::overlay::STICKERS;
use crate::timeline::{Body, Fx, Item, Kind as MediaKind, Picked, ROW_FX, ROW_FX2, ROW_MUSIC, ROW_SOUND, ROW_VIDEO, ROWS};
use crate::video;
use eframe::egui::{self, Align2, Color32, ColorImage, Context, Pos2, Rect, Sense, Stroke, StrokeKind, TextureHandle, TextureOptions, Ui, UiBuilder, Vec2, ViewportCommand, pos2, vec2};
use std::collections::HashMap;
use std::sync::mpsc::{self, Receiver, Sender};

const RAIL_W: f32 = 84.0;
const HEADER_H: f32 = 46.0;
const RULER_H: f32 = 28.0;
/// Narrower than this on screen, a clip or item is a plain bar.
const SLIVER: f32 = 14.0;
const NEAR_POINTER: f32 = 10.0;
/// How close, on screen, a dragged edge must come to something to line up with it.
const SNAP_PX: f32 = 9.0;
/// The playhead stays in view: when it leaves, the view moves so it sits this far in.
const FOLLOW_AT: f32 = 0.25;
/// What each track is for, shown faintly when it is empty.
const ROW_NAMES: [&str; ROWS] = ["Video", "Sound", "Effects", "Text, stickers and videos", "Music and sound effects"];
const TOOLBAR_H: f32 = 40.0;
const TRACK_HEADS_W: f32 = 156.0;
const TREE_W: f32 = 196.0;
const ROW_H: [f32; ROWS] = [58.0, 38.0, 38.0, 38.0, 38.0];
/// How far tracks can be squeezed or stretched by dragging.
const LEAST_ROWS: f32 = 0.6;
const MOST_ROWS: f32 = 2.5;
/// Narrowest the bin and the picture can be made.
const LEAST_BIN: f32 = 260.0;
const LEAST_VIEW: f32 = 320.0;
const ROW_GAP: f32 = 6.0;
const EDGE: f32 = 9.0;
const STRIP_COUNT: usize = 12;
const STRIP_SIZE: (usize, usize) = (120, 68);
const MIN_ZOOM: f32 = 1.0;
/// Room each ruler label needs, in points.
const RULER_LABEL_ROOM: f32 = 64.0;
const MAX_ZOOM: f32 = 60.0;
const YELLOW: Color32 = Color32::from_rgb(0xC8, 0xC8, 0x10);
const SOUND: Color32 = Color32::from_rgb(0x2C, 0x80, 0xA8);
const MUSIC: Color32 = Color32::from_rgb(0x48, 0x48, 0xA8);
const EFFECT: Color32 = Color32::from_rgb(0x1E, 0x70, 0x4A);
const FLASH: Color32 = Color32::from_rgb(0x78, 0x88, 0x28);
const TEXT: Color32 = Color32::from_rgb(0x98, 0x50, 0x88);
const OVERLAY: Color32 = Color32::from_rgb(0x2E, 0x6E, 0x8E);
const DESIGN: Color32 = Color32::from_rgb(0x7A, 0x5A, 0xB8);
const NUDGE_SECONDS: f64 = 5.0;
/// The loudest a clip can be set, as a multiple of how it was recorded.
const MOST_VOLUME: f32 = 2.0;
/// Film strips made at once. Each decodes a whole video, so a few keep memory in hand.
const STRIP_WORKERS: usize = 2;
const CANVASES: [(&str, (usize, usize)); 6] = [
    ("16:9", (1920, 1080)),
    ("4K", (3840, 2160)),
    ("9:16", (1080, 1920)),
    ("1:1", (1080, 1080)),
    ("4:5", (1080, 1350)),
    ("4:3", (1440, 1080)),
];
/// Picture shapes, each with the size it has at 1080 on its short side.
pub(super) const RATIOS: [(&str, (usize, usize)); 7] = [
    ("16:9", (1920, 1080)),
    ("9:16", (1080, 1920)),
    ("1:1", (1080, 1080)),
    ("4:5", (1080, 1350)),
    ("4:3", (1440, 1080)),
    ("3:2", (1620, 1080)),
    ("21:9", (2520, 1080)),
];
const RATIO_NAMES: [&str; 7] = ["16:9 Landscape", "9:16 Vertical", "1:1 Square", "4:5 Portrait", "4:3 Classic", "3:2 Photo", "21:9 Cinema"];

/// A shape at the resolution the project already has: its long side stays as it was.
pub(super) fn reshape(shape: (usize, usize), now: (usize, usize)) -> (usize, usize) {
    let long = now.0.max(now.1).max(shape.0.max(shape.1)) as f64;
    let k = long / shape.0.max(shape.1) as f64;
    let even = |v: usize| ((v as f64 * k).round() as usize) & !1;
    (even(shape.0).max(2), even(shape.1).max(2))
}

/// Which shape a size is, if it is one of them.
pub(super) fn ratio_of(size: (usize, usize)) -> Option<usize> {
    let r = size.0 as f64 / size.1.max(1) as f64;
    RATIOS.iter().position(|(_, s)| (s.0 as f64 / s.1 as f64 - r).abs() < 0.01)
}

const TEXT_COLOURS: [[f32; 3]; 6] = [[1.0, 1.0, 1.0], [0.0, 0.0, 0.0], [1.0, 0.85, 0.1], [1.0, 0.25, 0.25], [0.2, 0.75, 1.0], [0.3, 0.9, 0.5]];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Rail {
    Media,
    Video,
    Photo,
    Audio,
    Text,
    Transitions,
    Effect,
    Sticker,
    Adjustment,
    Brand,
    Assistant,
    Settings,
}

impl Rail {
    pub const ALL: [Rail; 11] = [Rail::Media, Rail::Video, Rail::Photo, Rail::Audio, Rail::Text, Rail::Transitions, Rail::Effect, Rail::Sticker, Rail::Adjustment, Rail::Brand, Rail::Assistant];

    fn name(self) -> &'static str {
        match self {
            Rail::Media => "Media",
            Rail::Video => "Video",
            Rail::Photo => "Photo",
            Rail::Audio => "Audio",
            Rail::Text => "Text",
            Rail::Transitions => "Transitions",
            Rail::Effect => "Effect",
            Rail::Sticker => "Sticker",
            Rail::Adjustment => "Adjustment",
            Rail::Brand => "Brand",
            Rail::Assistant => "Assistant",
            Rail::Settings => "Settings",
        }
    }

    fn icon(self) -> Icon {
        match self {
            Rail::Media => Icon::Folder01,
            Rail::Video => Icon::Video01,
            Rail::Photo => Icon::Image01,
            Rail::Audio => Icon::MusicNote01,
            Rail::Text => Icon::Text,
            Rail::Transitions => Icon::TransitionLeft,
            Rail::Effect => Icon::Sparkles,
            Rail::Sticker => Icon::Sticker,
            Rail::Adjustment => Icon::SlidersHorizontal,
            Rail::Brand => Icon::PaintBoard,
            Rail::Assistant => Icon::MagicWand01,
            Rail::Settings => Icon::Settings01,
        }
    }
}

/// What a press on the timeline is holding.
#[derive(Clone, Copy, PartialEq)]
enum Hold {
    Head,
    Clip(usize),
    TrimIn(u32),
    TrimOut(u32),
    Item(u32),
    ItemIn(u32),
    ItemOut(u32),
}

pub struct EditState {
    pub rail: Rail,
    zoom: f32,
    scroll: f32,
    hold: Option<Hold>,
    /// Frames of drag not yet turned into a whole-frame change.
    spare: f32,
    carrying: Option<u32>,
    strips: HashMap<u32, Vec<TextureHandle>>,
    strip_tx: Sender<(u32, Vec<ColorImage>)>,
    strip_rx: Receiver<(u32, Vec<ColorImage>)>,
    /// Film strips waiting for one of the few workers that make them.
    strip_queue: Option<Sender<(u32, video::Info)>>,
    sort_by_name: bool,
    pub fullscreen: bool,
    pub(super) motion: super::motion::MotionState,
    pub(super) extras: super::extras::ExtrasState,
    pub(super) design: super::design::DesignState,
    pub(super) captions: super::captions::CaptionsState,
    pub(super) brand: super::brand::BrandState,
    pub(super) assistant: super::assistant::AssistantState,
    pub(super) klipy: super::klipy::KlipyState,
    pub(super) cuts: super::cuts::CutsState,
    pub(super) templates: super::templates::TemplatesState,
    pub(super) clips: super::clipsel::ClipSel,
    /// Edges, cuts and the playhead pull a dragged item to them.
    pub snap: bool,
    /// Where in a dragged item the pointer took hold, in frames from its start.
    grab: i64,
    /// What the drag is lined up with this frame, for the guide line.
    snapped: Option<u32>,
    /// Items selected together with the picked one (see `many.rs`).
    pub(super) group: Vec<u32>,
}

impl Default for EditState {
    fn default() -> Self {
        let (strip_tx, strip_rx) = mpsc::channel();
        Self {
            rail: Rail::Media,
            zoom: 1.0,
            scroll: 0.0,
            hold: None,
            spare: 0.0,
            carrying: None,
            strips: HashMap::new(),
            strip_tx,
            strip_rx,
            strip_queue: None,
            sort_by_name: false,
            fullscreen: false,
            motion: Default::default(),
            extras: Default::default(),
            design: Default::default(),
            captions: Default::default(),
            brand: Default::default(),
            assistant: Default::default(),
            klipy: Default::default(),
            cuts: Default::default(),
            templates: Default::default(),
            clips: Default::default(),
            snap: true,
            grab: 0,
            snapped: None,
            group: Vec::new(),
        }
    }
}

/// A ruler or drag label: minutes and seconds, with frames when they matter.
fn ruler_label(frame: u32, fps: u32, frames: bool) -> String {
    let fps = fps.max(1);
    let s = frame / fps;
    let base = if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{}:{:02}", s / 60, s % 60) };
    if frames { format!("{base}.{:02}", frame % fps) } else { base }
}

fn clock(frame: u32, fps: f64) -> String {
    let fps_whole = fps.round().max(1.0) as u32;
    let s = frame / fps_whole;
    format!("{:02}:{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60, frame % fps_whole)
}

fn colour_of(t: &Item) -> Color32 {
    match &t.body {
        Body::Audio { .. } if t.row == ROW_MUSIC => MUSIC,
        Body::Audio { .. } => SOUND,
        Body::Effect { kind: Fx::BlockFlashes | Fx::Strobe, .. } => FLASH,
        Body::Effect { .. } => EFFECT,
        Body::Overlay { .. } => OVERLAY,
        Body::Graphic { .. } => DESIGN,
        _ => TEXT,
    }
}

fn icon_of(t: &Item) -> Icon {
    match &t.body {
        Body::Audio { .. } if t.row == ROW_SOUND => Icon::VolumeHigh,
        Body::Audio { .. } => Icon::MusicNote01,
        Body::Effect { kind: Fx::BlockFlashes | Fx::Strobe, .. } => Icon::Flash,
        Body::Effect { .. } => Icon::TransitionLeft,
        Body::Text { .. } => Icon::Text,
        Body::Sticker { .. } => Icon::Sticker,
        Body::Graphic { .. } => Icon::Layers01,
        Body::Overlay { .. } => Icon::Video01,
    }
}

impl App {
    /// Starts reading a media's small pictures in the background.
    pub(super) fn want_strip(&mut self, id: u32) {
        let Some(m) = self.project.media_of(id).cloned() else { return };
        let tx = self.edit.strip_tx.clone();
        let (w, h) = STRIP_SIZE;
        match (m.kind, m.info) {
            (MediaKind::Video, Some(info)) => {
                // A queue with a few workers, not a decoder per file at once: a project of
                // 150 4K clips started 150 ffmpegs together and ran the computer out of memory.
                let queue = self.edit.strip_queue.get_or_insert_with(|| {
                    let (jobs, rx) = mpsc::channel::<(u32, video::Info)>();
                    let rx = std::sync::Arc::new(std::sync::Mutex::new(rx));
                    for _ in 0..STRIP_WORKERS {
                        let (rx, tx) = (rx.clone(), tx.clone());
                        std::thread::spawn(move || {
                            loop {
                                let job = rx.lock().ok().and_then(|r| r.recv().ok());
                                let Some((id, info)) = job else { return };
                                if let Ok(frames) = video::strip(&info, STRIP_COUNT, w, h) {
                                    let images = frames.iter().map(|f| ColorImage::from_rgb([w, h], f)).collect();
                                    if tx.send((id, images)).is_err() {
                                        return;
                                    }
                                }
                            }
                        });
                    }
                    jobs
                });
                let _ = queue.send((id, info));
            }
            (MediaKind::Photo, _) => {
                if let Some(p) = self.stills.get(&id).cloned() {
                    std::thread::spawn(move || {
                        let thumb = p.cover(w, h);
                        let _ = tx.send((id, vec![ColorImage::from_rgba_unmultiplied([w, h], &thumb.to_rgba8())]));
                    });
                }
            }
            _ => {}
        }
    }

    /// Whether every picture in the bin has its film strip yet.
    pub(super) fn strips_ready(&self) -> bool {
        self.project.media.iter().filter(|m| m.kind != MediaKind::Audio).all(|m| self.edit.strips.contains_key(&m.id))
    }

    fn take_strips(&mut self, ctx: &Context) {
        while let Ok((id, images)) = self.edit.strip_rx.try_recv() {
            let tex = images.into_iter().enumerate().map(|(i, img)| ctx.load_texture(format!("strip-{id}-{i}"), img, TextureOptions::LINEAR)).collect();
            self.edit.strips.insert(id, tex);
        }
    }

    pub(super) fn edit_layout(&mut self, ui: &mut Ui, ctx: &Context, screen: Rect) -> Rect {
        self.take_strips(ctx);
        self.poll_captions(ctx);
        self.poll_assistant(ctx);
        if self.edit.fullscreen {
            // The picture and its controls alone, as large as the screen allows.
            return self.preview_area(ui, screen.shrink(4.0));
        }
        let full = screen.shrink(8.0);
        let rail = Rect::from_min_max(full.min, pos2(full.left() + RAIL_W, full.bottom()));
        let body = Rect::from_min_max(pos2(rail.right() + 8.0, full.top()), full.max);
        let header = Rect::from_min_size(body.min, vec2(body.width(), HEADER_H));
        let least_lower = RULER_H + TOOLBAR_H + 140.0;
        let lower_h = self.settings.layout.lower_h.unwrap_or((body.height() * 0.44).clamp(RULER_H + TOOLBAR_H + 280.0, 400.0)).clamp(least_lower, (body.height() - HEADER_H - 220.0).max(least_lower));
        let lower_h = if self.settings.layout.hide_timeline { 0.0 } else { lower_h };
        let lower = Rect::from_min_max(pos2(body.left(), body.bottom() - lower_h), body.max);
        let mid = Rect::from_min_max(pos2(body.left(), header.bottom() + 6.0), pos2(body.right(), lower.top() - if self.settings.layout.hide_timeline { 0.0 } else { 8.0 }));
        let bin_w = self.settings.layout.bin_w.unwrap_or((mid.width() * 0.36).clamp(330.0, 480.0)).clamp(LEAST_BIN, (mid.width() - LEAST_VIEW).max(LEAST_BIN));
        let (bin, view) = if self.settings.layout.inspector_right {
            let bin = Rect::from_min_max(pos2(mid.right() - bin_w, mid.top()), mid.max);
            (bin, Rect::from_min_max(mid.min, pos2(bin.left() - 8.0, mid.bottom())))
        } else {
            let bin = Rect::from_min_max(mid.min, pos2(mid.left() + bin_w, mid.bottom()));
            (bin, Rect::from_min_max(pos2(bin.right() + 8.0, mid.top()), mid.max))
        };

        self.rail(ui, rail);
        self.header(ui, header);
        self.page(ui, ctx, bin);
        let photo = self.preview_area(ui, view);
        if !self.settings.layout.hide_timeline {
            self.timeline(ui, lower);
            self.carry_ghost(ui, lower);
        }
        self.splitters(ui, mid, bin, lower);
        photo
    }

    /// Bars in the gaps between panels. Drag one to resize; double-click to put it back.
    fn splitters(&mut self, ui: &mut Ui, mid: Rect, bin: Rect, lower: Rect) {
        let side_x = if self.settings.layout.inspector_right { bin.left() - 8.0 } else { bin.right() };
        let side = Rect::from_min_max(pos2(side_x, mid.top()), pos2(side_x + 8.0, mid.bottom()));
        let under = Rect::from_min_max(pos2(mid.left(), mid.bottom()), pos2(mid.right(), lower.top()));
        let mut changed = false;
        for (r, across) in [(side, true), (under, false)] {
            if !across && self.settings.layout.hide_timeline { continue; }
            let resp = ui.interact(r, ui.scope_id().with(("split", across)), Sense::click_and_drag())
                .on_hover_text("Drag to resize · double-click to reset");
            let grip = if across { Rect::from_center_size(r.center(), vec2(2.0, 28.0)) } else { Rect::from_center_size(r.center(), vec2(34.0, 2.0)) };
            ui.painter().rect_filled(grip, 1, BUTTON_EDGE);
            if resp.hovered() || resp.dragged() {
                ui.ctx().set_cursor_icon(if across { egui::CursorIcon::ResizeHorizontal } else { egui::CursorIcon::ResizeVertical });
                let bar = if across { Rect::from_center_size(r.center(), vec2(3.0, r.height() * 0.3)) } else { Rect::from_center_size(r.center(), vec2(r.width() * 0.1, 3.0)) };
                ui.painter().rect_filled(bar, 2, ACCENT);
            }
            let d = resp.drag_delta();
            let layout = &mut self.settings.layout;
            if resp.dragged() {
                if across {
                    layout.bin_w = Some(bin.width() + if layout.inspector_right { -d.x } else { d.x });
                } else {
                    layout.lower_h = Some(lower.height() - d.y);
                }
            }
            if resp.double_clicked() {
                if across {
                    layout.bin_w = None;
                } else {
                    layout.lower_h = None;
                }
            }
            changed |= resp.drag_stopped() || resp.double_clicked();
        }
        if changed {
            crate::store::save_settings(&self.settings);
        }
    }

    /// Track heights, after the person's own stretch.
    fn row_heights(&self) -> [f32; ROWS] {
        let k = self.settings.layout.rows.unwrap_or(1.0).clamp(LEAST_ROWS, MOST_ROWS);
        ROW_H.map(|h| h * k)
    }

    fn rail(&mut self, ui: &mut Ui, r: Rect) {
        card(ui, r);
        // Every page and Settings fit the window's height, at most 72 points each.
        let cell = ((r.height() - 12.0) / (Rail::ALL.len() + 1) as f32).clamp(54.0, 72.0);
        let mut y = r.top() + 6.0;
        let put = |app: &mut Self, ui: &mut Ui, tab: Rail, y: f32| {
            let b = Rect::from_min_size(pos2(r.left() + 6.0, y), vec2(r.width() - 12.0, cell - 6.0));
            let resp = ui.interact(b, ui.scope_id().with(("rail", tab.name())), Sense::click());
            if resp.clicked() {
                app.edit.rail = tab;
            }
            let on = app.edit.rail == tab;
            let p = ui.painter();
            if on || resp.hovered() {
                p.rect_filled(b, 4, BUTTON);
            }
            if on {
                p.rect_stroke(b, 4, Stroke::new(1.0, BUTTON_EDGE), StrokeKind::Inside);
            }
            let tint = if on { Color32::WHITE } else { Color32::from_rgb(0xA8, 0xA8, 0xAC) };
            icons::paint(p, tab.icon(), Rect::from_center_size(pos2(b.center().x, b.top() + b.height() * 0.36), Vec2::splat(21.0)), tint);
            p.text(pos2(b.center().x, b.top() + b.height() * 0.74), Align2::CENTER_CENTER, tab.name(), semi(11.5), tint);
        };
        for tab in Rail::ALL {
            put(self, ui, tab, y);
            y += cell;
        }
        put(self, ui, Rail::Settings, (r.bottom() - cell).max(y));
    }

    fn header(&mut self, ui: &mut Ui, r: Rect) {
        card(ui, r);
        let y = r.center().y;
        let back = Rect::from_center_size(pos2(r.left() + 22.0, y), Vec2::splat(30.0));
        if icon_button(ui, back, Icon::ArrowLeft01, "Open, save and batch", false).clicked() {
            self.sheet = Some(Sheet::Home);
        }
        let layout_button = Rect::from_min_size(pos2(r.left() + 137.0, y - 14.0), vec2(68.0, 28.0));
        if action(ui, layout_button, "Layout", false, 12.0).clicked() { self.edit.rail = Rail::Settings; }
        let p = ui.painter();
        icons::paint(p, Icon::Cloud, Rect::from_center_size(pos2(r.left() + 52.0, y), Vec2::splat(18.0)), MUTED);
        let state = if self.history_at == 0 { "No changes" } else { "Edited" };
        p.text(pos2(r.left() + 66.0, y), Align2::LEFT_CENTER, state, regular(13.0), MUTED);
        let name = short(&self.project.name, 40);
        icons::paint(p, Icon::Copy01, Rect::from_center_size(pos2(r.center().x - text_width(ui, &name, semi(14.0)) / 2.0 - 16.0, y), Vec2::splat(17.0)), Color32::WHITE);
        p.text(r.center(), Align2::CENTER_CENTER, &name, semi(14.0), Color32::WHITE);
        let label = "Export";
        let w = text_width(ui, label, semi(13.0)) + 46.0;
        let b = Rect::from_min_size(pos2(r.right() - w - 8.0, y - 16.0), vec2(w, 32.0));
        // The picture's shape, and whether clips fill it or fit inside it.
        let fill_b = Rect::from_min_size(pos2(b.left() - 64.0, y - 16.0), vec2(56.0, 32.0));
        let fill = self.project.fill;
        if pill(ui, fill_b, if fill { "Fill" } else { "Fit" }, false, 12.5).on_hover_text(if fill { "Clips fill the frame, edges cropped. Click to fit them inside with bars." } else { "Clips fit inside the frame with bars. Click to fill it, cropping the edges." }).clicked() {
            self.project.fill = !fill;
        }
        let ratio_b = Rect::from_min_size(pos2(fill_b.left() - 158.0, y - 16.0), vec2(150.0, 32.0));
        let mut at = ratio_of(self.project.size).unwrap_or(usize::MAX);
        if dropdown(ui, ratio_b, None, &RATIO_NAMES, &mut at) && at < RATIOS.len() {
            self.project.size = reshape(RATIOS[at].1, self.project.size);
        }
        let resp = ui.interact(b, ui.scope_id().with("export"), Sense::click());
        if resp.clicked() {
            self.sheet = Some(Sheet::Export);
        }
        let p = ui.painter();
        p.rect_stroke(b, 4, Stroke::new(1.0, if resp.hovered() { Color32::WHITE } else { BUTTON_EDGE }), StrokeKind::Inside);
        p.text(pos2(b.left() + 12.0, y), Align2::LEFT_CENTER, label, semi(13.0), Color32::WHITE);
        icons::paint(p, Icon::Upload01, Rect::from_center_size(pos2(b.right() - 16.0, y), Vec2::splat(17.0)), Color32::WHITE);
    }

    fn page(&mut self, ui: &mut Ui, ctx: &Context, r: Rect) {
        if self.edit.rail == Rail::Adjustment {
            let tools = Rect::from_min_max(pos2(r.left(), r.bottom() - super::BOTTOM_H), r.max);
            let top = Rect::from_min_max(r.min, pos2(r.right(), tools.top() - 8.0));
            self.content_panel(ui, ctx, top);
            self.tool_panel(ui, tools);
            return;
        }
        card(ui, r);
        let inner = r.shrink2(vec2(14.0, 12.0));
        ui.scope_builder(UiBuilder::new().max_rect(inner).id_salt(("page", self.edit.rail.name())), |ui| {
            ui.set_clip_rect(inner.intersect(ui.clip_rect()));
            match self.edit.rail {
                Rail::Media | Rail::Video | Rail::Photo => self.bin(ui),
                Rail::Audio => self.audio_page(ui),
                other => {
                    // Text and Captions scroll apart, so Text opens at its top after a long Captions page.
                    let tab = if other == Rail::Text { self.edit.captions.tab } else { 0 };
                    egui::ScrollArea::vertical().id_salt(("page-scroll", other.name(), tab)).auto_shrink([false, false]).show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 4.0;
                        ui.set_width(ui.available_width() - 6.0);
                        let before = self.picked_body();
                        // Text shows it under its tabs, as the Captions tab edits nothing.
                        if matches!(other, Rail::Effect | Rail::Sticker) {
                            self.group_banner(ui);
                        }
                        match other {
                            Rail::Text => self.text_page(ui),
                            Rail::Transitions => self.transitions_page(ui),
                            Rail::Effect => self.effects_page(ui),
                            Rail::Sticker => self.sticker_page(ui),
                            Rail::Brand => self.brand_page(ui),
                            Rail::Assistant => self.assistant_page(ui),
                            _ => self.settings_page(ui),
                        }
                        self.spread_picked(before);
                    });
                }
            }
        });
    }

    pub(super) fn upload(&mut self) {
        let mut all: Vec<&str> = crate::export::PHOTO_EXTENSIONS.to_vec();
        all.extend(video::EXTENSIONS);
        all.extend(video::SOUND_EXTENSIONS);
        if let Some(files) = rfd::FileDialog::new().add_filter("Photos, videos and sound", &all).pick_files() {
            for f in files {
                self.import(&f, false);
            }
        }
    }

    /// The bin's sound files, or the sound effects library.
    fn audio_page(&mut self, ui: &mut Ui) {
        self.clip_sound_section(ui);
        let mut tab = self.edit.extras.sound_tab;
        segmented(ui, &["My Audio", "Sound Effects"], &mut tab);
        self.edit.extras.sound_tab = tab;
        if tab == 0 {
            self.bin(ui);
        } else {
            self.sound_library(ui);
        }
    }

    fn bin(&mut self, ui: &mut Ui) {
        let w = ui.available_width();
        let (row, _) = ui.allocate_exact_size(vec2(w, 34.0), Sense::hover());
        let b = Rect::from_min_size(row.min, vec2(96.0, 34.0));
        let resp = ui.interact(b, ui.scope_id().with("upload"), Sense::click());
        let p = ui.painter();
        p.rect_filled(b, 4, if resp.hovered() { BUTTON_EDGE } else { BUTTON });
        icons::paint(p, Icon::Upload01, Rect::from_center_size(pos2(b.left() + 18.0, b.center().y), Vec2::splat(17.0)), Color32::WHITE);
        p.text(pos2(b.left() + 34.0, b.center().y), Align2::LEFT_CENTER, "Upload", semi(13.0), Color32::WHITE);
        if resp.clicked() {
            self.upload();
        }
        let sort = Rect::from_min_size(pos2(row.right() - 76.0, row.top()), vec2(76.0, 34.0));
        let label = if self.edit.sort_by_name { "Name" } else { "Added" };
        if action(ui, sort, label, false, 12.5).on_hover_text("Sort").clicked() {
            self.edit.sort_by_name = !self.edit.sort_by_name;
        }
        ui.add_space(8.0);

        let want = match self.edit.rail {
            Rail::Video => Some(MediaKind::Video),
            Rail::Photo => Some(MediaKind::Photo),
            Rail::Audio => Some(MediaKind::Audio),
            _ => None,
        };
        let mut list: Vec<_> = self.project.media.iter().filter(|m| want.is_none_or(|k| m.kind == k)).cloned().collect();
        if self.edit.sort_by_name {
            list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        }
        if list.is_empty() {
            note(ui, "Upload files, or drop them anywhere on this window.");
            return;
        }
        let mut add = None;
        egui::ScrollArea::vertical().id_salt("bin").auto_shrink([false, false]).show(ui, |ui| {
            let gap = 10.0;
            let w = (ui.available_width() - gap - 6.0) / 2.0;
            let h = w * 0.56;
            for pair in list.chunks(2) {
                let (row, _) = ui.allocate_exact_size(vec2(w * 2.0 + gap, h + 22.0), Sense::hover());
                for (i, m) in pair.iter().enumerate() {
                    let r = Rect::from_min_size(pos2(row.left() + (w + gap) * i as f32, row.top()), vec2(w, h));
                    let resp = ui.interact(r, ui.scope_id().with(("media", m.id)), Sense::click_and_drag());
                    let p = ui.painter();
                    match self.edit.strips.get(&m.id).and_then(|s| s.first()) {
                        Some(t) => {
                            egui::Image::new((t.id(), r.size())).corner_radius(6).paint_at(ui, r);
                        }
                        None => {
                            p.rect_filled(r, 6, BUTTON);
                            let icon = if m.kind == MediaKind::Audio { Icon::MusicNote01 } else { Icon::Image01 };
                            icons::paint(p, icon, Rect::from_center_size(r.center(), Vec2::splat(26.0)), MUTED);
                        }
                    }
                    let p = ui.painter();
                    if resp.hovered() {
                        p.rect_stroke(r, 6, Stroke::new(1.5, Color32::WHITE), StrokeKind::Inside);
                        let plus = pos2(r.right() - 16.0, r.top() + 16.0);
                        p.circle_filled(plus, 11.0, ACCENT);
                        icons::paint(p, Icon::Add01, Rect::from_center_size(plus, Vec2::splat(15.0)), Color32::WHITE);
                    }
                    p.text(pos2(r.center().x, r.bottom() + 11.0), Align2::CENTER_CENTER, short(&m.name, 24), semi(11.5), TEXT_2);
                    if resp.clicked() {
                        add = Some(m.id);
                    }
                    if resp.drag_started() {
                        self.edit.carrying = Some(m.id);
                    }
                }
                ui.add_space(4.0);
            }
        });
        if let Some(id) = add {
            self.place(id, None);
        }
    }

    pub(super) fn picked_item(&mut self) -> Option<&mut Item> {
        let Some(Picked::Item(id)) = self.picked else { return None };
        self.project.items.iter_mut().find(|t| t.id == id)
    }

    fn add_item(&mut self, row: usize, seconds: f64, body: Body) {
        let (at, len, id) = (self.head, self.project.frames(seconds), self.project.new_id());
        self.project.items.push(Item { id, row, at, len, body });
        self.picked = Some(Picked::Item(id));
    }

    fn text_page(&mut self, ui: &mut Ui) {
        let mut tab = self.edit.captions.tab;
        segmented(ui, &["Text", "Templates", "Captions"], &mut tab);
        self.edit.captions.tab = tab;
        if tab == 1 {
            return self.templates_page(ui);
        }
        if tab == 2 {
            return self.captions_page(ui);
        }
        self.group_banner(ui);
        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(vec2(w, 38.0), Sense::hover());
        if action(ui, r, "Add Text", true, 13.5).clicked() {
            self.add_title();
        }
        ui.add_space(8.0);
        // One caption on its own: a step to all of them, which is what's usually wanted.
        let captions = self.project.items.iter().filter(|t| super::captions::is_caption(t)).count();
        let lone = self.selection().len() < 2 && self.picked_item().is_some_and(|t| super::captions::is_caption(t));
        if lone && captions > 1 {
            let (r, _) = ui.allocate_exact_size(vec2(w, 32.0), Sense::hover());
            if action(ui, r, &format!("Edit All {captions} Captions"), false, 12.5).on_hover_text("Select every caption, so a change here reaches them all").clicked() {
                self.select_captions();
            }
            ui.add_space(4.0);
        }
        let palette = self.project_brand().map(|b| b.colours.clone()).unwrap_or_default();
        let Some(t) = self.picked_item() else {
            note(ui, "Choose a text item on the timeline to change it. Ctrl or Shift and a click selects more; Select All, below the timeline, takes every one.");
            return;
        };
        let Body::Text { text, size, colour, .. } = &mut t.body else {
            note(ui, "Choose a text item on the timeline to change it.");
            return;
        };
        ui.add(egui::TextEdit::multiline(text).desired_rows(2).desired_width(f32::INFINITY));
        ui.add_space(4.0);
        slider(ui, "Size", size, 24.0, 320.0, 96.0, Fmt::Int);
        let dots: Vec<Color32> = TEXT_COLOURS.iter().map(|c| Color32::from_rgb((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8)).collect();
        let mut at = TEXT_COLOURS.iter().position(|c| c == colour).unwrap_or(usize::MAX);
        let was = at;
        swatches(ui, &dots, &mut at);
        if at != was && at < TEXT_COLOURS.len() {
            *colour = TEXT_COLOURS[at];
        }
        if !palette.is_empty() {
            ui.label(egui::RichText::new("Brand colours").font(regular(11.5)).color(MUTED));
            let dots: Vec<Color32> = palette.iter().map(|c| Color32::from_rgb((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8)).collect();
            let mut at = palette.iter().position(|c| c == colour).unwrap_or(usize::MAX);
            let was = at;
            swatches(ui, &dots, &mut at);
            if at != was && at < palette.len() {
                *colour = palette[at];
            }
        }
        self.style_section(ui);
        self.font_section(ui);
        self.animation_section(ui);
        self.keyframe_section(ui);
    }

    fn preview_button(&mut self, ui: &mut Ui) {
        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(vec2(w, 34.0), Sense::hover());
        if action(ui, r, "Preview on the Picture (P)", true, 12.5).clicked() {
            self.preview_fx();
        }
        ui.add_space(4.0);
    }

    fn transitions_page(&mut self, ui: &mut Ui) {
        let mut tab = self.edit.cuts.tab;
        segmented(ui, &["Transitions", "Cuts"], &mut tab);
        self.edit.cuts.tab = tab;
        if tab == 1 {
            return self.cuts_page(ui);
        }
        let Some(cut) = self.project.nearest_cut(self.head) else {
            note(ui, "Add a second clip to apply a transition.");
            self.transition_tiles(ui, None);
            return;
        };
        let now = self.project.clips[cut].transition;
        if let Some(t) = now {
            ui.label(egui::RichText::new(t.kind.name()).font(semi(13.0)).color(Color32::WHITE));
            let fps = self.project.fps as f32;
            let mut s = t.len as f32 / fps;
            slider(ui, "Seconds", &mut s, 0.1, 3.0, 1.0, Fmt::Plain);
            self.project.set_transition(cut, t.kind, (s * fps).round() as u32);
            let (preview, remove) = split_row(ui, 28.0, 6.0);
            if action(ui, preview, "Preview (P)", true, 12.5).clicked() {
                self.preview_fx();
            }
            if action(ui, remove, "Remove Transition", false, 12.5).clicked() {
                self.project.clips[cut].transition = None;
                return;
            }
        } else {
            note(ui, "Applies to the cut nearest the playhead.");
        }
        if let Some(k) = self.transition_tiles(ui, now.map(|t| t.kind)) {
            let len = now.map_or(self.project.frames(1.0), |t| t.len);
            self.project.set_transition(cut, k, len);
        }
    }

    fn effects_page(&mut self, ui: &mut Ui) {
        let now = match self.picked_item().map(|t| &t.body) {
            Some(Body::Effect { kind, .. }) => Some(*kind),
            _ => None,
        };
        if let Some(t) = self.picked_item()
            && let Body::Effect { kind, amount } = &mut t.body
        {
            ui.label(egui::RichText::new(kind.name()).font(semi(13.0)).color(Color32::WHITE));
            slider(ui, "Amount", amount, 0.0, 1.0, 1.0, Fmt::Percent);
            self.preview_button(ui);
        }
        note(ui, "Point at a tile to watch it on this frame. Click one to add it at the playhead; drag it on the timeline to move it, or its ends to resize.");
        if let Some(kind) = self.effect_tiles(ui, now) {
            let row = if matches!(kind, Fx::BlockFlashes | Fx::Strobe) { ROW_FX } else { ROW_FX2 };
            self.add_item(row, crate::timeline::ITEM_SECONDS, Body::Effect { kind, amount: 1.0 });
        }
    }

    fn sticker_page(&mut self, ui: &mut Ui) {
        match self.sticker_tabs(ui) {
            0 => self.emoji_library(ui),
            1 => self.icon_stickers(ui),
            2 => return self.meme_library(ui),
            3 => return self.clip_library(ui),
            _ => return self.layers_panel(ui),
        }
        ui.add_space(6.0);
        if let Some(t) = self.picked_item()
            && let Body::Sticker { size, .. } = &mut t.body
        {
            slider(ui, "Size", size, 40.0, 700.0, 220.0, Fmt::Int);
            self.animation_section(ui);
            self.keyframe_section(ui);
        }
    }

    fn icon_stickers(&mut self, ui: &mut Ui) {
        let w = ui.available_width();
        let cell = w / 4.0;
        let rows = STICKERS.len().div_ceil(4);
        let (r, _) = ui.allocate_exact_size(vec2(w, cell * rows as f32), Sense::hover());
        let mut chosen = None;
        for (i, icon) in STICKERS.iter().enumerate() {
            let c = Rect::from_min_size(pos2(r.left() + cell * (i % 4) as f32, r.top() + cell * (i / 4) as f32), Vec2::splat(cell)).shrink(5.0);
            let resp = ui.interact(c, ui.scope_id().with(("sticker", i)), Sense::click());
            let p = ui.painter();
            p.rect_filled(c, 4, if resp.hovered() { BUTTON_EDGE } else { BUTTON });
            icons::paint(p, *icon, Rect::from_center_size(c.center(), Vec2::splat(c.width() * 0.5)), Color32::WHITE);
            if resp.clicked() {
                chosen = Some(i);
            }
        }
        if let Some(icon) = chosen {
            self.add_item(ROW_FX2, crate::timeline::ITEM_SECONDS, Body::Sticker { icon, emoji: String::new(), size: 220.0, x: 0.5, y: 0.5, motion: Default::default() });
        }
    }

    fn workspace_settings(&mut self, ui: &mut Ui) {
        use crate::store::Layout;
        ui.strong("Workspace layout");
        note(ui, "Drag the dividers to resize panels. Changes are remembered on this computer.");
        let mut save = false;
        ui.horizontal_wrapped(|ui| {
            if ui.button("Editing").clicked() { self.settings.layout = Layout::default(); save = true; }
            if ui.button("Color").clicked() { self.settings.layout = Layout { inspector_right: true, hide_timeline: true, bin_w: Some(420.0), ..Default::default() }; save = true; }
            if ui.button("Motion").clicked() { self.settings.layout = Layout { inspector_right: true, lower_h: Some(360.0), layers_w: Some(240.0), rows: Some(1.25), ..Default::default() }; save = true; }
        });
        let layout = &mut self.settings.layout;
        save |= ui.checkbox(&mut layout.inspector_right, "Inspector on the right").changed();
        let mut timeline = !layout.hide_timeline;
        if ui.checkbox(&mut timeline, "Show timeline").changed() { layout.hide_timeline = !timeline; save = true; }
        let mut layers = !layout.hide_layers;
        if ui.add_enabled(timeline, egui::Checkbox::new(&mut layers, "Show layer stack")).changed() { layout.hide_layers = !layers; save = true; }
        let mut width = layout.layers_w.unwrap_or(TREE_W);
        if slider(ui, "Layer stack width", &mut width, 140.0, 320.0, TREE_W, Fmt::Int).changed() { layout.layers_w = Some(width); save = true; }
        ui.horizontal_wrapped(|ui| {
            if ui.button("Save my layout").clicked() { self.settings.saved_layout = Some(self.settings.layout.clone()); save = true; }
            if ui.add_enabled(self.settings.saved_layout.is_some(), egui::Button::new("Restore my layout")).clicked()
                && let Some(saved) = &self.settings.saved_layout { self.settings.layout = saved.clone(); save = true; }
            if ui.button("Reset layout").clicked() { self.settings.layout = Layout::default(); save = true; }
        });
        if save { crate::store::save_settings(&self.settings); }
    }

    fn settings_page(&mut self, ui: &mut Ui) {
        self.workspace_settings(ui);
        ui.separator();
        ui.label(egui::RichText::new("Project Name").font(semi(13.0)).color(Color32::WHITE));
        ui.add(egui::TextEdit::singleline(&mut self.project.name).desired_width(f32::INFINITY));
        ui.add_space(8.0);
        ui.label(egui::RichText::new("Picture Size").font(semi(13.0)).color(Color32::WHITE));
        let size = self.project.size;
        let mut chips = Chips::new(ui, 30.0);
        let mut chosen = None;
        for (label, s) in CANVASES {
            if chips.add(label, size == s) {
                chosen = Some(s);
            }
        }
        chips.end();
        if let Some(s) = chosen {
            self.project.size = s;
        }
        note(ui, &format!("{} × {}, {:.2} frames a second.", size.0, size.1, self.project.fps));
        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(vec2(w, 34.0), Sense::hover());
        if action(ui, r, "Keyboard Shortcuts (F1)", false, 13.0).clicked() {
            self.sheet = Some(Sheet::Keys);
        }
        ui.add_space(4.0);
        if !video::available() {
            note(ui, &format!("Video needs ffmpeg. {}", crate::platform::ffmpeg_hint()));
        }
        if self.audio_out.is_none() {
            note(ui, "No sound device was found, so playback is silent.");
        }
        self.plugins_section(ui);
        ui.add_space(8.0);
        ui.label(egui::RichText::new("KLIPY").font(semi(13.0)).color(Color32::WHITE));
        if self.settings.klipy_key.is_empty() {
            note(ui, if crate::klipy::key("").is_some() { "Using the key in KLIPY_API_KEY." } else { "No key yet. Paste one on Sticker, Clips, KLIPY." });
        } else {
            note(ui, "A key is saved on this computer for video memes.");
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::hover());
            if action(ui, r, "Remove KLIPY Key", false, 12.5).clicked() {
                self.settings.klipy_key.clear();
                crate::store::save_settings(&self.settings);
            }
        }
        ui.add_space(8.0);
        self.credits_section(ui);
        ui.add_space(8.0);
        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(vec2(w, 34.0), Sense::hover());
        if action(ui, r, "Save Project", false, 13.0).clicked() {
            self.save_project();
        }
        ui.add_space(4.0);
        let (r, _) = ui.allocate_exact_size(vec2(w, 34.0), Sense::hover());
        if action(ui, r, "Open Project", false, 13.0).clicked() {
            self.open_project();
        }
    }

    /// Picture, sound meter and transport. Returns the area the photo was drawn in.
    fn preview_area(&mut self, ui: &mut Ui, r: Rect) -> Rect {
        card(ui, r);
        let meter = Rect::from_min_max(r.min, pos2(r.left() + 40.0, r.bottom() - 46.0));
        let photo = Rect::from_min_max(pos2(meter.right(), r.top() + 2.0), pos2(r.right() - 2.0, r.bottom() - 46.0));
        let bar = Rect::from_min_max(pos2(r.left(), photo.bottom()), r.max);
        self.photo(ui, photo);
        // Layers move and resize on the picture, except while masks or the crop use it.
        if self.edit.rail != Rail::Adjustment || !matches!(self.tool, Tool::Masks | Tool::Crop) {
            self.layer_handles(ui, photo);
        }
        if !matches!(self.tool, Tool::Masks | Tool::Crop) || self.edit.rail != Rail::Adjustment {
            self.original_button(ui, photo);
        } else {
            self.hold_original = false;
        }

        let p = ui.painter();
        p.vline(meter.right(), meter.y_range(), Stroke::new(1.0, PANEL_EDGE));
        p.hline(r.x_range(), bar.top(), Stroke::new(1.0, PANEL_EDGE));
        let level = self.audio_out.as_ref().map_or(0.0, |o| o.level()).sqrt();
        if let Some(o) = self.audio_out.as_ref().filter(|_| self.run.is_none()) {
            o.fade_level();
        }
        let track = Rect::from_center_size(pos2(meter.center().x, meter.center().y - 6.0), vec2(6.0, meter.height() - 62.0));
        p.rect_filled(track, 3, TRACK);
        const CELLS: i32 = 40;
        let lit = (level * CELLS as f32) as i32;
        for i in 0..lit.min(CELLS) {
            let t = i as f32 / CELLS as f32;
            let col = if t > 0.9 { Color32::from_rgb(235, 80, 60) } else if t > 0.65 { Color32::from_rgb(220, 205, 40) } else { Color32::from_rgb(90, 200, 70) };
            let h = track.height() / CELLS as f32;
            let c = Rect::from_min_size(pos2(track.left(), track.bottom() - h * (i + 1) as f32), vec2(track.width(), h));
            p.rect_filled(c, 0, col);
        }
        icons::paint(p, Icon::VolumeHigh, Rect::from_center_size(pos2(meter.center().x, meter.bottom() - 18.0), Vec2::splat(18.0)), Color32::WHITE);

        let y = bar.center().y;
        let total = self.project.total();
        let count = format!("{}/{}", self.head, total);
        p.text(pos2(bar.left() + 14.0, y), Align2::LEFT_CENTER, count, semi(14.0), Color32::WHITE);
        let fps = self.project.fps;
        let cx = bar.center().x;
        let at = |n: f32| Rect::from_center_size(pos2(cx + n * 44.0, y), Vec2::splat(36.0));
        let nudge = self.project.frames(NUDGE_SECONDS);
        if icon_button(ui, at(-2.0), Icon::GoBackward5Sec, "Back five seconds", false).clicked() {
            self.seek_head(self.head.saturating_sub(nudge));
        }
        if icon_button(ui, at(-1.0), Icon::Previous, "Back one frame", false).clicked() {
            self.seek_head(self.head.saturating_sub(1));
        }
        let playing = self.run.is_some();
        if play_button(ui, at(0.0).center(), playing).clicked() {
            self.play_or_pause("the play button");
        }
        if icon_button(ui, at(1.0), Icon::Next, "Forward one frame", false).clicked() {
            self.seek_head(self.head + 1);
        }
        if icon_button(ui, at(2.0), Icon::GoForward5Sec, "Forward five seconds", false).clicked() {
            self.seek_head(self.head + nudge);
        }
        ui.painter().text(pos2(bar.right() - 56.0, y), Align2::RIGHT_CENTER, clock(self.head, fps), regular(12.5), MUTED);
        let end = Rect::from_center_size(pos2(bar.right() - 26.0, y), Vec2::splat(34.0));
        let tip = if self.edit.fullscreen { "Leave full screen (Esc or F)" } else { "Full screen (F, or double-click the picture)" };
        if icon_button(ui, end, Icon::ArrowExpand01, tip, self.edit.fullscreen).clicked() {
            self.set_fullscreen(ui.ctx(), !self.edit.fullscreen);
        }
        // Masks and crop use the picture for their own handles, so they keep double-clicks.
        let tools = self.edit.rail == Rail::Adjustment && matches!(self.tool, Tool::Masks | Tool::Crop);
        if !tools && ui.input(|i| i.pointer.button_double_clicked(egui::PointerButton::Primary)) && ui.rect_contains_pointer(photo) {
            self.set_fullscreen(ui.ctx(), !self.edit.fullscreen);
        }
        photo
    }

    /// The picture alone, filling the screen, or back to the editor.
    pub(super) fn set_fullscreen(&mut self, ctx: &Context, on: bool) {
        self.edit.fullscreen = on;
        ctx.send_viewport_cmd(ViewportCommand::Fullscreen(on));
    }

    fn track_heads(&mut self, ui: &mut Ui, r: Rect, ys: &[f32; ROWS]) {
        let rh = self.row_heights();
        // The gap under the video track stretches every track.
        let gap = Rect::from_min_max(pos2(r.left(), ys[ROW_VIDEO] + rh[ROW_VIDEO] - 2.0), pos2(r.right(), ys[ROW_SOUND] + 2.0));
        let resp = ui.interact(gap, ui.scope_id().with("track-height"), Sense::click_and_drag()).on_hover_text("Drag to make tracks taller or shorter. Double-click to reset.");
        if resp.hovered() || resp.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
            ui.painter().hline(r.left() + 8.0..=r.right() - 8.0, gap.center().y, Stroke::new(2.0, ACCENT));
        }
        if resp.dragged() {
            let k = ((rh[ROW_VIDEO] + resp.drag_delta().y) / ROW_H[ROW_VIDEO]).clamp(LEAST_ROWS, MOST_ROWS);
            self.settings.layout.rows = Some(k);
        }
        if resp.double_clicked() {
            self.settings.layout.rows = None;
        }
        if resp.drag_stopped() || resp.double_clicked() {
            crate::store::save_settings(&self.settings);
        }
        for (row, y) in ys.iter().enumerate() {
            let line = Rect::from_min_size(pos2(r.left(), *y), vec2(r.width(), rh[row]));
            let cell = 34.0;
            let x = |n: f32| Rect::from_center_size(pos2(r.left() + 24.0 + n * cell, line.center().y), Vec2::splat(30.0));
            let t = self.project.tracks[row];
            let lock = if t.locked { Icon::Locked } else { Icon::SquareUnlock02 };
            if icon_button(ui, x(0.0), lock, "Lock this track", t.locked).clicked() {
                self.project.tracks[row].locked ^= true;
            }
            let kind = if row == ROW_VIDEO { Icon::Video01 } else { Icon::MusicNote01 };
            icons::paint(ui.painter(), kind, Rect::from_center_size(x(1.0).center(), Vec2::splat(17.0)), MUTED);
            let sound = if t.muted { Icon::VolumeOff } else { Icon::VolumeHigh };
            if icon_button(ui, x(2.0), sound, "Mute this track", t.muted).clicked() {
                self.project.tracks[row].muted ^= true;
            }
            let eye = if t.hidden { Icon::ViewOff } else { Icon::View };
            if icon_button(ui, x(3.0), eye, "Hide this track", t.hidden).clicked() {
                self.project.tracks[row].hidden ^= true;
            }
            if row + 1 < ROWS {
                ui.painter().hline(r.left() + 8.0..=r.right() - 8.0, line.bottom() + ROW_GAP * 0.5, Stroke::new(1.0, PANEL_EDGE));
            }
        }
    }

    fn tree(&mut self, ui: &mut Ui, r: Rect) {
        ui.painter().vline(r.right(), r.y_range(), Stroke::new(1.0, PANEL_EDGE));
        let items: Vec<_> = self.project.items.iter().filter(|t| t.body.placed().is_some() && self.head >= t.at && self.head < t.end())
            .map(|t| (t.id, t.label(), icon_of(t), t.body.placed().is_some_and(|p| p.2.hidden))).rev().collect();
        let mut picked = None;
        let mut visibility = None;
        ui.scope_builder(UiBuilder::new().max_rect(r.shrink(8.0)).id_salt("layer-stack"), |ui| {
            ui.set_clip_rect(r.intersect(ui.clip_rect()));
            ui.horizontal(|ui| {
                ui.strong("Layers");
                if ui.small_button("Manage").clicked() { self.edit.rail = Rail::Sticker; self.show_layers_tab(); }
            });
            ui.label(egui::RichText::new("At playhead · front to back").font(regular(11.0)).color(MUTED));
            egui::ScrollArea::vertical().id_salt("layer-stack-scroll").auto_shrink([false, false]).show(ui, |ui| {
                for (id, name, icon, hidden) in &items {
                    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::hover());
                    let eye = Rect::from_min_size(row.min, vec2(30.0, row.height()));
                    let select = Rect::from_min_max(pos2(eye.right(), row.top()), row.max);
                    let resp = ui.interact(select, ui.scope_id().with(("layer", id)), Sense::click()).on_hover_text(name);
                    if self.is_selected(*id) { chosen_row(ui.painter(), row, 4); }
                    else if resp.hovered() { ui.painter().rect_filled(row, 4, BUTTON); }
                    if icon_button(ui, eye, if *hidden { Icon::ViewOff } else { Icon::View }, "Toggle layer visibility", false).clicked() { visibility = Some(*id); }
                    icons::paint(ui.painter(), *icon, Rect::from_center_size(pos2(select.left() + 12.0, row.center().y), Vec2::splat(16.0)), if *hidden { MUTED } else { TEXT_2 });
                    let text = Rect::from_min_max(pos2(select.left() + 26.0, row.top()), select.max);
                    ui.painter().with_clip_rect(text).text(pos2(text.left(), row.center().y), Align2::LEFT_CENTER, name, regular(12.0), if *hidden { MUTED } else { Color32::WHITE });
                    if resp.clicked() { picked = Some(*id); }
                }
                if items.is_empty() { ui.label("No overlays here"); }
                ui.separator();
                if let Some(shot) = self.project.at(self.head) {
                    let clip = &self.project.clips[shot.clip];
                    let name = self.project.media_of(clip.media).map_or("Base video", |m| m.name.as_str());
                    if ui.add(egui::Button::new(format!("Base · {}", short(name, 20))).selected(self.picked == Some(Picked::Clip(clip.id)))).clicked() {
                        self.click_clip(clip.id, false, false);
                        self.edit.rail = Rail::Adjustment;
                    }
                }
            });
        });
        if let Some(id) = visibility
            && let Some(t) = self.project.items.iter_mut().find(|t| t.id == id)
            && let Some((_, _, motion)) = t.body.placed_mut() { motion.hidden ^= true; }
        if let Some(id) = picked {
            if ui.input(|i| i.modifiers.command || i.modifiers.shift) { self.toggle_select(id); }
            else { self.select_only(id); }
            let body = self.project.items.iter().find(|t| t.id == id).map(|t| t.body.clone());
            match body {
                Some(Body::Text { .. }) => { self.edit.rail = Rail::Text; self.edit.captions.tab = 0; }
                Some(Body::Overlay { .. }) => { self.edit.rail = Rail::Sticker; self.show_clips_tab(); }
                _ => { self.edit.rail = Rail::Sticker; self.show_layers_tab(); }
            }
        }
    }

    fn timeline(&mut self, ui: &mut Ui, r: Rect) {
        card(ui, r);
        let rh = self.row_heights();
        let bottom = Rect::from_min_max(pos2(r.left(), r.bottom() - TOOLBAR_H), r.max);
        let heads = Rect::from_min_max(pos2(r.left(), r.top() + RULER_H), pos2(r.left() + TRACK_HEADS_W, bottom.top()));
        let tree_w = if self.settings.layout.hide_layers { 0.0 } else {
            self.settings.layout.layers_w.unwrap_or(TREE_W).clamp(140.0, 320.0).min((r.width() - TRACK_HEADS_W - 240.0).max(0.0))
        };
        let tree = Rect::from_min_max(pos2(heads.right(), r.top()), pos2(heads.right() + tree_w, bottom.top()));
        let area = Rect::from_min_max(pos2(tree.right(), r.top()), pos2(r.right() - 4.0, bottom.top()));
        let ruler = Rect::from_min_size(area.min, vec2(area.width(), RULER_H));
        let lanes = Rect::from_min_max(pos2(area.left(), ruler.bottom()), area.max);

        let mut ys = [0.0; ROWS];
        let mut y = lanes.top() + 4.0;
        for (i, slot) in ys.iter_mut().enumerate() {
            *slot = y;
            y += rh[i] + ROW_GAP;
        }
        self.track_heads(ui, heads, &ys);
        self.timeline_controls(ui, Rect::from_min_max(r.min, pos2(heads.right(), heads.top())));
        ui.painter().vline(heads.right(), r.top()..=bottom.top(), Stroke::new(1.0, PANEL_EDGE));
        if tree_w > 0.0 {
            self.tree(ui, tree);
            let grip = Rect::from_min_max(pos2(tree.right() - 3.0, tree.top()), pos2(tree.right() + 3.0, tree.bottom()));
            let resize = ui.interact(grip, ui.scope_id().with("layers-width"), Sense::click_and_drag());
            if resize.hovered() || resize.dragged() { ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal); }
            if resize.dragged() { self.settings.layout.layers_w = Some((tree_w + resize.drag_delta().x).clamp(140.0, 320.0)); }
            if resize.double_clicked() { self.settings.layout.layers_w = None; }
            if resize.drag_stopped() || resize.double_clicked() { crate::store::save_settings(&self.settings); }
        }
        ui.painter().hline(r.x_range(), bottom.top(), Stroke::new(1.0, PANEL_EDGE));

        let total = self.project.total().max(1);
        let fit = (area.width() - 30.0) / total as f32;
        let scale = fit * self.edit.zoom;
        let content = total as f32 * scale + 60.0;
        self.edit.scroll = self.edit.scroll.clamp(0.0, (content - area.width()).max(0.0));
        if ui.rect_contains_pointer(area) {
            let (delta, command) = ui.input(|i| (i.smooth_scroll_delta, i.modifiers.command));
            if command && delta.y != 0.0 {
                self.edit.zoom = (self.edit.zoom * (1.0 + delta.y * 0.002)).clamp(MIN_ZOOM, MAX_ZOOM);
            } else {
                self.edit.scroll = (self.edit.scroll - delta.x - delta.y).max(0.0);
            }
        }
        let left = area.left() + 6.0 - self.edit.scroll;
        let to_x = |f: u32| left + f as f32 * scale;
        let to_frame = |x: f32| ((x - left) / scale).max(0.0).round() as u32;

        self.ruler(ui, ruler, scale, left);
        let clipped = ui.painter().with_clip_rect(area);
        let index = self.timeline_index();
        let starts = &index.starts;
        let clips = self.project.clips.clone();
        let locked = |app: &Self, row: usize| app.project.tracks[row].locked;
        // Only what's on screen gets drawn, however long the project is.
        let first = starts.partition_point(|s| to_x(*s) < area.left()).saturating_sub(1);
        let last = starts.partition_point(|s| to_x(*s) < area.right());
        let mut go = None;
        let fps = self.project.fps.max(1.0);
        // Zoomed out, most of a long project is slivers. Those stay plain bars, and only
        // the ones near the pointer can be grabbed, which keeps hit-testing small.
        let pointer = ui.ctx().pointer_latest_pos();
        let near = |r: &Rect| pointer.is_some_and(|p| (p.x - r.center().x).abs() < r.width() + NEAR_POINTER);

        let lane_hit = ui.interact(lanes, ui.scope_id().with("lanes"), Sense::click_and_drag());
        if lane_hit.clicked()
            && let Some(p) = lane_hit.interact_pointer_pos()
        {
            go = Some(to_frame(p.x));
            self.picked = None;
        }
        if lane_hit.drag_started() {
            self.edit.hold = Some(Hold::Head);
        }
        let mut used = [false; ROWS];
        used[ROW_VIDEO] = first < last;
        if self.project.clips.is_empty() && self.project.items.is_empty() {
            clipped.text(lanes.center(), Align2::CENTER_CENTER, "Drag media here from the bin, or click a file there to add it", regular(13.0), MUTED);
        }

        let y0 = ys[ROW_VIDEO];
        for i in first..last {
            let c = &clips[i];
            let rect = Rect::from_min_size(pos2(to_x(starts[i]), y0), vec2((c.len as f32 * scale).max(4.0), rh[ROW_VIDEO]));
            if !rect.intersects(area) {
                continue;
            }
            let body = rect.intersect(area);
            clipped.rect_filled(rect, 3, BUTTON);
            if let Some(strip) = self.edit.strips.get(&c.media).filter(|s| !s.is_empty()) {
                let tile = rh[ROW_VIDEO] * STRIP_SIZE.0 as f32 / STRIP_SIZE.1 as f32;
                let seconds = self.project.media_of(c.media).map_or(1.0, |m| m.seconds).max(0.1);
                let mut x = rect.left();
                while x < rect.right() {
                    let along = c.source_at(((x - rect.left()) / rect.width()) as f64 * c.len as f64, fps);
                    let k = ((along / seconds * strip.len() as f64) as usize).min(strip.len() - 1);
                    let w = tile.min(rect.right() - x);
                    let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(w / tile, 1.0));
                    clipped.image(strip[k].id(), Rect::from_min_size(pos2(x, rect.top()), vec2(w, rect.height())), uv, Color32::WHITE);
                    x += tile;
                }
            }
            if (c.speed - 1.0).abs() > 1e-6 && rect.width() >= SLIVER * 3.0 {
                let label = super::motion::speed_label(c.speed);
                let badge = Rect::from_min_size(rect.left_top() + vec2(6.0, 6.0), vec2(text_width(ui, &label, semi(11.0)) + 26.0, 18.0));
                clipped.rect_filled(badge, 9, Color32::from_black_alpha(170));
                icons::paint(&clipped, Icon::DashboardSpeed02, Rect::from_center_size(pos2(badge.left() + 10.0, badge.center().y), Vec2::splat(12.0)), Color32::WHITE);
                clipped.text(pos2(badge.left() + 19.0, badge.center().y), Align2::LEFT_CENTER, label, semi(11.0), Color32::WHITE);
            }
            if let Some(t) = c.transition {
                let w = t.len as f32 * scale;
                let band = Rect::from_min_size(rect.min, vec2(w, rect.height()));
                clipped.rect_filled(band, 0, Color32::from_rgba_unmultiplied(255, 255, 255, 46));
                clipped.line_segment([band.left_bottom(), band.right_top()], Stroke::new(1.0, Color32::WHITE));
            }
            let primary = self.picked == Some(Picked::Clip(c.id));
            let picked = self.clip_selected(c.id);
            if rect.width() < SLIVER && !picked && !near(&rect) {
                continue;
            }
            let inner = (rect.width() > EDGE * 3.0).then(|| rect.shrink2(vec2(EDGE, 0.0))).unwrap_or(rect);
            let resp = ui.interact(inner.intersect(body), ui.scope_id().with(("clip", c.id)), Sense::click_and_drag());
            if resp.clicked() {
                let (ctrl, shift) = ui.input(|i| (i.modifiers.command, i.modifiers.shift));
                self.click_clip(c.id, ctrl, shift);
                // A plain click also moves the playhead there; picking more leaves it be.
                if !ctrl && !shift {
                    let at = ui.input(|i| i.pointer.interact_pos()).map_or(starts[i], |p| to_frame(p.x));
                    go = Some(at.clamp(starts[i], starts[i] + c.len.saturating_sub(1)));
                }
            }
            resp.context_menu(|ui| self.clip_menu(ui, c.id));
            if resp.drag_started() && !locked(self, ROW_VIDEO) {
                self.picked = Some(Picked::Clip(c.id));
                self.edit.hold = Some(Hold::Clip(i));
            }
            if picked && !primary {
                clipped.rect_stroke(rect, 3, Stroke::new(2.0, YELLOW), StrokeKind::Inside);
            } else if primary {
                clipped.rect_stroke(rect, 3, Stroke::new(3.0, YELLOW), StrokeKind::Inside);
                for (x, glyph) in [(rect.left() + 5.0, Icon::ArrowLeft01), (rect.right() - 5.0, Icon::ArrowRight01)] {
                    let bar = Rect::from_center_size(pos2(x, rect.center().y), vec2(10.0, rect.height()));
                    clipped.rect_filled(bar, 0, YELLOW);
                    icons::paint(&clipped, glyph, Rect::from_center_size(bar.center(), Vec2::splat(11.0)), Color32::BLACK);
                }
            } else {
                clipped.rect_stroke(rect, 3, Stroke::new(1.0, Color32::from_black_alpha(160)), StrokeKind::Inside);
            }
            if !locked(self, ROW_VIDEO) {
                for (side, x) in [(0, rect.left()), (1, rect.right())] {
                    let edge = Rect::from_center_size(pos2(x, rect.center().y), vec2(EDGE * 1.4, rect.height())).intersect(area);
                    let hit = ui.interact(edge, ui.scope_id().with(("edge", c.id, side)), Sense::drag());
                    if hit.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                    }
                    if hit.drag_started() {
                        self.picked = Some(Picked::Clip(c.id));
                        self.edit.hold = Some(if side == 0 { Hold::TrimIn(c.id) } else { Hold::TrimOut(c.id) });
                        self.edit.spare = 0.0;
                    }
                }
            }
        }

        // The clips' own sound shows under them until it is detached.
        let sound_on = !self.project.tracks[ROW_SOUND].muted;
        for i in first..last {
            let c = &clips[i];
            let has = self.project.media_of(c.media).is_some_and(|m| m.has_audio);
            if !has || c.detached {
                continue;
            }
            let bar = Rect::from_min_size(pos2(to_x(starts[i]), ys[ROW_SOUND]), vec2((c.len as f32 * scale - 2.0).max(4.0), rh[ROW_SOUND]));
            let fill = if sound_on && !c.muted { SOUND } else { Color32::from_gray(70) };
            used[ROW_SOUND] = true;
            clipped.rect_filled(bar, 5, fill);
            if self.clip_selected(c.id) {
                clipped.rect_stroke(bar, 5, Stroke::new(1.5, YELLOW), StrokeKind::Inside);
            }
            if bar.width() < SLIVER {
                continue;
            }
            let row = bar.intersect(area);
            // Click picks the clip, drag up or down sets the volume, double-click puts it back
            // to as recorded, right-click opens the clip's menu.
            let hit = ui.interact(row, ui.scope_id().with(("sound", c.id)), Sense::click_and_drag()).on_hover_text("Drag up or down for volume; double-click for 100%");
            let speaker = Rect::from_center_size(pos2(bar.left() + 18.0, bar.center().y), Vec2::splat(24.0));
            let mute = ui.interact(speaker.intersect(area), ui.scope_id().with(("sound-mute", c.id)), Sense::click()).on_hover_text(if c.muted { "Unmute (M)" } else { "Mute (M)" });
            if mute.clicked() {
                if !self.clip_selected(c.id) {
                    self.click_clip(c.id, false, false);
                }
                self.toggle_clip_mute();
            } else if hit.clicked() {
                let (ctrl, shift) = ui.input(|i| (i.modifiers.command, i.modifiers.shift));
                self.click_clip(c.id, ctrl, shift);
            }
            if hit.drag_started() && !self.clip_selected(c.id) {
                self.click_clip(c.id, false, false);
            }
            let targets = self.sound_targets();
            if hit.double_clicked() {
                for t in self.project.clips.iter_mut().filter(|t| targets.contains(&t.id)) {
                    t.volume = 1.0;
                }
            }
            if hit.dragged() {
                let change = -hit.drag_delta().y / bar.height().max(1.0) * MOST_VOLUME;
                for t in self.project.clips.iter_mut().filter(|t| targets.contains(&t.id)) {
                    t.volume = (t.volume + change).clamp(0.0, MOST_VOLUME);
                }
                let now = self.project.clips.iter().find(|t| t.id == c.id).map_or(c.volume, |t| t.volume);
                value_bubble(ui, pos2(hit.interact_pointer_pos().map_or(bar.center().x, |p| p.x), bar.top()), &format!("{:.0}%", now * 100.0));
            }
            hit.context_menu(|ui| self.clip_menu(ui, c.id));
            // While pointed at, the volume shows as a line: the middle is as recorded, the
            // top twice as loud. Left out otherwise, as it would cross out the label.
            if (hit.hovered() || hit.dragged()) && !c.muted {
                let now = self.project.clips.iter().find(|t| t.id == c.id).map_or(c.volume, |t| t.volume);
                let y = bar.bottom() - (now / MOST_VOLUME).clamp(0.0, 1.0) * bar.height();
                clipped.line_segment([pos2(bar.left() + 2.0, y), pos2(bar.right() - 2.0, y)], Stroke::new(2.0, YELLOW));
            }
            let tint = if mute.hovered() { YELLOW } else { Color32::WHITE };
            icons::paint(&clipped, if c.muted { Icon::VolumeOff } else { Icon::VolumeHigh }, Rect::from_center_size(speaker.center(), Vec2::splat(17.0)), tint);
            let label = match (c.muted, (c.volume - 1.0).abs() > 0.005) {
                (true, _) => "Original Sound, muted".to_string(),
                (false, true) => format!("Original Sound {:.0}%", c.volume * 100.0),
                (false, false) => "Original Sound".to_string(),
            };
            clipped.with_clip_rect(row).text(pos2(bar.left() + 36.0, bar.center().y), Align2::LEFT_CENTER, label, semi(12.0), Color32::WHITE);
        }

        let items = self.project.items.clone();
        let mut drawn_to = [f32::MIN; ROWS];
        for n in index.items_in(&items, to_frame(area.left()), to_frame(area.right()) + 1) {
            let t = &items[n];
            let row = t.row.min(ROWS - 1);
            let rect = Rect::from_min_size(pos2(to_x(t.at), ys[row]), vec2((t.len as f32 * scale).max(4.0), rh[row]));
            if !rect.intersects(area) {
                continue;
            }
            let picked = self.is_selected(t.id);
            let sliver = rect.width() < SLIVER;
            // Two slivers can't be told apart inside one pixel, so one stands for both.
            if sliver && !picked && rect.left() < drawn_to[row] {
                continue;
            }
            drawn_to[row] = rect.left() + 1.0;
            used[row] = true;
            clipped.rect_filled(rect, 5, colour_of(t));
            if !sliver {
                let badge = Rect::from_center_size(pos2(rect.left() + 18.0, rect.center().y), Vec2::splat(24.0));
                clipped.rect_filled(badge, 4, Color32::from_black_alpha(70));
                icons::paint(&clipped, icon_of(t), Rect::from_center_size(badge.center(), Vec2::splat(16.0)), Color32::WHITE);
                // A video layer goes by its file's name, so several can be told apart.
                let label = match &t.body {
                    Body::Overlay { media, .. } => self.project.media_of(*media).map_or_else(|| t.label(), |m| m.name.clone()),
                    _ => t.label(),
                };
                clipped.with_clip_rect(rect.intersect(area)).text(pos2(rect.left() + 38.0, rect.center().y), Align2::LEFT_CENTER, label, semi(12.0), Color32::WHITE);
            }
            if picked {
                clipped.rect_stroke(rect, 5, Stroke::new(2.0, YELLOW), StrokeKind::Inside);
            }
            if sliver && !picked && !near(&rect) {
                continue;
            }
            let free = !locked(self, row);
            let resp = ui.interact(rect.intersect(area).shrink2(vec2(EDGE.min(rect.width() / 3.0), 0.0)), ui.scope_id().with(("item", t.id)), Sense::click_and_drag());
            resp.context_menu(|ui| self.item_menu(ui, t.id));
            if resp.clicked() {
                if ui.input(|i| i.modifiers.command || i.modifiers.shift) {
                    self.toggle_select(t.id);
                } else {
                    self.select_only(t.id);
                }
                self.edit.rail = match t.body {
                    Body::Text { .. } => {
                        // A caption is changed on the Text tab, not where it was made.
                        self.edit.captions.tab = 0;
                        Rail::Text
                    }
                    Body::Sticker { .. } => Rail::Sticker,
                    Body::Graphic { .. } => {
                        self.show_layers_tab();
                        Rail::Sticker
                    }
                    Body::Overlay { .. } => {
                        self.show_clips_tab();
                        Rail::Sticker
                    }
                    Body::Effect { .. } => Rail::Effect,
                    Body::Audio { .. } => self.edit.rail,
                };
            }
            if resp.hovered() && free {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
            }
            if resp.drag_started() && free {
                // Dragging one of a selection moves them all; anything else, just itself.
                if self.is_selected(t.id) {
                    self.picked = Some(Picked::Item(t.id));
                } else {
                    self.select_only(t.id);
                }
                self.edit.hold = Some(Hold::Item(t.id));
                self.edit.spare = 0.0;
                self.edit.grab = resp.interact_pointer_pos().map_or(0, |p| to_frame(p.x) as i64 - t.at as i64);
            }
            if free {
                for (side, x) in [(0, rect.left()), (1, rect.right())] {
                    let edge = Rect::from_center_size(pos2(x, rect.center().y), vec2(EDGE * 1.4, rect.height())).intersect(area);
                    let hit = ui.interact(edge, ui.scope_id().with(("item-edge", t.id, side)), Sense::drag());
                    if hit.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                    }
                    if hit.drag_started() {
                        self.edit.hold = Some(if side == 0 { Hold::ItemIn(t.id) } else { Hold::ItemOut(t.id) });
                        self.edit.spare = 0.0;
                    }
                }
            }
            if self.picked == Some(Picked::Item(t.id)) {
                self.key_diamonds(ui, area, rect, t.id, &to_x, &to_frame);
            }
        }

        for (row, name) in ROW_NAMES.iter().enumerate().filter(|(r, _)| !used[*r]) {
            clipped.text(pos2(lanes.left() + 12.0, ys[row] + rh[row] / 2.0), Align2::LEFT_CENTER, name, regular(11.5), Color32::from_gray(80));
        }

        let ruler_hit = ui.interact(ruler, ui.scope_id().with("ruler"), Sense::click_and_drag());
        if ruler_hit.drag_started() {
            self.edit.hold = Some(Hold::Head);
        }
        if ruler_hit.clicked()
            && let Some(p) = ruler_hit.interact_pointer_pos()
        {
            go = Some(to_frame(p.x));
        }
        self.apply_hold(ui, scale, &to_frame, &ys, &mut go);
        if let Some(f) = go {
            self.seek_head(f);
        }
        // While a drag runs: the guide it lines up with, where a moved clip will land, and
        // the time or length being set, beside the pointer.
        if let Some(hold) = self.edit.hold {
            if let Some(f) = self.edit.snapped {
                clipped.vline(to_x(f), lanes.top()..=lanes.bottom(), Stroke::new(1.5, ACCENT));
            }
            if let (Hold::Clip(from), Some(p)) = (hold, pointer) {
                let slot = self.project.slot_at(to_frame(p.x));
                if slot != from && slot != from + 1 {
                    let at = index.starts.get(slot).copied().unwrap_or(self.project.video_end());
                    let x = to_x(at);
                    clipped.vline(x, ys[ROW_VIDEO] - 4.0..=ys[ROW_VIDEO] + rh[ROW_VIDEO] + 4.0, Stroke::new(3.0, ACCENT));
                }
            }
            if let (Some(text), Some(p)) = (self.drag_readout(hold), pointer) {
                value_bubble(ui, p - vec2(0.0, 14.0), &text);
            }
        } else if self.run.is_some() || ui.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Key { pressed: true, .. }))) {
            // The playhead stays in view as it plays, or as keys move it.
            let x = to_x(self.head);
            if x > area.right() - 24.0 || x < area.left() + 6.0 {
                self.edit.scroll = (self.edit.scroll + x - (area.left() + area.width() * FOLLOW_AT)).max(0.0);
                ui.ctx().request_repaint();
            }
        }

        // Dropping something carried from the bin.
        if let Some(id) = self.edit.carrying
            && ui.input(|i| i.pointer.any_released())
        {
            self.edit.carrying = None;
            if let Some(p) = ui.input(|i| i.pointer.interact_pos()).filter(|p| area.contains(*p)) {
                let at = to_frame(p.x);
                let slot = self.project.slot_at(at);
                self.place(id, Some(slot));
            }
        }

        let x = to_x(self.head);
        if x >= area.left() && x <= area.right() {
            let line = Stroke::new(1.5, Color32::WHITE);
            clipped.vline(x, ruler.bottom() - 4.0..=area.bottom(), line);
            clipped.circle_filled(pos2(x, ruler.bottom() - 4.0), 4.5, Color32::WHITE);
        }
        self.toolbar(ui, bottom);
    }

    fn ruler(&self, ui: &Ui, r: Rect, scale: f32, left: f32) {
        let p = ui.painter().with_clip_rect(r);
        let fps = self.project.fps.max(1.0) as f32;
        let whole = fps.round().max(1.0) as u32;
        // The first spacing, in frames, that leaves room for a label.
        let steps = [1, 2, 5, 10, whole / 2, whole, whole * 2, whole * 5, whole * 10, whole * 30, whole * 60, whole * 120, whole * 300, whole * 600];
        let step = steps.into_iter().filter(|s| *s > 0).find(|s| *s as f32 * scale > RULER_LABEL_ROOM).unwrap_or(whole * 1200);
        let minor = if step >= 5 { step / 5 } else { 1 };
        let first = ((r.left() - left) / scale).max(0.0) as u32 / minor * minor;
        let last = ((r.right() - left) / scale).max(0.0) as u32 + minor;
        let mut f = first;
        while f <= last {
            let x = left + f as f32 * scale;
            let major = f % step == 0;
            let tall = if major { 9.0 } else { 4.0 };
            p.vline(x, r.bottom() - tall..=r.bottom(), Stroke::new(1.0, if major { MUTED } else { BUTTON_EDGE }));
            if major {
                p.text(pos2(x + 4.0, r.center().y - 2.0), Align2::LEFT_CENTER, ruler_label(f, whole, step < whole), regular(11.0), MUTED);
            }
            f += minor;
        }
    }

    /// What a drag is setting, said beside the pointer: a start time, or a length.
    fn drag_readout(&self, hold: Hold) -> Option<String> {
        let fps = self.project.fps.max(1.0);
        let secs = |f: u32| format!("{:.2} s", f as f64 / fps);
        match hold {
            Hold::Item(id) => self.project.items.iter().find(|t| t.id == id).map(|t| ruler_label(t.at, fps.round() as u32, true)),
            Hold::ItemIn(id) | Hold::ItemOut(id) => self.project.items.iter().find(|t| t.id == id).map(|t| secs(t.len)),
            Hold::TrimIn(id) | Hold::TrimOut(id) => self.project.clips.iter().find(|c| c.id == id).map(|c| secs(c.len)),
            _ => None,
        }
    }

    /// Lines things up as they are dragged: the playhead, every cut, and the edges of every
    /// other item. `skip` is the item being dragged.
    fn snap_targets(&self, skip: Option<u32>) -> Vec<u32> {
        // Items moving together don't line up with each other.
        let moving: Vec<u32> = match skip {
            Some(id) if self.is_selected(id) => self.selection(),
            other => other.into_iter().collect(),
        };
        let index = self.timeline_index();
        let mut v = vec![0, self.head];
        for (i, s) in index.starts.iter().enumerate() {
            v.push(*s);
            v.push(s + self.project.clips[i].len);
        }
        for t in self.project.items.iter().filter(|t| !moving.contains(&t.id)) {
            v.push(t.at);
            v.push(t.end());
        }
        v.sort_unstable();
        v.dedup();
        v
    }

    /// Zoom out, a zoom bar, zoom in, fit, and the snapping magnet.
    fn timeline_controls(&mut self, ui: &mut Ui, r: Rect) {
        let y = r.center().y;
        let b = |n: f32| Rect::from_center_size(pos2(r.left() + 18.0 + n * 28.0, y), Vec2::splat(26.0));
        let snap = self.edit.snap;
        if icon_button(ui, b(0.0), Icon::Magnet, if snap { "Snapping on (N). Hold Alt to drag freely" } else { "Snapping off (N)" }, snap).clicked() {
            self.edit.snap = !snap;
        }
        if icon_button(ui, b(1.0), Icon::SearchMinus, "Zoom out (-)", false).clicked() {
            self.zoom_by(0.8);
        }
        // A short bar between the two: where the zoom sits, from whole project to frames.
        let track = Rect::from_min_max(pos2(b(1.0).right() + 2.0, y - 2.0), pos2(b(3.0).left() - 2.0, y + 2.0));
        let resp = ui.interact(track.expand2(vec2(0.0, 8.0)), ui.scope_id().with("zoom-bar"), Sense::click_and_drag()).on_hover_text("Zoom");
        if let Some(p) = resp.interact_pointer_pos()
            && (resp.dragged() || resp.clicked())
        {
            let t = ((p.x - track.left()) / track.width()).clamp(0.0, 1.0);
            self.edit.zoom = MIN_ZOOM * (MAX_ZOOM / MIN_ZOOM).powf(t);
        }
        let t = (self.edit.zoom / MIN_ZOOM).ln() / (MAX_ZOOM / MIN_ZOOM).ln();
        let p = ui.painter();
        p.rect_filled(track, 2, TRACK);
        p.rect_filled(Rect::from_min_max(track.min, pos2(track.left() + track.width() * t, track.bottom())), 2, ACCENT);
        p.circle_filled(pos2(track.left() + track.width() * t, y), 5.0, Color32::WHITE);
        if icon_button(ui, b(3.0), Icon::SearchAdd, "Zoom in (=)", false).clicked() {
            self.zoom_by(1.25);
        }
        if icon_button(ui, b(4.0), Icon::FitToScreen, "Fit the whole project (\\)", false).clicked() {
            self.zoom_fit();
        }
    }

    fn apply_hold(&mut self, ui: &mut Ui, scale: f32, to_frame: &impl Fn(f32) -> u32, ys: &[f32; ROWS], go: &mut Option<u32>) {
        let rh = self.row_heights();
        self.edit.snapped = None;
        let Some(hold) = self.edit.hold else { return };
        let (down, delta, pos, free) = ui.input(|i| (i.pointer.primary_down(), i.pointer.delta().x, i.pointer.interact_pos(), i.modifiers.alt));
        let at = pos.map(|p| to_frame(p.x) as i64);
        let skip = match hold {
            Hold::Item(id) | Hold::ItemIn(id) | Hold::ItemOut(id) => Some(id),
            _ => None,
        };
        let targets = if self.edit.snap && !free { self.snap_targets(skip) } else { Vec::new() };
        let reach = (SNAP_PX / scale).max(1.0) as i64;
        // The nearest target within reach, and how far it is.
        let pull = |f: i64| -> Option<(i64, u32)> {
            let i = targets.partition_point(|t| (*t as i64) < f);
            [i.checked_sub(1), Some(i)].into_iter().flatten().filter_map(|k| targets.get(k)).map(|t| (*t as i64 - f, *t)).filter(|(d, _)| d.abs() <= reach).min_by_key(|(d, _)| d.abs())
        };
        let mut lined = None;
        let mut snapped = |f: i64| match pull(f) {
            Some((d, t)) => {
                lined = Some(t);
                f + d
            }
            None => f,
        };
        match hold {
            Hold::Head => {
                if let Some(f) = at {
                    *go = Some(snapped(f).max(0) as u32);
                }
            }
            Hold::TrimIn(id) => {
                // The clip's start stays put on the timeline while its source moves, so this
                // edge follows the pointer's movement rather than its place.
                self.edit.spare += delta / scale;
                let whole = self.edit.spare.trunc();
                self.edit.spare -= whole;
                self.project.trim_clip_start(id, whole as i64);
            }
            Hold::TrimOut(id) => {
                let index = self.timeline_index();
                if let (Some(f), Some(i)) = (at, index.clip_index(id)) {
                    let end = (index.starts[i] + self.project.clips[i].len) as i64;
                    self.project.trim_clip_end(id, snapped(f) - end);
                }
            }
            Hold::Clip(_) => {
                if !down && let (Some(p), Hold::Clip(from)) = (pos, hold) {
                    let slot = self.project.slot_at(to_frame(p.x));
                    let to = if slot > from { slot - 1 } else { slot };
                    self.project.move_clip(from, to);
                    self.needs_activate = true;
                }
            }
            Hold::Item(id) => {
                let row = pos.and_then(|p| (ROW_SOUND..ROWS).find(|r| p.y >= ys[*r] - ROW_GAP / 2.0 && p.y < ys[*r] + rh[*r] + ROW_GAP / 2.0));
                let grab = self.edit.grab;
                if let (Some(f), Some(len)) = (at, self.project.items.iter().find(|t| t.id == id).map(|t| t.len as i64)) {
                    // Either end may line up; the closer one wins.
                    let want = (f - grab).max(0);
                    let start = pull(want);
                    let end = pull(want + len);
                    let pick = match (start, end) {
                        (Some(a), Some(b)) => Some(if a.0.abs() <= b.0.abs() { a } else { b }),
                        (a, b) => a.or(b),
                    };
                    let new_at = (want + pick.map_or(0, |p| p.0)).max(0) as u32;
                    lined = pick.map(|p| p.1);
                    let was = self.project.items.iter().find(|t| t.id == id).map_or(new_at, |t| t.at);
                    let moved = self.carry_selection(id, new_at as i64 - was as i64);
                    if let Some(t) = self.project.items.iter_mut().find(|t| t.id == id) {
                        t.at = (was as i64 + moved).max(0) as u32;
                        if let Some(r) = row.filter(|r| matches!(t.body, Body::Audio { .. }) == matches!(*r, ROW_SOUND | ROW_MUSIC)) {
                            t.row = r;
                        }
                    }
                }
            }
            Hold::ItemIn(id) => {
                let fps = self.project.fps.max(1.0);
                let want = at.map(&mut snapped);
                if let (Some(f), Some(t)) = (want, self.project.items.iter_mut().find(|t| t.id == id)) {
                    let d = (f - t.at as i64).clamp(-(t.at as i64), t.len as i64 - 2);
                    t.at = (t.at as i64 + d) as u32;
                    t.len = (t.len as i64 - d) as u32;
                    match &mut t.body {
                        Body::Audio { start, speed, .. } => *start = (*start + d as f64 / fps * *speed).max(0.0),
                        Body::Overlay { start, .. } => *start = (*start + d as f64 / fps).max(0.0),
                        _ => {}
                    }
                    // Keyframes stay on the same moment of the picture.
                    if let Some((_, _, motion)) = t.body.placed_mut() {
                        motion.shift_keys(d);
                    }
                }
            }
            Hold::ItemOut(id) => {
                let want = at.map(&mut snapped);
                if let (Some(f), Some(t)) = (want, self.project.items.iter_mut().find(|t| t.id == id)) {
                    t.len = (f - t.at as i64).max(2) as u32;
                }
            }
        }
        self.edit.snapped = lined;
        if !down {
            self.edit.hold = None;
            self.edit.spare = 0.0;
            self.edit.snapped = None;
            self.needs_activate = true;
        }
    }

    fn toolbar(&mut self, ui: &mut Ui, r: Rect) {
        let y = r.center().y;
        let at = |n: f32| Rect::from_center_size(pos2(r.left() + 22.0 + n * 38.0, y), Vec2::splat(32.0));
        if icon_button(ui, at(0.0), Icon::Undo02, "Undo", false).clicked() {
            self.undo();
        }
        if icon_button(ui, at(1.0), Icon::Redo02, "Redo", false).clicked() {
            self.redo();
        }
        if icon_button(ui, at(2.0), Icon::Delete02, "Delete what's selected", false).clicked()
            && !self.delete_selection()
            && !self.delete_clip_selection()
            && let Some(p) = self.picked.take()
        {
            self.project.delete(p);
            self.needs_activate = true;
        }
        let reset = Rect::from_min_size(pos2(at(3.0).left(), y - 14.0), vec2(74.0, 28.0));
        if pill(ui, reset, "Reset All", false, 13.0).on_hover_text("Clear this clip's grade").clicked() {
            self.grade = crate::pipeline::Grade::default();
            self.selected = None;
            self.active_layer = None;
        }
        let mid = r.center().x - 60.0;
        let tool = |n: f32| Rect::from_center_size(pos2(mid + n * 40.0, y), Vec2::splat(32.0));
        let muted = self.targets_muted();
        let (icon, tip) = if muted { (Icon::VolumeOff, "Unmute the picked clips (M)") } else { (Icon::VolumeHigh, "Mute the picked clips (M)") };
        if icon_button(ui, tool(0.0), icon, tip, muted).clicked() {
            self.toggle_clip_mute();
        }
        if icon_button(ui, tool(1.0), Icon::MusicNote01, "Add music", false).clicked() {
            self.edit.rail = Rail::Audio;
            self.upload();
        }
        if icon_button(ui, tool(2.0), Icon::Text, "Add text", false).clicked() {
            self.add_title();
        }
        if icon_button(ui, tool(3.0), Icon::Scissor, "Split at the playhead", false).clicked() {
            self.split();
        }
        if icon_button(ui, tool(4.0), Icon::TransitionLeft, "Transitions", false).clicked() {
            self.edit.rail = Rail::Transitions;
        }
        if icon_button(ui, tool(5.0), Icon::Sparkles, "Effects", false).clicked() {
            self.edit.rail = Rail::Effect;
        }
        let mut x = r.right() - 8.0;
        let mut word = |app: &mut Self, ui: &mut Ui, label: &str| -> bool {
            let w = text_width(ui, label, semi(13.0)) + 20.0;
            x -= w;
            let b = Rect::from_min_size(pos2(x, y - 14.0), vec2(w, 28.0));
            let _ = &app;
            pill(ui, b, label, false, 13.0).clicked()
        };
        if word(self, ui, "Select All") {
            self.select_all_items();
        }
        if word(self, ui, "Trim") {
            let (start, _) = self.nearer_end();
            self.trim_to_head(start);
        }
        if word(self, ui, "Detach Audio") {
            self.detach();
        }
        if word(self, ui, "Duplicate") {
            let target = self.picked.or_else(|| self.active.map(Picked::Clip));
            if let Some(p) = target {
                self.picked = self.project.duplicate(p);
            }
        }
        if word(self, ui, "Split") {
            self.split();
        }
        if word(self, ui, "Speed") {
            self.edit.motion.speed_open = !self.edit.motion.speed_open;
        }
        let anchor = Rect::from_min_size(pos2(x, y - 14.0), vec2(text_width(ui, "Speed", semi(13.0)) + 20.0, 28.0));
        self.speed_popup(ui, anchor);
    }

    pub(super) fn add_title(&mut self) {
        self.edit.rail = Rail::Text;
        let (mut colour, mut motion) = ([1.0; 3], Default::default());
        self.brand_new_text(&mut colour, &mut motion);
        self.add_item(ROW_FX2, crate::timeline::ITEM_SECONDS, Body::Text { text: "Your title".to_string(), size: 96.0, colour, x: 0.5, y: 0.5, motion });
    }

    pub(super) fn zoom_by(&mut self, k: f32) {
        self.edit.zoom = (self.edit.zoom * k).clamp(MIN_ZOOM, MAX_ZOOM);
    }

    pub(super) fn zoom_fit(&mut self) {
        self.edit.zoom = MIN_ZOOM;
        self.edit.scroll = 0.0;
    }

    pub(super) fn split(&mut self) {
        match self.picked {
            Some(Picked::Item(id)) => {
                self.project.split_item(id, self.head);
            }
            _ => {
                self.sync_active();
                if self.project.split_clip(self.head).is_some() {
                    self.needs_activate = true;
                }
            }
        }
    }

    /// Whether the playhead sits nearer the active clip's start, and how far in it is.
    fn nearer_end(&self) -> (bool, i64) {
        let index = self.timeline_index();
        let found = self.active.and_then(|id| index.clip_index(id));
        let Some(i) = found else { return (true, 0) };
        let into = self.head.saturating_sub(index.starts[i]) as i64;
        (into * 2 < self.project.clips[i].len as i64, into)
    }

    /// Cuts one end of the active clip back to the playhead.
    pub(super) fn trim_to_head(&mut self, start: bool) {
        let index = self.timeline_index();
        let Some((id, i)) = self.active.and_then(|id| Some((id, index.clip_index(id)?))) else { return };
        let (into, len) = (self.head.saturating_sub(index.starts[i]) as i64, self.project.clips[i].len as i64);
        if start {
            self.project.trim_clip_start(id, into);
            self.head = index.starts[i];
        } else {
            self.project.trim_clip_end(id, into + 1 - len);
        }
        self.needs_activate = true;
    }

    pub(super) fn detach(&mut self) {
        let target = match self.picked {
            Some(Picked::Clip(id)) => Some(id),
            _ => self.active,
        };
        if let Some(id) = target
            && self.project.detach_audio(id).is_none()
        {
            self.say("That clip has no sound to detach");
        }
    }

    fn carry_ghost(&mut self, ui: &mut Ui, _timeline: Rect) {
        let (Some(id), Some(p)) = (self.edit.carrying, ui.input(|i| i.pointer.hover_pos())) else { return };
        let (w, h) = (THUMB.0 as f32 * 0.6, THUMB.1 as f32 * 0.6);
        let r = Rect::from_center_size(p, vec2(w, h));
        let painter = ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::unique("carry")));
        match self.edit.strips.get(&id).and_then(|s| s.first()) {
            Some(t) => {
                painter.image(t.id(), r, Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), Color32::from_white_alpha(210));
            }
            None => {
                painter.rect_filled(r, 6, BUTTON);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_reads_hours_minutes_seconds_frames() {
        assert_eq!(clock(0, 24.0), "00:00:00:00");
        assert_eq!(clock(24 * 61 + 5, 24.0), "00:01:01:05");
        assert_eq!(clock(30 * 3600, 29.97), "01:00:00:00");
    }
}
