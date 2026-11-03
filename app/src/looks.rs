//! The preset collection. Every table here is calculated from the small recipes
//! below; none of it comes from another product's colour files. The Cinema and Deep
//! Looks names and slider settings follow the original, as listed in MECHANICS.md.
//! The Studio, Film and Mood looks are this app's own.

use crate::effects::Diffusion;
use crate::film::{FilmLook, Split};
use crate::lut::{Lut3D, LutBank};
use crate::pipeline::{Grade, LutRef, Tint};
use crate::tools::{Mixer, Wheels};
use rayon::prelude::*;
use std::sync::Arc;

/// How one colour table is built.
#[derive(Clone, Copy)]
pub struct Recipe {
    pub name: &'static str,
    pub group: &'static str,
    /// Strength of the S-curve for red, green and blue.
    pub curve: [f32; 3],
    /// Where each channel's midpoint sits; below 1 brightens the channel.
    pub mid: [f32; 3],
    pub lift: [f32; 3],
    pub top: [f32; 3],
    pub shadow: [f32; 3],
    pub high: [f32; 3],
    pub sat: f32,
    /// Removes colour from the brightest parts, as film prints do.
    pub roll: f32,
    /// Pulls colours toward the orange and teal axis.
    pub axis: f32,
    /// Channel weights for black and white looks.
    pub mono: Option<[f32; 3]>,
}

const BASE: Recipe = Recipe {
    name: "",
    group: "",
    curve: [1.0, 1.0, 1.0],
    mid: [1.0, 1.0, 1.0],
    lift: [0.0, 0.0, 0.0],
    top: [1.0, 1.0, 1.0],
    shadow: [0.0, 0.0, 0.0],
    high: [0.0, 0.0, 0.0],
    sat: 1.0,
    roll: 0.0,
    axis: 0.0,
    mono: None,
};

#[inline]
fn s_curve(x: f32, a: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    let p = x.powf(a);
    p / (p + (1.0 - x).powf(a))
}

impl Recipe {
    pub fn eval(&self, c: [f32; 3]) -> [f32; 3] {
        let mut c = c;
        if let Some(m) = self.mono {
            let v = c[0] * m[0] + c[1] * m[1] + c[2] * m[2];
            c = [v, v, v];
        }
        // The orange-teal pull: warm hues move toward orange, cool ones toward teal.
        if self.axis != 0.0 {
            let warmth = (c[0] - c[2]).clamp(-1.0, 1.0);
            let l = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
            let t = self.axis * (warmth.abs()).sqrt().min(1.0);
            let target = if warmth > 0.0 { [l * 1.26, l * 0.98, l * 0.62] } else { [l * 0.56, l * 1.02, l * 1.2] };
            for i in 0..3 {
                c[i] += (target[i] - c[i]) * t * 0.55;
            }
        }
        let l = (0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]).clamp(0.0, 1.0);
        let mut o = [0.0; 3];
        for i in 0..3 {
            let v = c[i].clamp(0.0, 1.0).powf(self.mid[i]);
            let v = s_curve(v, self.curve[i]);
            let s = (1.0 - l) * (1.0 - l);
            let h = l * l;
            let v = v + self.shadow[i] * s + self.high[i] * h;
            o[i] = self.lift[i] + v * (self.top[i] - self.lift[i]);
        }
        let l2 = 0.2126 * o[0] + 0.7152 * o[1] + 0.0722 * o[2];
        let sat = self.sat * (1.0 - self.roll * l2.clamp(0.0, 1.0).powi(3));
        for v in &mut o {
            *v = l2 + (*v - l2) * sat;
        }
        o
    }

    pub fn id(&self) -> String {
        format!("look:{}", self.name)
    }
}

macro_rules! recipe {
    ($name:literal, $group:literal, { $($f:ident : $v:expr),* $(,)? }) => {
        Recipe { name: $name, group: $group, $($f: $v,)* ..BASE }
    };
}

