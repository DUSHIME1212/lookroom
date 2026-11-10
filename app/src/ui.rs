//! The window: photo on top, the tool panel beneath it, and the tool row at the bottom.
//! On a wide window the panels move to a column beside the photo.

pub mod edit;
pub(crate) use safety::install_panic_hook;
mod assistant;
mod brand;
mod captions;
mod clipsel;
mod cuts;
mod design;
mod extras;
mod keys;
mod motion;
mod kit;
mod klipy;
mod layers;
mod library;
mod many;
mod onpicture;
mod overlay;
mod panels;
mod playback;
mod safety;
mod sheets;
mod templates;

use crate::audio;
use crate::crop::{Crop, Sides};
use crate::export::{self, PhotoOptions, VideoOptions};
use crate::icons::{self, Icon, TOOLS, Tool};
use crate::looks::{self, BuiltIn, RECIPES};
use crate::lut::{Lut3D, LutBank};
use crate::masks::{Layer, Mask, Shape, ShotMatch};
use crate::pipeline::{self, Grade, LutRef, Picture};
use crate::player::{self, Composer, Laid, Scene};
use crate::scene;
use crate::scopes::{self, Kind};
use crate::store::{self, Settings, UserPreset};
use crate::timeline::{Index, Kind as MediaKind, Media, Picked, Project};
use crate::track;
use crate::video::{self, Feed, Info};
use eframe::egui::{
    self, Align2, Color32, ColorImage, Context, FontData, FontDefinitions, FontFamily, Pos2, Rect,
    Sense, Stroke, StrokeKind, TextureHandle, TextureOptions, Ui, UiBuilder, Vec2, pos2, vec2,
};
use kit::*;
use panels::{COLOR_TABS, EFFECT_TABS, FILM_TABS};
use sheets::{Sheet, Task};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

// Preset panel: a share of the editor's height, from the preset screen's code.
const PRESET_PANEL_SHARE: f32 = 0.2;
const PRESET_PANEL_MIN: f32 = 150.0;
const PRESET_PANEL_MAX: f32 = 250.0;
const TOOL_PANEL_SHARE: f32 = 0.37;
const TOOL_PANEL_MIN: f32 = 230.0;
const TOOL_PANEL_MAX: f32 = 350.0;
/// How far the panel's handle can go in either direction.
const PANEL_MIN: f32 = 150.0;
const PHOTO_MIN: f32 = 150.0;

const PREVIEW_MAX: usize = 1400;
const CLIP_PREVIEW: usize = 1280;
const THUMB: (usize, usize) = (192, 154);
const WIDE_LAYOUT: f32 = 900.0;
/// From this width the window becomes the full edit workspace.
const EDIT_LAYOUT: f32 = 1100.0;
const SIDE_COLUMN: f32 = 400.0;
const HISTORY_LIMIT: usize = 100;
const UI_BENCH_FRAMES: usize = 200;
/// Frames left out of the timing, while the first picture and film strips arrive.
const UI_BENCH_SKIP: usize = 60;
const BOTTOM_H: f32 = 108.0;
const TOP_H: f32 = 44.0;
const TAB_ROW_H: f32 = 34.0;
const SCOPE_W: f32 = 150.0;
const SCOPE_H: f32 = 84.0;
const VECTOR_SIDE: f32 = 116.0;

const MASK_TABS: [&str; 8] = [
    "Shot Match",
    "Image Edit Layer",
    "Linear Mask",
    "Radial Mask",
    "Rectangle Mask",
    "Draw Mask",
    "Curve Mask",
    "HSL Qualifier",
];
const SHOT_TAB: usize = 0;
const DEFAULT_BRUSH: (f32, f32, f32) = (0.04, 0.5, 1.0);

/// The interface's regular, semibold and bold faces, with the face's place in its file.
/// Windows reads its three files directly; elsewhere the installed fonts are searched.
fn ui_faces() -> [Option<(Vec<u8>, u32)>; 3] {
    if cfg!(windows) {
        return crate::platform::WINDOWS_UI.map(|(path, _)| std::fs::read(path).ok().map(|b| (b, 0)));
    }
    use resvg::usvg::fontdb::{Family, Query, Weight};
    let o = crate::overlay::fonts();
    let families: Vec<Family> = crate::platform::SANS.iter().map(|n| Family::Name(n)).chain([Family::SansSerif]).collect();
    [400, 600, 700].map(|w| {
        let id = o.fontdb.query(&Query { families: &families, weight: Weight(w), ..Query::default() })?;
        o.fontdb.with_face_data(id, |data, index| (data.to_vec(), index))
    })
}

fn setup_fonts(ctx: &Context) {
    let mut fonts = FontDefinitions::default();
    let base: Vec<String> = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mut family = |key: &str, face: &Option<(Vec<u8>, u32)>, name: &str| {
        let mut list = Vec::new();
        if let Some((bytes, index)) = face {
            let mut data = FontData::from_owned(bytes.clone());
            data.index = *index;
            fonts.font_data.insert(key.to_string(), Arc::new(data));
            list.push(key.to_string());
        }
        list.extend(base.iter().cloned());
        (name.to_string(), list)
    };
    let faces = ui_faces();
    let regular_family = family("ui", &faces[0], "reg");
    let semibold = family("ui-semi", &faces[1], "semibold");
    let boldest = family("ui-bold", &faces[2], "bold");
    fonts.families.insert(FontFamily::Proportional, regular_family.1);
    fonts.families.insert(FontFamily::Name("semibold".into()), semibold.1);
    fonts.families.insert(FontFamily::Name("bold".into()), boldest.1);
    ctx.set_fonts(fonts);
}

/// What a drag on the photo is moving.
#[derive(Clone, Copy, PartialEq)]
enum Drag {
    Centre,
    Turn,
    Width,
    Height,
    Corner,
    Point(usize),
    Paint,
    CropMove,
    CropSides(Sides),
}

/// The state of the view, so a change to any part redraws it.
#[derive(Clone, Copy, PartialEq)]
struct ViewKey {
    framing: bool,
    overlay: Option<usize>,
    compare: bool,
    frame: u32,
    scope: Option<Kind>,
    picture: u64,
    head: u32,
}

/// One tab's small choices, remembered while moving between tools.
#[derive(Default)]
struct Pick {
    zone_view: usize,
    curve_mode: usize,
    curve_channel: usize,
    slice: usize,
    mixer: usize,
    band: usize,
    tetra: usize,
    film_view: usize,
    wheel: usize,
    lut: usize,
    track_scope: usize,
    mask_view: usize,
    curve_point: Option<usize>,
    rgb_open: bool,
}

/// The video behind the clip under the playhead. Frames here count the source's own,
/// which is what masks and tracking are keyed to.
struct Clip {
    info: Info,
    feed: Feed,
    media: u32,
    /// The frame on screen, once the decoder has delivered it.
    arrived: Option<u32>,
    frame: u32,
    first: u32,
    last: u32,
}

struct Job {
    picture: Arc<Picture>,
    grade: Grade,
    bank: LutBank,
    framing: bool,
    overlay: Option<usize>,
    compare: bool,
    frame: u32,
    scope: Option<Kind>,
    project: Arc<Project>,
    index: Arc<Index>,
    head: u32,
    /// Set when two clips are mixing here, which takes the whole compositor.
    scene: Option<Arc<Scene>>,
    /// A control is being dragged: answer quickly now, and exactly once it lets go.
    draft: bool,
}

struct Drawn {
    view: ColorImage,
    before: Option<ColorImage>,
    scope: Option<ColorImage>,
}

/// Grades on its own thread, so sliders stay smooth however heavy the edit is.
struct Worker {
    jobs: Sender<Job>,
    done: Receiver<Drawn>,
    pending: usize,
}

fn image_of(p: &Picture) -> ColorImage {
    ColorImage::from_rgba_unmultiplied([p.w, p.h], &p.to_rgba8())
}

