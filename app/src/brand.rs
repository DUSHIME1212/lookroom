//! Brand kits: a company's logo, colours, fonts, caption look, watermark and voice, kept
//! once and used on every project. Each kit is one JSON file in the Brands folder with the
//! logo inside it, so a kit can be handed to the rest of a team as a single file.

use crate::store;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The most colours a kit holds; more than this is a palette, not a brand.
pub const MOST_COLOURS: usize = 8;
/// What a shared kit's file ends in.
pub const EXTENSION: &str = "lookbrand";

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(default)]
pub struct Brand {
    pub id: String,
    pub name: String,
    /// The logo as SVG. Pictures are wrapped in one, so every logo draws as one layer.
    pub logo: String,
    /// The first is the main colour.
    pub colours: Vec<[f32; 3]>,
    /// Titles and other text; empty means the app's own font.
    pub heading_font: String,
    pub body_font: String,
    pub title_colour: [f32; 3],
    pub captions: CaptionLook,
    pub watermark: Watermark,
    /// How the brand sounds, for the assistant: tone, words to use and to avoid.
    pub voice: String,
}

impl Default for Brand {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: "New Brand".into(),
            logo: String::new(),
            colours: vec![[0.48, 0.38, 1.0], [1.0, 1.0, 1.0], [0.08, 0.08, 0.1]],
            heading_font: String::new(),
            body_font: String::new(),
            title_colour: [1.0; 3],
            captions: CaptionLook::default(),
            watermark: Watermark::default(),
            voice: String::new(),
        }
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(default)]
pub struct CaptionLook {
    pub font: String,
    pub size: f32,
    pub colour: [f32; 3],
    pub weight: u16,
    pub outline: f32,
    /// One of the Auto Captions styles, by place in its list.
    pub style: usize,
}

impl Default for CaptionLook {
    fn default() -> Self {
        Self { font: String::new(), size: 64.0, colour: [1.0; 3], weight: 700, outline: 0.06, style: 0 }
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug)]
#[serde(default)]
pub struct Watermark {
    pub on: bool,
    /// Top left, top right, bottom left, bottom right.
    pub corner: usize,
    /// The logo's height, as a share of the picture's.
    pub size: f32,
    pub opacity: f32,
}

impl Default for Watermark {
    fn default() -> Self {
        Self { on: true, corner: 1, size: 0.1, opacity: 0.85 }
    }
}

pub const CORNERS: [&str; 4] = ["Top Left", "Top Right", "Bottom Left", "Bottom Right"];

/// Room between a watermark and the picture's edge, as a share of its height.
const MARGIN: f32 = 0.04;

impl Watermark {
    /// The logo's centre on a picture `pw` by `ph`, for a logo `lw` by `lh` drawn there.
    pub fn centre(&self, lw: f32, lh: f32, pw: f32, ph: f32) -> (f32, f32) {
        let m = MARGIN * ph;
        let left = self.corner % 2 == 0;
        let top = self.corner < 2;
        let x = if left { m + lw / 2.0 } else { pw - m - lw / 2.0 };
        let y = if top { m + lh / 2.0 } else { ph - m - lh / 2.0 };
        (x / pw.max(1.0), y / ph.max(1.0))
    }
}

pub fn dir() -> PathBuf {
    store::data_dir().join("Brands")
}

fn file_of(id: &str) -> PathBuf {
    dir().join(format!("{id}.json"))
}

/// Every kit, by name.
pub fn list() -> Vec<Brand> {
    let mut v: Vec<Brand> = std::fs::read_dir(dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "json"))
        .filter_map(|e| serde_json::from_str::<Brand>(&std::fs::read_to_string(e.path()).ok()?).ok())
        .filter(|b| !b.id.is_empty())
        .collect();
    v.sort_by_key(|b| b.name.to_lowercase());
    v
}

pub fn save(b: &Brand) -> Result<(), String> {
    std::fs::create_dir_all(dir()).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(b).map_err(|e| e.to_string())?;
    std::fs::write(file_of(&b.id), text).map_err(|e| e.to_string())
}

pub fn delete(id: &str) {
    let _ = std::fs::remove_file(file_of(id));
}