/// The tables shown on the LUTs tab.
pub const RECIPES: &[Recipe] = &[
    // Print: warm and cool print stocks at three white points.
    recipe!("Warm Print 55", "Print", { curve: [1.42, 1.38, 1.30], mid: [0.96, 1.02, 1.1], lift: [0.03, 0.022, 0.018], top: [1.0, 0.95, 0.86], shadow: [-0.01, 0.0, 0.02], sat: 1.04, roll: 0.5, axis: 0.3 }),
    recipe!("Warm Print 60", "Print", { curve: [1.42, 1.38, 1.32], mid: [0.98, 1.02, 1.06], lift: [0.028, 0.022, 0.02], top: [1.0, 0.965, 0.9], shadow: [-0.01, 0.0, 0.02], sat: 1.04, roll: 0.5, axis: 0.28 }),
    recipe!("Warm Print 65", "Print", { curve: [1.42, 1.4, 1.34], mid: [1.0, 1.02, 1.03], lift: [0.026, 0.022, 0.022], top: [0.99, 0.975, 0.94], sat: 1.03, roll: 0.5, axis: 0.25 }),
    recipe!("Cool Print 55", "Print", { curve: [1.5, 1.46, 1.46], mid: [1.02, 1.0, 1.02], lift: [0.018, 0.024, 0.024], top: [0.98, 0.97, 0.9], shadow: [-0.015, 0.008, 0.02], sat: 0.98, roll: 0.55, axis: 0.34 }),
    recipe!("Cool Print 60", "Print", { curve: [1.5, 1.46, 1.48], mid: [1.04, 1.0, 1.0], lift: [0.018, 0.024, 0.028], top: [0.975, 0.98, 0.94], shadow: [-0.015, 0.008, 0.02], sat: 0.98, roll: 0.55, axis: 0.32 }),
    recipe!("Cool Print 65", "Print", { curve: [1.5, 1.46, 1.5], mid: [1.06, 1.0, 0.98], lift: [0.018, 0.024, 0.032], top: [0.96, 0.98, 0.98], shadow: [-0.015, 0.008, 0.024], sat: 0.97, roll: 0.55, axis: 0.3 }),
    // Film: negative and slide stock characters.
    recipe!("Daylight 50", "Film", { curve: [1.3, 1.34, 1.3], sat: 1.34, top: [1.0, 0.99, 0.97], roll: 0.2, high: [0.0, 0.0, -0.02] }),
    recipe!("Daylight 100", "Film", { curve: [1.24, 1.26, 1.24], mid: [0.98, 1.0, 1.04], sat: 1.2, lift: [0.012, 0.01, 0.01], roll: 0.3 }),
    recipe!("Everyday 200", "Film", { curve: [1.14, 1.12, 1.1], mid: [0.92, 0.97, 1.08], lift: [0.03, 0.022, 0.016], top: [1.0, 0.97, 0.88], sat: 1.12, roll: 0.35 }),
    recipe!("Everyday 400", "Film", { curve: [1.12, 1.12, 1.1], mid: [0.96, 0.95, 1.0], lift: [0.03, 0.03, 0.03], top: [0.98, 0.98, 0.94], shadow: [0.0, 0.012, 0.016], sat: 1.04, roll: 0.3 }),
    recipe!("Portrait 160", "Film", { curve: [1.06, 1.06, 1.06], mid: [0.94, 0.97, 1.0], lift: [0.026, 0.022, 0.02], top: [0.99, 0.975, 0.94], sat: 0.92, roll: 0.5 }),
    recipe!("Portrait 400", "Film", { curve: [1.08, 1.08, 1.06], mid: [0.92, 0.96, 1.0], lift: [0.03, 0.026, 0.024], top: [0.99, 0.97, 0.93], sat: 0.96, roll: 0.5 }),
    recipe!("Portrait 800", "Film", { curve: [1.12, 1.12, 1.1], mid: [0.92, 0.96, 0.98], lift: [0.036, 0.03, 0.028], top: [0.99, 0.97, 0.93], sat: 1.0, roll: 0.45 }),
    recipe!("Fine Stock 400", "Film", { curve: [1.08, 1.12, 1.1], mid: [1.0, 0.94, 0.94], lift: [0.03, 0.034, 0.032], top: [0.97, 0.99, 0.95], shadow: [0.0, 0.01, 0.012], sat: 0.9, roll: 0.4 }),
    recipe!("Green Stock 1600", "Film", { curve: [1.2, 1.18, 1.16], mid: [1.02, 0.94, 0.98], lift: [0.03, 0.04, 0.034], shadow: [-0.01, 0.018, 0.008], sat: 1.02, roll: 0.3 }),
    recipe!("Soft Pro 400", "Film", { curve: [1.02, 1.02, 1.04], mid: [0.92, 0.92, 0.9], lift: [0.04, 0.04, 0.04], top: [0.98, 0.98, 0.97], shadow: [0.0, 0.008, 0.016], sat: 0.86, roll: 0.45 }),
    recipe!("Motion 250", "Film", { curve: [1.2, 1.2, 1.2], mid: [0.98, 1.0, 1.04], lift: [0.024, 0.022, 0.02], top: [0.99, 0.97, 0.92], sat: 1.02, roll: 0.5, axis: 0.14 }),
    recipe!("Motion 500", "Film", { curve: [1.22, 1.2, 1.22], mid: [1.04, 1.0, 0.96], lift: [0.02, 0.024, 0.03], top: [0.97, 0.98, 0.97], sat: 1.0, roll: 0.5, axis: 0.2 }),
    // Creative: stronger graded moods.
    recipe!("End of Days", "Creative", { curve: [1.5, 1.4, 1.3], mid: [0.9, 1.04, 1.34], top: [1.0, 0.86, 0.6], lift: [0.02, 0.012, 0.0], sat: 0.84, roll: 0.3 }),
    recipe!("Street Crew", "Creative", { curve: [1.34, 1.3, 1.3], mid: [0.92, 0.98, 1.06], sat: 1.24, axis: 0.2 }),
    recipe!("Steel Town", "Creative", { curve: [1.3, 1.3, 1.26], mid: [1.1, 1.0, 0.86], top: [0.92, 0.97, 1.0], lift: [0.0, 0.012, 0.03], sat: 0.8, roll: 0.3 }),
    recipe!("Last Light", "Creative", { curve: [1.4, 1.34, 1.3], mid: [1.12, 1.02, 0.84], shadow: [-0.03, 0.0, 0.05], high: [0.02, 0.0, -0.05], sat: 1.12, axis: 0.3 }),
    recipe!("Outlaw", "Creative", { curve: [1.44, 1.4, 1.34], mid: [0.92, 1.02, 1.2], lift: [0.03, 0.022, 0.012], top: [0.97, 0.92, 0.78], sat: 0.76, roll: 0.3 }),
    recipe!("Cold Metal", "Creative", { curve: [1.46, 1.42, 1.4], mid: [1.08, 1.0, 0.94], sat: 0.72, roll: 0.3, top: [0.96, 1.0, 1.0] }),
    recipe!("Harvest", "Creative", { curve: [1.26, 1.22, 1.2], mid: [0.9, 0.98, 1.2], top: [1.0, 0.95, 0.8], sat: 1.26, roll: 0.2 }),
    recipe!("Somber", "Creative", { curve: [1.3, 1.28, 1.26], mid: [1.24, 1.22, 1.18], lift: [0.0, 0.006, 0.014], sat: 0.76, roll: 0.3 }),
    recipe!("Hard Push", "Creative", { curve: [1.8, 1.76, 1.72], sat: 1.34 }),
    recipe!("Green Canopy", "Creative", { curve: [1.2, 1.16, 1.2], mid: [1.04, 0.92, 1.06], shadow: [-0.01, 0.022, 0.0], sat: 1.18 }),
    recipe!("Harbor Morning", "Creative", { curve: [1.16, 1.14, 1.12], mid: [0.98, 0.94, 0.9], lift: [0.03, 0.04, 0.05], top: [1.0, 0.97, 0.9], sat: 0.96, roll: 0.3 }),
    recipe!("Golden Hour", "Creative", { curve: [1.22, 1.2, 1.2], mid: [0.86, 0.96, 1.2], top: [1.0, 0.94, 0.76], sat: 1.2, roll: 0.2 }),
    recipe!("Night Rise", "Creative", { curve: [1.4, 1.36, 1.3], mid: [1.32, 1.18, 0.96], shadow: [-0.02, 0.0, 0.05], sat: 0.8, axis: 0.2 }),
    recipe!("Trench", "Creative", { curve: [1.46, 1.42, 1.4], mid: [1.06, 1.0, 1.02], sat: 0.54, lift: [0.018, 0.018, 0.014], roll: 0.2 }),
    recipe!("Soft Mono", "Creative", { curve: [1.1, 1.1, 1.1], mid: [0.96, 0.96, 0.96], lift: [0.02, 0.02, 0.02], top: [0.98, 0.98, 0.98], mono: Some([0.3, 0.59, 0.11]) }),
    recipe!("Code Green", "Creative", { curve: [1.46, 1.3, 1.46], mid: [1.2, 0.96, 1.24], sat: 0.8, shadow: [-0.01, 0.03, 0.0] }),
];

