//! The Color, Film and Effects tool panels.

use super::kit::*;
use super::App;
use crate::color::{self, Source, ToneMap};
use crate::crop::Crop;
use crate::curves::{Curves, Mode};
use crate::effects::{Crt, DIFFUSERS, Diffusion, TEXTURES, Texture};
use crate::film::{FilmLook, Foliage, GAUGES, OUTPUTS, PAPERS, PRINTS, PrintSim, SPLITS, STOCKS, Split, Super8, TETRA, Tetra};
use crate::icons::{self, Icon, Tool};
use crate::scopes::{self, Spread};
use crate::pipeline::{Grade, Tint};
use crate::tools::{BANDS, ColorLuminance, HslMixer, Mixer, SLICES, Separation, Slices, Wheels, ZONES, Zones};
use eframe::egui::{Align2, Color32, DragValue, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};

pub const COLOR_TABS: [&str; 15] = [
    "Color Management",
    "LUTs",
    "Balance",
    "Light",
    "HDR Grade",
    "Color Wheels",
    "Tone Wheels",
    "Curves",
    "RGB Curves",
    "Color Slice",
    "Channel Mixer",
    "Color Separation",
    "Details",
    "HSL Mixer",
    "Color Luminance",
];
pub const FILM_TABS: [&str; 11] = [
    "Film Look",
    "Super 8",
    "Film Density",
    "Film Saturation",
    "Tetra Color Mix",
    "Filmic Split",
    "Bleach Bypass",
    "Saturate",
    "Technicolor 3-Strip",
    "Greens/Foliage",
    "Print Simulation",
];
pub const EFFECT_TABS: [&str; 11] = [
    "Grain",
    "Halation",
    "Diffusion",
    "Bloom",
    "Gate Weave",
    "Cinematic Frame",
    "CRT Emulation",
    "Textures",
    "Chromatic Aberration",
    "Lens Distortion",
    "Vignette",
];

const FRAME_RATIOS: [(&str, f32); 7] = [
    ("Off", 0.0),
    ("1:1", 1.0),
    ("4:5", 0.8),
    ("16:9", 16.0 / 9.0),
    ("1.85:1", 1.85),
    ("2.39:1", 2.39),
    ("2.76:1", 2.76),
];
const WHEELS: [&str; 4] = ["Lift", "Gamma", "Gain", "Offset"];
const JOG_SPEED: f32 = 0.0015;
const WHEEL_REACH: f32 = 0.5;
const WHEEL_SIDE: f32 = 132.0;
const TONE_WHEEL_SIDE: f32 = 92.0;
const MODE_BUTTON: f32 = 34.0;
const GRAPH_H: f32 = 146.0;
const RGB_GRAPH_H: f32 = 170.0;
const CHIP_H: f32 = 56.0;
const HUE_DOTS: [f32; 6] = [0.0, 60.0, 120.0, 180.0, 240.0, 300.0];
/// A dot lands on a point already this close instead of adding another.
const DOT_REACH: f32 = 0.02;
const BOX_W: f32 = 62.0;
const ARC_STEPS: usize = 8;

fn dot(c: [f32; 3]) -> Color32 {
    let v = color::from_hsv(c);
    Color32::from_rgb((v[0] * 255.0) as u8, (v[1] * 255.0) as u8, (v[2] * 255.0) as u8)
}

fn slice_colours() -> Vec<Color32> {
    SLICES.iter().enumerate().map(|(i, s)| if i == 6 { dot([26.0, 0.45, 0.85]) } else { dot([s.1, 0.8, 0.95]) }).collect()
}

/// Colours along the short way round the wheel, for a gradient slider.
fn hue_arc(from: f32, to: f32) -> Vec<Color32> {
    let gap = color::hue_gap(to, from);
    (0..=ARC_STEPS).map(|i| dot([from + gap * i as f32 / ARC_STEPS as f32, 0.85, 0.95])).collect()
}

/// The two pictures on a curve mode's button: what it reads, and what it changes.
fn mode_icons(m: Mode) -> (Icon, Icon) {
    match m {
        Mode::HueHue => (Icon::Colors, Icon::Colors),
        Mode::HueSat => (Icon::Colors, Icon::Droplet),
        Mode::HueLum => (Icon::Colors, Icon::Sun03),
        Mode::LumSat => (Icon::Sun03, Icon::Droplet),
        Mode::SatSat => (Icon::Droplet, Icon::Droplet),
        Mode::SatLum => (Icon::Droplet, Icon::Sun03),
    }
}

/// A number in a dark box. Drag it sideways, or click and type.
fn number_box(ui: &mut Ui, r: Rect, v: &mut f32, min: f32, max: f32) -> bool {
    let w = &mut ui.visuals_mut().widgets;
    for s in [&mut w.inactive, &mut w.hovered, &mut w.active] {
        s.weak_bg_fill = Color32::BLACK;
        s.bg_fill = Color32::BLACK;
        s.bg_stroke = Stroke::new(1.0, BUTTON_EDGE);
        s.corner_radius = 7.into();
    }
    ui.put(r, DragValue::new(v).speed(0.25).range(min..=max).fixed_decimals(2)).changed()
}

fn band_colours() -> Vec<Color32> {
    BANDS.iter().map(|b| dot([b.1, 0.8, 0.95])).collect()
}

