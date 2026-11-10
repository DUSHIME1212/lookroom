//! Transition catalogue and procedural mattes. Preset identifiers are saved verbatim;
//! existing projects retain their original names. Preview and export share this renderer.
use crate::effects::noise01;
use crate::overlay::{blur, mixed, sample};
use crate::pipeline::Picture;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::f32::consts::{PI, TAU};

#[derive(Clone, Copy)]
pub(crate) enum Recipe {
    Legacy,
    Mask(u8, f32, bool),
    Move(u8, f32),
    Warp(u8, f32),
    Light(u8, f32),
}

macro_rules! catalogue {
    ($($id:ident => ($name:literal, $group:literal, $recipe:expr)),+ $(,)?) => {
        #[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug)]
        pub enum Wipe { $($id),+ }
        impl Wipe {
            pub const ALL: [Self; [$(stringify!($id)),+].len()] = [$(Self::$id),+];
            pub fn name(self) -> &'static str { match self { $(Self::$id => $name),+ } }
            pub fn category(self) -> &'static str { match self { $(Self::$id => $group),+ } }
            pub(crate) fn recipe(self) -> Recipe { match self { $(Self::$id => $recipe),+ } }
        }
    }
}

use Recipe::*;
catalogue! {
    Dissolve => ("Cross Dissolve", "Classic", Legacy),
    DipToBlack => ("Dip to Black", "Classic", Legacy),
    Flash => ("Flash White", "Light", Legacy),
    BlurDissolve => ("Blur", "Classic", Legacy),
    WipeLeft => ("Wipe Left", "Wipes", Legacy),
    WipeRight => ("Wipe Right", "Wipes", Legacy),
    WipeUp => ("Wipe Up", "Wipes", Legacy),
    WipeDown => ("Wipe Down", "Wipes", Legacy),
    Diagonal => ("Diagonal Wipe", "Wipes", Legacy),
    Clock => ("Clock Wipe", "Wipes", Legacy),
    Iris => ("Circle Open", "Masks", Legacy),
    Blinds => ("Blinds", "Wipes", Legacy),
    SlideLeft => ("Slide Left", "Motion", Legacy),
    SlideRight => ("Slide Right", "Motion", Legacy),
    SlideUp => ("Slide Up", "Motion", Legacy),
    Cover => ("Cover", "Motion", Legacy),
    WhipPan => ("Whip Pan", "Motion", Legacy),
    ZoomIn => ("Zoom In", "Motion", Legacy),
    ZoomOut => ("Zoom Out", "Motion", Legacy),
    CrossZoom => ("Cross Zoom", "Motion", Legacy),
    Spin => ("Spin", "Motion", Legacy),
    GlitchCut => ("Glitch", "Distortion", Legacy),
    Pixelate => ("Pixelate", "Distortion", Legacy),
    CircleClose => ("Circle Close", "Masks", Mask(0, 0.012, true)),
    CircleFeather => ("Feathered Circle", "Masks", Mask(0, 0.14, false)),
    EllipseOpen => ("Ellipse Open", "Masks", Mask(1, 0.012, false)),
    EllipseClose => ("Ellipse Close", "Masks", Mask(1, 0.012, true)),
    DiamondOpen => ("Diamond Open", "Masks", Mask(2, 0.012, false)),
    DiamondClose => ("Diamond Close", "Masks", Mask(2, 0.012, true)),
    BoxOpen => ("Box Open", "Masks", Mask(3, 0.012, false)),
    BoxClose => ("Box Close", "Masks", Mask(3, 0.012, true)),
    StarOpen => ("Star Open", "Masks", Mask(4, 0.012, false)),
    StarClose => ("Star Close", "Masks", Mask(4, 0.012, true)),
    HeartOpen => ("Heart Open", "Masks", Mask(5, 0.012, false)),
    HeartClose => ("Heart Close", "Masks", Mask(5, 0.012, true)),
    HexOpen => ("Hexagon Open", "Masks", Mask(6, 0.012, false)),
    HexClose => ("Hexagon Close", "Masks", Mask(6, 0.012, true)),
    SlitHorizontal => ("Horizontal Slit", "Masks", Mask(7, 0.012, false)),
    SlitVertical => ("Vertical Slit", "Masks", Mask(8, 0.012, false)),
    CrossOpen => ("Cross Open", "Masks", Mask(9, 0.012, false)),
    CrossClose => ("Cross Close", "Masks", Mask(9, 0.012, true)),
    LumaBright => ("Luma Highlights First", "Masks", Mask(10, 0.045, false)),
    LumaDark => ("Luma Shadows First", "Masks", Mask(10, 0.045, true)),
    ChromaReveal => ("Chroma Reveal", "Masks", Mask(11, 0.06, false)),
    ChromaErase => ("Chroma Erase", "Masks", Mask(11, 0.06, true)),
    InkReveal => ("Ink Reveal", "Texture", Mask(12, 0.07, false)),
    InkErase => ("Ink Erase", "Texture", Mask(12, 0.07, true)),
    CloudsReveal => ("Cloud Reveal", "Texture", Mask(13, 0.16, false)),
    CloudsErase => ("Cloud Erase", "Texture", Mask(13, 0.16, true)),
    FilmGrainReveal => ("Grain Dissolve", "Texture", Mask(14, 0.04, false)),
    MosaicReveal => ("Mosaic Reveal", "Texture", Mask(15, 0.025, false)),
    MosaicErase => ("Mosaic Erase", "Texture", Mask(15, 0.025, true)),
    CheckerReveal => ("Checkerboard", "Texture", Mask(16, 0.025, false)),
    DotsReveal => ("Halftone Dots", "Texture", Mask(17, 0.025, false)),
    DotsErase => ("Halftone Close", "Texture", Mask(17, 0.025, true)),
    BrushAcross => ("Brush Across", "Texture", Mask(18, 0.035, false)),
    BrushBack => ("Brush Back", "Texture", Mask(18, 0.035, true)),
    TornPaper => ("Torn Paper", "Texture", Mask(19, 0.008, false)),
    TornPaperBack => ("Torn Paper Back", "Texture", Mask(19, 0.008, true)),
    SoftLeft => ("Soft Wipe Left", "Wipes", Mask(20, 0.14, true)),
    SoftRight => ("Soft Wipe Right", "Wipes", Mask(20, 0.14, false)),
    SoftUp => ("Soft Wipe Up", "Wipes", Mask(21, 0.14, true)),
    SoftDown => ("Soft Wipe Down", "Wipes", Mask(21, 0.14, false)),
    DiagonalBack => ("Diagonal Back", "Wipes", Mask(22, 0.015, true)),
    DiagonalCross => ("Diagonal Cross", "Wipes", Mask(23, 0.015, false)),
    ClockBack => ("Counterclockwise", "Wipes", Mask(24, 0.015, true)),
    FanBlades => ("Fan Blades", "Wipes", Mask(25, 0.015, false)),
    SpiralReveal => ("Spiral Reveal", "Wipes", Mask(26, 0.025, false)),
    RingsReveal => ("Concentric Rings", "Wipes", Mask(27, 0.025, false)),
    BlindsVertical => ("Vertical Blinds", "Wipes", Mask(28, 0.015, false)),
    BlindsDiagonal => ("Diagonal Blinds", "Wipes", Mask(29, 0.015, false)),
    Chevron => ("Chevron", "Wipes", Mask(30, 0.015, false)),
    Zigzag => ("Zigzag", "Wipes", Mask(31, 0.015, false)),
    WaveWipe => ("Wave Wipe", "Wipes", Mask(32, 0.025, false)),
    WaveWipeBack => ("Wave Wipe Back", "Wipes", Mask(32, 0.025, true)),
    SlideDown => ("Slide Down", "Motion", Move(0, 3.0)),
    PushDiagonal => ("Push Diagonal", "Motion", Move(0, 4.0)),
    WhipRight => ("Whip Right", "Motion", Move(1, 1.0)),
    WhipUp => ("Whip Up", "Motion", Move(1, 2.0)),
    WhipDown => ("Whip Down", "Motion", Move(1, 3.0)),
    WhipDiagonal => ("Whip Diagonal", "Motion", Move(1, 4.0)),
    CoverRight => ("Cover Right", "Motion", Move(2, 1.0)),
    CoverUp => ("Cover Up", "Motion", Move(2, 2.0)),
    CoverDown => ("Cover Down", "Motion", Move(2, 3.0)),
    UncoverLeft => ("Uncover Left", "Motion", Move(3, 0.0)),
    UncoverRight => ("Uncover Right", "Motion", Move(3, 1.0)),
    UncoverUp => ("Uncover Up", "Motion", Move(3, 2.0)),
    UncoverDown => ("Uncover Down", "Motion", Move(3, 3.0)),
    SnapZoom => ("Snap Zoom", "Motion", Move(4, 1.0)),
    PullBack => ("Pull Back", "Motion", Move(4, -1.0)),
    ZoomTopLeft => ("Zoom Top Left", "Motion", Move(5, 0.0)),
    ZoomTopRight => ("Zoom Top Right", "Motion", Move(5, 1.0)),
    ZoomBottomLeft => ("Zoom Bottom Left", "Motion", Move(5, 2.0)),
    ZoomBottomRight => ("Zoom Bottom Right", "Motion", Move(5, 3.0)),
    SpinBack => ("Spin Back", "Motion", Move(6, -1.0)),
    SpinZoom => ("Spin Zoom", "Motion", Move(6, 2.0)),
    ElasticZoom => ("Elastic Zoom", "Motion", Move(7, 1.0)),
    BounceZoom => ("Bounce Zoom", "Motion", Move(7, 2.0)),
    Swirl => ("Swirl", "Distortion", Warp(0, 1.0)),
    SwirlBack => ("Swirl Back", "Distortion", Warp(0, -1.0)),
    Ripple => ("Ripple", "Distortion", Warp(1, 1.0)),
    RippleFine => ("Fine Ripple", "Distortion", Warp(1, 2.0)),
    WaveHorizontal => ("Horizontal Wave", "Distortion", Warp(2, 1.0)),
    WaveVertical => ("Vertical Wave", "Distortion", Warp(2, -1.0)),
    Fisheye => ("Fisheye", "Distortion", Warp(3, 1.0)),
    Pinch => ("Pinch", "Distortion", Warp(3, -1.0)),
    Kaleidoscope => ("Kaleidoscope", "Distortion", Warp(4, 6.0)),
    Prism => ("Prism Split", "Distortion", Warp(5, 1.0)),
    RgbZoom => ("RGB Zoom", "Distortion", Warp(5, 2.0)),
    ScanlineTear => ("Scanline Tear", "Distortion", Warp(6, 1.0)),
    DigitalBlocks => ("Digital Blocks", "Distortion", Warp(6, 2.0)),
    StretchHorizontal => ("Horizontal Stretch", "Distortion", Warp(7, 1.0)),
    StretchVertical => ("Vertical Stretch", "Distortion", Warp(7, -1.0)),
    PixelStretch => ("Pixel Stretch", "Distortion", Warp(8, 1.0)),
    WarmLeak => ("Warm Light Leak", "Light", Light(0, 0.0)),
    CoolLeak => ("Cool Light Leak", "Light", Light(0, 1.0)),
    FilmBurn => ("Film Burn", "Light", Light(1, 0.0)),
    BlueBurn => ("Blue Burn", "Light", Light(1, 1.0)),
    ExposureBloom => ("Exposure Bloom", "Light", Light(2, 0.0)),
    DreamGlow => ("Dream Glow", "Light", Light(2, 1.0)),
    LightSweep => ("Light Sweep", "Light", Light(3, 0.0)),
    LightSweepBack => ("Light Sweep Back", "Light", Light(3, 1.0)),
    WarmFlash => ("Warm Flash", "Light", Light(4, 0.0)),
    CoolFlash => ("Cool Flash", "Light", Light(4, 1.0)),
}