/// A kit someone shared. It gets an id of its own, so it never replaces one already here.
pub fn import(path: &Path) -> Result<Brand, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut b: Brand = serde_json::from_str(&text).map_err(|_| "That file isn't a Lookroom brand kit.".to_string())?;
    b.id = store::new_id();
    b.colours.truncate(MOST_COLOURS);
    save(&b)?;
    Ok(b)
}

pub fn export(b: &Brand, path: &Path) -> Result<(), String> {
    let text = serde_json::to_string_pretty(b).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| e.to_string())
}

/// Reads a logo file as SVG. Pictures (PNG, JPEG, WebP) go inside an SVG of their own size,
/// and SVG logos inside another, so a logo of many shapes still draws as one layer.
pub fn logo_from(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let (mime, w, h) = if ext == "svg" {
        let tree = resvg::usvg::Tree::from_data(&bytes, &resvg::usvg::Options::default()).map_err(|_| "That SVG could not be read.".to_string())?;
        ("image/svg+xml", tree.size().width(), tree.size().height())
    } else {
        let img = image::load_from_memory(&bytes).map_err(|_| "That picture could not be read. Use PNG, JPEG, WebP or SVG.".to_string())?;
        let mime = match ext.as_str() {
            "jpg" | "jpeg" => "image/jpeg",
            "webp" => "image/webp",
            _ => "image/png",
        };
        (mime, img.width() as f32, img.height() as f32)
    };
    Ok(wrap(mime, &bytes, w, h))
}

fn wrap(mime: &str, bytes: &[u8], w: f32, h: f32) -> String {
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{w}" height="{h}" viewBox="0 0 {w} {h}"><image width="{w}" height="{h}" xlink:href="data:{mime};base64,{}"/></svg>"#,
        base64(bytes)
    )
}

fn base64(bytes: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, b)| n | (*b as u32) << (16 - 8 * i));
        for i in 0..4 {
            out.push(if i <= chunk.len() { ABC[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

/// A colour as #RRGGBB, for people and for the assistant.
pub fn hex(c: [f32; 3]) -> String {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02X}{:02X}{:02X}", b(c[0]), b(c[1]), b(c[2]))
}

/// Reads #RGB or #RRGGBB.
pub fn parse_hex(s: &str) -> Option<[f32; 3]> {
    let s = s.trim().trim_start_matches('#');
    let digits: Vec<u8> = match s.len() {
        3 => s.chars().map(|c| c.to_digit(16).map(|d| d as u8 * 17)).collect::<Option<_>>()?,
        6 => (0..3).map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).ok()).collect::<Option<_>>()?,
        _ => return None,
    };
    Some([digits[0] as f32 / 255.0, digits[1] as f32 / 255.0, digits[2] as f32 / 255.0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn colours_go_to_hex_and_back() {
        assert_eq!(hex([1.0, 0.5, 0.0]), "#FF8000");
        assert_eq!(parse_hex("#ff8000").map(hex).as_deref(), Some("#FF8000"));
        assert_eq!(parse_hex("fff"), Some([1.0; 3]));
        assert!(parse_hex("#12345").is_none() && parse_hex("zzzzzz").is_none());
    }

    #[test]
    fn a_watermark_sits_inside_its_corner() {
        let w = Watermark { corner: 3, ..Watermark::default() };
        let (x, y) = w.centre(200.0, 100.0, 1920.0, 1080.0);
        assert!(x > 0.85 && x < 1.0 && y > 0.8 && y < 1.0, "{x} {y}");
        let (x, y) = Watermark { corner: 0, ..w }.centre(200.0, 100.0, 1920.0, 1080.0);
        assert!(x < 0.15 && y < 0.15, "{x} {y}");
    }

    #[test]
    fn a_picture_logo_becomes_one_layer_of_svg() {
        let mut png = Vec::new();
        image::RgbaImage::from_pixel(8, 4, image::Rgba([255, 0, 0, 255])).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let svg = wrap("image/png", &png, 8.0, 4.0);
        let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()).unwrap();
        assert_eq!((tree.size().width(), tree.size().height()), (8.0, 4.0));
        assert_eq!(tree.root().children().len(), 1);
    }

    #[test]
    fn old_or_partial_kits_still_open() {
        let b: Brand = serde_json::from_str(r#"{"id":"x","name":"Acme"}"#).unwrap();
        assert_eq!(b.name, "Acme");
        assert_eq!(b.captions, CaptionLook::default());
    }
}