/// What each tool does, shown by the panel's information button.
pub fn about(tab: &str) -> &'static str {
    match tab {
        "Color Management" => "Converts log and HDR footage to a normal picture. Choose the camera format, then how highlights roll off. HDR clips set this for you. DaVinci Wide Gamut grades in a wider space and converts at the very end.",
        "LUTs" => "Stack several LUTs and set each one's intensity. They run in order, left to right.",
        "Balance" => "Temperature and tint, as an offset around 6500K.",
        "Light" => "Exposure in stops, contrast and the film curve's toe and shoulder, plus tone ranges and saturation.",
        "HDR Grade" => "Moves exposure and saturation by zone, from deep blacks to specular highlights, with soft edges between zones.",
        "Color Wheels" => "Lift for shadows, Gamma for midtones, Gain for highlights, Offset for everything. The strip under the wheel is the master. Lum Mix keeps the master from shifting colour.",
        "Tone Wheels" => "Tints shadows, midtones, highlights or the whole picture. Distance from the centre sets the strength.",
        "Curves" => "Six curves for colour: pick a mode, then press on the graph or tap a colour dot to place a point. Drag it, or type in the boxes. Double click a point to remove it. The pale line shows where the picture's colours sit.",
        "RGB Curves" => "Tone curves for luma, red, green and blue.",
        "Color Slice" => "Seven hue slices, including one just for skin. Density adds weight to a colour instead of darkening it.",
        "Channel Mixer" => "Rebuilds each output channel from the three inputs. Swap channels, or make a monochrome mix.",
        "Color Separation" => "Anchors a subject colour and pushes the rest of the palette away from it.",
        "Details" => "Clarity for broad local contrast, Structure for fine texture, Sharpness for edges, Dehaze for atmosphere.",
        "HSL Mixer" => "Shifts hue, saturation and value inside one colour range. Range sets how wide it reaches.",
        "Color Luminance" => "Brightens or darkens a colour while keeping its hue exactly where it is.",
        "Film Look" => "Aims for the character of a film stock and its print. These are tuned by eye, not measured from real film. Color picks the stock, gauge and print; Texture adds the physical side of film.",
        "Super 8" => "Small-gauge film: faded dyes, a scanned frame, flicker and dust.",
        "Film Density" => "Adds weight and richness to colours without crushing detail. The weights choose which channels take it.",
        "Film Saturation" => "Saturation worked out on a log scale, so strong colours compress instead of clipping.",
        "Tetra Color Mix" => "Leans one corner of the colour cube toward its neighbours. Red, green and blue each take a little of another channel; cyan, magenta and yellow give one up. Colours blend smoothly between corners, and greys stay grey.",
        "Filmic Split" => "Split-tone styles taken from how film stocks treat shadows and highlights.",
        "Bleach Bypass" => "Skipped silver: lower saturation and harder contrast.",
        "Saturate" => "Richer dyes: colours deepen as they saturate.",
        "Technicolor 3-Strip" => "Three-strip look: each colour record printed through its complement.",
        "Greens/Foliage" => "Adjusts greens only, from olive to emerald.",
        "Print Simulation" => "The limits of photographic paper: matte, neutral or glossy.",
        "Grain" => "Film grain, strongest in the midtones. It changes every frame on video.",
        "Halation" => "The red glow around bright areas, from light bouncing back through the film.",
        "Diffusion" => "Filters that scatter light. GlimmerGlass sparkles, Black Pro-Mist keeps blacks, Pro-Mist lifts them, CineBloom spreads wide.",
        "Bloom" => "A soft glow from the brightest parts of the picture.",
        "Gate Weave" => "The film's small wander in the gate. It only shows on video, because it happens between frames.",
        "Cinematic Frame" => "Letterbox or pillarbox bars, included in the export.",
        "CRT Emulation" => "Scanlines, a phosphor mask, screen curve and glow.",
        "Textures" => "Overlays paper, canvas, dust, scratches, light leaks or film burn.",
        "Chromatic Aberration" => "Separates red and blue toward the corners, like a simple lens.",
        "Lens Distortion" => "Bends straight lines outward or inward.",
        "Vignette" => "Darkens or lightens the corners.",
        "Masks & Layers" => "Each layer holds its own edit, limited by its masks. Pick a mask from the row below, then Edit Layer to grade just that area. On video, Tracking makes a mask follow its subject.",
        "Crop" => "Drag the box or its corners, choose a ratio, straighten, turn or flip. Masks stay on the same part of the picture.",
        _ => "",
    }
}

/// Puts one tool back to where it started, leaving the rest of the edit alone.
pub fn reset(g: &mut Grade, tab: &str) {
    let d = Grade::default();
    match tab {
        "Color Management" => g.cst = d.cst,
        "LUTs" => g.luts.clear(),
        "Balance" => (g.temp, g.tint, g.hue) = (0.0, 0.0, 0.0),
        "Light" => {
            (g.exposure, g.contrast, g.film_contrast, g.toe, g.shoulder) = (0.0, 1.0, 0.0, 0.0, 0.0);
            (g.brightness, g.highlight, g.shadow, g.whites, g.blacks) = (0.0, 0.0, 0.0, 0.0, 0.0);
            (g.gamma, g.saturation, g.vibrance, g.fade) = (0.0, 1.0, 0.0, 0.0);
        }
        "HDR Grade" => g.zones = Zones::default(),
        "Color Wheels" => g.wheels = Wheels::default(),
        "Tone Wheels" => {
            for t in [&mut g.shadows_tint, &mut g.midtones_tint, &mut g.highlights_tint, &mut g.global_tint] {
                *t = Tint::default();
            }
        }
        "Curves" => {
            let c = Curves::default();
            g.curves = Curves { master: g.curves.master.clone(), red: g.curves.red.clone(), green: g.curves.green.clone(), blue: g.curves.blue.clone(), ..c };
        }
        "RGB Curves" => {
            let c = Curves::default();
            (g.curves.master, g.curves.red, g.curves.green, g.curves.blue) = (c.master, c.red, c.green, c.blue);
        }
        "Color Slice" => g.slices = Slices::default(),
        "Channel Mixer" => g.mixer = Mixer::default(),
        "Color Separation" => g.separation = Separation::default(),
        "Details" => (g.clarity, g.structure, g.sharpness, g.dehaze) = (0.0, 0.0, 0.0, 0.0),
        "HSL Mixer" => g.hsl = HslMixer::default(),
        "Color Luminance" => g.color_lum = ColorLuminance::default(),
        "Film Look" => g.film = FilmLook::default(),
        "Super 8" => g.super8 = Super8::default(),
        "Film Density" => (g.film_density, g.density_rgb) = (0.0, [1.0; 3]),
        "Film Saturation" => g.film_saturation = 1.0,
        "Tetra Color Mix" => g.tetra = Tetra::default(),
        "Filmic Split" => g.split = Split::default(),
        "Bleach Bypass" => g.bleach = 0.0,
        "Saturate" => g.saturate = 0.0,
        "Technicolor 3-Strip" => g.technicolor = 0.0,
        "Greens/Foliage" => g.foliage = Foliage::default(),
        "Print Simulation" => g.print = PrintSim::default(),
        "Grain" => (g.grain_intensity, g.grain_size, g.grain_roughness) = (0.0, 1.0, 1.0),
        "Halation" => (g.halation_intensity, g.halation_threshold, g.halation_glow) = (0.0, 0.7, 50.0),
        "Diffusion" => g.diffusion = Diffusion::default(),
        "Bloom" => (g.bloom_intensity, g.bloom_threshold) = (0.0, 0.75),
        "Gate Weave" => g.gate_weave = 0.0,
        "Cinematic Frame" => g.frame_ratio = 0.0,
        "CRT Emulation" => g.crt = Crt::default(),
        "Textures" => g.texture = Texture::default(),
        "Chromatic Aberration" => g.aberration = 0.0,
        "Lens Distortion" => g.lens = 0.0,
        "Vignette" => g.vignette = 0.0,
        "Crop" => g.crop = Crop::default(),
        "Masks & Layers" => g.layers.clear(),
        _ => {}
    }
}