pub const CATEGORIES: [&str; 8] = ["All", "Masks", "Wipes", "Motion", "Distortion", "Texture", "Light", "Classic"];

fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Continuous, fixed-seed noise: patterns stay attached to the image throughout a cut.
fn noise(u: f32, v: f32, scale: f32) -> f32 {
    let (x, y) = (u * scale, v * scale);
    let (ix, iy) = (x.floor() as u32, y.floor() as u32);
    let (a, b) = (smooth(x.fract()), smooth(y.fract()));
    let n = |dx, dy| noise01(ix + dx, iy + dy, 73);
    (n(0, 0) * (1.0 - a) + n(1, 0) * a) * (1.0 - b) + (n(0, 1) * (1.0 - a) + n(1, 1) * a) * b
}

fn luma(c: [f32; 3]) -> f32 {
    c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722
}

/// A scalar matte: progress sweeps its threshold from zero to one. Geometric
/// masks use aspect-correct coordinates, so circles stay circular in portrait too.
fn matte(id: u8, u: f32, v: f32, aspect: f32, old: [f32; 3]) -> f32 {
    let (x, y) = ((u - 0.5) * aspect, v - 0.5);
    let radius = x.hypot(y);
    let diag = (aspect * aspect + 1.0).sqrt() * 0.5;
    let angle = y.atan2(x);
    let centred = ((u - 0.5).abs() * 2.0, (v - 0.5).abs() * 2.0);
    match id {
        0 => radius / diag,
        1 => (centred.0.powi(2) + centred.1.powi(2)).sqrt() / 2.0f32.sqrt(),
        2 => (x.abs() + y.abs()) / ((aspect + 1.0) * 0.5),
        3 => centred.0.max(centred.1),
        4 => (radius * 0.36 / (diag * (0.68 + 0.32 * (5.0 * (angle + PI * 0.5)).cos()))).sqrt(),
        5 => {
            // Polar heart with a notch at the top and a tapered lower point.
            let a = angle + PI * 0.5;
            let edge = 0.62 - 0.28 * a.cos() - 0.14 * (2.0 * a).cos();
            (radius * 0.2 / (diag * edge.max(0.2))).sqrt()
        }
        6 => radius * ((angle + PI / 6.0).rem_euclid(PI / 3.0) - PI / 6.0).cos() / diag,
        7 => centred.1,
        8 => centred.0,
        9 => centred.0.min(centred.1),
        10 => 1.0 - luma(old).clamp(0.0, 1.0),
        11 => old.iter().copied().fold(0.0, f32::max) - old.iter().copied().fold(1.0, f32::min),
        12 => noise(u, v, 7.0) * 0.7 + noise(u, v, 27.0) * 0.3,
        13 => noise(u, v, 4.0),
        14 => noise01((u * 1400.0) as u32, (v * 900.0) as u32, 83),
        15 => noise01((u * 14.0) as u32, (v * 10.0) as u32, 42),
        16 => (((u * 12.0) as u32 + (v * 8.0) as u32) % 2) as f32 * 0.5 + (u * 12.0).fract() * 0.5,
        17 => (((u * 14.0).fract() - 0.5).powi(2) + ((v * 14.0 / aspect).fract() - 0.5).powi(2)).sqrt() * 2.0f32.sqrt(),
        18 => u * 0.8 + noise(u, v, 30.0) * 0.2,
        19 => u * 0.84 + noise(0.0, v, 55.0) * 0.16,
        20 => u,
        21 => v,
        22 => (u + v) * 0.5,
        23 => (1.0 - u + v) * 0.5,
        24 => ((angle + PI * 0.5) / TAU).rem_euclid(1.0),
        25 => ((angle + PI * 0.5) * 4.0 / TAU).rem_euclid(1.0),
        26 => (angle / TAU + radius / diag * 2.0).rem_euclid(1.0),
        27 => (radius / diag * 7.0).fract(),
        28 => (u * 10.0).fract(),
        29 => ((u + v) * 7.0).fract(),
        30 => (u + (v - 0.5).abs()) / 1.5,
        31 => u * 0.8 + ((v * 7.0).fract() - 0.5).abs() * 0.4,
        32 => u * 0.8 + 0.1 + (v * TAU * 3.0).sin() * 0.1,
        _ => unreachable!("unknown matte"),
    }
    .clamp(0.0, 1.0)
}