#[derive(Clone)]
pub struct BuiltIn {
    pub id: String,
    pub name: String,
    pub group: &'static str,
    pub grade: Grade,
}

fn table(name: &str, intensity: f32) -> Vec<LutRef> {
    vec![LutRef { id: format!("look:{name}"), name: name.to_string(), intensity }]
}

fn tint(c: [f32; 3], opacity: f32) -> Tint {
    Tint { r: c[0], g: c[1], b: c[2], opacity }
}

fn film(stock: usize, print: usize, intensity: f32, grain: f32, halation: f32) -> FilmLook {
    FilmLook { on: true, stock, print, intensity, grain, halation, ..FilmLook::default() }
}

fn split(style: usize, amount: f32, balance: f32) -> Split {
    Split { style, amount, balance }
}

/// The original's saved contrast assumes its stronger print chain; its plain print
/// presets sit at 0.9, so that is treated as neutral on this app's gentler tables.
fn print_contrast(saved: f32) -> f32 {
    1.0 + (saved - PRINT_NEUTRAL_CONTRAST) * PRINT_CONTRAST_SCALE
}
const PRINT_NEUTRAL_CONTRAST: f32 = 0.9;
const PRINT_CONTRAST_SCALE: f32 = 0.6;

/// The group order on the Presets tab.
pub const PRESET_GROUPS: [&str; 5] = ["Cinema", "Deep Looks", "Studio", "Film", "Mood"];