fn wheel_values(ui: &mut Ui, r: Rect, v: &[f32; 4]) {
    // The master first, then its colours, as a colourist reads them.
    let rows = [(v[3], 0), (v[0], 1), (v[1], 2), (v[2], 3)];
    let h = 22.0;
    for (i, (value, ch)) in rows.iter().enumerate() {
        let c = Rect::from_min_size(pos2(r.left(), r.top() + (h + 5.0) * i as f32), vec2(r.width(), h));
        let p = ui.painter();
        p.rect_filled(c, 6, BUTTON);
        p.text(c.center(), Align2::CENTER_CENTER, format!("{value:.2}"), semi(11.0), CHANNELS[*ch]);
    }
}

fn tone_wheel(ui: &mut Ui, c: eframe::egui::Pos2, radius: f32, label: &str, t: &mut Tint) {
    let hue = color::to_hsv([t.r, t.g, t.b])[0];
    let a = angle_of(hue).to_radians();
    let mut ball = vec2(a.cos(), a.sin()) * t.opacity.clamp(0.0, 1.0);
    let id = ui.scope_id().with(("tone", label));
    let resp = trackball(ui, c, radius, &mut ball, id);
    if resp.changed() {
        let picked = hue_at(ball.y.atan2(ball.x).to_degrees());
        let [r, g, b] = color::from_hsv([picked, 1.0, 1.0]);
        *t = Tint { r, g, b, opacity: ball.length().min(1.0) };
    }
    ui.painter().text(c + vec2(0.0, radius + 11.0), Align2::CENTER_CENTER, label, semi(12.0), Color32::WHITE);
}

impl App {
    pub(super) fn tool_body(&mut self, ui: &mut Ui, tab: &'static str) {
        match self.tool {
            Tool::Filters if tab == "LUTs" => self.luts_body(ui),
            Tool::Filters => self.color_body(ui, tab),
            Tool::Film => self.film_body(ui, tab),
            _ => self.effects_body(ui, tab),
        }
    }

