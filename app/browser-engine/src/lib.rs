//! Browser bindings for the actual desktop colour, effect and transition engine.
//! Media decoding, file dialogs and playback are supplied by the browser, not FFmpeg.
#![allow(dead_code)]
use wasm_bindgen::prelude::*;

#[path = "../../src/color.rs"] mod color;
#[path = "../../src/crop.rs"] mod crop;
#[path = "../../src/curves.rs"] mod curves;
#[path = "../../src/effects.rs"] mod effects;
#[path = "../../src/film.rs"] mod film;
#[path = "../../src/icon_set.rs"] mod icon_set;
#[path = "../../src/looks.rs"] mod looks;
#[path = "../../src/lut.rs"] mod lut;
#[path = "../../src/masks.rs"] mod masks;
#[path = "../../src/overlay.rs"] mod overlay;
#[path = "../../src/pipeline.rs"] mod pipeline;
#[path = "../../src/platform.rs"] mod platform;
#[path = "../../src/scene.rs"] mod scene;
#[path = "../../src/shared.rs"] mod shared;
#[path = "../../src/textanim.rs"] mod textanim;
#[path = "../../src/timeline.rs"] mod timeline;
#[path = "../../src/tools.rs"] mod tools;
#[path = "../../src/transitions.rs"] mod transitions;

// Host adapters retain the shared modules' types, without linking desktop audio/UI.
mod store {
    pub fn fonts_dir() -> std::path::PathBuf { std::path::PathBuf::from("/fonts") }
}
mod video {
    #[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
    pub struct Info {
        pub path: std::path::PathBuf,
        pub w: usize, pub h: usize, pub fps: f64, pub frames: u32,
        pub has_audio: bool, pub bits: u32, pub codec: String, pub tagged: bool,
        pub hdr: Option<crate::color::Source>,
    }
}
mod icons {
    pub use crate::icon_set::Icon;
    pub struct Pixel([u8; 4]);
    impl Pixel {
        pub fn r(&self) -> u8 { self.0[0] } pub fn g(&self) -> u8 { self.0[1] }
        pub fn b(&self) -> u8 { self.0[2] } pub fn a(&self) -> u8 { self.0[3] }
    }
    pub struct Raster { pub size: [usize; 2], pub pixels: Vec<Pixel> }
    pub fn rasterize(icon: Icon, px: u32) -> Option<Raster> {
        let svg = icon.svg().replace("currentColor", "#fff");
        let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()).ok()?;
        let mut map = resvg::tiny_skia::Pixmap::new(px, px)?;
        let scale = px as f32 / tree.size().width();
        resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut map.as_mut());
        Some(Raster { size: [px as usize; 2], pixels: map.data().chunks_exact(4).map(|c| Pixel([c[0], c[1], c[2], c[3]])).collect() })
    }
}

fn picture(bytes: &[u8], w: usize, h: usize) -> Result<pipeline::Picture, JsValue> {
    if w == 0 || h == 0 || w > 4096 || h > 4096 || w.checked_mul(h).and_then(|n| n.checked_mul(4)) != Some(bytes.len()) {
        return Err(JsValue::from_str("Invalid frame size (maximum 4096 pixels per side)"));
    }
    Ok(pipeline::Picture { w, h, px: bytes.chunks_exact(4).map(|c| [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0]).collect() })
}

#[wasm_bindgen]
pub struct Engine {
    bank: lut::LutBank,
    grades: Vec<looks::BuiltIn>,
}

#[wasm_bindgen]
impl Engine {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self { Self { bank: looks::build_bank(), grades: looks::presets() } }

    pub fn catalogue(&self) -> String {
        serde_json::json!({
            "transitions": transitions::Wipe::ALL.iter().map(|k| serde_json::json!({"id": k, "name": k.name(), "category": k.category()})).collect::<Vec<_>>(),
            "effects": timeline::Fx::ALL.iter().map(|k| serde_json::json!({"id": k, "name": k.name()})).collect::<Vec<_>>(),
            "looks": self.grades.iter().map(|g| serde_json::json!({"id": g.id, "name": g.name, "group": g.group, "grade": g.grade})).collect::<Vec<_>>(),
            "categories": transitions::CATEGORIES,
        }).to_string()
    }

    pub fn sample(&self, w: usize, h: usize) -> Result<Vec<u8>, JsValue> {
        if w == 0 || h == 0 || w > 2048 || h > 2048 { return Err(JsValue::from_str("Invalid sample size")); }
        Ok(scene::sample_picture(w, h).to_rgba8())
    }

    pub fn grade(&self, rgba: &[u8], w: usize, h: usize, settings: &str, frame: u32) -> Result<Vec<u8>, JsValue> {
        let g: pipeline::Grade = serde_json::from_str(settings).map_err(|e| JsValue::from_str(&e.to_string()))?;
        Ok(pipeline::render_at(&picture(rgba, w, h)?, &g, &self.bank, true, true, frame).to_rgba8())
    }

    pub fn transition(&self, a: &[u8], b: &[u8], w: usize, h: usize, id: &str, t: f32) -> Result<Vec<u8>, JsValue> {
        let kind: transitions::Wipe = serde_json::from_value(serde_json::Value::String(id.into())).map_err(|_| JsValue::from_str("Unknown transition"))?;
        if !t.is_finite() { return Err(JsValue::from_str("Invalid transition progress")); }
        Ok(overlay::blend(&picture(a, w, h)?, &picture(b, w, h)?, kind, t).to_rgba8())
    }

    pub fn effect(&self, rgba: &[u8], w: usize, h: usize, id: &str, amount: f32, t: f32, frame: u32) -> Result<Vec<u8>, JsValue> {
        let kind: timeline::Fx = serde_json::from_value(serde_json::Value::String(id.into())).map_err(|_| JsValue::from_str("Unknown effect"))?;
        if !amount.is_finite() || !t.is_finite() { return Err(JsValue::from_str("Invalid effect settings")); }
        let mut p = picture(rgba, w, h)?;
        overlay::effect(&mut p, kind, amount.clamp(0.0, 1.0), t.clamp(0.0, 1.0), frame);
        Ok(p.to_rgba8())
    }
}