/// The Presets tab. Cinema and Deep Looks come first, in the original's order, with its
/// slider and colour grade values (see MECHANICS.md). A white grade means "no tint"
/// there, so those are left out. Cinema looks sit on print tables; Deep Looks use none.
pub fn presets() -> Vec<BuiltIn> {
    let mut v = Vec::new();
    let mut add = |name: &str, group: &'static str, g: Grade| {
        v.push(BuiltIn { id: format!("builtin:{name}"), name: name.to_string(), group, grade: g });
    };
    let d = Grade::default;
    let warm = [1.0, 0.6, 0.35];
    let gold = [1.0, 0.8, 0.0];
    let ice = [0.3, 0.87, 1.0];

    add("Deep Cine", "Cinema", Grade {
        luts: table("Cool Print 60", 1.0),
        temp: -870.0, tint: -16.0, exposure: -0.22, brightness: -0.042, contrast: print_contrast(0.66),
        highlight: -0.32, saturation: 1.16, vibrance: 0.27,
        shadows_tint: tint(ice, 0.15), midtones_tint: tint(warm, 0.17), highlights_tint: tint(gold, 0.23),
        ..d()
    });
    add("Tealer", "Cinema", Grade {
        luts: table("Cool Print 60", 1.0),
        temp: -870.0, tint: -16.0, exposure: -0.24, brightness: -0.045, contrast: print_contrast(0.27),
        highlight: -0.19, whites: -0.3, blacks: -0.28, saturation: 1.16, vibrance: 0.27,
        shadows_tint: tint([1.0, 0.93, 0.48], 0.5), midtones_tint: tint([0.8, 1.0, 0.62], 0.39),
        highlights_tint: tint([0.56, 0.86, 1.0], 0.38),
        ..d()
    });
    add("Deep Cine2", "Cinema", Grade {
        luts: table("Cool Print 60", 1.0),
        temp: -870.0, tint: -16.0, exposure: -0.1, brightness: -0.099, contrast: print_contrast(0.66),
        highlight: -0.41, saturation: 1.16, vibrance: 0.27,
        shadows_tint: tint(ice, 0.15), midtones_tint: tint(warm, 0.17), highlights_tint: tint(gold, 0.23),
        ..d()
    });
    add("Brampton", "Cinema", Grade {
        luts: table("Cool Print 60", 1.0),
        exposure: -0.24, contrast: print_contrast(0.99),
        ..d()
    });
    for (name, lut) in [
        ("Fuji D55", "Cool Print 55"),
        ("Fuji D60", "Cool Print 60"),
        ("Fuji D65", "Cool Print 65"),
        ("Kodak D55", "Warm Print 55"),
        ("Kodak D60", "Warm Print 60"),
        ("Kodak D65", "Warm Print 65"),
    ] {
        add(name, "Cinema", Grade { luts: table(lut, 1.0), contrast: print_contrast(0.9), ..d() });
    }

    add("Blades", "Deep Looks", Grade {
        luts: table("Cool Print 60", 1.0),
        temp: -870.0, tint: -16.0, exposure: -0.22, brightness: -0.042, contrast: print_contrast(0.66),
        highlight: -0.32, saturation: 1.16, vibrance: 0.27,
        shadows_tint: tint([0.06, 1.0, 0.7], 0.15), midtones_tint: tint([1.0, 0.59, 0.43], 0.17),
        highlights_tint: tint(gold, 0.23),
        ..d()
    });
    add("Bind", "Deep Looks", Grade {
        temp: -660.0, tint: -17.6, exposure: -0.38, contrast: print_contrast(0.3), highlight: -0.4, shadow: 0.036,
        whites: -0.34, blacks: 0.82,
        shadows_tint: tint([0.0, 0.03, 1.0], 0.26), midtones_tint: tint([0.3, 1.0, 0.94], 0.45),
        highlights_tint: tint([0.52, 0.69, 1.0], 0.31),
        ..d()
    });
    add("Deep V", "Deep Looks", Grade {
        temp: 2280.0, exposure: -0.2, contrast: 1.17, highlight: -0.25, shadow: 0.032,
        saturation: 1.37, vibrance: 0.15,
        ..d()
    });
    // Greenery's newer file saves contrast as 0; that is read as "not set" (see MECHANICS.md).
    add("Greenery", "Deep Looks", Grade {
        saturation: 1.56, vibrance: 0.34,
        shadows_tint: tint([0.5, 0.62, 1.0], 0.5), highlights_tint: tint([1.0, 0.72, 0.27], 0.5),
        ..d()
    });
    add("G58", "Deep Looks", Grade {
        tint: -19.2, exposure: -0.26, contrast: 0.75, saturation: 1.56, vibrance: 0.34,
        ..d()
    });

    let (studio, stock, mood) = (PRESET_GROUPS[2], PRESET_GROUPS[3], PRESET_GROUPS[4]);
    add("Harbor Light", studio, Grade {
        luts: table("Motion 500", 0.7),
        contrast: 1.08, highlight: -0.22, shadow: 0.08, vibrance: 0.12,
        shadows_tint: tint([0.12, 0.55, 0.62], 0.22), highlights_tint: tint([1.0, 0.78, 0.5], 0.18),
        ..d()
    });
    add("Neon Rain", studio, Grade {
        temp: -620.0, tint: 10.0, exposure: -0.28, contrast: 1.16, highlight: -0.18, saturation: 1.14,
        shadows_tint: tint([0.28, 0.2, 0.85], 0.18), halation_intensity: 0.22, bloom_intensity: 0.14,
        ..d()
    });
    add("Long Shadow", studio, Grade {
        exposure: -0.36, contrast: 1.2, shadow: -0.08, film_contrast: 0.3, film_density: 0.32,
        saturation: 0.92, vignette: 0.24,
        ..d()
    });
    add("Blue Hour", studio, Grade {
        temp: -1250.0, tint: 6.0, exposure: -0.12, contrast: 1.05, highlight: -0.14, saturation: 0.96, fade: 0.05,
        midtones_tint: tint([0.45, 0.6, 1.0], 0.14),
        ..d()
    });
    add("Golden Road", studio, Grade {
        temp: 950.0, tint: 6.0, contrast: 1.07, highlight: -0.16, vibrance: 0.16,
        highlights_tint: tint([1.0, 0.72, 0.3], 0.24), halation_intensity: 0.18,
        ..d()
    });

    add("Soft Daylight", stock, Grade { film: film(1, 0, 0.72, 0.14, 0.18), ..d() });
    add("Tungsten Night", stock, Grade {
        film: film(0, 3, 0.7, 0.24, 0.42), exposure: -0.55, temp: -560.0, tint: 6.0, contrast: 1.12, highlight: -0.1,
        ..d()
    });
    add("Pastel Print", stock, Grade {
        film: film(3, 4, 0.8, 0.12, 0.1), fade: 0.14, saturation: 0.94, shadow: 0.12,
        ..d()
    });
    add("Silver Skip", stock, Grade {
        bleach: 0.62, contrast: 1.04, exposure: -0.06, grain_intensity: 0.16,
        ..d()
    });
    add("Three Strip", stock, Grade {
        technicolor: 0.26, saturate: 0.1, contrast: 1.06, highlight: -0.12,
        ..d()
    });

    add("Quiet Morning", mood, Grade {
        temp: 240.0, contrast: 0.9, shadow: 0.14, fade: 0.18, saturation: 0.86,
        diffusion: Diffusion { filter: 2, strength: 0.24, size: 1.2 },
        ..d()
    });
    add("Dust Bowl", mood, Grade {
        temp: 520.0, saturation: 0.72, contrast: 1.04, fade: 0.08, split: split(5, 0.6, 0.1), grain_intensity: 0.2,
        ..d()
    });
    add("Cold Iron", mood, Grade {
        temp: -520.0, saturation: 0.58, contrast: 1.18, split: split(6, 0.36, -0.1),
        ..d()
    });
    add("Ember", mood, Grade {
        exposure: -0.16, contrast: 1.13, temp: 620.0, vibrance: 0.1,
        wheels: Wheels { gain: [0.05, 0.0, -0.05, 0.0], lift: [0.012, 0.0, -0.012, 0.0], ..Wheels::default() },
        vignette: 0.16,
        ..d()
    });
    add("Plain Silver", mood, Grade {
        mixer: Mixer { mono: true, mono_mix: [0.42, 0.46, 0.12], ..Mixer::default() },
        contrast: 1.14, grain_intensity: 0.16,
        ..d()
    });
    v
}