    fn color_body(&mut self, ui: &mut Ui, tab: &str) {
        match tab {
            "Color Management" => {
                let names: Vec<&str> = Source::ALL.iter().map(|s| s.name()).collect();
                let tones: Vec<&str> = ToneMap::ALL.iter().map(|s| s.name()).collect();
                let g = self.target();
                let mut at = Source::ALL.iter().position(|s| *s == g.cst.source).unwrap_or(0);
                let mut tone = ToneMap::ALL.iter().position(|s| *s == g.cst.tone).unwrap_or(0);
                let (a, b) = split_row(ui, 56.0, 10.0);
                if dropdown(ui, a, Some("Input Color Space"), &names, &mut at) {
                    g.cst.source = Source::ALL[at];
                }
                if dropdown(ui, b, Some("Tone Mapping"), &tones, &mut tone) {
                    g.cst.tone = ToneMap::ALL[tone];
                }
                ui.add_space(10.0);
                let gamut = format!("Gamut: {}. Output: Rec.709.", g.cst.source.gamut_name());
                toggle(ui, "DaVinci Wide Gamut workflow", &mut g.cst.wide);
                ui.add_space(4.0);
                note(ui, &gamut);
            }
            "Balance" => {
                let g = self.target();
                slider(ui, "Temperature", &mut g.temp, -3000.0, 3000.0, 0.0, Fmt::Kelvin);
                slider(ui, "Tint", &mut g.tint, -100.0, 100.0, 0.0, Fmt::Int);
                slider(ui, "Hue", &mut g.hue, -180.0, 180.0, 0.0, Fmt::Int);
            }
            "Light" => {
                let g = self.target();
                slider(ui, "Exposure", &mut g.exposure, -3.0, 3.0, 0.0, Fmt::Stops);
                slider(ui, "Contrast", &mut g.contrast, 0.0, 2.0, 1.0, Fmt::Percent);
                slider(ui, "Film Contrast", &mut g.film_contrast, -1.0, 1.0, 0.0, Fmt::Percent);
                slider(ui, "Shadow Toe", &mut g.toe, -1.0, 1.0, 0.0, Fmt::Percent);
                slider(ui, "Highlight Shoulder", &mut g.shoulder, -1.0, 1.0, 0.0, Fmt::Percent);
                slider(ui, "Highlights", &mut g.highlight, -1.0, 1.0, 0.0, Fmt::Percent);
                slider(ui, "Shadows", &mut g.shadow, -1.0, 1.0, 0.0, Fmt::Percent);
                slider(ui, "Whites", &mut g.whites, -1.0, 1.0, 0.0, Fmt::Percent);
                slider(ui, "Blacks", &mut g.blacks, -1.0, 1.0, 0.0, Fmt::Percent);
                slider(ui, "Brightness", &mut g.brightness, -0.5, 0.5, 0.0, Fmt::Plain);
                slider(ui, "Gamma", &mut g.gamma, -1.0, 1.0, 0.0, Fmt::Plain);
                slider(ui, "Saturation", &mut g.saturation, 0.0, 2.0, 1.0, Fmt::Percent);
                slider(ui, "Vibrance", &mut g.vibrance, -1.0, 1.0, 0.0, Fmt::Percent);
                slider(ui, "Fade", &mut g.fade, 0.0, 1.0, 0.0, Fmt::Percent);
            }
            "HDR Grade" => {
                let mut view = self.pick.zone_view;
                segmented(ui, &["Exposure", "Saturation"], &mut view);
                self.pick.zone_view = view;
                let g = self.target();
                for (i, (name, _)) in ZONES.iter().enumerate() {
                    if view == 0 {
                        slider(ui, name, &mut g.zones.exposure[i], -3.0, 3.0, 0.0, Fmt::Stops);
                    } else {
                        slider(ui, name, &mut g.zones.saturation[i], -1.0, 1.0, 0.0, Fmt::Percent);
                    }
                }
            }
            "Color Wheels" => self.wheels_tab(ui),
            "Tone Wheels" => {
                let g = self.target();
                let w = ui.available_width();
                let (r, _) = ui.allocate_exact_size(vec2(w, (TONE_WHEEL_SIDE + 30.0) * 2.0), Sense::hover());
                let radius = TONE_WHEEL_SIDE / 2.0;
                let cell = |x: f32, y: f32| pos2(r.left() + w * x, r.top() + (TONE_WHEEL_SIDE + 30.0) * y + radius + 2.0);
                tone_wheel(ui, cell(0.25, 0.0), radius, "Shadows", &mut g.shadows_tint);
                tone_wheel(ui, cell(0.75, 0.0), radius, "Midtones", &mut g.midtones_tint);
                tone_wheel(ui, cell(0.25, 1.0), radius, "Highlights", &mut g.highlights_tint);
                tone_wheel(ui, cell(0.75, 1.0), radius, "Global", &mut g.global_tint);
            }
            "Curves" => {
                self.curves_tab(ui);
                ui.add_space(6.0);
                self.rgb_fold(ui);
            }
            "RGB Curves" => {
                let h = (ui.available_height() - 56.0).clamp(150.0, 240.0);
                self.rgb_curve(ui, h);
            }
            "Color Slice" => {
                let mut at = self.pick.slice;
                swatches(ui, &slice_colours(), &mut at);
                self.pick.slice = at;
                let g = self.target();
                let s = &mut g.slices.0[at];
                ui.label(eframe::egui::RichText::new(SLICES[at].0).font(semi(13.0)).color(MUTED));
                slider(ui, "Hue", &mut s.hue, -1.0, 1.0, 0.0, Fmt::Percent);
                slider(ui, "Saturation", &mut s.saturation, -1.0, 1.0, 0.0, Fmt::Percent);
                slider(ui, "Density", &mut s.density, -1.0, 1.0, 0.0, Fmt::Percent);
            }
            "Channel Mixer" => {
                let mut at = self.pick.mixer;
                segmented(ui, &["Red Output", "Green Output", "Blue Output"], &mut at);
                self.pick.mixer = at;
                let g = self.target();
                let m = &mut g.mixer;
                if m.mono {
                    slider(ui, "Red", &mut m.mono_mix[0], -2.0, 2.0, 0.3, Fmt::Percent);
                    slider(ui, "Green", &mut m.mono_mix[1], -2.0, 2.0, 0.59, Fmt::Percent);
                    slider(ui, "Blue", &mut m.mono_mix[2], -2.0, 2.0, 0.11, Fmt::Percent);
                } else {
                    let row = match at {
                        0 => &mut m.red,
                        1 => &mut m.green,
                        _ => &mut m.blue,
                    };
                    let rest = |i: usize| if i == at { 1.0 } else { 0.0 };
                    slider(ui, "Red", &mut row[0], -2.0, 2.0, rest(0), Fmt::Percent);
                    slider(ui, "Green", &mut row[1], -2.0, 2.0, rest(1), Fmt::Percent);
                    slider(ui, "Blue", &mut row[2], -2.0, 2.0, rest(2), Fmt::Percent);
                }
                ui.add_space(4.0);
                let mut chips = Chips::new(ui, 28.0);
                let mono = m.mono;
                if chips.add("Monochrome", mono) {
                    m.mono = !m.mono;
                }
                if chips.add("Swap R/G", false) {
                    std::mem::swap(&mut m.red, &mut m.green);
                }
                if chips.add("Swap G/B", false) {
                    std::mem::swap(&mut m.green, &mut m.blue);
                }
                if chips.add("Swap R/B", false) {
                    std::mem::swap(&mut m.red, &mut m.blue);
                }
                chips.end();
            }
            "Color Separation" => {
                let band = hue_colours();
                let g = self.target();
                let s = &mut g.separation;
                slider_on(ui, "Anchor Color", &mut s.anchor, 0.0, 360.0, 28.0, Fmt::Degrees, Some(&band));
                slider(ui, "Separation", &mut s.amount, -1.0, 1.0, 0.0, Fmt::Percent);
                slider(ui, "Protected Width", &mut s.width, 5.0, 90.0, 30.0, Fmt::Degrees);
                slider(ui, "Palette Saturation", &mut s.saturation, -1.0, 1.0, 0.0, Fmt::Percent);
            }
            "Details" => {
                let g = self.target();
                slider(ui, "Clarity", &mut g.clarity, -1.0, 1.0, 0.0, Fmt::Percent);
                slider(ui, "Structure", &mut g.structure, -1.0, 1.0, 0.0, Fmt::Percent);
                slider(ui, "Sharpness", &mut g.sharpness, 0.0, 1.0, 0.0, Fmt::Percent);
                slider(ui, "Dehaze", &mut g.dehaze, -1.0, 1.0, 0.0, Fmt::Percent);
            }
            "HSL Mixer" => {
                let mut at = self.pick.band;
                swatches(ui, &band_colours(), &mut at);
                self.pick.band = at;
                let g = self.target();
                let b = &mut g.hsl.0[at];
                ui.label(eframe::egui::RichText::new(BANDS[at].0).font(semi(13.0)).color(MUTED));
                slider(ui, "Hue", &mut b.hue, -1.0, 1.0, 0.0, Fmt::Percent);
                slider(ui, "Saturation", &mut b.saturation, -1.0, 1.0, 0.0, Fmt::Percent);
                slider(ui, "Value", &mut b.value, -1.0, 1.0, 0.0, Fmt::Percent);
                slider(ui, "Range", &mut b.range, 0.3, 2.0, 1.0, Fmt::Percent);
            }
            "Color Luminance" => {
                let g = self.target();
                for (i, (name, _)) in BANDS.iter().enumerate() {
                    slider(ui, name, &mut g.color_lum.0[i], -1.0, 1.0, 0.0, Fmt::Percent);
                }
            }
            _ => {}
        }
    }