impl Worker {
    fn new(ctx: Context) -> Self {
        let (jobs, inbox) = mpsc::channel::<Job>();
        let (out, done) = mpsc::channel();
        std::thread::spawn(move || {
            let mut laid = Laid::default();
            while let Ok(mut job) = inbox.recv() {
                let mut skipped = 0;
                while let Ok(newer) = inbox.try_recv() {
                    job = newer;
                    skipped += 1;
                }
                let Job { picture, grade, bank, framing, overlay, compare, frame, scope, project, index, head, scene, draft } = job;
                let mut pic = match scene {
                    Some(s) => {
                        let (w, h) = player::fit_size(project.size, PREVIEW_MAX);
                        Composer::new(s, w, h, false, false).frame(head)
                    }
                    None => {
                        let mut p = if draft {
                            let table = pipeline::bake_lut(&grade, &bank, crate::film::PLAY_TABLE);
                            pipeline::render_play(&picture, &grade, &table, &bank, frame)
                        } else {
                            pipeline::render_at(&picture, &grade, &bank, true, !framing, frame)
                        };
                        if !framing {
                            // The project's shape, so text sits where it will in the video.
                            // Mask editing and Before and After keep the clip's own shape to line up.
                            if overlay.is_none() && !compare {
                                p = player::frame_to(p, project.size, project.fill);
                                // A punch-in, as playback and export show it.
                                if let Some(z) = project.at(head).map(|s| project.clips[s.clip].zoom).filter(|z| *z > 1.001) {
                                    p = crate::overlay::zoom(p, z);
                                }
                            }
                            laid.apply(&mut p, &project, &index, head, &Default::default());
                        }
                        p
                    }
                };
                let scope = scope.map(|k| {
                    let s = scopes::draw(k, &pic);
                    ColorImage::from_rgba_unmultiplied([s.w, s.h], &s.rgba)
                });
                if let Some(i) = overlay {
                    pipeline::tint_mask(&mut pic, &picture, &grade, i, !framing, frame);
                }
                // "Before" has to line up with the cropped picture beside it.
                let before = compare.then(|| image_of(&grade.crop.geometry(picture.w, picture.h, !framing).apply(&picture)));
                let drawn = Drawn { view: player::image_of(&pic), before, scope };
                for _ in 0..skipped {
                    let _ = out.send(Drawn { view: ColorImage::default(), before: None, scope: None });
                }
                if out.send(drawn).is_err() {
                    return;
                }
                ctx.request_repaint();
            }
        });
        Self { jobs, done, pending: 0 }
    }
}

/// Steps for trying the app without a person, from `--input`:
/// "drag:x0,y0,x1,y1;click:x,y;scroll:x,y,dy;key:ctrl+S;paste:FILE;wait:30", in window points,
/// one event a frame.
fn scripted_input(spec: &str) -> Vec<egui::Event> {
    const DRAG_STEPS: usize = 12;
    let button = |pos: Pos2, pressed: bool| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    let mut out = Vec::new();
    for part in spec.split(';') {
        let Some((kind, numbers)) = part.split_once(':') else { continue };
        // The text of a file, as if pasted: a design copied from Figma, say.
        if kind == "paste" {
            if let Ok(text) = std::fs::read_to_string(numbers) {
                out.push(egui::Event::Paste(text));
            }
            continue;
        }
        if kind == "key" {
            let (chord, name) = numbers.rsplit_once('+').unwrap_or(("", numbers));
            if let Some(key) = egui::Key::from_name(name) {
                let (ctrl, shift) = (chord.contains("ctrl"), chord.contains("shift"));
                let modifiers = egui::Modifiers { ctrl, command: ctrl, shift, ..egui::Modifiers::NONE };
                out.push(egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers });
                out.push(egui::Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers });
            }
            continue;
        }
        let n: Vec<f32> = numbers.split(',').filter_map(|v| v.trim().parse().ok()).collect();
        match (kind, n.as_slice()) {
            ("drag", [x0, y0, x1, y1]) => {
                let (a, b) = (pos2(*x0, *y0), pos2(*x1, *y1));
                out.push(egui::Event::PointerMoved(a));
                out.push(button(a, true));
                for s in 1..=DRAG_STEPS {
                    out.push(egui::Event::PointerMoved(a + (b - a) * (s as f32 / DRAG_STEPS as f32)));
                }
                out.push(button(b, false));
            }
            // Pressed and not let go, for checking what holding a button does.
            ("hold", [x, y]) => {
                let at = pos2(*x, *y);
                out.push(egui::Event::PointerMoved(at));
                out.push(button(at, true));
            }
            // The wheel turned over a point: positive moves the content up.
            ("scroll", [x, y, dy]) => {
                out.push(egui::Event::PointerMoved(pos2(*x, *y)));
                out.push(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -*dy),
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Unknown,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            ("wait", [n]) => {
                for _ in 0..*n as usize {
                    out.push(egui::Event::PointerGone);
                }
            }
            ("click", [x, y]) => {
                let at = pos2(*x, *y);
                out.push(egui::Event::PointerMoved(at));
                out.push(button(at, true));
                out.push(button(at, false));
            }
            // The other button, for menus; and clicks with Ctrl or Shift held.
            ("rclick" | "ctrlclick" | "shiftclick", [x, y]) => {
                let at = pos2(*x, *y);
                let which = if kind == "rclick" { egui::PointerButton::Secondary } else { egui::PointerButton::Primary };
                let (ctrl, shift) = (kind == "ctrlclick", kind == "shiftclick");
                let modifiers = egui::Modifiers { ctrl, command: ctrl, shift, ..egui::Modifiers::NONE };
                out.push(egui::Event::PointerMoved(at));
                if ctrl || shift {
                    out.push(egui::Event::ModifiersChanged(modifiers));
                }
                for pressed in [true, false] {
                    out.push(egui::Event::PointerButton { pos: at, button: which, pressed, modifiers });
                }
                if ctrl || shift {
                    out.push(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
                }
            }
            _ => {}
        }
    }
    out.reverse();
    out
}

/// What can be set from the command line, for trying the app without a person.
#[derive(Default)]
pub struct Start {
    pub screenshot: Option<String>,
    pub open: Vec<String>,
    pub tool: Option<String>,
    pub input: Option<String>,
    pub grade: Option<Grade>,
    pub scope: Option<String>,
    pub sheet: Option<String>,
    pub compare: bool,
    pub frame: Option<u32>,
    pub dump: Option<String>,
    pub play_test: Option<String>,
    pub ui_bench: Option<String>,
}

pub struct App {
    ui_log: Option<(String, Vec<f32>)>,
    frame_began: Option<Instant>,
    project: Project,
    active: Option<u32>,
    head: u32,
    picked: Option<Picked>,
    stills: HashMap<u32, Arc<Picture>>,
    exifs: HashMap<u32, Option<Vec<u8>>>,
    shown_media: Option<u32>,
    needs_activate: bool,
    run: Option<Box<playback::Run>>,
    audio_out: Option<audio::Output>,
    play_side: usize,
    edit: edit::EditState,
    drawn_project: Option<Project>,
    thumbs_for: Option<u32>,
    index_cache: Option<(Project, Arc<Index>)>,
    spare_feeds: Vec<(u32, Feed)>,
    play_log: Option<(String, Vec<Instant>)>,
    screenshot: Option<String>,
    dump: Option<String>,
    script: Vec<egui::Event>,
    frames: u32,
    shot_sent: bool,
    shot_done: Arc<AtomicBool>,

    bank: LutBank,
    builtins: Vec<BuiltIn>,
    plugins: crate::plugins::Plugins,
    /// Playback stops here, for previewing one stretch.
    play_until: Option<u32>,
    user: Vec<UserPreset>,
    settings: Settings,
    /// Autosave and crash recovery; off for scripted test runs.
    safety: Option<safety::Safety>,
    imported: Vec<(String, String)>,

    full: Arc<Picture>,
    preview: Arc<Picture>,
    picture_id: u64,
    photo_name: String,
    photo_path: Option<PathBuf>,
    exif: Option<Vec<u8>>,
    clip: Option<Clip>,

    grade: Grade,
    drawn: Option<Grade>,
    drawn_key: Option<ViewKey>,
    drawn_draft: bool,
    worker: Worker,
    history: Vec<(String, Project)>,
    history_at: usize,
    active_layer: Option<usize>,
    active_mask: usize,
    show_mask: bool,
    add_to_layer: bool,
    brush: (f32, f32, f32),
    erase: bool,
    drag: Option<Drag>,
    thumbs: HashMap<String, TextureHandle>,
    view_tex: Option<TextureHandle>,
    orig_tex: Option<TextureHandle>,
    scope_tex: Option<TextureHandle>,