pub fn build_bank() -> LutBank {
    let built: Vec<(String, Arc<Lut3D>)> = RECIPES.par_iter().map(|r| (r.id(), Arc::new(Lut3D::from_fn(33, |c| r.eval(c))))).collect();
    LutBank { luts: built.into_iter().collect() }
}

/// The group order on the LUTs tab.
pub const LUT_GROUPS: &[&str] = &["Print", "Film", "Creative"];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::{self, Picture};
    use crate::scene;

    fn luma(c: [f32; 3]) -> f32 {
        0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
    }

    /// The looks made for this app; the original's are held to its own values instead.
    fn own() -> Vec<BuiltIn> {
        presets().into_iter().filter(|p| PRESET_GROUPS[2..].contains(&p.group)).collect()
    }

    #[test]
    fn the_original_fifteen_come_first_in_order() {
        let names: Vec<String> = presets().into_iter().take(15).map(|p| p.name).collect();
        assert_eq!(names, [
            "Deep Cine", "Tealer", "Deep Cine2", "Brampton", "Fuji D55", "Fuji D60", "Fuji D65",
            "Kodak D55", "Kodak D60", "Kodak D65", "Blades", "Bind", "Deep V", "Greenery", "G58",
        ]);
    }

    #[test]
    fn the_original_values_are_back() {
        let list = presets();
        let find = |n: &str| list.iter().find(|p| p.name == n).unwrap();
        assert_eq!(find("Deep Cine").grade.temp, -870.0);
        assert_eq!(find("Bind").grade.blacks, 0.82);
        assert_eq!(find("Deep V").grade.temp, 2280.0);
        assert_eq!(find("G58").grade.contrast, 0.75);
        assert_eq!(find("Kodak D60").grade.luts[0].name, "Warm Print 60");
    }

    #[test]
    fn every_group_has_its_looks_and_no_name_repeats() {
        let list = presets();
        assert_eq!(list.len(), 30);
        for (g, n) in PRESET_GROUPS.iter().zip([10, 5, 5, 5, 5]) {
            assert_eq!(list.iter().filter(|p| p.group == *g).count(), n, "{g}");
        }
        let mut names: Vec<&str> = list.iter().map(|p| p.name.as_str()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 30);
    }

    #[test]
    fn every_preset_table_exists() {
        let bank = build_bank();
        for p in presets() {
            for l in &p.grade.luts {
                assert!(bank.get(&l.id).is_some(), "{} needs {}", p.name, l.id);
            }
        }
    }

    #[test]
    fn no_look_crushes_or_blows_out_the_picture() {
        let bank = build_bank();
        let base = scene::sample_picture(240, 160);
        for p in own() {
            let out = pipeline::render(&base, &p.grade, &bank, false);
            let n = out.px.len() as f32;
            let mean = out.px.iter().map(|c| luma(*c)).sum::<f32>() / n;
            let lost = out.px.iter().filter(|c| luma(**c) > 0.985 || luma(**c) < 0.01).count() as f32 / n;
            assert!((0.18..0.72).contains(&mean), "{} has mean {mean}", p.name);
            assert!(lost < 0.08, "{} loses {lost} of its pixels", p.name);
        }
    }

    #[test]
    fn every_look_is_visibly_its_own() {
        let bank = build_bank();
        let base = scene::sample_picture(120, 80);
        let list = own();
        let shots: Vec<Picture> = list.iter().map(|p| pipeline::render(&base, &p.grade, &bank, false)).collect();
        for (i, a) in shots.iter().enumerate() {
            for (j, b) in shots.iter().enumerate().skip(i + 1) {
                let gap = a.px.iter().zip(&b.px).map(|(x, y)| (0..3).map(|k| (x[k] - y[k]).abs()).sum::<f32>()).sum::<f32>();
                let gap = gap / (a.px.len() * 3) as f32;
                assert!(gap > 0.012, "{} and {} are too alike ({gap})", list[i].name, list[j].name);
            }
        }
    }

    #[test]
    fn skin_stays_skin_in_every_colour_look() {
        let bank = build_bank();
        let skin = Picture { w: 3, h: 1, px: vec![[0.93, 0.76, 0.66], [0.76, 0.55, 0.42], [0.42, 0.28, 0.2]] };
        for p in own().iter().filter(|p| !p.grade.mixer.mono) {
            let out = pipeline::render(&skin, &p.grade, &bank, false);
            for c in &out.px {
                assert!(c[0] > c[2], "{} turns skin cold: {c:?}", p.name);
            }
        }
    }

    #[test]
    fn the_black_and_white_look_has_no_colour_left() {
        let list = presets();
        let mono = list.iter().find(|p| p.grade.mixer.mono).unwrap();
        let out = pipeline::render(&scene::sample_picture(60, 40), &Grade { grain_intensity: 0.0, ..mono.grade.clone() }, &build_bank(), false);
        assert!(out.px.iter().all(|c| (c[0] - c[2]).abs() < 1e-4));
    }
}