    /// The six corners as tiles, each showing its colour and the way it leans.
    fn tetra_chips(&mut self, ui: &mut Ui) {
        const GAP: f32 = 6.0;
        let w = ui.available_width();
        let (row, _) = ui.allocate_exact_size(vec2(w, CHIP_H), Sense::hover());
        let cell = (w - GAP * 5.0) / 6.0;
        for (i, chip) in TETRA.iter().enumerate() {
            let r = Rect::from_min_size(pos2(row.left() + (cell + GAP) * i as f32, row.top()), vec2(cell, CHIP_H));
            let resp = ui.interact(r, ui.scope_id().with(("chip", i)), Sense::click());
            if resp.clicked() {
                self.pick.tetra = i;
            }
            let on = self.pick.tetra == i;
            let p = ui.painter();
            let fill = if on { Color32::from_rgb(0x0E, 0x2A, 0x3E) } else { Color32::TRANSPARENT };
            let edge = if on { Stroke::new(2.0, Color32::WHITE) } else { Stroke::new(1.0, BUTTON_EDGE) };
            p.rect_filled(r, 9, fill);
            p.rect_stroke(r, 9, edge, StrokeKind::Inside);
            let c = pos2(r.center().x, r.top() + 20.0);
            gradient_dot(p, c, 10.0, dot([chip.hue, 0.85, 0.95]), dot([chip.leans[0].toward, 0.85, 0.95]));
            if on {
                p.circle_stroke(c, 10.0, Stroke::new(1.5, Color32::WHITE));
            }
            let label = pos2(r.center().x, r.bottom() - 13.0);
            p.text(label, Align2::CENTER_CENTER, chip.label, semi(10.5), Color32::from_rgb(0xC8, 0xC8, 0xCC));
        }
    }

    /// The picture's spread along each axis, worked out once for each picture.
    fn picture_spread(&mut self) -> Spread {
        match self.spread {
            Some((id, s)) if id == self.picture_id => s,
            _ => {
                let s = scopes::spread(&self.preview);
                self.spread = Some((self.picture_id, s));
                s
            }
        }
    }