    tool: Tool,
    subs: HashMap<&'static str, usize>,
    pick: Pick,
    tab_shown: Option<(Tool, usize)>,
    selected: Option<String>,
    stack: bool,
    compare: bool,
    split: f32,
    scope: Option<Kind>,
    sheet: Option<Sheet>,
    name_buf: String,
    toast: Option<(String, Instant)>,
    task: Option<Task>,
    tracking: Option<(usize, usize, track::Run)>,
    photo_opts: PhotoOptions,
    clip_opts: VideoOptions,
    /// How long the export will take, worked out in the background.
    export_guess: sheets::Guess,
    batch_files: Vec<PathBuf>,
    spread: Option<(u64, scopes::Spread)>,
    hold_original: bool,
    panel_extra: f32,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, start: Start) -> Self {
        setup_fonts(&cc.egui_ctx);
        let mut v = egui::Visuals::dark();
        v.panel_fill = Color32::BLACK;
        v.window_fill = MENU;
        v.extreme_bg_color = Color32::from_rgb(0x10, 0x10, 0x12);
        v.widgets.inactive.weak_bg_fill = BUTTON;
        v.selection.bg_fill = ACCENT;
        cc.egui_ctx.set_visuals(v);
        cc.egui_ctx.global_style_mut(|s| {
            s.spacing.scroll = egui::style::ScrollStyle::thin();
            for font in s.text_styles.values_mut() {
                font.size = font.size.min(kit::MEDIUM_FONT_SIZE);
            }
        });

        let mut bank = looks::build_bank();
        let mut imported = Vec::new();
        if let Ok(dir) = std::fs::read_dir(store::luts_dir()) {
            for e in dir.flatten() {
                let path = e.path();
                if path.extension().is_some_and(|x| x.eq_ignore_ascii_case("cube"))
                    && let Ok(text) = std::fs::read_to_string(&path)
                    && let Ok(lut) = Lut3D::parse_cube(&text)
                {
                    let name = stem(&path);
                    let id = format!("file:{name}");
                    bank.luts.insert(id.clone(), Arc::new(lut));
                    imported.push((id, name));
                }
            }
        }

        let full = Arc::new(scene::sample_picture(1800, 1200));
        let preview = Arc::new(full.fit(PREVIEW_MAX));
        let plugins = crate::plugins::load();
        let mut builtins = looks::presets();
        builtins.extend(plugins.looks.iter().cloned());
        let first = builtins.first().map(|b| (b.id.clone(), b.grade.clone()));
        let mut project = Project::default();
        let sample = project.new_id();
        project.media.push(Media { id: sample, path: None, name: "Sample landscape".to_string(), kind: MediaKind::Photo, seconds: 0.0, info: None, has_audio: false, size: (full.w, full.h) });
        project.insert_clip(sample, 0);
        project.suit_first(sample);
        let mut stills = HashMap::new();
        stills.insert(sample, full.clone());
        // Scripted test runs leave no autosaves and look for no crashes.
        let scripted = start.screenshot.is_some() || start.input.is_some() || start.play_test.is_some() || start.ui_bench.is_some();
        let mut app = Self {
            ui_log: start.ui_bench.map(|o| (o, Vec::new())),
            frame_began: None,
            active: project.clips.first().map(|c| c.id),
            project,
            head: 0,
            picked: None,
            stills,
            exifs: HashMap::new(),
            shown_media: Some(sample),
            needs_activate: false,
            run: None,
            audio_out: audio::Output::open(),
            play_side: player::PLAY_SIDES[1],
            edit: edit::EditState::default(),
            drawn_project: None,
            thumbs_for: Some(sample),
            index_cache: None,
            spare_feeds: Vec::new(),
            play_log: None,
            screenshot: start.screenshot,
            dump: start.dump,
            script: start.input.as_deref().map(scripted_input).unwrap_or_default(),
            frames: 0,
            shot_sent: false,
            shot_done: Arc::new(AtomicBool::new(false)),
            bank,
            builtins,
            plugins,
            play_until: None,
            user: store::load_presets(),
            safety: (!scripted).then(safety::Safety::start),
            settings: store::load_settings(),
            imported,
            full,
            preview,
            picture_id: 0,
            photo_name: "Sample landscape".to_string(),
            photo_path: None,
            exif: None,
            clip: None,
            grade: Grade::default(),
            drawn: None,
            drawn_key: None,
            drawn_draft: false,
            worker: Worker::new(cc.egui_ctx.clone()),
            history: Vec::new(),
            history_at: 0,
            active_layer: None,
            active_mask: 0,
            show_mask: false,
            add_to_layer: false,
            brush: DEFAULT_BRUSH,
            erase: false,
            drag: None,
            thumbs: HashMap::new(),
            view_tex: None,
            orig_tex: None,
            scope_tex: None,
            tool: Tool::Filters,
            subs: HashMap::new(),
            pick: Pick::default(),
            tab_shown: None,
            selected: None,
            stack: true,
            compare: start.compare,
            split: 0.5,
            scope: Some(Kind::Parade),
            sheet: None,
            name_buf: String::new(),
            toast: None,
            task: None,
            tracking: None,
            photo_opts: PhotoOptions::default(),
            clip_opts: VideoOptions::default(),
            export_guess: Default::default(),
            batch_files: Vec::new(),
            spread: None,
            hold_original: false,
            panel_extra: 0.0,
        };
        if let Some((id, grade)) = first {
            app.grade = grade;
            app.selected = Some(id);
        }
        if let Some(g) = start.grade {
            app.grade = g;
            app.selected = None;
        }
        app.sync_active();
        if app.ui_log.is_some() && start.open.is_empty() {
            // Keeps the sample picture, with hundreds of clips and items around it.
            let mut big = crate::bench::stress(500, 2000);
            big.media[0].id = sample;
            let id = big.clips[0].media;
            for c in big.clips.iter_mut() {
                c.media = sample;
            }
            let _ = id;
            app.active = big.clips.first().map(|c| c.id);
            app.grade = (*big.clips[0].grade).clone();
            app.project = big;
        }
        app.want_strip(sample);
        for path in &start.open {
            let path = Path::new(path);
            if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("lookroom")) {
                app.load_project(path);
            } else {
                app.import(path, true);
            }
        }
        app.activate(&cc.egui_ctx);
        if let Some(f) = start.frame {
            app.seek_head(f);
            app.activate(&cc.egui_ctx);
        }
        app.sync_active();
        app.history = vec![("Start".to_string(), app.project.clone())];
        match start.scope.as_deref() {
            Some("off") => app.scope = None,
            Some(name) => app.scope = Kind::ALL.iter().copied().find(|k| k.name().to_lowercase().contains(name)),
            None => {}
        }
        // "--tool luts:1" opens a chosen tab and sub-tab directly.
        if let Some(spec) = start.tool {
            let (tool, sub) = spec.split_once(':').unwrap_or((spec.as_str(), "0"));
            if let Some(t) = TOOLS.iter().find(|t| key_of(**t) == tool) {
                app.tool = *t;
                let n = sub.parse().unwrap_or(0);
                if *t == Tool::Masks {
                    if n != SHOT_TAB {
                        app.add_tab(n);
                    }
                } else {
                    app.subs.insert(key_of(*t), n);
                }
            }
        }
        app.sheet = start.sheet.as_deref().and_then(Sheet::named);
        if let Some(out) = start.play_test {
            app.play_log = Some((out, Vec::new()));
            app.start_play("--play-test");
        }
        app.set_orig_texture(&cc.egui_ctx);
        app.rebuild_all_thumbs(&cc.egui_ctx);
        app
    }

    fn say(&mut self, text: impl Into<String>) {
        self.toast = Some((text.into(), Instant::now()));
    }

    fn frame(&self) -> u32 {
        self.clip.as_ref().map_or(0, |c| c.frame.min(c.info.frames))
    }

    fn set_orig_texture(&mut self, ctx: &Context) {
        self.orig_tex = Some(upload(ctx, "original", &self.preview));
    }

    /// Tiles always show the photo with that one preset, not the current edit.
    fn rebuild_all_thumbs(&mut self, ctx: &Context) {
        let base = self.preview.cover(THUMB.0, THUMB.1);
        let mut jobs: Vec<(String, Grade)> = Vec::new();
        for b in &self.builtins {
            jobs.push((b.id.clone(), b.grade.clone()));
        }
        for u in &self.user {
            jobs.push((u.id.clone(), u.grade.clone()));
        }
        for r in RECIPES {
            jobs.push((r.id(), lut_grade(&r.id(), r.name)));
        }
        for (id, name) in &self.imported {
            jobs.push((id.clone(), lut_grade(id, name)));
        }
        for (id, g) in jobs {
            let pic = pipeline::render(&base, &g, &self.bank, false);
            self.thumbs.insert(id.clone(), upload(ctx, &id, &pic));
        }
    }

    fn rebuild_thumb(&mut self, ctx: &Context, id: &str, g: &Grade) {
        let base = self.preview.cover(THUMB.0, THUMB.1);
        let pic = pipeline::render(&base, g, &self.bank, false);
        self.thumbs.insert(id.to_string(), upload(ctx, id, &pic));
    }

    fn pick_media(&mut self) {
        let mut all: Vec<&str> = export::PHOTO_EXTENSIONS.to_vec();
        all.extend(video::EXTENSIONS);
        all.extend(video::SOUND_EXTENSIONS);
        if let Some(files) = rfd::FileDialog::new()
            .add_filter("Photos, videos and sound", &all)
            .add_filter("Photos", &export::PHOTO_EXTENSIONS)
            .add_filter("Videos", &video::EXTENSIONS)
            .pick_files()
        {
            for f in files {
                self.import(&f, true);
            }
        }
    }

    fn import_luts(&mut self, ctx: &Context) {
        let Some(files) = rfd::FileDialog::new().add_filter("LUT or HaldCLUT", &["cube", "png", "tif", "tiff"]).pick_files() else {
            return;
        };
        let mut added = 0;
        for f in files {
            added += self.import_lut_file(ctx, &f) as usize;
        }
        if added > 0 {
            self.say(format!("{added} LUT(s) added to your library"));
        }
    }

    fn import_lut_file(&mut self, ctx: &Context, f: &Path) -> bool {
        let cube = f.extension().is_some_and(|x| x.eq_ignore_ascii_case("cube"));
        let parsed = if cube {
            std::fs::read_to_string(f).map_err(|e| e.to_string()).and_then(|t| Lut3D::parse_cube(&t))
        } else {
            export::open_photo(f).and_then(|o| Lut3D::from_hald(o.picture.w, o.picture.h, &o.picture.px))
        };
        match parsed {
            Ok(lut) => {
                let name = stem(f);
                let id = format!("file:{name}");
                let _ = std::fs::create_dir_all(store::luts_dir());
                let _ = std::fs::write(store::luts_dir().join(format!("{name}.cube")), lut.to_cube(&name));
                self.bank.luts.insert(id.clone(), Arc::new(lut));
                if !self.imported.iter().any(|(i, _)| *i == id) {
                    self.imported.push((id.clone(), name.clone()));
                }
                let g = lut_grade(&id, &name);
                self.rebuild_thumb(ctx, &id, &g);
                true
            }
            Err(e) => {
                self.say(format!("{}: {e}", stem(f)));
                false
            }
        }
    }

    fn export_lut(&mut self, size: usize) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Cube LUT", &["cube"])
            .set_file_name(format!("my-look-{size}.cube"))
            .save_file()
        else {
            return;
        };
        let lut = pipeline::bake_lut(&self.grade, &self.bank, size);
        let title = stem(&path);
        match std::fs::write(&path, lut.to_cube(&title)) {
            Ok(()) => self.say(format!("{title} saved to your LUT collection")),
            Err(e) => self.say(format!("Export failed: {e}")),
        }
    }

    fn persist(&mut self) {
        if let Err(e) = store::save_presets(&self.user) {
            self.say(format!("Failed to save preset: {e}"));
        }
    }

    /// The edit without the things that belong to one photo.
    fn look_only(&self) -> Grade {
        Grade { crop: Crop::default(), layers: Vec::new(), ..self.grade.clone() }
    }

    /// The edit the sliders act on: the chosen layer's, or the whole photo's.
    fn target(&mut self) -> &mut Grade {
        match self.active_layer {
            Some(i) if i < self.grade.layers.len() => &mut self.grade.layers[i].grade,
            _ => &mut self.grade,
        }
    }

    fn toggle_lut(&mut self, id: &str) {
        let name = self.lut_name(id);
        let stack = self.stack;
        let g = self.target();
        let on = g.luts.iter().any(|l| l.id == id);
        if stack {
            if on {
                g.luts.retain(|l| l.id != id);
            } else {
                g.luts.push(LutRef { id: id.to_string(), name, intensity: 1.0 });
            }
        } else if on && g.luts.len() == 1 {
            g.luts.clear();
        } else {
            g.luts = vec![LutRef { id: id.to_string(), name, intensity: 1.0 }];
        }
        let last = self.target().luts.len().saturating_sub(1);
        self.pick.lut = last;
        self.selected = None;
    }

    fn lut_name(&self, id: &str) -> String {
        RECIPES
            .iter()
            .find(|r| r.id() == id)
            .map(|r| r.name.to_string())
            .or_else(|| self.imported.iter().find(|(i, _)| i == id).map(|(_, n)| n.clone()))
            .unwrap_or_else(|| id.to_string())
    }

    fn next_name(&self, base: &str) -> String {
        let n = self.grade.layers.iter().filter(|l| l.name.starts_with(base)).count() + 1;
        format!("{base} {n}")
    }

    /// A tab on the masks row: a new layer, or a new mask when adding to a layer.
    fn add_tab(&mut self, tab: usize) {
        let label = MASK_TABS[tab.min(MASK_TABS.len() - 1)];
        if tab == SHOT_TAB {
            let layer = Layer { name: self.next_name(label), ..Layer::default() };
            self.push_layer(layer);
            let i = self.grade.layers.len() - 1;
            if !self.choose_reference(i) {
                self.grade.layers.pop();
                self.active_layer = None;
            }
            return;
        }
        let shape = shape_of_tab(tab);
        if self.add_to_layer
            && let (Some(s), Some(i)) = (shape, self.active_layer)
            && let Some(l) = self.grade.layers.get_mut(i)
        {
            l.masks.push(Mask::new(s));
            self.active_mask = l.masks.len() - 1;
            self.add_to_layer = false;
            return;
        }
        let layer = Layer {
            name: self.next_name(label),
            masks: shape.map(Mask::new).into_iter().collect(),
            ..Layer::default()
        };
        self.push_layer(layer);
    }

    fn push_layer(&mut self, layer: Layer) {
        self.grade.layers.push(layer);
        self.active_layer = Some(self.grade.layers.len() - 1);
        self.active_mask = 0;
    }

    /// Matches the picture as it looks now to a chosen photo. False if none is chosen.
    fn choose_reference(&mut self, layer: usize) -> bool {
        let Some(path) = rfd::FileDialog::new().add_filter("Photos", &export::PHOTO_EXTENSIONS).pick_file() else {
            return false;
        };
        match export::open_photo(&path) {
            Ok(o) => {
                // The match starts from everything below this layer.
                let below = Grade { layers: self.grade.layers[..layer].to_vec(), ..self.grade.clone() };
                let now = pipeline::render(&self.preview, &below, &self.bank, false);
                let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                self.grade.layers[layer].grade.shot = Some(ShotMatch::new(name, &now.px, &o.picture.px));
                true
            }
            Err(e) => {
                self.say(format!("That file could not be opened: {e}"));
                false
            }
        }
    }

    fn turned_size(&self) -> (f32, f32) {
        let (w, h) = (self.preview.w as f32, self.preview.h as f32);
        if self.grade.crop.turns % 2 == 1 { (h, w) } else { (w, h) }
    }

    fn framing(&self) -> bool {
        self.tool == Tool::Crop && self.sub("crop") == 0
    }

    fn overlay_layer(&self) -> Option<usize> {
        if self.tool == Tool::Masks && self.show_mask { self.active_layer } else { None }
    }

    /// The picture shown for the crop box: turned and straightened, but not cut.
    fn uncut(&self) -> Picture {
        self.grade.crop.geometry(self.preview.w, self.preview.h, false).apply(&self.preview)
    }

    fn sub(&self, key: &'static str) -> usize {
        self.subs.get(key).copied().unwrap_or(0)
    }

    /// Takes in what the worker finished, and hands it the edit if that has changed.
    fn refresh_view(&mut self, ctx: &Context) {
        while let Ok(d) = self.worker.done.try_recv() {
            self.worker.pending = self.worker.pending.saturating_sub(1);
            if d.view.size[0] == 0 {
                continue;
            }
            if self.run.is_some() {
                continue;
            }
            store_texture(ctx, &mut self.view_tex, "view", d.view);
            if let Some(b) = d.before {
                store_texture(ctx, &mut self.orig_tex, "original", b);
            }
            if let Some(s) = d.scope {
                store_texture(ctx, &mut self.scope_tex, "scope", s);
            }
        }
        let key = ViewKey {
            framing: self.framing(),
            overlay: self.overlay_layer(),
            compare: self.compare || self.hold_original,
            frame: self.frame(),
            scope: self.scope,
            picture: self.picture_id,
            head: self.head,
        };
        // Something held down means more changes are coming, so answer quickly until it
        // lets go, and then once exactly. A click that changes nothing renders nothing.
        let held = ctx.input(|i| i.pointer.any_down() || !i.keys_down.is_empty());
        let same = self.drawn.as_ref() == Some(&self.grade) && self.drawn_key == Some(key) && self.drawn_project.as_ref() == Some(&self.project);
        if self.run.is_some() || (same && (held || !self.drawn_draft)) {
            return;
        }
        let index = self.timeline_index();
        // A transition or a video overlay needs the whole compositor, which decodes.
        let overlaid = index.items_in(&self.project.items, self.head, self.head + 1).into_iter().any(|i| matches!(self.project.items[i].body, crate::timeline::Body::Overlay { .. }));
        let mixing = overlaid || index.shot(&self.project, self.head).is_some_and(|s| s.blend.is_some());
        let scene = mixing.then(|| self.scene());
        let draft = held && !key.framing && key.overlay.is_none();
        let job = Job {
            picture: self.preview.clone(),
            grade: self.grade.clone(),
            bank: self.bank.clone(),
            framing: key.framing,
            overlay: key.overlay,
            compare: key.compare,
            frame: key.frame,
            scope: key.scope,
            project: Arc::new(self.project.clone()),
            index,
            head: self.head,
            scene,
            draft,
        };
        if self.worker.jobs.send(job).is_ok() {
            self.worker.pending += 1;
        }
        self.drawn = Some(self.grade.clone());
        self.drawn_project = Some(self.project.clone());
        self.drawn_key = Some(key);
        self.drawn_draft = draft;
    }

    /// Saves a step once the pointer lifts, so a slider's drag is one step, not hundreds.
    fn note_history(&mut self, ctx: &Context) {
        self.sync_active();
        if ctx.input(|i| i.pointer.any_down()) || self.project == self.history[self.history_at].1 {
            return;
        }
        let label = self.tab_name().to_string();
        self.history.truncate(self.history_at + 1);
        self.history.push((label, self.project.clone()));
        if self.history.len() > HISTORY_LIMIT {
            self.history.remove(0);
        }
        self.history_at = self.history.len() - 1;
    }

    fn go_to_history(&mut self, at: usize) {
        self.history_at = at.min(self.history.len() - 1);
        self.project = self.history[self.history_at].1.clone();
        // Nothing to carry back: the clip's grade is the one being restored.
        let keep = self.active.filter(|id| self.project.clips.iter().any(|c| c.id == *id));
        self.active = None;
        self.clip = self.clip.take().filter(|_| keep.is_some());
        self.needs_activate = true;
        if self.active_layer.is_some_and(|i| i >= self.grade.layers.len()) {
            self.active_layer = None;
        }
    }

    fn undo(&mut self) {
        if self.history_at > 0 {
            self.go_to_history(self.history_at - 1);
        }
    }

    fn redo(&mut self) {
        if self.history_at + 1 < self.history.len() {
            self.go_to_history(self.history_at + 1);
        }
    }

    fn tab_name(&self) -> &'static str {
        let tabs = self.tabs();
        match self.tool {
            Tool::Masks => "Masks",
            Tool::Presets => "Preset",
            Tool::Luts => "LUTs",
            Tool::Crop => tabs[self.sub("crop").min(tabs.len() - 1)],
            t => tabs[self.sub(key_of(t)).min(tabs.len() - 1)],
        }
    }

    fn top_bar(&mut self, ui: &mut Ui, r: Rect) {
        let y = r.center().y;
        let side = 40.0;
        if round_button(ui, pos2(r.left() + side / 2.0, y), side, Icon::ArrowLeft01, "Open and batch").clicked() {
            self.sheet = Some(Sheet::Home);
        }
        if round_button(ui, pos2(r.right() - side / 2.0, y), side, Icon::Share01, "Export").clicked() {
            self.sheet = Some(Sheet::Export);
        }
        let cell = 40.0;
        let n = 5.0;
        let bar = Rect::from_center_size(pos2(r.center().x, y), vec2(cell * n + 12.0, 40.0));
        ui.painter().rect_filled(bar, 20, BUTTON);
        let at = |i: f32| Rect::from_min_size(pos2(bar.left() + 6.0 + cell * i, bar.top()), vec2(cell, 40.0));
        if icon_button(ui, at(0.0), Icon::Undo02, "Undo", false).clicked() {
            self.undo();
        }
        if icon_button(ui, at(1.0), Icon::Redo02, "Redo", false).clicked() {
            self.redo();
        }
        if icon_button(ui, at(2.0), Icon::SquareSplitHorizontal, "Before and after", self.compare).clicked() {
            self.compare = !self.compare;
        }
        if icon_button(ui, at(3.0), Icon::History, "Undo history", false).clicked() {
            self.sheet = Some(Sheet::History);
        }
        if icon_button(ui, at(4.0), Icon::ChartHistogram, "Scopes", self.scope.is_some()).clicked() {
            self.toggle_scope();
        }
    }

    fn photo(&mut self, ui: &mut Ui, area: Rect) {
        let Some(view) = &self.view_tex else { return };
        let (iw, ih) = (view.size()[0] as f32, view.size()[1] as f32);
        let s = (area.width() / iw).min(area.height() / ih);
        let rect = Rect::from_center_size(area.center(), vec2(iw * s, ih * s));
        let p = ui.painter().with_clip_rect(area);
        let uv = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));
        p.image(view.id(), rect, uv, Color32::WHITE);
        let playing = self.run.is_some();
        // Held down, the button under the picture shows it untouched.
        if self.hold_original
            && let Some(orig) = &self.orig_tex
        {
            p.image(orig.id(), rect, uv, Color32::WHITE);
        }

        let framing = self.framing();
        let on_photo = framing || self.tool == Tool::Masks;
        // While playing, the bars are already in the picture.
        let ratio = if framing || playing { 0.0 } else { self.grade.frame_ratio };
        let (bx, by) = pipeline::frame_bars(rect.width(), rect.height(), ratio);
        if by > 0.0 {
            p.rect_filled(Rect::from_min_size(rect.min, vec2(rect.width(), by)), 0.0, Color32::BLACK);
            p.rect_filled(Rect::from_min_size(pos2(rect.left(), rect.bottom() - by), vec2(rect.width(), by)), 0.0, Color32::BLACK);
        }
        if bx > 0.0 {
            p.rect_filled(Rect::from_min_size(rect.min, vec2(bx, rect.height())), 0.0, Color32::BLACK);
            p.rect_filled(Rect::from_min_size(pos2(rect.right() - bx, rect.top()), vec2(bx, rect.height())), 0.0, Color32::BLACK);
        }

        if self.compare
            && !self.hold_original
            && !playing
            && let Some(orig) = &self.orig_tex
        {
            let split_x = rect.left() + rect.width() * self.split;
            let left = Rect::from_min_max(rect.min, pos2(split_x, rect.bottom()));
            let left_uv = Rect::from_min_max(Pos2::ZERO, pos2(self.split, 1.0));
            p.image(orig.id(), left, left_uv, Color32::WHITE);
            p.line_segment([pos2(split_x, rect.top()), pos2(split_x, rect.bottom())], Stroke::new(1.5, Color32::WHITE));
            if !on_photo {
                let grab = Rect::from_center_size(rect.center(), vec2(rect.width(), rect.height()));
                let handle = ui.interact(grab, ui.scope_id().with("split"), Sense::click_and_drag());
                if let Some(pos) = handle.interact_pointer_pos() {
                    self.split = ((pos.x - rect.left()) / rect.width()).clamp(0.02, 0.98);
                }
            }
        }

        if playing {
            // Handles would sit on a picture that has moved on.
        } else if framing {
            self.crop_overlay(ui, rect);
        } else if self.tool == Tool::Masks {
            self.mask_overlay(ui, rect);
        }
        self.scope_overlay(ui, area);
    }

    /// The scope floats over the corner of the picture. Clicking moves to the next kind.
    fn scope_overlay(&mut self, ui: &mut Ui, area: Rect) {
        let (Some(kind), Some(tex)) = (self.scope, &self.scope_tex) else { return };
        let round = kind == Kind::Vector;
        let size = if round { Vec2::splat(VECTOR_SIDE) } else { vec2(SCOPE_W, SCOPE_H) };
        let r = Rect::from_min_size(pos2(area.right() - size.x - 4.0, area.top() + 4.0), size);
        let tex_id = tex.id();
        let resp = ui.interact(r, ui.scope_id().with("scope"), Sense::click()).on_hover_text(kind.name());
        if resp.clicked() {
            self.scope = Some(kind.next());
        }
        let p = ui.painter().with_clip_rect(area);
        let uv = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));
        if round {
            let c = r.center();
            let radius = VECTOR_SIDE / 2.0;
            p.circle_filled(c, radius, Color32::from_black_alpha(190));
            p.image(tex_id, r, uv, Color32::WHITE);
            let hair = Stroke::new(1.0, Color32::from_white_alpha(40));
            p.circle_stroke(c, radius - 1.0, hair);
            p.circle_stroke(c, radius * 0.5, hair);
            p.line_segment([c - vec2(radius, 0.0), c + vec2(radius, 0.0)], hair);
            p.line_segment([c - vec2(0.0, radius), c + vec2(0.0, radius)], hair);
            for k in 0..36 {
                let a = (k as f32 * 10.0).to_radians();
                let d = vec2(a.cos(), a.sin());
                p.line_segment([c + d * (radius - 4.0), c + d * (radius - 1.0)], hair);
            }
            let spot = |rgb: [f32; 3]| {
                let [u, v] = scopes::chroma(rgb);
                c + vec2(u, -v) * radius * scopes::VECTOR_SCALE
            };
            let targets: [([f32; 3], Color32); 6] = [
                ([1.0, 0.0, 0.0], CHANNELS[1]),
                ([1.0, 0.0, 1.0], Color32::from_rgb(235, 90, 225)),
                ([0.0, 0.0, 1.0], CHANNELS[3]),
                ([0.0, 1.0, 1.0], Color32::from_rgb(90, 225, 235)),
                ([0.0, 1.0, 0.0], CHANNELS[2]),
                ([1.0, 1.0, 0.0], Color32::from_rgb(235, 225, 90)),
            ];
            for (rgb, col) in targets {
                let at = spot(rgb);
                p.rect_stroke(Rect::from_center_size(at, Vec2::splat(5.0)), 0, Stroke::new(1.0, col), StrokeKind::Middle);
            }
            // The line that healthy skin tones gather along, whatever the person's colouring.
            let skin = spot([0.82, 0.58, 0.45]) - c;
            p.line_segment([c, c + skin.normalized() * (radius - 2.0)], Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 190, 150, 90)));
        } else {
            p.rect_filled(r, 6, Color32::from_black_alpha(190));
            p.image(tex_id, r.shrink(2.0), uv, Color32::WHITE);
            for k in 1..4 {
                let y = r.top() + r.height() * k as f32 / 4.0;
                p.hline(r.x_range(), y, Stroke::new(1.0, Color32::from_white_alpha(18)));
            }
            if kind == Kind::Parade {
                for k in 1..3 {
                    p.vline(r.left() + r.width() * k as f32 / 3.0, r.y_range(), Stroke::new(1.0, Color32::from_white_alpha(30)));
                }
            }
            // Ten-bit code values, the numbers a colourist reads a scope by.
            if kind != Kind::Histogram {
                for (k, label) in ["1023", "768", "512", "256", "0"].iter().enumerate() {
                    let y = r.top() + 5.0 + (r.height() - 10.0) * k as f32 / 4.0;
                    p.text(pos2(r.left() + 3.0, y), Align2::LEFT_CENTER, *label, regular(8.5), Color32::from_white_alpha(170));
                }
            }
        }
    }

    /// Picks up the frame the decoder found for the paused picture.
    fn tick_clip(&mut self, ctx: &Context) {
        let Some(c) = &mut self.clip else { return };
        let mut newest = None;
        while let Ok(got) = c.feed.frames.try_recv() {
            newest = Some(got);
        }
        if let Some((f, p)) = newest {
            let exact = f == c.frame;
            if exact {
                c.arrived = Some(f);
            }
            // A nearby frame beats a frozen picture while a drag outruns the decoder.
            if exact || c.arrived != Some(c.frame) {
                self.preview = p.clone();
                self.full = p;
                self.picture_id += 1;
            }
            ctx.request_repaint();
        } else if self.run.is_none() && c.arrived != Some(c.frame) {
            // Only while a frame is on its way; a paused editor shouldn't use the processor.
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    /// Play button, time and scrub bar along the bottom of the picture.
    fn transport(&mut self, ui: &mut Ui, r: Rect) {
        let (frame, frames, playing, fps) = (self.head, self.project.total(), self.run.is_some(), self.project.fps);
        let clock = |f: u32| {
            let s = (f as f64 / fps.max(1.0)) as u32;
            format!("{:02}:{:02}", s / 60, s % 60)
        };
        let y = r.center().y;
        let button = Rect::from_center_size(pos2(r.left() + 16.0, y), Vec2::splat(30.0));
        if icon_button(ui, button, if playing { Icon::Pause } else { Icon::Play }, "Play", false).clicked() {
            self.play_or_pause("the play button");
        }
        let p = ui.painter();
        p.text(pos2(r.left() + 38.0, y), Align2::LEFT_CENTER, clock(frame), semi(12.0), Color32::WHITE);
        p.text(pos2(r.right(), y), Align2::RIGHT_CENTER, clock(frames.saturating_sub(1)), semi(12.0), Color32::WHITE);
        let bar = Rect::from_min_max(pos2(r.left() + 82.0, y - 12.0), pos2(r.right() - 44.0, y + 12.0));
        let resp = ui.interact(bar, ui.scope_id().with("scrub"), Sense::click_and_drag());
        let t = frame as f32 / frames.saturating_sub(1).max(1) as f32;
        if let Some(at) = resp.interact_pointer_pos()
            && (resp.dragged() || resp.clicked())
        {
            let f = ((at.x - bar.left()) / bar.width()).clamp(0.0, 1.0);
            self.seek_head((f * frames.saturating_sub(1) as f32).round() as u32);
        }
        let p = ui.painter();
        let track = Rect::from_min_max(pos2(bar.left(), y - 2.0), pos2(bar.right(), y + 2.0));
        p.rect_filled(track, 2, Color32::from_white_alpha(60));
        p.rect_filled(Rect::from_min_max(track.min, pos2(bar.left() + bar.width() * t, track.bottom())), 2, Color32::WHITE);
    }

    /// Press and hold to see the picture before any edits.
    fn original_button(&mut self, ui: &mut Ui, area: Rect) {
        let w = text_width(ui, "Original Image", semi(12.5)) + 28.0;
        let r = Rect::from_min_size(pos2(area.left() + 6.0, area.bottom() - 36.0), vec2(w, 30.0));
        let resp = ui.interact(r, ui.scope_id().with("original"), Sense::click_and_drag());
        self.hold_original = resp.is_pointer_button_down_on();
        let p = ui.painter();
        p.rect_filled(r, 15, if self.hold_original { ACCENT } else { Color32::from_rgba_unmultiplied(0x2C, 0x2C, 0x2E, 225) });
        p.text(r.center(), Align2::CENTER_CENTER, "Original Image", semi(12.5), Color32::WHITE);
    }

    /// The bar along the top of the panel: drag it to trade picture for controls.
    fn panel_handle(&mut self, ui: &mut Ui, panel: Rect) {
        let bar = Rect::from_center_size(pos2(panel.center().x, panel.top()), vec2(120.0, 16.0));
        let resp = ui.interact(bar, ui.scope_id().with("panel-handle"), Sense::drag());
        if resp.dragged() {
            self.panel_extra -= resp.drag_delta().y;
        }
        let tint = if resp.hovered() || resp.dragged() { Color32::WHITE } else { Color32::from_white_alpha(190) };
        ui.painter().rect_filled(Rect::from_center_size(pos2(panel.center().x, panel.top() + 1.0), vec2(38.0, 4.0)), 2, tint);
    }

    fn tool_panel(&mut self, ui: &mut Ui, r: Rect) {
        card(ui, r);
        let tabs = self.tabs();
        let key = key_of(self.tool);
        let masks = self.tool == Tool::Masks;
        let cur = if masks {
            self.active_layer.and_then(|i| self.grade.layers.get(i)).map_or(usize::MAX, tab_of_layer)
        } else {
            self.sub(key)
        };
        let row = Rect::from_min_size(r.min + vec2(8.0, 8.0), vec2(r.width() - 16.0, TAB_ROW_H));
        let mut clicked = None;
        ui.scope_builder(UiBuilder::new().max_rect(row).id_salt(("tabs", key)), |ui| {
            let hovered = ui.rect_contains_pointer(row);
            egui::ScrollArea::horizontal()
                .id_salt(("tab-row", key))
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    if hovered {
                        let dy = ui.input(|i| i.smooth_scroll_delta.y);
                        if dy != 0.0 {
                            ui.scroll_with_delta(vec2(dy, 0.0));
                        }
                    }
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        for (i, t) in tabs.iter().enumerate() {
                            let w = text_width(ui, t, semi(13.0)) + 26.0;
                            let (cell, _) = ui.allocate_exact_size(vec2(w, TAB_ROW_H), Sense::hover());
                            let resp = pill(ui, cell, t, i == cur, 13.0);
                            if resp.clicked() {
                                clicked = Some(i);
                            }
                            // Brings the chosen tab into view when the tool or tab changes.
                            if i == cur && self.tab_shown != Some((self.tool, cur)) {
                                self.tab_shown = Some((self.tool, cur));
                                resp.scroll_to_me(None);
                            }
                        }
                    });
                });
        });
        if let Some(i) = clicked {
            if masks {
                self.add_tab(i);
            } else {
                self.subs.insert(key, i);
            }
        }

        let n = TOOLS.len() as f32;
        let size = 40.0;
        let gap = 4.0;
        let total = n * size + (n - 1.0) * gap;
        let cy = r.bottom() - 8.0 - size / 2.0 - 2.0;
        let capsule = Rect::from_center_size(pos2(r.center().x, cy), vec2(total + 12.0, size + 6.0));
        ui.painter().rect_filled(capsule, 23, BUTTON);
        let mut bx = r.center().x - total / 2.0;
        for t in TOOLS {
            let rect = Rect::from_center_size(pos2(bx + size / 2.0, cy), Vec2::splat(size));
            let resp = ui.interact(rect, ui.scope_id().with(t.tip()), Sense::click()).on_hover_text(t.tip());
            let on = self.tool == t;
            let p = ui.painter();
            if on {
                p.circle_filled(rect.center(), size / 2.0, RAISED);
                p.circle_stroke(rect.center(), size / 2.0, Stroke::new(1.5, ACCENT));
            } else if resp.hovered() {
                p.circle_filled(rect.center(), size / 2.0, BUTTON_EDGE);
            }
            let inner = Rect::from_center_size(rect.center(), Vec2::splat(22.0));
            match t.colours().filter(|_| !on) {
                Some(c) => icons::paint_gradient(p, t.icon(), inner, c),
                None => icons::paint(p, t.icon(), inner, Color32::WHITE),
            }
            if resp.clicked() {
                self.tool = t;
            }
            bx += size + gap;
        }
    }

    fn tabs(&self) -> Vec<&'static str> {
        match self.tool {
            Tool::Filters => COLOR_TABS.to_vec(),
            Tool::Film => FILM_TABS.to_vec(),
            Tool::Effects => EFFECT_TABS.to_vec(),
            Tool::Masks => MASK_TABS.to_vec(),
            Tool::Presets => vec!["All", "My Presets", "Cinema", "Deep Looks", "Studio", "Film", "Mood", "Plugins"],
            Tool::Crop => vec!["Crop"],
            Tool::Luts => vec!["All", "Favorites", "User LUTs", "Print", "Film", "Creative"],
        }
    }

    /// Title, then sort, reset and information buttons.
    fn panel_header(&mut self, ui: &mut Ui, r: Rect, title: &str) {
        let y = r.top() + 11.0;
        ui.painter().text(pos2(r.left(), y), Align2::LEFT_CENTER, title, bold(15.0), Color32::WHITE);
        let cell = 30.0;
        let at = |i: f32| Rect::from_center_size(pos2(r.right() - cell * (i + 0.5), y), Vec2::splat(cell));
        if title == "Curves" {
            let options = ["Reset All Curves", "About Curves"];
            match menu_button(ui, at(0.0), Icon::MoreHorizontal, "Curve options", true, &options) {
                Some(0) => {
                    self.target().curves = Default::default();
                    self.pick.curve_point = None;
                }
                Some(_) => self.sheet = Some(Sheet::About(panels::about(title).to_string(), title.to_string())),
                None => {}
            }
            return;
        }
        if icon_button(ui, at(0.0), Icon::InformationCircle, "About this tool", false).clicked() {
            self.sheet = Some(Sheet::About(panels::about(title).to_string(), title.to_string()));
        }
        if icon_button(ui, at(1.0), Icon::RefreshCcw, "Reset this tool", false).clicked() {
            let g = self.target();
            panels::reset(g, title);
        }
        // Who these sliders are working on, with a way back to the whole picture.
        if let Some(l) = self.active_layer.and_then(|i| self.grade.layers.get(i))
            && !matches!(self.tool, Tool::Masks | Tool::Presets | Tool::Crop)
        {
            let label = format!("{} ×", short(&l.name, 18));
            let w = text_width(ui, &label, semi(11.5)) + 18.0;
            let b = Rect::from_min_size(pos2(at(1.0).left() - w - 6.0, y - 11.0), vec2(w, 22.0));
            if action(ui, b, &label, true, 11.5).clicked() {
                self.active_layer = None;
            }
        }
    }

    fn content_panel(&mut self, ui: &mut Ui, ctx: &Context, r: Rect) {
        card(ui, r);
        let inner = r.shrink2(vec2(16.0, 12.0));
        match self.tool {
            Tool::Presets => {
                ui.scope_builder(UiBuilder::new().max_rect(inner).id_salt("presets"), |ui| self.presets_panel(ui, ctx));
            }
            Tool::Luts => {
                self.panel_header(ui, inner, "LUTs");
                self.scroll_body(ui, ctx, inner, "LUTs");
            }
            Tool::Masks => {
                self.panel_header(ui, inner, "Masks & Layers");
                self.scroll_body(ui, ctx, inner, "Masks");
            }
            Tool::Crop => {
                self.panel_header(ui, inner, "Crop");
                self.scroll_body(ui, ctx, inner, "Crop");
            }
            _ => {
                let tab = self.tab_name();
                self.panel_header(ui, inner, tab);
                self.scroll_body(ui, ctx, inner, tab);
            }
        }
    }

    fn scroll_body(&mut self, ui: &mut Ui, ctx: &Context, inner: Rect, tab: &'static str) {
        let body = Rect::from_min_max(pos2(inner.left(), inner.top() + 30.0), inner.max);
        let tool = self.tool;
        let scrolled = tab != "RGB Curves";
        ui.scope_builder(UiBuilder::new().max_rect(body).id_salt(("body", tab)), |ui| {
            ui.set_clip_rect(body.intersect(ui.clip_rect()));
            let draw = |app: &mut Self, ui: &mut Ui| match tool {
                Tool::Luts => app.luts_body(ui),
                Tool::Masks => app.masks_body(ui, ctx),
                Tool::Crop => app.crop_body(ui),
                _ => app.tool_body(ui, tab),
            };
            if scrolled {
                egui::ScrollArea::vertical().id_salt(("scroll", tab)).auto_shrink([false, false]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    ui.set_width(ui.available_width() - 6.0);
                    draw(self, ui);
                });
            } else {
                draw(self, ui);
            }
        });
    }

    /// Writes how evenly frames reached the screen, then closes.
    fn finish_play_test(&mut self, ctx: &Context) {
        let playing = self.run.is_some();
        let Some((out, shown)) = &self.play_log else { return };
        if playing || shown.len() < 2 {
            ctx.request_repaint();
            return;
        }
        let mut gaps: Vec<f64> = shown.windows(2).map(|w| (w[1] - w[0]).as_secs_f64() * 1000.0).collect();
        let total: f64 = gaps.iter().sum();
        gaps.sort_by(f64::total_cmp);
        let at = |q: f64| gaps[((gaps.len() - 1) as f64 * q) as usize];
        let fps = self.project.fps;
        let long = gaps.iter().filter(|g| **g > 2000.0 / fps.max(1.0)).count();
        let text = format!(
            "shown {} of {} frames, {:.1} fps of {:.1}. gaps ms: median {:.1}, p95 {:.1}, worst {:.1}. hitches over 2 frames: {}. played at {} px\n",
            shown.len(),
            self.project.total(),
            gaps.len() as f64 / (total / 1000.0).max(1e-3),
            fps,
            at(0.5),
            at(0.95),
            at(1.0),
            long,
            self.play_side,
        );
        let _ = std::fs::write(out, text);
        self.play_log = None;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    /// Times the app's own work each frame, while a stress project is on screen.
    fn note_frame(&mut self, ctx: &Context) {
        let Some((out, spent)) = &mut self.ui_log else { return };
        if let Some(b) = self.frame_began {
            spent.push(b.elapsed().as_secs_f32() * 1000.0);
        }
        ctx.request_repaint();
        if spent.len() >= UI_BENCH_FRAMES {
            let settled = &mut spent[UI_BENCH_SKIP..];
            settled.sort_by(f32::total_cmp);
            let text = format!("app work per frame: median {:.2} ms, worst {:.2} ms\n", settled[settled.len() / 2], settled[settled.len() - 1]);
            let _ = std::fs::write(out, text);
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn drive_screenshot(&mut self, ctx: &Context) {
        self.note_frame(ctx);
        let Some(path) = self.screenshot.clone() else { return };
        ctx.request_repaint();
        self.frames += 1;
        let settled = self.clip.as_ref().is_none_or(|c| c.arrived == Some(c.frame)) && self.strips_ready();
        let idle = settled && self.worker.pending == 0 && self.tracking.is_none() && self.task.is_none() && self.run.is_none();
        if self.frames >= 120 && self.script.is_empty() && !self.shot_sent && idle {
            self.shot_sent = true;
            if let (Some(p), Ok(text)) = (&self.dump, serde_json::to_string_pretty(&self.grade)) {
                let _ = std::fs::write(p, text);
                if let Ok(whole) = serde_json::to_string_pretty(&self.project) {
                    let _ = std::fs::write(format!("{p}.project"), whole);
                }
            }
            let done = self.shot_done.clone();
            ctx.request_screenshot(move |img| {
                let p = PathBuf::from(&path);
                if let Some(dir) = p.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| c.to_array()).collect();
                let _ = image::save_buffer(&p, &bytes, img.size[0] as u32, img.size[1] as u32, image::ExtendedColorType::Rgba8);
                done.store(true, Ordering::SeqCst);
            });
        }
        if self.shot_done.load(Ordering::SeqCst) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// Lookups for the project as it stands, rebuilt only when it has actually changed.
    fn timeline_index(&self) -> Arc<Index> {
        match &self.index_cache {
            Some((p, i)) if p.clips.same(&self.project.clips) && p.items.same(&self.project.items) => i.clone(),
            _ => Arc::new(Index::of(&self.project)),
        }
    }

    fn keep_index(&mut self) {
        let fresh = self.index_cache.as_ref().is_some_and(|(p, _)| p.clips.same(&self.project.clips) && p.items.same(&self.project.items));
        if !fresh {
            self.index_cache = Some((self.project.clone(), Arc::new(Index::of(&self.project))));
        }
    }

    fn toggle_scope(&mut self) {
        self.scope = match self.scope {
            Some(_) => None,
            None => Some(Kind::Parade),
        };
    }

    fn toast_view(&mut self, ui: &mut Ui, ctx: &Context, photo: Rect) {
        let Some((t, at)) = &self.toast else { return };
        if at.elapsed() < Duration::from_secs(3) {
            let p = ui.painter();
            let g = p.layout_no_wrap(t.clone(), semi(13.0), Color32::WHITE);
            let c = pos2(photo.center().x, photo.bottom() - 26.0);
            let r = Rect::from_center_size(c, g.size() + vec2(28.0, 14.0));
            p.rect_filled(r, 14, MENU);
            p.rect_stroke(r, 14, Stroke::new(1.0, BUTTON_EDGE), StrokeKind::Inside);
            p.galley(r.center() - g.size() / 2.0, g, Color32::WHITE);
            ctx.request_repaint_after(Duration::from_millis(200));
        } else {
            self.toast = None;
        }
    }
}

fn key_of(t: Tool) -> &'static str {
    match t {
        Tool::Filters => "filters",
        Tool::Film => "film",
        Tool::Effects => "effects",
        Tool::Masks => "masks",
        Tool::Presets => "presets",
        Tool::Crop => "crop",
        Tool::Luts => "luts",
    }
}

fn shape_of_tab(tab: usize) -> Option<Shape> {
    match tab {
        2 => Some(Shape::Linear),
        3 => Some(Shape::Radial),
        4 => Some(Shape::Rectangle),
        5 => Some(Shape::Draw),
        6 => Some(Shape::Curve),
        7 => Some(Shape::Qualifier),
        _ => None,
    }
}

fn tab_of_layer(l: &Layer) -> usize {
    match l.masks.first().map(|m| m.shape) {
        Some(Shape::Linear) => 2,
        Some(Shape::Radial) => 3,
        Some(Shape::Rectangle) => 4,
        Some(Shape::Draw) => 5,
        Some(Shape::Curve) => 6,
        Some(Shape::Qualifier) => 7,
        None if l.grade.shot.is_some() => SHOT_TAB,
        None => 1,
    }
}

fn lut_grade(id: &str, name: &str) -> Grade {
    Grade {
        luts: vec![LutRef { id: id.to_string(), name: name.to_string(), intensity: 1.0 }],
        ..Grade::default()
    }
}

fn stem(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "LUT".into())
}