fn direction(n: f32) -> (f32, f32) {
    match n as u8 {
        0 => (1.0, 0.0),
        1 => (-1.0, 0.0),
        2 => (0.0, 1.0),
        3 => (0.0, -1.0),
        _ => (1.0, 1.0),
    }
}

/// Sampling in normalized coordinates gives the same effect at preview/export sizes.
fn at(p: &Picture, u: f32, v: f32) -> [f32; 3] {
    sample(p, u * p.w as f32 - 0.5, v * p.h as f32 - 0.5)
}

pub(crate) fn render(a: &Picture, b: &Picture, recipe: Recipe, t: f32) -> Picture {
    let e = smooth(t);
    let swell = (PI * t).sin();
    let aspect = b.w as f32 / b.h as f32;
    let glow = if matches!(recipe, Light(2, _)) {
        let radius = (b.w.max(b.h) as f32 * 0.018 * swell).round() as usize;
        Some((blur(a, radius), blur(b, radius)))
    } else {
        None
    };
    let mut px = vec![[0.0; 3]; b.w * b.h];
    px.par_chunks_mut(b.w).enumerate().for_each(|(y, row)| {
        for (x, out) in row.iter_mut().enumerate() {
            let (u, v) = ((x as f32 + 0.5) / b.w as f32, (y as f32 + 0.5) / b.h as f32);
            let (old, new) = (at(a, u, v), at(b, u, v));
            *out = match recipe {
                Mask(id, feather, reverse) => {
                    let d = matte(id, u, v, aspect, old);
                    let d = if reverse { 1.0 - d } else { d };
                    // Include a one-pixel antialiasing band even on a sharp mask.
                    let f = feather.max(1.0 / b.w.min(b.h) as f32);
                    let k = smooth((e * (1.0 + 2.0 * f) - d) / (2.0 * f));
                    mixed(old, new, k)
                }
                Move(id, arg) if id <= 3 => {
                    let (dx, dy) = direction(arg);
                    let inside = |su: f32, sv: f32| (0.0..=1.0).contains(&su) && (0.0..=1.0).contains(&sv);
                    let moving = |offset: f32| {
                        let (au, av) = (u + dx * offset, v + dy * offset);
                        let (bu, bv) = (u - dx * (1.0 - offset), v - dy * (1.0 - offset));
                        match id {
                            2 => {
                                if inside(bu, bv) {
                                    at(b, bu, bv)
                                } else {
                                    old
                                }
                            }
                            3 => {
                                if inside(au, av) {
                                    at(a, au, av)
                                } else {
                                    new
                                }
                            }
                            _ => {
                                if inside(au, av) {
                                    at(a, au, av)
                                } else {
                                    at(b, bu, bv)
                                }
                            }
                        }
                    };
                    if id == 1 {
                        let mut sum = [0.0; 3];
                        for tap in 0..9 {
                            let c = moving(e + (tap as f32 / 8.0 - 0.5) * swell * 0.18);
                            for ch in 0..3 {
                                sum[ch] += c[ch] / 9.0;
                            }
                        }
                        sum
                    } else {
                        moving(e)
                    }
                }
                Move(id, arg) => {
                    let mut centre = (0.5, 0.5);
                    if id == 5 {
                        centre = (if arg as u8 % 2 == 0 { 0.15 } else { 0.85 }, if arg < 2.0 { 0.15 } else { 0.85 });
                    }
                    let (dx, dy) = ((u - centre.0) * aspect, v - centre.1);
                    let angle = if id == 6 { swell * arg * PI * 0.55 } else { 0.0 };
                    let (sin, cos) = angle.sin_cos();
                    let z = match id {
                        4 if arg < 0.0 => 1.0 - swell * 0.38,
                        4 => 1.0 + swell.powi(3) * 1.8,
                        7 => 1.0 + swell * (0.45 + 0.23 * (t * TAU * (2.0 + arg)).sin()),
                        _ => 1.0 + swell * 0.8,
                    };
                    let (su, sv) = (centre.0 + (dx * cos - dy * sin) / (z * aspect), centre.1 + (dx * sin + dy * cos) / z);
                    mixed(at(a, su, sv), at(b, su, sv), e)
                }
                Warp(id, arg) => {
                    let (dx, dy) = ((u - 0.5) * aspect, v - 0.5);
                    let r = dx.hypot(dy);
                    let (mut su, mut sv) = (u, v);
                    match id {
                        0 => {
                            let angle = swell * arg * 4.0 * (-r * 2.0).exp();
                            let (sin, cos) = angle.sin_cos();
                            su = 0.5 + (dx * cos - dy * sin) / aspect;
                            sv = 0.5 + dx * sin + dy * cos;
                        }
                        1 => {
                            let q = 1.0 + (r * 36.0 * arg - t * TAU * 2.0).sin() * swell * 0.15;
                            su = 0.5 + dx * q / aspect;
                            sv = 0.5 + dy * q;
                        }
                        2 if arg > 0.0 => su += (v * TAU * 5.0 + t * TAU).sin() * swell * 0.09,
                        2 => sv += (u * TAU * 5.0 + t * TAU).sin() * swell * 0.09,
                        3 => {
                            let q = 1.0 + arg * swell * 0.8 * (-r * r * 3.0).exp();
                            su = 0.5 + dx * q / aspect;
                            sv = 0.5 + dy * q;
                        }
                        4 => {
                            let sector = TAU / arg;
                            let angle = ((dy.atan2(dx) + t * PI).rem_euclid(sector) - sector * 0.5).abs();
                            su += (0.5 + r * angle.cos() / aspect - u) * swell;
                            sv += (0.5 + r * angle.sin() - v) * swell;
                        }
                        6 => {
                            let band = (v * if arg > 1.0 { 12.0 } else { 65.0 }) as u32;
                            su += (noise01(band, (t * 18.0) as u32, 61) - 0.5) * swell * 0.4;
                            if arg > 1.0 {
                                sv += (noise01((u * 12.0) as u32, band, 29) - 0.5) * swell * 0.12;
                            }
                        }
                        7 if arg > 0.0 => su = 0.5 + (u - 0.5) / (1.0 + swell * 3.0),
                        7 => sv = 0.5 + (v - 0.5) / (1.0 + swell * 3.0),
                        8 => {
                            let cells = 1.0 + (1.0 - swell) * 100.0;
                            su += ((u * cells).floor() / cells - u) * swell;
                        }
                        _ => {}
                    }
                    if id == 5 {
                        std::array::from_fn(|ch| {
                            let split = (ch as f32 - 1.0) * swell * 0.035;
                            let (ru, rv) = if arg > 1.0 { (u + dx * split / aspect * 3.0, v + dy * split * 3.0) } else { (u + split, v) };
                            at(a, ru, rv)[ch] * (1.0 - e) + at(b, ru, rv)[ch] * e
                        })
                    } else {
                        mixed(at(a, su, sv), at(b, su, sv), e)
                    }
                }
                Light(id, arg) => {
                    let mut base = mixed(old, new, e);
                    if let Some((ga, gb)) = &glow {
                        let soft = mixed(at(ga, u, v), at(gb, u, v), e);
                        if arg > 0.5 {
                            base = mixed(base, soft, swell * 0.65);
                        } else {
                            base = std::array::from_fn(|ch| 1.0 - (1.0 - base[ch]) * (1.0 - soft[ch] * swell * 0.45));
                        }
                    }
                    let tint = if arg > 0.5 { [0.3, 0.65, 1.0] } else { [1.0, 0.55, 0.18] };
                    let energy = match id {
                        0 => swell * (1.0 - u + 0.2 * (v * 8.0 + t * 4.0).sin()).max(0.0) * 1.4,
                        1 => swell * smooth((noise(u, v, 6.0) + swell - 0.7) * 3.0),
                        2 => swell * (0.3 + luma(base) * 0.8),
                        3 => {
                            let pos = if arg > 0.5 { 1.0 - e } else { e };
                            swell * (-(u - pos + (v - 0.5) * 0.25).powi(2) / 0.015).exp()
                        }
                        _ => swell.powi(6),
                    }
                    .clamp(0.0, 1.0);
                    std::array::from_fn(|ch| 1.0 - (1.0 - base[ch]) * (1.0 - energy * if id == 2 && arg < 0.5 { 1.0 } else { tint[ch] }))
                }
                Legacy => unreachable!("legacy transitions use the original compositor"),
            };
        }
    });
    Picture { w: b.w, h: b.h, px }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::blend;
    use std::collections::HashSet;

    fn pictures(w: usize, h: usize) -> (Picture, Picture) {
        let a = Picture {
            w,
            h,
            px: (0..w * h)
                .map(|i| {
                    let (u, v) = ((i % w) as f32 / w as f32, (i / w) as f32 / h as f32);
                    [u, v, (u * 13.0).sin() * 0.4 + 0.5]
                })
                .collect(),
        };
        let b = Picture { w, h, px: a.px.iter().map(|c| [1.0 - c[1], c[2], 1.0 - c[0]]).collect() };
        (a, b)
    }

    #[test]
    fn catalogue_has_over_one_hundred_unique_saved_presets() {
        assert!(Wipe::ALL.len() >= 100);
        let mut names = HashSet::new();
        let mut ids = HashSet::new();
        for kind in Wipe::ALL {
            assert!(names.insert(kind.name()), "duplicate label {kind:?}");
            assert!(CATEGORIES.contains(&kind.category()));
            let saved = serde_json::to_string(&kind).unwrap();
            assert!(ids.insert(saved.clone()));
            assert_eq!(serde_json::from_str::<Wipe>(&saved).unwrap(), kind);
        }
        assert_eq!(serde_json::from_str::<Wipe>("\"Dissolve\"").unwrap(), Wipe::Dissolve);
        assert_eq!(serde_json::from_str::<Wipe>("\"Iris\"").unwrap(), Wipe::Iris);
    }

    #[test]
    fn all_presets_are_finite_deterministic_and_finish_in_both_aspects() {
        for (w, h) in [(48, 27), (27, 48), (1, 1)] {
            let (a, b) = pictures(w, h);
            for kind in Wipe::ALL {
                assert_eq!(blend(&a, &b, kind, 0.0).px, a.px, "{kind:?}");
                assert_eq!(blend(&a, &b, kind, 1.0).px, b.px, "{kind:?}");
                for t in [0.01, 0.25, 0.5, 0.75, 0.99] {
                    let p = blend(&a, &b, kind, t);
                    assert_eq!((p.w, p.h, p.px.len()), (w, h, w * h));
                    assert!(p.px.iter().flatten().all(|v| v.is_finite() && (-0.001..=1.001).contains(v)), "{kind:?} at {t}");
                    assert_eq!(p.px, blend(&a, &b, kind, t).px, "{kind:?} changes between renders");
                }
            }
        }
    }

    #[test]
    fn procedural_masks_reveal_monotonically_without_an_end_pop() {
        let a = Picture { w: 64, h: 36, px: vec![[0.0; 3]; 64 * 36] };
        let b = Picture { w: 64, h: 36, px: vec![[1.0; 3]; 64 * 36] };
        for kind in Wipe::ALL {
            if !matches!(kind.recipe(), Mask(..)) {
                continue;
            }
            let mut previous = a.px.clone();
            for step in 1..=20 {
                let next = blend(&a, &b, kind, step as f32 / 20.0);
                assert!(previous.iter().zip(&next.px).all(|(a, b)| a[0] <= b[0] + 1e-5), "{kind:?} reverses");
                previous = next.px;
            }
            assert!(blend(&a, &b, kind, 0.999).px.iter().all(|c| c[0] > 0.999), "{kind:?} ends abruptly");
        }
    }

    #[test]
    fn luma_masks_follow_content_and_reverse_order() {
        let (a, _) = pictures(32, 18);
        let b = Picture { w: a.w, h: a.h, px: vec![[1.0; 3]; a.px.len()] };
        let bright = blend(&a, &b, Wipe::LumaBright, 0.5);
        let dark = blend(&a, &b, Wipe::LumaDark, 0.5);
        assert!(bright.px.last().unwrap()[0] > dark.px.last().unwrap()[0]);
        assert!(dark.px[0][0] > bright.px[0][0]);
    }

    #[test]
    fn presets_have_distinct_visual_signatures() {
        // Enough resolution for the legacy blur's radius to exceed one pixel.
        let (a, b) = pictures(96, 54);
        let mut signatures = HashSet::new();
        for kind in Wipe::ALL {
            let signature: Vec<u8> =
                [0.23, 0.51, 0.78].into_iter().flat_map(|t| blend(&a, &b, kind, t).px.into_iter().flatten().map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)).collect();
            assert!(signatures.insert(signature), "{kind:?} duplicates another preset");
        }
    }
}