    fn curves_tab(&mut self, ui: &mut Ui) {
        let spread = self.picture_spread();
        let w = ui.available_width();
        let (row, _) = ui.allocate_exact_size(vec2(w, MODE_BUTTON), Sense::hover());
        for (i, m) in Mode::ALL.iter().enumerate() {
            let r = Rect::from_min_size(pos2(row.left() + (MODE_BUTTON + 4.0) * i as f32, row.top()), Vec2::splat(MODE_BUTTON));
            let resp = ui.interact(r, ui.scope_id().with(("mode", i)), Sense::click()).on_hover_text(m.name());
            if resp.clicked() {
                self.pick.curve_mode = i;
                self.pick.curve_point = None;
            }
            let on = self.pick.curve_mode == i;
            let p = ui.painter();
            if on || resp.hovered() {
                p.rect_filled(r, 8, BUTTON);
            }
            if on {
                p.rect_stroke(r, 8, Stroke::new(1.0, BUTTON_EDGE), StrokeKind::Inside);
            }
            let (big, small) = mode_icons(*m);
            let tint = if on { Color32::WHITE } else { Color32::from_rgb(0xC0, 0xC0, 0xC4) };
            icons::paint(p, big, Rect::from_min_size(r.min + vec2(6.0, 6.0), Vec2::splat(17.0)), tint);
            icons::paint(p, small, Rect::from_min_size(r.min + vec2(18.0, 17.0), Vec2::splat(12.0)), tint);
        }
        let mode = Mode::ALL[self.pick.curve_mode.min(Mode::ALL.len() - 1)];
        let end = |n: f32| Rect::from_min_size(pos2(row.right() - MODE_BUTTON * (n + 1.0), row.top()), Vec2::splat(MODE_BUTTON));
        let line = end(1.0).left() - 5.0;
        ui.painter().vline(line, row.top() + 4.0..=row.bottom() - 4.0, Stroke::new(1.0, BUTTON_EDGE));
        if icon_button(ui, end(1.0), Icon::RefreshCcw, "Reset this curve", false).clicked() {
            self.target().curves.mode_mut(mode).clear();
            self.pick.curve_point = None;
        }
        let actions = ["Add Six Hue Points", "Remove Chosen Point"];
        match menu_button(ui, end(0.0), Icon::MoreHorizontal, "More curve actions", false, &actions) {
            Some(0) => {
                let pts = self.target().curves.mode_mut(mode);
                for h in HUE_DOTS {
                    if !pts.iter().any(|q| (q[0] - h / 360.0).abs() < DOT_REACH) {
                        pts.push([h / 360.0, 0.5]);
                    }
                }
                pts.sort_by(|a, b| a[0].total_cmp(&b[0]));
                self.pick.curve_point = None;
            }
            Some(_) => {
                if let Some(i) = self.pick.curve_point.take() {
                    let pts = self.target().curves.mode_mut(mode);
                    if i < pts.len() {
                        pts.remove(i);
                    }
                }
            }
            None => {}
        }
        ui.add_space(6.0);

        let (backdrop, axis) = match mode {
            Mode::HueHue | Mode::HueSat | Mode::HueLum => (Backdrop::Hue, 0),
            Mode::LumSat => (Backdrop::Light, 1),
            Mode::SatSat | Mode::SatLum => (Backdrop::Colour, 2),
        };
        let look = CurveLook { flat: true, wraps: mode.wraps(), backdrop, spread: Some(&spread[axis]), ink: Color32::WHITE };
        let mut chosen = self.pick.curve_point;
        curve_box(ui, GRAPH_H, self.target().curves.mode_mut(mode), &look, &mut chosen);

        if mode.by_hue() {
            let (row, _) = ui.allocate_exact_size(vec2(w, 24.0), Sense::hover());
            for (i, h) in HUE_DOTS.iter().enumerate() {
                let c = pos2(row.left() + 8.0 + 26.0 * i as f32, row.center().y);
                let hit = Rect::from_center_size(c, Vec2::splat(24.0));
                let resp = ui.interact(hit, ui.scope_id().with(("dot", i)), Sense::click()).on_hover_text("Place a point at this colour");
                ui.painter().circle_filled(c, 7.5, dot([*h, 0.9, 0.95]));
                if resp.clicked() {
                    let at = h / 360.0;
                    let pts = self.target().curves.mode_mut(mode);
                    chosen = pts.iter().position(|q| (q[0] - at).abs() < DOT_REACH).or_else(|| {
                        pts.push([at, 0.5]);
                        pts.sort_by(|a, b| a[0].total_cmp(&b[0]));
                        pts.iter().position(|q| (q[0] - at).abs() < 1e-5)
                    });
                }
            }
            ui.add_space(4.0);
        }

        let (row, _) = ui.allocate_exact_size(vec2(w, 26.0), Sense::hover());
        let y = row.center().y;
        ui.painter().text(pos2(row.left(), y), Align2::LEFT_CENTER, mode.title(), semi(12.5), Color32::WHITE);
        let (first, second) = mode.labels();
        let right_box = Rect::from_min_size(pos2(row.right() - BOX_W, row.top()), vec2(BOX_W, 26.0));
        let second_w = text_width(ui, second, regular(12.0));
        ui.painter().text(pos2(right_box.left() - 8.0, y), Align2::RIGHT_CENTER, second, regular(12.0), TEXT_2);
        let left_box = Rect::from_min_size(pos2(right_box.left() - 14.0 - second_w - BOX_W, row.top()), vec2(BOX_W, 26.0));
        ui.painter().text(pos2(left_box.left() - 8.0, y), Align2::RIGHT_CENTER, first, regular(12.0), TEXT_2);
        let pts = self.target().curves.mode_mut(mode);
        let picked = chosen.filter(|i| *i < pts.len());
        let (mut input, mut effect) = picked.map_or((0.0, 0.0), |i| (mode.input_shown(pts[i][0]), mode.effect_shown(pts[i][1])));
        let top = if mode.by_hue() { 360.0 } else { 100.0 };
        let (low, high) = (mode.effect_shown(0.0), mode.effect_shown(1.0));
        let (a, b) = ui
            .scope(|ui| {
                ui.add_enabled_ui(picked.is_some(), |ui| {
                    (number_box(ui, left_box, &mut input, 0.0, top), number_box(ui, right_box, &mut effect, low, high))
                })
                .inner
            })
            .inner;
        if let Some(i) = picked.filter(|_| a || b) {
            pts[i] = [mode.input_stored(input), mode.effect_stored(effect)];
            pts.sort_by(|x, y| x[0].total_cmp(&y[0]));
        }
        self.pick.curve_point = chosen;
    }

    /// RGB Curves, folded away beneath the colour curves until it is wanted.
    fn rgb_fold(&mut self, ui: &mut Ui) {
        let w = ui.available_width();
        let (row, resp) = ui.allocate_exact_size(vec2(w, 34.0), Sense::click());
        if resp.clicked() {
            self.pick.rgb_open = !self.pick.rgb_open;
        }
        let p = ui.painter();
        p.text(pos2(row.left(), row.center().y), Align2::LEFT_CENTER, "RGB Curves", bold(15.0), Color32::WHITE);
        let at = |n: f32| Rect::from_center_size(pos2(row.right() - 14.0 - 30.0 * n, row.center().y), Vec2::splat(20.0));
        icons::paint(p, if self.pick.rgb_open { Icon::ArrowUp01 } else { Icon::ArrowDown01 }, at(0.0), ACCENT);
        icons::paint(p, Icon::Spline, at(1.0), Color32::WHITE);
        if self.pick.rgb_open {
            ui.add_space(4.0);
            self.rgb_curve(ui, RGB_GRAPH_H);
        }
    }

    fn rgb_curve(&mut self, ui: &mut Ui, h: f32) {
        let mut at = self.pick.curve_channel;
        segmented(ui, &["Y", "R", "G", "B"], &mut at);
        self.pick.curve_channel = at;
        let look = CurveLook { flat: false, wraps: false, backdrop: Backdrop::Plain, spread: None, ink: CHANNELS[at] };
        let mut chosen = None;
        curve_box(ui, h, self.target().curves.channel_mut(at), &look, &mut chosen);
    }