fn short(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(n - 1).collect::<String>())
    }
}

fn upload(ctx: &Context, name: &str, p: &Picture) -> TextureHandle {
    ctx.load_texture(name, image_of(p), TextureOptions::LINEAR)
}

fn store_texture(ctx: &Context, slot: &mut Option<TextureHandle>, name: &str, img: ColorImage) {
    match slot {
        Some(t) if t.size() == img.size => t.set(img, TextureOptions::LINEAR),
        _ => *slot = Some(ctx.load_texture(name, img, TextureOptions::LINEAR)),
    }
}

impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 1.0]
    }

    fn raw_input_hook(&mut self, _ctx: &Context, raw: &mut egui::RawInput) {
        self.frame_began = Some(Instant::now());
        // One scripted step each frame, once the window has settled.
        if self.frames >= 20
            && let Some(e) = self.script.pop()
        {
            raw.events.push(e);
        }
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        for path in dropped {
            let is_cube = path.extension().is_some_and(|x| x.eq_ignore_ascii_case("cube"));
            if is_cube {
                if self.import_lut_file(&ctx, &path) {
                    self.say("LUT added to your library");
                }
            } else {
                self.import(&path, true);
            }
        }

        if std::mem::take(&mut self.needs_activate) && self.run.is_none() {
            self.activate(&ctx);
        }
        kit::note_input(&ctx);
        self.keep_index();
        self.tick_play(&ctx);
        self.tick_clip(&ctx);
        self.tick_tasks(&ctx);
        self.run_keys(&ctx);
        self.refresh_view(&ctx);
        self.note_history(&ctx);
        self.tick_autosave();
        self.restore_offer(&ctx);

        // Preset tiles show the picture under the playhead, redrawn only when they're wanted.
        let media = self.active.and_then(|id| self.project.clips.iter().find(|c| c.id == id)).map(|c| c.media);
        if matches!(self.tool, Tool::Presets | Tool::Luts) && self.thumbs_for != media && (self.clip.is_none() || self.frame() < u32::MAX) {
            self.thumbs_for = media;
            self.rebuild_all_thumbs(&ctx);
        }

        let screen = ui.max_rect();
        if screen.width() >= EDIT_LAYOUT {
            let photo = self.edit_layout(ui, &ctx, screen);
            self.task_view(ui, &ctx, photo);
            self.sheets(&ctx);
            self.toast_view(ui, &ctx, photo);
            self.finish_play_test(&ctx);
            self.drive_screenshot(&ctx);
            return;
        }
        let full = screen.shrink(10.0);
        let wide = screen.width() >= WIDE_LAYOUT;
        let column = if wide {
            Rect::from_min_max(pos2(full.right() - SIDE_COLUMN, full.top()), full.max)
        } else {
            full
        };
        let top = if wide {
            Rect::from_min_size(full.min, vec2(full.width() - SIDE_COLUMN - 10.0, TOP_H))
        } else {
            Rect::from_min_size(full.min, vec2(full.width(), TOP_H))
        };
        let tools = Rect::from_min_max(pos2(column.left(), column.bottom() - BOTTOM_H), column.max);
        let content = if wide {
            Rect::from_min_max(column.min, pos2(column.right(), tools.top() - 8.0))
        } else {
            let preset = (screen.height() * PRESET_PANEL_SHARE).clamp(PRESET_PANEL_MIN, PRESET_PANEL_MAX);
            let h = match self.tool {
                Tool::Presets => preset,
                _ => {
                    let rest = (screen.height() * TOOL_PANEL_SHARE).clamp(TOOL_PANEL_MIN, TOOL_PANEL_MAX);
                    let most = (screen.height() - BOTTOM_H - TOP_H - PHOTO_MIN).max(PANEL_MIN);
                    self.panel_extra = self.panel_extra.clamp(PANEL_MIN - rest, (most - rest).max(0.0));
                    rest + self.panel_extra
                }
            };
            Rect::from_min_max(pos2(column.left(), tools.top() - 8.0 - h), pos2(column.right(), tools.top() - 8.0))
        };
        let bottom = if wide { full.bottom() } else { content.top() - 8.0 };
        let right = if wide { column.left() - 10.0 } else { full.right() };
        let mut photo = Rect::from_min_max(pos2(full.left(), top.bottom() + 6.0), pos2(right, bottom));
        if self.clip.is_some() || self.project.clips.len() > 1 {
            let bar = Rect::from_min_max(pos2(photo.left(), photo.bottom() - 34.0), photo.max);
            photo.max.y = bar.top() - 4.0;
            self.photo(ui, photo);
            self.transport(ui, bar);
        } else {
            self.photo(ui, photo);
        }
        self.top_bar(ui, top);
        self.content_panel(ui, &ctx, content);
        if !wide && self.tool != Tool::Presets {
            self.panel_handle(ui, content);
        }
        if matches!(self.tool, Tool::Masks | Tool::Crop) {
            self.hold_original = false;
        } else {
            self.original_button(ui, photo);
        }
        self.tool_panel(ui, tools);
        self.task_view(ui, &ctx, photo);
        self.sheets(&ctx);
        self.toast_view(ui, &ctx, photo);
        self.finish_play_test(&ctx);
        self.drive_screenshot(&ctx);
    }
}