    fn wheels_tab(&mut self, ui: &mut Ui) {
        let mut at = self.pick.wheel;
        segmented(ui, &WHEELS, &mut at);
        self.pick.wheel = at;
        let id = ui.scope_id();
        let g = self.target();
        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(vec2(w, WHEEL_SIDE), Sense::hover());
        let radius = WHEEL_SIDE / 2.0;
        let centre = r.center();
        let v = g.wheels.wheel_mut(at);
        let mut ball = ball_of([v[0], v[1], v[2]]) / WHEEL_REACH;
        let resp = trackball(ui, centre, radius, &mut ball, id.with(("wheel", at)));
        if resp.changed() {
            let rgb = rgb_of(ball * WHEEL_REACH);
            v[..3].copy_from_slice(&rgb);
        }
        let column = Rect::from_min_size(pos2(r.left(), r.top() + 12.0), vec2(52.0, 120.0));
        wheel_values(ui, column, v);
        let back = Rect::from_center_size(pos2(r.right() - 16.0, r.center().y), Vec2::splat(32.0));
        if icon_button(ui, back, Icon::RefreshCcw, "Reset this wheel", false).clicked() {
            *v = [0.0; 4];
        }
        ui.add_space(4.0);
        jog(ui, &mut v[3], JOG_SPEED);
        ui.add_space(4.0);
        let mut mix = g.wheels.lum_mix * 100.0;
        slider(ui, "Lum Mix", &mut mix, 0.0, 100.0, 100.0, Fmt::Int);
        g.wheels.lum_mix = mix / 100.0;
    }

    fn film_body(&mut self, ui: &mut Ui, tab: &str) {
        match tab {
            "Film Look" => {
                let mut view = self.pick.film_view;
                let names: Vec<&str> = STOCKS.iter().map(|s| s.name).collect();
                let prints: Vec<&str> = PRINTS.iter().map(|s| s.name).collect();
                let f = &mut self.target().film;
                toggle(ui, "Film Look", &mut f.on);
                ui.add_space(2.0);
                slider(ui, "Intensity", &mut f.intensity, 0.0, 1.0, 0.77, Fmt::Percent);
                ui.add_space(2.0);
                segmented(ui, &["Color", "Texture"], &mut view);
                self.pick.film_view = view;
                let f = &mut self.target().film;
                if view == 0 {
                    let (a, b) = split_row(ui, 56.0, 10.0);
                    let mut changed = dropdown(ui, a, Some("Stock"), &names, &mut f.stock);
                    changed |= dropdown(ui, b, Some("Gauge"), &GAUGES, &mut f.gauge);
                    ui.add_space(8.0);
                    let (a, b) = split_row(ui, 56.0, 10.0);
                    changed |= dropdown(ui, a, Some("Output"), &OUTPUTS, &mut f.output);
                    changed |= dropdown(ui, b, Some("Print Stock"), &prints, &mut f.print);
                    if changed {
                        f.on = true;
                    }
                } else {
                    let before = *f;
                    slider(ui, "Halation", &mut f.halation, 0.0, 1.0, 0.25, Fmt::Percent);
                    slider(ui, "Grain", &mut f.grain, 0.0, 1.0, 0.2, Fmt::Percent);
                    slider(ui, "Softness", &mut f.softness, 0.0, 1.0, 0.15, Fmt::Percent);
                    slider(ui, "Gate Weave", &mut f.weave, 0.0, 1.0, 0.0, Fmt::Percent);
                    slider(ui, "Dust", &mut f.dust, 0.0, 1.0, 0.0, Fmt::Percent);
                    if *f != before {
                        f.on = true;
                    }
                }
            }
            "Super 8" => {
                let s = &mut self.target().super8;
                toggle(ui, "Super 8", &mut s.on);
                ui.add_space(2.0);
                let before = *s;
                slider(ui, "Amount", &mut s.amount, 0.0, 1.0, 0.7, Fmt::Percent);
                slider(ui, "Flicker", &mut s.flicker, 0.0, 1.0, 0.3, Fmt::Percent);
                slider(ui, "Dust", &mut s.dust, 0.0, 1.0, 0.3, Fmt::Percent);
                slider(ui, "Softness", &mut s.softness, 0.0, 1.0, 0.4, Fmt::Percent);
                toggle(ui, "Scanned Frame", &mut s.frame);
                if *s != before {
                    s.on = true;
                }
            }
            "Film Density" => {
                let g = self.target();
                slider(ui, "Density", &mut g.film_density, 0.0, 1.0, 0.0, Fmt::Percent);
                slider(ui, "Red Weight", &mut g.density_rgb[0], 0.0, 2.0, 1.0, Fmt::Percent);
                slider(ui, "Green Weight", &mut g.density_rgb[1], 0.0, 2.0, 1.0, Fmt::Percent);
                slider(ui, "Blue Weight", &mut g.density_rgb[2], 0.0, 2.0, 1.0, Fmt::Percent);
            }
            "Film Saturation" => {
                let g = self.target();
                slider(ui, "Film Saturation", &mut g.film_saturation, 0.0, 2.0, 1.0, Fmt::Percent);
            }
            "Tetra Color Mix" => {
                self.tetra_chips(ui);
                ui.add_space(10.0);
                let chip = &TETRA[self.pick.tetra.min(TETRA.len() - 1)];
                for l in &chip.leans {
                    let g = self.target();
                    let mut v = g.tetra.lean_of(chip, l);
                    let band = hue_arc(chip.hue, l.toward);
                    slider_on(ui, l.name, &mut v, -1.0, 1.0, 0.0, Fmt::Percent, Some(&band));
                    g.tetra.set_lean(chip, l, v);
                    ui.add_space(6.0);
                }
            }
            "Filmic Split" => {
                let names: Vec<&str> = SPLITS.iter().map(|s| s.0).collect();
                let g = self.target();
                let w = ui.available_width();
                let (r, _) = ui.allocate_exact_size(vec2(w, 34.0), Sense::hover());
                dropdown(ui, r, None, &names, &mut g.split.style);
                ui.add_space(8.0);
                slider(ui, "Amount", &mut g.split.amount, 0.0, 1.0, 0.0, Fmt::Percent);
                slider(ui, "Balance", &mut g.split.balance, -1.0, 1.0, 0.0, Fmt::Percent);
            }
            "Bleach Bypass" => {
                let g = self.target();
                slider(ui, "Amount", &mut g.bleach, 0.0, 1.0, 0.0, Fmt::Percent);
            }
            "Saturate" => {
                let g = self.target();
                slider(ui, "Amount", &mut g.saturate, -1.0, 1.0, 0.0, Fmt::Percent);
            }
            "Technicolor 3-Strip" => {
                let g = self.target();
                slider(ui, "3-Strip Amount", &mut g.technicolor, 0.0, 1.0, 0.0, Fmt::Percent);
            }
            "Greens/Foliage" => {
                let f = &mut self.target().foliage;
                slider(ui, "Olive to Emerald", &mut f.hue, -1.0, 1.0, 0.0, Fmt::Percent);
                slider(ui, "Saturation", &mut f.saturation, -1.0, 1.0, 0.0, Fmt::Percent);
                slider(ui, "Lightness", &mut f.lightness, -1.0, 1.0, 0.0, Fmt::Percent);
            }
            "Print Simulation" => {
                let g = self.target();
                let was = g.print.paper;
                segmented(ui, &PAPERS, &mut g.print.paper);
                if g.print.paper != was && g.print.amount == 0.0 {
                    g.print.amount = 1.0;
                }
                slider(ui, "Amount", &mut g.print.amount, 0.0, 1.0, 0.0, Fmt::Percent);
            }
            _ => {}
        }
    }

    fn effects_body(&mut self, ui: &mut Ui, tab: &str) {
        let video = self.clip.is_some();
        let g = self.target();
        match tab {
            "Grain" => {
                slider(ui, "Intensity", &mut g.grain_intensity, 0.0, 1.0, 0.0, Fmt::Percent);
                slider(ui, "Size", &mut g.grain_size, 0.5, 4.0, 1.0, Fmt::Plain);
                slider(ui, "Roughness", &mut g.grain_roughness, 0.0, 1.0, 1.0, Fmt::Percent);
            }
            "Halation" => {
                slider(ui, "Intensity", &mut g.halation_intensity, 0.0, 1.0, 0.0, Fmt::Percent);
                slider(ui, "Threshold", &mut g.halation_threshold, 0.0, 1.0, 0.7, Fmt::Percent);
                slider(ui, "Glow", &mut g.halation_glow, 1.0, 100.0, 50.0, Fmt::Int);
            }
            "Diffusion" => {
                let w = ui.available_width();
                let (r, _) = ui.allocate_exact_size(vec2(w, 34.0), Sense::hover());
                dropdown(ui, r, None, &DIFFUSERS, &mut g.diffusion.filter);
                ui.add_space(8.0);
                slider(ui, "Strength", &mut g.diffusion.strength, 0.0, 1.0, 0.0, Fmt::Percent);
                slider(ui, "Size", &mut g.diffusion.size, 0.25, 2.5, 1.0, Fmt::Plain);
            }
            "Bloom" => {
                slider(ui, "Intensity", &mut g.bloom_intensity, 0.0, 1.0, 0.0, Fmt::Percent);
                slider(ui, "Threshold", &mut g.bloom_threshold, 0.0, 1.0, 0.75, Fmt::Percent);
            }
            "Gate Weave" => {
                slider(ui, "Amount", &mut g.gate_weave, 0.0, 1.0, 0.0, Fmt::Percent);
                if !video {
                    note(ui, "Weave happens between frames, so it only shows on video.");
                }
            }
            "Cinematic Frame" => {
                let current = g.frame_ratio;
                let mut chips = Chips::new(ui, 30.0);
                for (label, ratio) in FRAME_RATIOS {
                    if chips.add(label, current == ratio) {
                        g.frame_ratio = ratio;
                    }
                }
                chips.end();
            }
            "CRT Emulation" => {
                let c = &mut g.crt;
                slider(ui, "Amount", &mut c.amount, 0.0, 1.0, 0.0, Fmt::Percent);
                slider(ui, "Scanlines", &mut c.scanlines, 0.0, 1.0, 0.6, Fmt::Percent);
                slider(ui, "Phosphor Mask", &mut c.mask, 0.0, 1.0, 0.5, Fmt::Percent);
                slider(ui, "Screen Curve", &mut c.curve, 0.0, 1.0, 0.35, Fmt::Percent);
                slider(ui, "Glow", &mut c.glow, 0.0, 1.0, 0.4, Fmt::Percent);
            }
            "Textures" => {
                let t = &mut g.texture;
                let w = ui.available_width();
                let (r, _) = ui.allocate_exact_size(vec2(w, 34.0), Sense::hover());
                dropdown(ui, r, None, &TEXTURES, &mut t.kind);
                ui.add_space(8.0);
                slider(ui, "Amount", &mut t.amount, 0.0, 1.0, 0.0, Fmt::Percent);
                slider(ui, "Scale", &mut t.scale, 0.3, 3.0, 1.0, Fmt::Plain);
            }
            "Chromatic Aberration" => {
                slider(ui, "Amount", &mut g.aberration, -1.0, 1.0, 0.0, Fmt::Percent);
            }
            "Lens Distortion" => {
                slider(ui, "Barrel to Pincushion", &mut g.lens, -1.0, 1.0, 0.0, Fmt::Percent);
            }
            "Vignette" => {
                slider(ui, "Vignette", &mut g.vignette, -1.0, 1.0, 0.0, Fmt::Percent);
            }
            _ => {}
        }
    }
}
